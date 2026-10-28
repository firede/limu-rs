//! Ishikawa diagrams read and laid out as Pele reads and draws them, checked against the
//! oracle's ishikawa and ishikawa-render fixtures.

use limu_fixtures::load;
use serde::Deserialize;

use super::db::IshikawaDb;
use super::lexer::{TOKEN_NAMES, tokenize};
use super::parse;
use super::parser::parse_ishikawa;
use super::render::{
    ROLE_BONES, ROLE_CATEGORY, ROLE_CAUSE, ROLE_HEAD, ROLE_LINK, ROLE_SPINE, ROLE_TITLE, layout,
};
use crate::detect::DiagramType;
use crate::errors::ErrorCode;
use crate::js::number::to_string as n;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::Paint;
use crate::scene::{Item, Op, Outline, Painting, Part, Path, Scene};
use crate::testing::pelesvg::{cap_join, id};
use crate::testing::{Enc, Failures, PeleError, same, sorted};
use crate::value::{Map, Value};

#[derive(Deserialize)]
struct Cases {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    src: String,
    tokens: Vec<(String, String)>,
    direct: Outcome,
    #[serde(rename = "type")]
    diagram: Option<String>,
    #[serde(flatten)]
    outcome: Outcome,
}

#[derive(Deserialize)]
struct Outcome {
    model: Option<Enc>,
    error: Option<PeleError>,
}

fn dump_tokens(src: &str) -> Vec<(String, String)> {
    let t = tokenize(src);
    (0..t.types.len() - 2)
        .map(|i| {
            let text = src[t.starts[i]..t.ends[i]].to_string();
            (TOKEN_NAMES[t.types[i]].to_string(), text)
        })
        .collect()
}

/// A node as the oracle writes it: its text and its children.
fn dump_node(db: &IshikawaDb, i: usize) -> Value {
    let node = &db.nodes[i];
    let children = node.children.iter().map(|&c| dump_node(db, c)).collect();
    Value::Array(vec![
        Value::String(node.text.clone()),
        Value::Array(children),
    ])
}

/// The model as the oracle writes Pele's, its title undefined where the front matter gives
/// none.
fn dump_model(db: &IshikawaDb) -> Value {
    let mut out = Map::new();
    let title = match &db.title {
        Some(t) => Value::String(t.clone()),
        None => {
            let mut u = Map::new();
            u.insert("$", Value::String("undefined".into()));
            Value::Object(u)
        }
    };
    out.insert("title", title);
    let root = if db.nodes.is_empty() {
        Value::Null
    } else {
        dump_node(db, 0)
    };
    out.insert("root", root);
    Value::Object(out)
}

/// Whether a parse's outcome is the one Pele had.
fn matches(got: &Result<IshikawaDb, crate::Error>, want: &Outcome) -> bool {
    match (got, &want.error, &want.model) {
        (Err(e), Some(want), _) => want.is(e),
        (Ok(m), None, Some(model)) => same(&sorted(&dump_model(m)), &sorted(&model.0)),
        _ => false,
    }
}

