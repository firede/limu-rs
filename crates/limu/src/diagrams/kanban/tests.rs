//! Kanban boards parsed against what Pele makes of the same cases, in the oracle's kanban
//! fixture, and laid out against what it draws of them, in its kanban-render fixture.

use limu_fixtures::load;
use serde::Deserialize;

use super::db::KanbanNode;
use super::render::{
    ROLE_ASSIGNED, ROLE_CARD, ROLE_COLUMN, ROLE_COUNT, ROLE_PRIORITY, ROLE_TICKET, ROLE_TITLE,
};
use super::{KanbanDb, layout, parse, parse_kanban, tokenize};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::len16;
use crate::js::number::to_string as n;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::Paint;
use crate::scene::{Align, Item, Op, Outline, Painting, PictureSource, Rect, Scene};
use crate::testing::pelesvg::id;
use crate::testing::{Enc, Failures, PeleError, pele_num, same, sorted};
use crate::value::{Map, Value};

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Tokens {
    types: Vec<u8>,
    texts: Vec<String>,
    starts: Vec<usize>,
}

#[derive(Deserialize)]
struct Result {
    model: Option<Enc>,
    error: Option<PeleError>,
}

#[derive(Deserialize)]
struct Case {
    src: String,
    #[serde(rename = "type")]
    diagram: Option<String>,
    tokens: Tokens,
    direct: Result,
    /// Whether the pipeline's result is the direct one.
    #[serde(default)]
    same: bool,
    model: Option<Enc>,
    error: Option<PeleError>,
}

/// A node as the oracle writes Pele's, without the fields Pele leaves undefined.
fn dump_node(node: &KanbanNode) -> Map {
    let mut o = Map::new();
    o.insert("id", Value::String(node.id.clone()));
    o.insert("level", Value::Number(node.level as f64));
    o.insert("label", Value::String(node.label.clone()));
    for (key, v) in [
        ("icon", &node.icon),
        ("assigned", &node.assigned),
        ("ticket", &node.ticket),
        ("priority", &node.priority),
        ("cssClasses", &node.css_classes),
    ] {
        if let Some(v) = v {
            o.insert(key, Value::String(v.clone()));
        }
    }
    o
}

/// The board as the oracle writes Pele's.
fn dump_model(m: &KanbanDb) -> Value {
    let sections = m.sections.iter().map(|s| {
        let mut o = dump_node(&s.node);
        let items = s.items.iter().map(|i| Value::Object(dump_node(i)));
        o.insert("items", Value::Array(items.collect()));
        Value::Object(o)
    });
    let mut out = Map::new();
    out.insert("sections", Value::Array(sections.collect()));
    if let Some(t) = m.title.as_deref().filter(|t| !t.is_empty()) {
        out.insert("title", Value::String(t.to_string()));
    }
    sorted(&Value::Object(out))
}

/// Whether what was parsed is what Pele made: the same error, or the same model. It says how
/// they differ where they do.
fn compare(
    got: std::result::Result<KanbanDb, Error>,
    error: &Option<PeleError>,
    model: &Option<Enc>,
) -> std::result::Result<(), String> {
    match (got, error) {
        (Err(e), Some(w)) if w.is(&e) => Ok(()),
        (Err(e), Some(w)) => Err(format!("error {e:?}\nPele {w:?}")),
        (Err(e), None) => Err(format!("error {e:?}, Pele had none")),
        (Ok(_), Some(w)) => Err(format!("no error, Pele {w:?}")),
        (Ok(m), None) => {
            let d = dump_model(&m);
            match model {
                Some(w) if same(&d, &sorted(&w.0)) => Ok(()),
                _ => Err(format!("model {d:?}\nPele {model:?}")),
            }
        }
    }
}

/// A case as the oracle runs it: the tokens, the parser given the raw text, and then Pele's
/// parseSource for a kanban board.
fn run(c: &Case) -> std::result::Result<(), String> {
    let t = tokenize(&c.src);
    let types: Vec<u8> = t.types.iter().map(|&t| t as u8).collect();
    let starts: Vec<usize> = t.starts.iter().map(|&s| len16(&c.src[..s])).collect();
    if (&types, &t.texts, &starts) != (&c.tokens.types, &c.tokens.texts, &c.tokens.starts) {
        return Err(format!(
            "tokens {types:?} {:?} {starts:?}\nPele {:?} {:?} {:?}",
            t.texts, c.tokens.types, c.tokens.texts, c.tokens.starts
        ));
    }
    let d = &c.direct;
    let mut db = KanbanDb::default();
    let direct = parse_kanban(&c.src, &mut db).map(|()| db);
    compare(direct, &d.error, &d.model).map_err(|e| format!("direct {e}"))?;
    let p = match prepare(&c.src, &ParseOptions::default()) {
        Ok(p) => p,
        Err(e) if e.code == ErrorCode::UnsupportedDiagram && c.error.is_none() => {
            return match &c.diagram {
                Some(t) => Err(format!("no type, Pele {t:?}")),
                None => Ok(()),
            };
        }
        Err(e) => return compare(Err(e), &c.error, &None),
    };
    let ty = p.found.diagram.name();
    if c.diagram.as_deref() != Some(ty) {
        return Err(format!("type {ty}, Pele {:?}", c.diagram));
    }
    if p.found.diagram != DiagramType::Kanban {
        return Ok(());
    }
    let got = p.parse_with(false, |s| parse(s, p.title.as_deref()));
    let (error, model) = if c.same {
        (&d.error, &d.model)
    } else {
        (&c.error, &c.model)
    };
    compare(got, error, model)
}

