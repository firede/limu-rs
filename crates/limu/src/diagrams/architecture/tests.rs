//! Architecture diagrams read and laid out as Pele reads and draws them, checked against the
//! oracle's architecture and architecture-render fixtures.

use limu_fixtures::load;
use serde::Deserialize;

use super::db::{ArchEdge, ArchitectureDb};
use super::icons::{GLYPH_STROKE, GLYPHS};
use super::parse;
use super::parser::{
    ARCHITECTURE_TOKENS, ArchitectureAst, EdgeAst, T_ACC_DESCR, T_ACC_TITLE, T_ALIGN,
    T_ARROW_GROUP, T_COLON, T_COLUMN, T_DASH, T_DASHES, T_DIRECTION, T_GROUP, T_HEADER, T_ICON,
    T_ID, T_IN, T_INTO, T_JUNCTION, T_LABEL, T_NEWLINE, T_ROW, T_SERVICE, T_STRING, T_TITLE,
    T_YAML, parse_architecture, table,
};
use super::render::{
    ROLE_EDGE, ROLE_EDGE_LABEL, ROLE_GROUP, ROLE_ICON, ROLE_ICON_TEXT, ROLE_JUNCTION, ROLE_SERVICE,
    ROLE_TITLE, layout,
};
use crate::common::tokens::tokenize;
use crate::detect::DiagramType;
use crate::errors::ErrorCode;
use crate::js::number::to_string as n;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::Paint;
use crate::scene::{Item, Op, Outline, Path, PictureSource, Scene};
use crate::testing::pele_num;
use crate::testing::pelesvg::{cap_join, id, own_fill};
use crate::testing::{Enc, Failures, PeleError, same, sorted};
use crate::value::Value;

#[derive(Deserialize)]
struct Cases {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    src: String,
    direct: Direct,
    #[serde(rename = "type")]
    diagram: Option<String>,
    model: Option<Enc>,
    error: Option<PeleError>,
}

#[derive(Deserialize)]
struct Direct {
    tokens: Option<Vec<(String, String)>>,
    ast: Option<Enc>,
    error: Option<PeleError>,
}

/// The kinds of token, numbered by hand, are the indexes of their types in
/// ARCHITECTURE_TOKENS.
#[test]
fn token_kinds() {
    let names: Vec<&str> = ARCHITECTURE_TOKENS.iter().map(|t| t.name).collect();
    assert_eq!(
        names,
        [
            "architecture-beta",
            "junction",
            "service",
            "column",
            "group",
            "align",
            "row",
            "--",
            "in",
            ":",
            "-",
            "ARROW_DIRECTION",
            "ARROW_GROUP",
            "ARROW_INTO",
            "ACC_DESCR",
            "ACC_TITLE",
            "TITLE",
            "STRING",
            "ID",
            "NEWLINE",
            "WHITESPACE",
            "YAML",
            "DIRECTIVE",
            "SINGLE_LINE_COMMENT",
            "ARCH_ICON",
            "ARCH_TITLE"
        ]
    );
    let kinds = [
        T_HEADER,
        T_JUNCTION,
        T_SERVICE,
        T_COLUMN,
        T_GROUP,
        T_ALIGN,
        T_ROW,
        T_DASHES,
        T_IN,
        T_COLON,
        T_DASH,
        T_DIRECTION,
        T_ARROW_GROUP,
        T_INTO,
        T_ACC_DESCR,
        T_ACC_TITLE,
        T_TITLE,
        T_STRING,
        T_ID,
        T_NEWLINE,
    ];
    for (i, k) in kinds.into_iter().enumerate() {
        assert_eq!(k, i, "kind {}", names[i]);
    }
    assert_eq!((T_YAML, T_ICON, T_LABEL), (21, 24, 25));
}

/// A string as the fixture writes it: null where Pele leaves it undefined or empty.
fn opt(s: &Option<String>) -> Value {
    match s.as_deref() {
        Some(s) if !s.is_empty() => Value::String(s.to_string()),
        _ => Value::Null,
    }
}

fn object(entries: Vec<(&str, Value)>) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn s(text: &str) -> Value {
    Value::String(text.to_string())
}

fn strings(items: &[String]) -> Value {
    Value::Array(items.iter().map(|t| s(t)).collect())
}

#[allow(clippy::too_many_arguments)]
fn dump_edge(
    lhs: &str,
    lg: bool,
    ld: char,
    li: bool,
    title: &Option<String>,
    ri: bool,
    rd: char,
    rhs: &str,
    rg: bool,
) -> Value {
    Value::Array(vec![
        s(lhs),
        Value::Bool(lg),
        s(&ld.to_string()),
        Value::Bool(li),
        opt(title),
        Value::Bool(ri),
        s(&rd.to_string()),
        s(rhs),
        Value::Bool(rg),
    ])
}

