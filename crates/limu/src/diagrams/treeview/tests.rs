//! Tree views read and laid out as Pele reads and draws them, checked against the oracle's
//! treeview and treeview-render fixtures.

use limu_fixtures::load;
use serde::Deserialize;

use super::boxdrawing::preprocess_box_drawing;
use super::icons::{GLYPH_STROKE, get_node_icon, glyph, icon_config};
use super::model::{NodeType, TreeViewModel};
use super::parse;
use super::parser::{TREEVIEW_TOKENS, TreeViewAst, parse_tree_view, table};
use super::render::{
    ROLE_DESCRIPTION, ROLE_DIRECTORY, ROLE_FILE, ROLE_GUIDE, ROLE_ICON, ROLE_TITLE, layout,
    tree_settings,
};
use crate::common::tokens::tokenize;
use crate::detect::DiagramType;
use crate::errors::ErrorCode;
use crate::js::number::to_string as n;
use crate::layout::{Monospace, Options};
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::Paint;
use crate::scene::{Align, Item, Op, Outline, Path, PictureSource, Rect, Scene};
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
    direct: Direct,
    #[serde(rename = "box")]
    boxed: Boxed,
    #[serde(rename = "type")]
    diagram: Option<String>,
    model: Option<Enc>,
    error: Option<PeleError>,
}

/// What Pele makes of the text given to its parser as it is.
#[derive(Deserialize)]
struct Direct {
    tokens: Option<Vec<(String, String)>>,
    ast: Option<Enc>,
    error: Option<PeleError>,
}

/// What Pele's box-drawing preprocessor makes of the text.
#[derive(Deserialize)]
struct Boxed {
    text: Option<String>,
    #[serde(rename = "lineMap")]
    line_map: Option<Vec<(usize, usize)>>,
    error: Option<PeleError>,
}

/// What the oracle writes of a value Pele leaves undefined.
fn undefined() -> Value {
    let mut u = Map::new();
    u.insert("$", Value::String("undefined".into()));
    Value::Object(u)
}

/// A string, or undefined where there is none.
fn enc(s: Option<&str>) -> Value {
    s.map_or_else(undefined, |s| Value::String(s.to_string()))
}

fn dump_tokens(src: &str) -> Option<Vec<(String, String)>> {
    let t = tokenize(src, table(), DiagramType::TreeView).ok()?;
    let n = t.kinds.len() - 1;
    Some(
        (0..n)
            .map(|i| {
                let text = src[t.starts[i]..t.ends[i]].to_string();
                (TREEVIEW_TOKENS[t.kinds[i]].name.to_string(), text)
            })
            .collect(),
    )
}

/// The tree as the oracle writes Pele's.
fn dump_ast(ast: &TreeViewAst) -> Value {
    let nodes = ast
        .nodes
        .iter()
        .map(|node| {
            let indent = node
                .indent
                .map_or_else(undefined, |i| Value::Number(i as f64));
            Value::Array(vec![
                indent,
                Value::String(node.name.clone()),
                enc(node.class_annotation.as_deref()),
                enc(node.icon_annotation.as_deref()),
                enc(node.desc_annotation.as_deref()),
            ])
        })
        .collect();
    let mut out = Map::new();
    out.insert("title", enc(ast.title.as_deref()));
    out.insert("accTitle", enc(ast.acc_title.as_deref()));
    out.insert("accDescr", enc(ast.acc_descr.as_deref()));
    out.insert("nodes", Value::Array(nodes));
    Value::Object(out)
}

/// A node as the oracle writes it: its id, level, name, type, icon, class and description,
/// the icon it shows, and its children.
fn dump_node(m: &TreeViewModel, i: usize, config: &Map) -> Value {
    let node = &m.nodes[i];
    let icons = icon_config(config);
    let children = node
        .children
        .iter()
        .map(|&c| dump_node(m, c, config))
        .collect();
    let node_type = match node.node_type {
        NodeType::File => "file",
        NodeType::Directory => "directory",
    };
    Value::Array(vec![
        Value::Number(i as f64),
        Value::Number(node.level as f64),
        Value::String(node.name.clone()),
        Value::String(node_type.into()),
        enc(node.icon.as_deref()),
        enc(node.css_class.as_deref()),
        enc(node.description.as_deref()),
        enc(get_node_icon(node, &icons).as_deref()),
        Value::Array(children),
    ])
}