#[test]
fn parse_matches_pele() {
    let f: Fixture = load("kanban");
    let mut failures = Failures::new();
    for c in &f.cases {
        let got = run(c);
        failures.check(got.is_ok(), || format!("{:?}: {}", c.src, got.unwrap_err()));
    }
    failures.done("kanban", 5500);
}

#[derive(Deserialize)]
struct Renders {
    cases: Vec<Render>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Render {
    src: String,
    max_width: Option<f64>,
    font_size: Option<f64>,
    #[serde(default)]
    icons: bool,
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

/// The icons the host has in the cases drawn with icons, as the oracle's ICONS.
const HOST_ICONS: [&str; 5] = ["star", "inbox", "phone", "fa:fa-car", "fa fa-x"];

/// A paint as Pele's SVG names it, "-" for none given. Pele gives text in the theme's color
/// of text no fill of its own.
fn paint_name(p: Option<Paint>) -> &'static str {
    match p {
        None => "-",
        Some(p) if p == Paint::FG => "-",
        Some(p) if p == Paint::BG => "var(--_bg)",
        Some(p) if p == Paint::MUTED => "var(--_m)",
        Some(p) if p == Paint::SURFACE => "var(--_s)",
        Some(p) if p == Paint::BORDER => "var(--_b)",
        Some(p) if p == Paint::ACCENT => "var(--_c)",
        Some(p) if p == Paint::CRITICAL => "var(--pele-critical,#e45756)",
        Some(_) => "?",
    }
}

/// The class Pele gives the text of an item of a role.
fn text_class(role: &str) -> &'static str {
    match role {
        ROLE_COLUMN => "pele-cluster-label",
        ROLE_CARD => "pele-label",
        ROLE_TICKET => "pele-ticket",
        ROLE_ASSIGNED => "pele-assigned",
        _ => "",
    }
}