#[test]
fn parse_matches_pele() {
    let f: Cases = load("ishikawa");
    let mut failures = Failures::new();
    for c in &f.cases {
        let tokens = dump_tokens(&c.src);
        failures.check(tokens == c.tokens, || {
            format!("tokens of {:?}: {tokens:?}, Pele {:?}", c.src, c.tokens)
        });
        let mut db = IshikawaDb::default();
        let got = parse_ishikawa(&c.src, &mut db).map(|()| db);
        failures.check(matches(&got, &c.direct), || {
            format!(
                "parse_ishikawa({:?}): {got:?}, Pele {:?} {:?}",
                c.src, c.direct.model, c.direct.error
            )
        });

        let want = &c.outcome;
        let p = prepare(&c.src, &ParseOptions::default());
        let ok = match (&p, &want.error) {
            (Err(e), Some(want)) => want.is(e),
            (Err(e), None) => e.code == ErrorCode::UnsupportedDiagram && c.diagram.is_none(),
            (Ok(p), _) if c.diagram.as_deref() != Some(p.found.diagram.name()) => false,
            (Ok(p), _) if p.found.diagram == DiagramType::Ishikawa => {
                matches(&p.parse_with(false, |s| parse(s, p.title.as_deref())), want)
            }
            (Ok(_), _) => true,
        };
        failures.check(ok, || {
            format!(
                "{:?}: model {:?}, error {:?}",
                c.src, want.model, want.error
            )
        });
    }
    failures.done("ishikawa", 3 * 7000);
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
const HOST_ICONS: [&str; 3] = ["fa:fa-car", "fa:fa-user", "fab:fa-github"];

/// A path's commands, every number in full.
fn path_line(p: &Path) -> String {
    let mut out = Vec::new();
    let mut at = 0;
    for &op in &p.ops {
        let mut s = match op {
            Op::MoveTo => "M",
            Op::LineTo => "L",
            Op::QuadTo => "Q",
            Op::CubeTo => "C",
            Op::Close => "Z",
        }
        .to_string();
        for xy in p.args[at..at + op.args()].chunks(2) {
            s.push_str(&format!(" {},{}", n(xy[0]), n(xy[1])));
        }
        at += op.args();
        out.push(s);
    }
    out.join(" ")
}

/// A number of the fixture, as JavaScript's String wrote it.
fn number(s: &str) -> f64 {
    s.parse().expect("a number")
}

/// A line of the fixture, with the outline of the head, which Pele writes with an arc, drawn
/// again with arc_to from Pele's corners, radii and flags, as the scene draws it.
fn pele_line(line: &str) -> String {
    let Some(rest) = line.strip_prefix("outline ") else {
        return line.to_string();
    };
    let f: Vec<&str> = rest.split(' ').collect();
    let point = |s: &str| {
        let (x, y) = s.split_once(',').expect("a point");
        (number(x), number(y))
    };
    let mut p = Path::new();
    let mut i = 0;
    while i < f.len() && f[i] != "var(--_s)" {
        match f[i] {
            "M" => {
                let (x, y) = point(f[i + 1]);
                p.move_to(x, y);
                i += 1;
            }
            "L" => {
                let (x, y) = point(f[i + 1]);
                p.line_to(x, y);
                i += 1;
            }
            "A" => {
                let a: Vec<&str> = f[i + 1].split(',').collect();
                let (x, y) = point(f[i + 2]);
                p.arc_to(
                    number(a[0]),
                    number(a[1]),
                    number(a[2]),
                    a[3] == "1",
                    a[4] == "1",
                    x,
                    y,
                );
                i += 2;
            }
            "Z" => {
                p.close();
            }
            _ => return line.to_string(),
        }
        i += 1;
    }
    format!("outline {} {}", path_line(&p), f[i..].join(" "))
}

/// A paint as Pele's SVG names it, "-" for none given.
fn paint_name(p: Option<Paint>) -> &'static str {
    match p {
        None => "-",
        Some(p) if p == Paint::FG => "var(--_fg)",
        Some(p) if p == Paint::MUTED => "var(--_m)",
        Some(p) if p == Paint::LINE => "var(--_l)",
        Some(p) if p == Paint::SURFACE => "var(--_s)",
        Some(p) if p == Paint::BORDER => "var(--_b)",
        Some(p) if p == Paint::NONE => "none",
        _ => "?",
    }
}