/// The model as the oracle writes Pele's, with the icon and layout settings of the config.
fn dump_model(m: &TreeViewModel, config: &Map) -> Value {
    let icons = icon_config(config);
    let s = tree_settings(config);
    let mut out = Map::new();
    out.insert("count", Value::Number(m.nodes.len() as f64));
    out.insert("root", dump_node(m, 0, config));
    out.insert(
        "icons",
        Value::Array(vec![
            Value::Bool(icons.show_icons),
            Value::String(icons.default_icon_pack.to_string()),
        ]),
    );
    let settings = [s.padding_x, s.padding_y, s.row_indent, s.line_thickness];
    out.insert(
        "settings",
        Value::Array(settings.into_iter().map(Value::Number).collect()),
    );
    let named = [
        ("title", &m.title),
        ("accTitle", &m.acc_title),
        ("accDescr", &m.acc_descr),
    ];
    for (key, value) in named {
        if let Some(v) = value.as_deref().filter(|v| !v.is_empty()) {
            out.insert(key, Value::String(v.to_string()));
        }
    }
    Value::Object(out)
}

#[test]
fn parse_matches_pele() {
    let f: Cases = load("treeview");
    assert!(f.cases.len() >= 5000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    let (mut models, mut refused, mut boxed) = (0, 0, 0);
    for c in &f.cases {
        let tokens = dump_tokens(&c.src);
        failures.check(tokens == c.direct.tokens, || {
            format!(
                "tokens of {:?}: {tokens:?}, Pele {:?}",
                c.src, c.direct.tokens
            )
        });
        let got = parse_tree_view(&c.src);
        let ok = match (&got, &c.direct.error, &c.direct.ast) {
            (Err(e), Some(want), _) => want.is(e),
            (Ok(ast), None, Some(want)) => same(&sorted(&dump_ast(ast)), &sorted(&want.0)),
            _ => false,
        };
        failures.check(ok, || {
            format!(
                "parse_tree_view({:?}): {got:?}, Pele {:?} {:?}",
                c.src, c.direct.ast, c.direct.error
            )
        });

        let pre = preprocess_box_drawing(&c.src);
        let ok = match (&pre, &c.boxed.error) {
            (Err(e), Some(want)) => want.is(e),
            (Ok(pre), None) => {
                let pairs: Vec<(usize, usize)> = (pre.line_map.iter().enumerate())
                    .map(|(k, &line)| (k + 1, line))
                    .collect();
                if !pairs.is_empty() {
                    boxed += 1;
                }
                Some(&pre.text) == c.boxed.text.as_ref()
                    && Some(&pairs) == c.boxed.line_map.as_ref()
            }
            _ => false,
        };
        failures.check(ok, || {
            format!(
                "preprocess_box_drawing({:?}): {pre:?}, Pele {:?} {:?} {:?}",
                c.src, c.boxed.text, c.boxed.line_map, c.boxed.error
            )
        });

        let p = prepare(&c.src, &ParseOptions::default());
        let ok = match (&p, &c.error) {
            (Err(e), Some(want)) => want.is(e),
            (Err(e), None) => e.code == ErrorCode::UnsupportedDiagram && c.diagram.is_none(),
            (Ok(p), _) if c.diagram.as_deref() != Some(p.found.diagram.name()) => false,
            (Ok(p), want) if p.found.diagram == DiagramType::TreeView => {
                match (p.parse_with(false, |s| parse(s, p.title.as_deref())), want) {
                    (Err(e), Some(want)) => {
                        refused += 1;
                        want.is(&e)
                    }
                    (Ok(m), None) => {
                        models += 1;
                        c.model.as_ref().is_some_and(|want| {
                            same(&sorted(&dump_model(&m, &p.config)), &sorted(&want.0))
                        })
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
    assert!(
        models >= 3000 && refused >= 1000 && boxed >= 1000,
        "only {models} models, {refused} errors and {boxed} trees drawn with boxes"
    );
    failures.done("treeview", 3 * 7000);
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
    /// Whether the names are set in the monospace face, as Pele's monospace option lists
    /// files.
    #[serde(default)]
    mono: bool,
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

/// The icons the host has in the cases drawn with icons, as the oracle's ICONS.
const HOST_ICONS: [&str; 7] = [
    "logos:react",
    "mdi:home",
    "material:typescript",
    "mermaid-treeview:folder",
    "fa:fa-car",
    "p:x",
    "material:docker",
];

/// A paint as Pele's SVG names it, "-" for none given.
fn paint_name(p: Option<Paint>) -> &'static str {
    match p {
        None => "?",
        Some(p) if p == Paint::FG => "-",
        Some(p) if p == Paint::MUTED => "var(--_m)",
        Some(p) if p == Paint::BORDER => "var(--_b)",
        Some(p) if p == Paint::ACCENT => "var(--_c)",
        Some(p) if p == Paint::NONE => "none",
        _ => "?",
    }
}

/// Text as Pele's esc writes it, the control characters XML does not allow dropped.
fn clean(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            !(c < ' ' && !matches!(c, '\t' | '\n' | '\r')
                || matches!(c, '\x7f' | '\u{fffe}' | '\u{ffff}'))
        })
        .collect()
}

/// A path's commands, every number in full.
fn path_line(p: &Path) -> String {
    let mut out = Vec::new();
    let mut at = 0;
    for &op in p.ops() {
        let mut s = match op {
            Op::MoveTo => "M",
            Op::LineTo => "L",
            Op::QuadTo => "Q",
            Op::CubeTo => "C",
            Op::Close => "Z",
        }
        .to_string();
        for xy in p.args()[at..at + op.args()].chunks(2) {
            s.push_str(&format!(" {},{}", n(xy[0]), n(xy[1])));
        }
        at += op.args();
        out.push(s);
    }
    out.join(" ")
}

/// The lines of a scene as the oracle reads Pele's SVG, every number as JavaScript's String
/// writes it.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let pt = |x: f64, y: f64| format!("{},{}", n(x), n(y));
    let rect = |r: Rect| format!("{},{}", pt(r.x, r.y), pt(r.w, r.h));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    if let Some(t) = &sc.title {
        lines.push(format!("acc-title {}", id(t)));
    }
    if let Some(d) = &sc.desc {
        lines.push(format!("acc-desc {}", id(d)));
    }
    let label = |lines: &mut Vec<String>, it: &Item| {
        let Some(p) = &it.label else {
            return;
        };
        let l = &p.label;
        // The class Pele writes a label of each role with, and its weight and style.
        let (class, look) = match it.role.as_str() {
            ROLE_TITLE => ("pele-title", "b -"),
            ROLE_DIRECTORY => ("pele-label", "b -"),
            ROLE_FILE => ("pele-label", "- -"),
            _ => ("pele-tree-description", "- i"),
        };
        // Pele's labelSvg writes a label of any lines, its leftLabelSvg only the lines with
        // text.
        let some = (it.role == ROLE_TITLE && !l.lines.is_empty())
            || l.lines
                .iter()
                .flatten()
                .any(|span| !span.text.is_empty() || span.icon.is_some());
        if !some {
            return;
        }
        let anchor = match p.align {
            Align::Start => "start",
            Align::Middle => "middle",
            Align::End => "end",
        };
        let at = format!(
            "{anchor} {class} {} {look} {}",
            paint_name(it.style.text),
            pt(p.at.x, p.at.y)
        );
        if let [line] = l.lines.as_slice()
            && let [span] = line.as_slice()
            && span.icon.is_none()
        {
            lines.push(format!(
                "label {at} | {}:{}",
                span.face.bits(),
                clean(&span.text)
            ));
            return;
        }
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
        let widths: Vec<String> = l.widths.iter().map(|&w| n(w)).collect();
        lines.push(format!(
            "label {at} {},{},{},{} | {} | {}",
            n(l.size),
            n(l.line_height),
            n(l.width),
            n(l.height),
            widths.join(","),
            spans.join(" / ")
        ));
    };
    // The icon Pele draws itself.
    let glyph_lines = |lines: &mut Vec<String>, it: &Item| {
        let b = it.bounds;
        let name = it.shape.as_ref().map_or("", |s| s.as_str());
        lines.push(format!("icon {name} {}", rect(b)));
        let width = match it.style.stroke_width {
            Some(w) if w == GLYPH_STROKE * b.w / 24.0 => n(GLYPH_STROKE),
            _ => "?".into(),
        };
        let want = glyph(name).map(|(_, p)| p.scaled(b.w / 24.0, b.x, b.y));
        let outline = match &it.parts[0].outline {
            Outline::Path(p) if Some(p) == want.as_ref() => "",
            _ => " (outline differs)",
        };
        lines.push(format!(
            "glyph {name} {} {width} {} {}{outline}",
            paint_name(it.style.stroke),
            cap_join(it),
            paint_name(it.style.fill)
        ));
    };
    let items = &sc.items;
    let mut i = 0;
    while i < items.len() {
        let it = &items[i];
        match it.role.as_str() {
            ROLE_TITLE | ROLE_DESCRIPTION => label(&mut lines, it),
            ROLE_GUIDE => {
                let Outline::Path(p) = &it.parts[0].outline else {
                    panic!("a guide's path");
                };
                lines.push(format!(
                    "guide {} {} {} {} {}",
                    path_line(p),
                    paint_name(it.style.stroke),
                    n(it.style.stroke_width.unwrap_or(f64::NAN)),
                    paint_name(it.style.fill),
                    cap_join(it)
                ));
            }
            role @ (ROLE_DIRECTORY | ROLE_FILE) => {
                let kind = if role == ROLE_DIRECTORY {
                    "dir"
                } else {
                    "file"
                };
                lines.push(format!(
                    "node {kind} {} [{}]",
                    id(it.id.as_deref().unwrap_or("")),
                    it.class.join(" ")
                ));
                for part in &it.parts {
                    let Outline::Box { rect: r, rounded } = part.outline else {
                        panic!("a highlight's box");
                    };
                    lines.push(format!(
                        "rect {} {} {}/{}",
                        rect(r),
                        if rounded { "r" } else { "-" },
                        paint_name(it.style.fill),
                        n(it.style.fill_opacity.unwrap_or(1.0))
                    ));
                }
                if let Some(pic) = &it.picture {
                    let PictureSource::Icon(name) = &pic.source else {
                        panic!("an icon");
                    };
                    lines.push(format!("icon {} {}", id(name), rect(pic.bounds)));
                    lines.push("host".into());
                }
                if items.get(i + 1).is_some_and(|next| next.role == ROLE_ICON) {
                    i += 1;
                    glyph_lines(&mut lines, &items[i]);
                }
                label(&mut lines, it);
            }
            role => lines.push(format!("unknown role {role}")),
        }
        i += 1;
    }
    lines
}

/// What Pele's render draws of tree views, read back out of its SVG as one line per thing
/// drawn, in the order drawn, with every number in full, against the same lines written of
/// the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("treeview-render");
    assert!(f.cases.len() >= 4000, "only {} cases", f.cases.len());
    let icons = |name: &str| HOST_ICONS.contains(&name);
    let mut failures = Failures::new();
    let mut tight = 0;
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a tree view");
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("a tree view that parses");
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            font_size: c.font_size.unwrap_or(16.0),
            icons: if c.icons { Some(&icons) } else { None },
            monospace: if c.mono {
                Monospace::FILES
            } else {
                Monospace::NONE
            },
            ..Options::default()
        };
        let sc = layout(&m, &p.config, &o);
        let loose = Options {
            max_width: f64::INFINITY,
            ..o
        };
        if c.max_width.is_some() && sc.width < layout(&m, &p.config, &loose).width {
            tight += 1;
        }
        let got = scene_lines(&sc);
        let diff = drawings.diff(k, &c.drawing, &got, None);
        failures.check(diff.is_none(), || {
            format!(
                "{:?}, max {:?}, size {:?}, icons {}, mono {}: {}",
                c.src,
                c.max_width,
                c.font_size,
                c.icons,
                c.mono,
                diff.unwrap()
            )
        });
    }
    assert!(tight >= 500, "only {tight} laid out tightly");
    failures.done("renders", 4400);
}

/// A tree view with no entries is the padding about nothing, as Pele draws it.
#[test]
fn empty() {
    let m = parse("treeView-beta\n", None).expect("a tree view");
    let sc = layout(&m, &Map::new(), &Options::default());
    assert_eq!((sc.width, sc.height, sc.items.len()), (16.0, 16.0, 0));
}
