//! Mindmaps parsed against what Pele makes of the same cases, in the oracle's mindmap
//! fixture, and laid out against what it draws of them, in its mindmap-render fixture.

use limu_fixtures::load;
use serde::Deserialize;

use super::render::{ROLE_EDGE, ROLE_NODE, ROLE_ROOT, ROLE_TITLE, ROOT_SHAPE, TINT, outline};
use super::{MindmapDb, layout, parse, parse_mindmap, tokenize};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::len16;
use crate::js::number::to_string as n;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::{Paint, Theme};
use crate::scene::{Item, Op, Outline, Painting, Part, Scene};
use crate::testing::pelesvg::{cap_join, id, own_fill};
use crate::testing::{Enc, Failures, PeleError, same, sorted};
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

/// The map as the oracle writes Pele's, without the fields Pele leaves undefined: a node's
/// children by their ids, the root's section, and a class or an icon not given.
fn dump_model(m: &MindmapDb) -> Value {
    let nodes = m.nodes.iter().map(|node| {
        let mut o = Map::new();
        o.insert("id", Value::Number(node.id as f64));
        o.insert("nodeId", Value::String(node.node_id.clone()));
        o.insert("level", Value::Number(node.level as f64));
        o.insert("descr", Value::String(node.descr.clone()));
        o.insert("type", Value::Number(node.kind as f64));
        let children = node.children.iter().map(|&c| Value::Number(c as f64));
        o.insert("children", Value::Array(children.collect()));
        o.insert("isRoot", Value::Bool(node.is_root));
        if let Some(s) = node.section {
            o.insert("section", Value::Number(s as f64));
        }
        if let Some(c) = &node.class {
            o.insert("class", Value::String(c.clone()));
        }
        if let Some(i) = &node.icon {
            o.insert("icon", Value::String(i.clone()));
        }
        Value::Object(o)
    });
    let mut out = Map::new();
    out.insert("nodes", Value::Array(nodes.collect()));
    if let Some(t) = m.title.as_deref().filter(|t| !t.is_empty()) {
        out.insert("title", Value::String(t.to_string()));
    }
    sorted(&Value::Object(out))
}