fn dump_ast(t: &ArchitectureAst) -> Value {
    let groups = t
        .groups
        .iter()
        .map(|g| Value::Array(vec![s(&g.id), opt(&g.icon), opt(&g.title), opt(&g.parent)]));
    let services = t.services.iter().map(|x| {
        Value::Array(vec![
            s(&x.id),
            opt(&x.icon),
            opt(&x.icon_text),
            opt(&x.title),
            opt(&x.parent),
        ])
    });
    let junctions = t
        .junctions
        .iter()
        .map(|j| Value::Array(vec![s(&j.id), opt(&j.parent)]));
    let edges = t.edges.iter().map(|e: &EdgeAst| {
        dump_edge(
            &e.lhs_id,
            e.lhs_group,
            e.lhs_dir,
            e.lhs_into,
            &e.title,
            e.rhs_into,
            e.rhs_dir,
            &e.rhs_id,
            e.rhs_group,
        )
    });
    let alignments = t
        .alignments
        .iter()
        .map(|a| Value::Array(vec![s(&a.direction), strings(&a.members)]));
    object(vec![
        ("title", opt(&t.title)),
        ("accTitle", opt(&t.acc_title)),
        ("accDescr", opt(&t.acc_descr)),
        ("groups", Value::Array(groups.collect())),
        ("services", Value::Array(services.collect())),
        ("junctions", Value::Array(junctions.collect())),
        ("edges", Value::Array(edges.collect())),
        ("alignments", Value::Array(alignments.collect())),
    ])
}

fn dump_model(m: &ArchitectureDb) -> Value {
    let nodes = m.nodes.values().map(|x| {
        if x.junction {
            Value::Array(vec![s(&x.id), s("junction"), opt(&x.parent)])
        } else {
            Value::Array(vec![
                s(&x.id),
                s("service"),
                opt(&x.icon),
                opt(&x.icon_text),
                opt(&x.title),
                opt(&x.parent),
            ])
        }
    });
    let groups = m
        .groups
        .values()
        .map(|g| Value::Array(vec![s(&g.id), opt(&g.icon), opt(&g.title), opt(&g.parent)]));
    let edges = m.edges.iter().map(|e: &ArchEdge| {
        dump_edge(
            &e.lhs_id,
            e.lhs_group,
            e.lhs_dir,
            e.lhs_into,
            &e.title,
            e.rhs_into,
            e.rhs_dir,
            &e.rhs_id,
            e.rhs_group,
        )
    });
    let hints = m
        .layout_hints
        .iter()
        .map(|h| Value::Array(vec![s(&h.direction), strings(&h.members)]));
    object(vec![
        ("title", opt(&m.title)),
        ("accTitle", opt(&m.acc_title)),
        ("accDescr", opt(&m.acc_descr)),
        ("nodes", Value::Array(nodes.collect())),
        ("groups", Value::Array(groups.collect())),
        ("edges", Value::Array(edges.collect())),
        ("hints", Value::Array(hints.collect())),
    ])
}

fn dump_tokens(src: &str) -> Option<Vec<(String, String)>> {
    let t = tokenize(src, table(), DiagramType::Architecture).ok()?;
    let n = t.kinds.len() - 1;
    Some(
        (0..n)
            .map(|i| {
                let name = ARCHITECTURE_TOKENS[t.kinds[i]].name.to_string();
                (name, src[t.starts[i]..t.ends[i]].to_string())
            })
            .collect(),
    )
}