/// The lines of a scene as the oracle reads Pele's SVG.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let num = pele_num;
    let pt = |x: f64, y: f64| format!("{},{}", num(x), num(y));
    let rect = |r: Rect| format!("{},{}", pt(r.x, r.y), pt(r.w, r.h));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    // The lines of a label as Pele's textBlock writes them: each starting or ending at the
    // point the label is placed at, the first one's box starting where the label's does.
    let block = |lines: &mut Vec<String>, it: &Item| {
        let Some(p) = &it.label else {
            return;
        };
        let l = &p.label;
        let anchor = if p.align == Align::End {
            "end"
        } else {
            "start"
        };
        let top = p.at.y - l.height / 2.0;
        for (i, line) in l.lines.iter().enumerate() {
            let spans: Vec<String> = line
                .iter()
                .map(|s| match &s.icon {
                    Some(icon) => format!("i:{icon}:{}", n(s.width)),
                    None => format!("t:{}:{}:{}", s.face.bits(), s.text, n(s.width)),
                })
                .collect();
            let cy = top + (i as f64 + 0.5) * l.line_height;
            lines.push(format!(
                "line {} {} {} {anchor} {} {}",
                text_class(it.role.as_str()),
                paint_name(it.style.text),
                pt(p.at.x, cy),
                n(l.widths[i]),
                spans.join("|")
            ));
        }
    };
    let boxed = |lines: &mut Vec<String>, it: &Item| {
        let part = &it.parts[0];
        let Outline::Box { rect: r, rounded } = part.outline else {
            lines.push("rect of a path".into());
            return;
        };
        let stroke = if part.paint == Painting::FillOnly {
            "-"
        } else {
            paint_name(it.style.stroke)
        };
        lines.push(format!(
            "rect {}{} {} {stroke}",
            rect(r),
            if rounded { " r" } else { "" },
            paint_name(it.style.fill)
        ));
    };
    let icon = |lines: &mut Vec<String>, it: &Item| {
        if let Some(pic) = &it.picture {
            let PictureSource::Icon(name) = &pic.source else {
                lines.push("picture of an image".into());
                return;
            };
            lines.push(format!("icon {name} {}", rect(pic.bounds)));
        }
    };
    // Where Pele writes a label of one line: its point, moved down to the line's baseline.
    let baseline = |it: &Item| {
        let p = it.label.as_ref().expect("a label");
        let l = &p.label;
        pt(
            p.at.x,
            p.at.y - l.height / 2.0 + l.line_height / 2.0 + l.size * 0.35,
        )
    };
    for (i, it) in sc.items.iter().enumerate() {
        let next = sc
            .items
            .get(i + 1)
            .filter(|n| n.role.as_str() == ROLE_PRIORITY);
        match it.role.as_str() {
            ROLE_TITLE => lines.push(format!("title {}", baseline(it))),
            ROLE_COLUMN => {
                lines.push(format!(
                    "column {} [{}]",
                    id(it.id.as_deref().unwrap_or("")),
                    it.class.join(" ")
                ));
                boxed(&mut lines, it);
                block(&mut lines, it);
            }
            ROLE_COUNT => {
                let l = &it.label.as_ref().expect("a count").label;
                lines.push(format!(
                    "count {} {} {}",
                    baseline(it),
                    paint_name(it.style.text),
                    num(l.size)
                ));
                // Pele draws a column's icon after its count.
                icon(&mut lines, &sc.items[i - 1]);
            }
            ROLE_CARD => {
                // Pele marks a card of a priority with a class, which the stripe after it
                // says.
                let mut classes = it.class.clone();
                if let Some(stripe) = next {
                    let shape = stripe.shape.as_ref().map_or("", |s| s.as_str());
                    classes.insert(0, format!("pele-priority-{shape}"));
                }
                lines.push(format!(
                    "card {} [{}]",
                    id(it.id.as_deref().unwrap_or("")),
                    classes.join(" ")
                ));
                boxed(&mut lines, it);
                // Pele draws the stripe of a priority before the card's text.
                if let Some(stripe) = next {
                    lines.push(stripe_line(stripe));
                }
                block(&mut lines, it);
                icon(&mut lines, it);
            }
            ROLE_PRIORITY => {}
            ROLE_TICKET => {
                if let Some(link) = &it.link {
                    lines.push(format!("link {link}"));
                }
                block(&mut lines, it);
            }
            ROLE_ASSIGNED => block(&mut lines, it),
            role => lines.push(format!("unknown role {role}")),
        }
    }
    lines
}

/// The stripe of a card's priority as the oracle reads Pele's.
fn stripe_line(it: &Item) -> String {
    let num = pele_num;
    let Outline::Path(path) = &it.parts[0].outline else {
        return "stripe of a box".into();
    };
    let mut points = Vec::new();
    let mut at = 0;
    for &op in path.ops() {
        let letter = match op {
            Op::MoveTo => "M",
            Op::LineTo => "L",
            Op::QuadTo => "Q",
            Op::CubeTo => "C",
            Op::Close => "Z",
        };
        let a = &path.args()[at..];
        points.push(format!("{letter} {},{}", num(a[0]), num(a[1])));
        at += op.args();
    }
    let s = it.author.over(&it.style);
    format!(
        "priority {} {} {} {} {} {}",
        points.join(" "),
        paint_name(it.style.stroke),
        it.style.opacity.map_or("-".to_string(), n),
        n(it.style.stroke_width.unwrap_or(1.0)),
        s.cap.map_or("-".to_string(), |c| c.to_string()),
        s.join.map_or("-".to_string(), |j| j.to_string())
    )
}

/// What Pele's render draws of kanban boards, read back out of its SVG as one line per thing
/// drawn, in the order drawn, against the same lines written of the scene. Pele draws a
/// column's insides about the column and a card's about the card. The oracle moves them to
/// where they are on the page, as the scene holds them, before it writes numbers as Pele's
/// SVG writes them. Each line of a column's or a card's text is written as where it starts
/// or ends, and what is in it.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("kanban-render");
    assert!(f.cases.len() >= 3500, "only {} cases", f.cases.len());
    let icons = |name: &str| HOST_ICONS.contains(&name);
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a kanban board");
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("a kanban board that parses");
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            font_size: c.font_size.unwrap_or(16.0),
            icons: if c.icons { Some(&icons) } else { None },
            ..Options::default()
        };
        let got = scene_lines(&layout(&m, &p.config, &o));
        let diff = drawings.diff(k, &c.drawing, &got, None);
        failures.check(diff.is_none(), || {
            format!(
                "{:?}, max {:?}, size {:?}, icons {}: {}",
                c.src,
                c.max_width,
                c.font_size,
                c.icons,
                diff.unwrap()
            )
        });
    }
    failures.done("renders", 3500);
}