/// Whether what was parsed is what Pele made: the same error, or the same model. It says how
/// they differ where they do.
fn compare(
    got: std::result::Result<MindmapDb, Error>,
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
/// parseSource for a mindmap.
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
    let mut db = MindmapDb::default();
    let direct = parse_mindmap(&c.src, &mut db).map(|()| db);
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
    if p.found.diagram != DiagramType::Mindmap {
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
    let f: Fixture = load("mindmap");
    let mut failures = Failures::new();
    for c in &f.cases {
        let got = run(c);
        failures.check(got.is_ok(), || format!("{:?}: {}", c.src, got.unwrap_err()));
    }
    failures.done("mindmap", 5500);
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
    breakpoint: Option<f64>,
    #[serde(default)]
    keep: bool,
    #[serde(default)]
    icons: bool,
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

/// The icons the host has in the cases drawn with icons, as the oracle's ICONS.
const HOST_ICONS: [&str; 6] = [
    "fa fa-book",
    "star",
    "phone",
    "fa:fa-car",
    "mdi:home",
    "fa fa-x",
];

/// A paint as Pele's SVG names it, "-" for none given, with its opacity after a slash.
fn paint_name(p: Option<Paint>) -> String {
    let Some(p) = p.filter(|&p| p != Paint::NONE) else {
        return "-".into();
    };
    if p == Paint::SURFACE {
        return "var(--_s)".into();
    }
    if p == Paint::BORDER {
        return "var(--_b)".into();
    }
    for k in 0..8 {
        let color = Theme::PELE.series[k];
        if p == Paint::series(k) {
            return format!("var(--pele-series-{},{color})", k + 1);
        }
        if p == Paint::series(k).alpha(TINT) {
            return format!("var(--pele-series-{},{color})/{}", k + 1, n(TINT));
        }
    }
    "?".into()
}

/// The parts of a node are its outline drawn about the center of its box, as Pele draws a
/// node's shape about the node. The center taken from the box may be off the one the node was
/// drawn about in the last bit, and the outline was moved onto the page after it was drawn,
/// so the numbers are allowed that much. Pele's outlines of the
/// shapes are compared in the module draw.
fn same_parts(got: &[Part], want: &[Part]) -> bool {
    let near = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(1.0);
    got.len() == want.len()
        && got.iter().zip(want).all(|(g, w)| {
            g.paint == w.paint
                && match (&g.outline, &w.outline) {
                    (Outline::Path(g), Outline::Path(w)) => {
                        g.ops() == w.ops()
                            && g.args().len() == w.args().len()
                            && g.args().iter().zip(w.args()).all(|(&a, &b)| near(a, b))
                    }
                    (
                        Outline::Box {
                            rect: g,
                            rounded: gr,
                        },
                        Outline::Box {
                            rect: w,
                            rounded: wr,
                        },
                    ) => {
                        gr == wr
                            && near(g.x, w.x)
                            && near(g.y, w.y)
                            && near(g.w, w.w)
                            && near(g.h, w.h)
                    }
                    _ => false,
                }
        })
}

/// The lines of a scene as the oracle reads Pele's SVG, every number as JavaScript's String
/// writes it.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let pt = |x: f64, y: f64| format!("{},{}", n(x), n(y));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    let label = |lines: &mut Vec<String>, it: &Item, class: &str| {
        let Some(p) = &it.label else {
            return;
        };
        let l = &p.label;
        let fill = match it.style.text {
            Some(t) if t == Paint::FG => "-".to_string(),
            t => paint_name(t),
        };
        let align = match p.align {
            crate::scene::Align::Start => "start",
            _ => "middle",
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
                        None => format!("t:{}:{}:{}", span.face.bits(), span.text, n(span.width)),
                    })
                    .collect();
                s.join("|")
            })
            .collect();
        lines.push(format!(
            "label {class} {fill} {align} {} {},{},{},{} | {} | {}",
            pt(p.at.x, p.at.y),
            n(l.size),
            n(l.line_height),
            n(l.width),
            n(l.height),
            widths.join(","),
            spans.join(" / ")
        ));
    };
    let width = |it: &Item| it.style.stroke_width.map_or("-".to_string(), n);
    let mut section = None;
    for (i, it) in sc.items.iter().enumerate() {
        let role = it.role.as_str();
        // Pele draws each first-level branch in a group of its own, its links first.
        if role == ROLE_EDGE || role == ROLE_NODE {
            let s = it
                .tags
                .first()
                .and_then(|t| t.strip_prefix("section-"))
                .and_then(|s| s.parse::<usize>().ok());
            if s.is_none() {
                lines.push(format!("item {i} has no section: {:?}", it.tags));
            }
            let after_node = i > 0 && sc.items[i - 1].role.as_str() == ROLE_NODE;
            if s != section || role == ROLE_EDGE && after_node {
                lines.push(format!("branch {}", s.unwrap_or(0)));
                section = s;
            }
        }
        match role {
            ROLE_TITLE => label(&mut lines, it, "pele-title"),
            ROLE_EDGE => {
                let Outline::Path(path) = &it.parts[0].outline else {
                    lines.push("edge of a box".into());
                    continue;
                };
                let mut steps = Vec::new();
                let mut at = 0;
                for &op in path.ops() {
                    let mut s = match op {
                        Op::MoveTo => "M",
                        Op::LineTo => "L",
                        Op::QuadTo => "Q",
                        Op::CubeTo => "C",
                        Op::Close => "Z",
                    }
                    .to_string();
                    for xy in path.args()[at..at + op.args()].chunks(2) {
                        s.push_str(&format!(" {}", pt(xy[0], xy[1])));
                    }
                    at += op.args();
                    steps.push(s);
                }
                let fill = if it.parts[0].paint == Painting::StrokeOnly {
                    "none"
                } else {
                    "?"
                };
                lines.push(format!(
                    "edge {} {fill} {} {} {}",
                    steps.join(" "),
                    paint_name(it.style.stroke),
                    width(it),
                    cap_join(it)
                ));
            }
            ROLE_NODE | ROLE_ROOT => {
                let mut shape = it.shape.as_ref().map_or("", |s| s.as_str());
                if shape == ROOT_SHAPE && role == ROLE_NODE {
                    shape = "branch";
                }
                let root = if role == ROLE_ROOT { " root" } else { "" };
                let b = it.bounds;
                lines.push(format!(
                    "node {} {shape}{root} [{}] {},{}",
                    id(it.id.as_deref().unwrap_or("")),
                    it.class.join(" "),
                    pt(b.x, b.y),
                    pt(b.w, b.h)
                ));
                for part in &it.parts {
                    if part.paint != Painting::FillAndStroke {
                        lines.push(format!("part painted {:?}", part.paint));
                        continue;
                    }
                    let r = match part.outline {
                        Outline::Box { rounded: true, .. } => "r",
                        _ => "-",
                    };
                    lines.push(format!(
                        "part {r} {} {} {} {}",
                        paint_name(own_fill(it)),
                        paint_name(it.style.stroke),
                        width(it),
                        cap_join(it)
                    ));
                }
                let shape = it.shape.as_ref().map_or("", |s| s.as_str());
                let frame = if shape == "text" {
                    Vec::new()
                } else {
                    outline(shape, b.x + b.w / 2.0, b.y + b.h / 2.0, b.w, b.h)
                };
                if !same_parts(&it.parts, &frame) {
                    lines.push("parts differ from the outline".into());
                }
                if let Some(pic) = &it.picture {
                    let crate::scene::PictureSource::Icon(name) = &pic.source else {
                        lines.push("picture of an image".into());
                        continue;
                    };
                    let r = pic.bounds;
                    lines.push(format!(
                        "icon {} {},{}",
                        id(name),
                        pt(r.x, r.y),
                        pt(r.w, r.h)
                    ));
                }
                label(&mut lines, it, "pele-label");
            }
            role => lines.push(format!("unknown role {role}")),
        }
    }
    lines
}