#[test]
fn parse_matches_pele() {
    let f: Cases = load("architecture");
    assert!(f.cases.len() >= 6000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    let mut models = 0;
    for c in &f.cases {
        let tokens = dump_tokens(&c.src);
        failures.check(tokens == c.direct.tokens, || {
            format!(
                "tokens of {:?}: {tokens:?}, Pele {:?}",
                c.src, c.direct.tokens
            )
        });
        let tree = parse_architecture(&c.src);
        let ok = match (&tree, &c.direct.error, &c.direct.ast) {
            (Err(e), Some(want), _) => want.is(e),
            (Ok(t), None, Some(want)) => same(&sorted(&dump_ast(t)), &sorted(&want.0)),
            _ => false,
        };
        failures.check(ok, || {
            format!(
                "parse_architecture({:?}): {tree:?}, Pele {:?} {:?}",
                c.src, c.direct.ast, c.direct.error
            )
        });

        let p = prepare(&c.src, &ParseOptions::default());
        let ok = match (&p, &c.error) {
            (Err(e), Some(want)) => want.is(e),
            (Err(e), None) => e.code == ErrorCode::UnsupportedDiagram && c.diagram.is_none(),
            (Ok(p), _) if c.diagram.as_deref() != Some(p.found.diagram.name()) => false,
            (Ok(p), want) if p.found.diagram == DiagramType::Architecture => {
                let got = p.parse_with(false, |s| parse(s, p.title.as_deref()));
                match (&got, want, &c.model) {
                    (Err(e), Some(want), _) => want.is(e),
                    (Ok(m), None, Some(model)) => {
                        models += 1;
                        same(&sorted(&dump_model(m)), &sorted(&model.0))
                    }
                    _ => false,
                }
            }
            (Ok(_), _) => true,
        };
        failures.check(ok, || {
            format!("{:?}: model {:?}, error {:?}", c.src, c.model, c.error)
        });
    }
    failures.done("architecture", 3 * 6000);
    assert!(models >= 1000, "only {models} models");
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
fn host_icons(name: &str) -> bool {
    matches!(
        name,
        "logos:aws-lambda" | "mdi:home" | "star" | "database" | "fa:fa-car"
    )
}

/// A paint as Pele's SVG names it, "-" for none given.
fn paint_name(p: Option<Paint>) -> String {
    let Some(p) = p.filter(|&p| p != Paint::NONE) else {
        return "-".into();
    };
    let named = [
        (Paint::FG, "var(--_fg)"),
        (Paint::BG, "var(--_bg)"),
        (Paint::MUTED, "var(--_m)"),
        (Paint::LINE, "var(--_l)"),
        (Paint::SURFACE, "var(--_s)"),
        (Paint::SURFACE_ALT.alpha(0.5), "var(--_a)/0.5"),
        (Paint::BORDER, "var(--_b)"),
    ];
    named
        .iter()
        .find(|(q, _)| *q == p)
        .map_or("?", |(_, name)| name)
        .to_string()
}

/// What the oracle calls the labels of each role, by the class Pele gives them.
fn text_class(role: &str) -> &'static str {
    match role {
        ROLE_TITLE => "title",
        ROLE_GROUP => "group",
        ROLE_SERVICE => "service",
        ROLE_ICON_TEXT => "icon-text",
        ROLE_EDGE_LABEL => "edge",
        _ => "",
    }
}

/// The lines of a scene as the oracle reads Pele's SVG, every number as JavaScript's String
/// writes it.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let pt = |x: f64, y: f64| format!("{},{}", n(x), n(y));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    if let Some(t) = &sc.title {
        lines.push(format!("acc-title {}", id(t)));
    }
    if let Some(d) = &sc.desc {
        lines.push(format!("acc-desc {}", id(d)));
    }
    let label = |it: &Item| {
        let p = it.label.as_ref()?;
        let l = &p.label;
        let fill = match it.style.text {
            Some(t) if t != Paint::FG => paint_name(Some(t)),
            _ => "-".into(),
        };
        let widths: Vec<String> = l.widths.iter().map(|&w| n(w)).collect();
        let spans: Vec<String> = l
            .lines
            .iter()
            .map(|line| {
                line.iter()
                    .map(|span| match &span.icon {
                        Some(icon) => format!("i:{icon}:{}", n(span.width)),
                        None => format!("t:{}:{}:{}", span.face.bits(), span.text, n(span.width)),
                    })
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .collect();
        Some(format!(
            "label {} {fill} {} {},{},{},{} | {} | {}",
            text_class(it.role.as_str()),
            pt(p.at.x, p.at.y),
            n(l.size),
            n(l.line_height),
            n(l.width),
            n(l.height),
            widths.join(","),
            spans.join(" / ")
        ))
    };
    let rect = |it: &Item| {
        let part = &it.parts[0];
        let (r, radius) = match &part.outline {
            Outline::Box { rect, rounded } => (*rect, if *rounded { "r" } else { "-" }),
            // A label's patch, a path with the item's box.
            Outline::Path(_) => (it.bounds, "r3"),
        };
        format!(
            "rect {},{} {radius} {} {} {}",
            pt(r.x, r.y),
            pt(r.w, r.h),
            paint_name(own_fill(it)),
            paint_name(it.style.stroke),
            cap_join(it)
        )
    };
    // The icon of an item: a picture the host draws, or Pele's glyph, the item after it.
    let icon = |owner: &Item, next: Option<&Item>, lines: &mut Vec<String>| {
        if let Some(pic) = &owner.picture {
            let PictureSource::Icon(name) = &pic.source else {
                panic!("a picture that is not an icon");
            };
            let b = pic.bounds;
            lines.push(format!(
                "icon {} {},{}",
                id(name),
                pt(b.x, b.y),
                pt(b.w, b.h)
            ));
            lines.push("host".into());
        }
        let Some(next) = next.filter(|it| it.role.as_str() == ROLE_ICON) else {
            return false;
        };
        let b = next.bounds;
        let name = next
            .shape
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_default();
        lines.push(format!("icon {name} {},{}", pt(b.x, b.y), pt(b.w, b.h)));
        let width = match next.style.stroke_width {
            Some(w) if w == GLYPH_STROKE * b.w / 24.0 => n(GLYPH_STROKE),
            _ => "?".into(),
        };
        let d = GLYPHS
            .iter()
            .find(|(g, _)| *g == name)
            .map_or("", |(_, d)| d);
        let want = Path::parse(d)
            .expect("a glyph")
            .scaled(b.w / 24.0, b.x, b.y);
        let Outline::Path(got) = &next.parts[0].outline else {
            panic!("glyph {name} is no path");
        };
        assert_eq!(got.ops(), want.ops(), "glyph {name}: steps differ");
        assert!(
            got.args().len() == want.args().len()
                && got
                    .args()
                    .iter()
                    .zip(want.args())
                    .all(|(a, b)| (a - b).abs() <= 1e-9),
            "glyph {name}: {:?}, not {:?}",
            got.args(),
            want.args()
        );
        let fill = if next.style.fill == Some(Paint::NONE) {
            "none"
        } else {
            "?"
        };
        lines.push(format!(
            "glyph {d} {} {width} {} {fill}",
            paint_name(next.style.stroke),
            cap_join(next)
        ));
        true
    };
    let mut i = 0;
    while i < sc.items.len() {
        let it = &sc.items[i];
        let next = sc.items.get(i + 1);
        let name = id(it.id.as_deref().unwrap_or(""));
        match it.role.as_str() {
            ROLE_TITLE => lines.extend(label(it)),
            ROLE_GROUP => {
                lines.push(format!("group {name}"));
                lines.push(rect(it));
                if icon(it, next, &mut lines) {
                    i += 1;
                }
                lines.extend(label(it));
            }
            ROLE_EDGE => {
                lines.push(format!("edge {name}"));
                let Outline::Path(path) = &it.parts[0].outline else {
                    panic!("an edge of a box");
                };
                let mut steps = Vec::new();
                let mut at = 0;
                for &op in path.ops() {
                    let mut step = match op {
                        Op::MoveTo => "M",
                        Op::LineTo => "L",
                        Op::QuadTo => "Q",
                        Op::CubeTo => "C",
                        Op::Close => "Z",
                    }
                    .to_string();
                    for xy in path.args()[at..at + op.args()].chunks(2) {
                        step.push_str(&format!(" {}", pt(xy[0], xy[1])));
                    }
                    at += op.args();
                    steps.push(step);
                }
                lines.push(format!("path {} {}", steps.join(" "), cap_join(it)));
                for m in it.kind.edge().expect("an edge").ends.iter().flatten() {
                    lines.push(format!(
                        "marker {} {} {} {}",
                        m.kind.name(),
                        pt(m.at.x, m.at.y),
                        pt(m.dx, m.dy),
                        paint_name(it.style.stroke)
                    ));
                }
            }
            ROLE_EDGE_LABEL => {
                lines.push(format!("edge-label {name}"));
                lines.push(rect(it));
                lines.extend(label(it));
            }
            ROLE_SERVICE => {
                lines.push(format!("service {name}"));
                lines.push(rect(it));
                if icon(it, next, &mut lines) {
                    i += 1;
                } else if let Some(next) = next.filter(|n| n.role.as_str() == ROLE_ICON_TEXT) {
                    lines.extend(label(next));
                    i += 1;
                }
                lines.extend(label(it));
            }
            ROLE_JUNCTION => {
                lines.push(format!("junction {name}"));
                // The circle, as Path::ellipse draws it: from its right end, four quarters.
                let Outline::Path(path) = &it.parts[0].outline else {
                    panic!("a junction of a box");
                };
                let a = path.args();
                let (cx, cy) = (a[6], a[1]);
                lines.push(format!(
                    "circle {} {} {}",
                    pt(cx, cy),
                    pele_num(a[0] - cx),
                    paint_name(own_fill(it))
                ));
            }
            role => lines.push(format!("unknown role {role}")),
        }
        i += 1;
    }
    lines
}

/// What Pele's render draws of architecture diagrams, read back out of its SVG as a line for
/// each thing drawn, in the order drawn, with every number in full. Pele draws the body of a
/// diagram about a point of its own and the inside of a node about the node. The oracle moves
/// them to where they are on the page, as the scene holds them.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("architecture-render");
    assert!(f.cases.len() >= 4000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("an architecture diagram");
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("an architecture diagram that parses");
        let o = Options {
            max_width: c.max_width.filter(|&w| w > 0.0).unwrap_or(f64::INFINITY),
            font_size: c.font_size.filter(|&s| s > 0.0).unwrap_or(16.0),
            icons: if c.icons { Some(&host_icons) } else { None },
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
    failures.done("renders", 4000);
}