/// The lines of a scene as the oracle reads Pele's SVG, every number as JavaScript's String
/// writes it.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let pt = |x: f64, y: f64| format!("{},{}", n(x), n(y));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    // Drawn across, a diagram has a spine, and the weight of headings is the head's only.
    let across = sc.items.iter().any(|it| it.role == ROLE_SPINE);
    let label = |lines: &mut Vec<String>, it: &Item| {
        let Some(p) = it.label.as_ref().filter(|p| !p.label.is_empty()) else {
            return;
        };
        let l = &p.label;
        let fill = match it.style.text {
            Some(t) if t == Paint::FG => "-",
            t => paint_name(t),
        };
        let role = it.role.as_str();
        let weight = if role == ROLE_TITLE {
            "var(--_tw)"
        } else if role == ROLE_HEAD || role == ROLE_CATEGORY && !across {
            "var(--_hw)"
        } else {
            "-"
        };
        let widths: Vec<String> = l.widths.iter().map(|&w| n(w)).collect();
        let spans: Vec<String> = l
            .lines
            .iter()
            .map(|line| {
                let s: Vec<String> = line
                    .iter()
                    .map(|span| match &span.icon {
                        Some(icon) => format!("i:{icon}:{}", n(span.width)),
                        None => format!("{}:{}:{}", span.face.bits(), span.text, n(span.width)),
                    })
                    .collect();
                s.join(" ")
            })
            .collect();
        lines.push(format!(
            "label {fill} {weight} {} {} {},{},{},{} | {} | {}",
            n(l.size),
            pt(p.at.x, p.at.y),
            n(l.size),
            n(l.line_height),
            n(l.width),
            n(l.height),
            widths.join(","),
            spans.join(" | ")
        ));
    };
    let paint = |it: &Item, part: &Part| {
        let (mut fill, mut stroke) = (it.style.fill, it.style.stroke);
        match part.paint {
            Painting::StrokeOnly => fill = Some(Paint::NONE),
            Painting::FillOnly => stroke = Some(Paint::NONE),
            _ => {}
        }
        format!(
            "{} {} {}",
            paint_name(fill),
            paint_name(stroke),
            cap_join(it)
        )
    };
    let path_of = |part: &Part| match &part.outline {
        Outline::Path(p) => path_line(p),
        Outline::Box { .. } => "box".to_string(),
    };
    let path = |lines: &mut Vec<String>, kind: &str, it: &Item| {
        let part = &it.parts[0];
        lines.push(format!("{kind} {} {}", path_of(part), paint(it, part)));
        for m in it.edge().into_iter().flat_map(|e| e.ends.iter().flatten()) {
            lines.push(format!(
                "marker {} {} {} {}",
                m.kind.name(),
                pt(m.at.x, m.at.y),
                pt(m.dx, m.dy),
                paint_name(it.style.stroke)
            ));
        }
    };
    let rect = |lines: &mut Vec<String>, it: &Item| {
        let part = &it.parts[0];
        let (r, radius) = match &part.outline {
            Outline::Box { rect, rounded } => (*rect, if *rounded { "r" } else { "?" }.to_string()),
            Outline::Path(p) => {
                // A box with corners of its own, a path in the item's box.
                let r = it.bounds;
                let radius = [4.0, 12.0].into_iter().find(|&rx| {
                    let mut want = Path::new();
                    want.round_rect(r, rx);
                    want == *p
                });
                (
                    r,
                    radius.map_or("?".to_string(), |rx| format!("r{}", n(rx))),
                )
            }
        };
        lines.push(format!(
            "rect {},{} {radius} {}",
            pt(r.x, r.y),
            pt(r.w, r.h),
            paint(it, part)
        ));
    };
    for it in &sc.items {
        let role = it.role.as_str();
        match role {
            ROLE_TITLE => label(&mut lines, it),
            ROLE_SPINE => path(&mut lines, "spine", it),
            ROLE_BONES => {
                lines.push(format!("category {}", id(it.id.as_deref().unwrap_or(""))));
                path(&mut lines, "bones", it);
            }
            ROLE_LINK => path(&mut lines, "link", it),
            ROLE_CATEGORY | ROLE_HEAD | ROLE_CAUSE => {
                if !across {
                    let kind = match role {
                        ROLE_HEAD => "head",
                        ROLE_CATEGORY => "category",
                        _ => "cause",
                    };
                    let name = id(it.id.as_deref().unwrap_or(""));
                    lines.push(format!("node {kind} {name}"));
                    if !it.parts.is_empty() {
                        rect(&mut lines, it);
                    }
                } else if role == ROLE_HEAD {
                    lines.push(format!("head {}", id(it.id.as_deref().unwrap_or(""))));
                    let part = &it.parts[0];
                    lines.push(format!("outline {} {}", path_of(part), paint(it, part)));
                } else if role == ROLE_CATEGORY {
                    rect(&mut lines, it);
                }
                label(&mut lines, it);
            }
            role => lines.push(format!("unknown role {role}")),
        }
    }
    lines
}

/// What Pele's render draws of Ishikawa diagrams, read back out of its SVG as one line per
/// thing drawn, in the order drawn, with every number in full, against the same lines
/// written of the scene. Pele draws the body of a diagram drawn across about a point of its
/// own. The oracle moves it to where it is on the page, as the scene holds it.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("ishikawa-render");
    assert!(f.cases.len() >= 4000, "only {} cases", f.cases.len());
    let icons = |name: &str| HOST_ICONS.contains(&name);
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("an Ishikawa diagram");
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("an Ishikawa diagram that parses");
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            font_size: c.font_size.unwrap_or(16.0),
            icons: if c.icons { Some(&icons) } else { None },
            ..Options::default()
        };
        let got = scene_lines(&layout(&m, &p.config, &o));
        let diff = drawings.diff(k, &c.drawing, &got, Some(&pele_line));
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
    failures.done("renders", 4900);
}