/// What Pele's render draws of mindmaps, read back out of its SVG as one line per thing
/// drawn, in the order drawn, with every number in full, against the same lines written of
/// the scene. Pele draws the map about a point of its own and the inside of a node about the
/// node. The oracle moves them to where they are on the page, as the scene holds them.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("mindmap-render");
    assert!(f.cases.len() >= 4000, "only {} cases", f.cases.len());
    let icons = |name: &str| HOST_ICONS.contains(&name);
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a mindmap");
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("a mindmap that parses");
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            font_size: c.font_size.unwrap_or(16.0),
            direction_breakpoint: c.breakpoint.unwrap_or(0.0),
            keep_direction: c.keep,
            icons: if c.icons { Some(&icons) } else { None },
            ..Options::default()
        };
        let got = scene_lines(&layout(&m, &p.config, &o));
        let diff = drawings.diff(k, &c.drawing, &got, None);
        failures.check(diff.is_none(), || {
            format!(
                "{:?}, max {:?}, size {:?}, breakpoint {:?}, keep {}, icons {}: {}",
                c.src,
                c.max_width,
                c.font_size,
                c.breakpoint,
                c.keep,
                c.icons,
                diff.unwrap()
            )
        });
    }
    failures.done("renders", 4000);
}

/// Inputs built to find the worst cases, as Pele's tests/mindmap/stress.test.ts builds them:
/// long runs the lexer could scan again, maps of many nodes or many levels, and labels of
/// 50,000 characters. Each must finish quickly, with a scene or an error, laid out across and
/// in a narrow host, running down.
#[test]
fn worst_cases() {
    const N: usize = 50_000;
    let repeat =
        |count: usize, line: &dyn Fn(usize) -> String| -> String { (0..count).map(line).collect() };
    let cases: Vec<(&str, String)> = vec![
        (
            "one line of shaped nodes",
            format!("mindmap\n{}", "(a) ".repeat(N / 4)),
        ),
        (
            "one line of spaces between ids",
            format!("mindmap\na(b){}x", " ".repeat(N)),
        ),
        (
            "blank lines of spaces",
            format!("mindmap\nroot\n{} a", " \n".repeat(N / 2)),
        ),
        (
            "a newline then a long run of spaces, repeated",
            format!(
                "mindmap\nroot{}",
                format!("\n{}a", " ".repeat(99)).repeat(N / 100)
            ),
        ),
        (
            "whitespace that never reaches a comment",
            format!("mindmap\nroot\n{}%", " \t\n".repeat(N / 3)),
        ),
        (
            "comment markers",
            format!("mindmap\nroot\n{}", "%%".repeat(N / 2)),
        ),
        (
            "lone closing braces in a node",
            format!("mindmap\nroot[{}]", "}\n".repeat(N / 2)),
        ),
        (
            "unclosed markdown text",
            format!("mindmap\nroot[\"`{}", "a\n".repeat(N / 2)),
        ),
        (
            "unclosed icon",
            format!("mindmap\nroot\n::icon({}", "x\n".repeat(N / 2)),
        ),
        (
            "dashes and brackets",
            format!("mindmap\n{}", "-)(-".repeat(N / 4)),
        ),
        (
            "16,000 nodes on one level",
            format!("mindmap\nroot\n{}", " a\n".repeat(16_000)),
        ),
        (
            "3,000 levels of nesting",
            format!(
                "mindmap\n{}",
                repeat(3000, &|i| format!("{}n\n", " ".repeat(i)))
            ),
        ),
        (
            "indentation that rises and falls",
            format!(
                "mindmap\nroot\n{}",
                repeat(2400, &|i| format!("{}n\n", " ".repeat(1 + i % 40)))
            ),
        ),
        (
            "5,000 branches with two leaves each",
            format!("mindmap\nroot\n{}", " b\n  l\n  l\n".repeat(5000)),
        ),
        (
            "1,400 shaped nodes with icons and classes",
            format!(
                "mindmap\nroot\n{}",
                " a((b))\n ::icon(fa fa-x)\n :::c d\n".repeat(1400)
            ),
        ),
        (
            "one 50,000 character label",
            format!("mindmap\nroot[{}]", "word ".repeat(N / 5)),
        ),
        (
            "one 50,000 character word",
            format!("mindmap\nroot[{}]", "w".repeat(N)),
        ),
        (
            "label of emphasis markers",
            format!("mindmap\nroot[\"`{}`\"]", "*a _b ".repeat(N / 6)),
        ),
        (
            "label of line breaks and entities",
            format!("mindmap\nroot[{}]", "#35;&amp;<br>".repeat(N / 13)),
        ),
        (
            "a class list of 25,000 names",
            format!("mindmap\nroot\n:::{}", "a ".repeat(N / 2)),
        ),
        (
            "a second root after 16,000 nodes",
            format!("mindmap\n root\n{} b\n", "  a\n".repeat(16_000)),
        ),
        (
            "deep branches each followed by a shallow one",
            format!(
                "mindmap\nroot\n{}",
                repeat(160, &|_| repeat(24, &|i| format!(
                    "{}n\n",
                    " ".repeat(1 + i)
                )))
            ),
        ),
        (
            "an icon name of 50,000 characters, with a host",
            format!("mindmap\nroot\n::icon({})", "fa ".repeat(N / 3)),
        ),
    ];
    let all = |_: &str| true;
    for (name, src) in &cases {
        let started = std::time::Instant::now();
        let options = ParseOptions {
            limit: None,
            ..ParseOptions::default()
        };
        if let Ok(p) = prepare(src, &options)
            && let Ok(m) = p.parse_with(false, |s| parse(s, p.title.as_deref()))
        {
            let host = Options {
                icons: Some(&all),
                ..Options::default()
            };
            layout(&m, &p.config, &host);
            let narrow = Options {
                max_width: 300.0,
                ..Options::default()
            };
            layout(&m, &p.config, &narrow);
        }
        let took = started.elapsed();
        assert!(took.as_secs_f64() < 2.0, "{name}: took {took:?}");
    }
}
