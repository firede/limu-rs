//! Treemaps read and laid out as Pele reads and draws them, checked against the oracle's
//! treemap, treemap-format and treemap-render fixtures.

use limu_fixtures::load;
use serde::Deserialize;

use super::format::formatter;
use super::model::{TreemapModel, TreemapNode};
use super::parse;
use super::parser::{RowAst, TREEMAP_TOKENS, TreemapAst, parse_treemap, table};
use super::render::{ROLE_LEAF, ROLE_SECTION, ROLE_TITLE, ROLE_VALUE, layout};
use crate::common::tokens::tokenize;
use crate::css::declarations;
use crate::detect::DiagramType;
use crate::errors::ErrorCode;
use crate::js::math::round;
use crate::js::number::to_string as n;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::{Paint, Style, Theme};
use crate::scene::{Align, Item, Outline, Scene};
use crate::testing::pelesvg::{cap_join, id, own_fill, text_ink};
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

/// The kinds of token, numbered by hand, are the indexes of their types in TREEMAP_TOKENS.
#[test]
fn token_kinds() {
    let names: Vec<&str> = TREEMAP_TOKENS.iter().map(|t| t.name).collect();
    assert_eq!(
        names,
        [
            "ACC_DESCR",
            "ACC_TITLE",
            "TITLE",
            "TREEMAP_KEYWORD",
            "CLASS_DEF",
            "STYLE_SEPARATOR",
            "SEPARATOR",
            "COMMA",
            "INDENTATION",
            "WS",
            "ML_COMMENT",
            "NL",
            "ID2",
            "NUMBER2",
            "STRING2"
        ]
    );
}

fn undefined() -> Value {
    Value::Object(
        [("$".to_string(), Value::String("undefined".into()))]
            .into_iter()
            .collect(),
    )
}

/// A string as the oracle writes it, or undefined.
fn enc_str(s: &Option<String>) -> Value {
    s.as_ref()
        .map_or_else(undefined, |s| Value::String(s.clone()))
}

fn string(v: &str) -> Value {
    Value::String(v.to_string())
}

fn object(entries: Vec<(&str, Value)>) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn dump_ast(t: &TreemapAst) -> Value {
    let indent = |i: &Option<usize>| i.map_or_else(undefined, |i| Value::Number(i as f64));
    let rows = t
        .rows
        .iter()
        .map(|r| match r {
            RowAst::ClassDef {
                indent: i,
                class_name,
                style_text,
            } => Value::Array(vec![
                string("classDef"),
                indent(i),
                string(class_name),
                enc_str(style_text),
            ]),
            RowAst::Row { indent: i, item } => Value::Array(vec![
                string("row"),
                indent(i),
                string(if item.value.is_some() {
                    "Leaf"
                } else {
                    "Section"
                }),
                string(&item.name),
                item.value.map_or_else(undefined, Value::Number),
                enc_str(&item.class_selector),
            ]),
        })
        .collect();
    object(vec![
        ("title", enc_str(&t.title)),
        ("accTitle", enc_str(&t.acc_title)),
        ("accDescr", enc_str(&t.acc_descr)),
        ("rows", Value::Array(rows)),
    ])
}

fn strings(v: &[String]) -> Value {
    Value::Array(v.iter().map(|s| string(s)).collect())
}

fn dump_node(m: &TreemapModel, node: &TreemapNode) -> Value {
    let children = node.children.as_ref().map_or_else(undefined, |c| {
        Value::Array(c.iter().map(|&i| dump_node(m, &m.nodes[i])).collect())
    });
    object(vec![
        ("name", string(&node.name)),
        ("children", children),
        ("value", node.value.map_or_else(undefined, Value::Number)),
        ("classSelector", enc_str(&node.class_selector)),
        ("css", node.css.as_deref().map_or_else(undefined, strings)),
    ])
}

/// The model as the oracle writes it, without its type and what is undefined.
fn dump_model(m: &TreemapModel) -> Value {
    let mut out = Map::new();
    let roots = m.roots.iter().map(|&i| dump_node(m, &m.nodes[i])).collect();
    let classes = m
        .classes
        .iter()
        .map(|(name, styles)| Value::Array(vec![string(name), strings(styles)]))
        .collect();
    out.insert("roots", Value::Array(roots));
    out.insert("classes", Value::Array(classes));
    for (k, v) in [
        ("title", &m.title),
        ("accTitle", &m.acc_title),
        ("accDescr", &m.acc_descr),
    ] {
        if let Some(v) = v.as_ref().filter(|v| !v.is_empty()) {
            out.insert(k, string(v));
        }
    }
    Value::Object(out)
}

fn dump_tokens(src: &str) -> Option<Vec<(String, String)>> {
    let t = tokenize(src, table(), DiagramType::Treemap).ok()?;
    let n = t.kinds.len() - 1;
    Some(
        (0..n)
            .map(|i| {
                let name = TREEMAP_TOKENS[t.kinds[i]].name.to_string();
                (name, src[t.starts[i]..t.ends[i]].to_string())
            })
            .collect(),
    )
}

#[test]
fn parse_matches_pele() {
    let f: Cases = load("treemap");
    assert!(f.cases.len() >= 5000, "only {} cases", f.cases.len());
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
        let tree = parse_treemap(&c.src);
        let ok = match (&tree, &c.direct.error, &c.direct.ast) {
            (Err(e), Some(want), _) => want.is(e),
            (Ok(t), None, Some(want)) => same(&sorted(&dump_ast(t)), &sorted(&want.0)),
            _ => false,
        };
        failures.check(ok, || {
            format!(
                "parse_treemap({:?}): {tree:?}, Pele {:?} {:?}",
                c.src, c.direct.ast, c.direct.error
            )
        });

        let p = prepare(&c.src, &ParseOptions::default());
        let ok = match (&p, &c.error) {
            (Err(e), Some(want)) => want.is(e),
            (Err(e), None) => e.code == ErrorCode::UnsupportedDiagram && c.diagram.is_none(),
            (Ok(p), _) if c.diagram.as_deref() != Some(p.found.diagram.name()) => false,
            (Ok(p), want) if p.found.diagram == DiagramType::Treemap => {
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
    failures.done("treemap", 3 * 5000);
    assert!(models >= 1500, "only {models} models");
}

#[derive(Deserialize)]
struct Formats {
    cases: Vec<(String, Enc, String)>,
}

/// What Pele's formatter writes of values with specifiers of d3-format.
#[test]
fn format_matches_pele() {
    let f: Formats = load("treemap-format");
    assert!(f.cases.len() >= 10000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    for (spec, value, want) in &f.cases {
        let Value::Number(v) = value.0 else {
            panic!("a value that is no number: {value:?}");
        };
        let got = formatter(spec)(v);
        failures.check(got == *want, || {
            format!("formatter({spec:?})({v:?}) = {got:?}, Pele {want:?}")
        });
    }
    failures.done("formats", 10000);
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
    padding: Option<f64>,
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

/// What of an author's style Pele writes on a shape: all but the color of its text.
fn shape_view(s: &Style) -> Style {
    Style {
        text: None,
        ..s.clone()
    }
}

/// What of an author's style Pele writes on a text that changes how limu draws it: its color
/// and its opacity.
fn text_view(s: &Style) -> Style {
    Style {
        text: s.text,
        opacity: s.opacity,
        ..Style::default()
    }
}

/// What view takes of an author's style: as "author=" and the view, or as "css=", as Pele
/// writes no style, where the view is that of no style. A line of Pele's with no style is so
/// read as it is.
fn author(view: fn(&Style) -> Style, s: &Style) -> String {
    let v = view(s);
    if v == view(&Style::default()) {
        "css=".into()
    } else {
        format!("author={v:?}")
    }
}

/// Text with the escapes of a URL's path read, as Go's url.PathUnescape reads them, or none
/// where an escape is not two hexadecimal digits.
fn unescape(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// A line of the fixture with the style Pele writes, "css=", read as limu reads declarations,
/// "author=": on a shape as it is, and on a text with its fill, which is the color the author
/// gave, read as that color.
fn author_line(line: &str) -> String {
    let Some(at) = line.find(" css=") else {
        return line.to_string();
    };
    let end = line[at + 1..].find(' ').map_or(line.len(), |e| e + at + 1);
    let Some(written) = unescape(&line[at + " css=".len()..end]) else {
        return line.to_string();
    };
    let mut decls: Vec<String> = written.split(';').map(str::to_string).collect();
    let field = if line.starts_with("section ") || line.starts_with("leaf ") {
        author(shape_view, &declarations(&decls).style)
    } else {
        for d in &mut decls {
            if let Some(rest) = d.strip_prefix("fill:") {
                *d = format!("color:{rest}");
            }
        }
        author(text_view, &declarations(&decls).style)
    };
    format!("{} {field}{}", &line[..at], &line[end..])
}

/// How a box is filled, as the oracle reads Pele's fill and fill-opacity.
fn fill_name(p: Option<Paint>) -> String {
    match p {
        Some(p) if p == Paint::NONE => return "none -".into(),
        Some(p) => {
            for k in 0..8 {
                let name = format!("var(--pele-series-{},{})", k + 1, Theme::PELE.series[k]);
                if p == Paint::series(k) {
                    return format!("{name} -");
                }
                if p == Paint::series(k).with_opacity(0.12) {
                    return format!("{name} 0.12");
                }
            }
        }
        None => {}
    }
    "? ?".into()
}

/// How a box is stroked, "-" for none.
fn stroke_name(p: Option<Paint>) -> String {
    if p == Some(Paint::NONE) {
        return "-".into();
    }
    let name = fill_name(p);
    name[..name.find(' ').expect("a fill and its opacity")].to_string()
}

/// The fill the oracle reads of an item's text: the color the default look picks on a fill,
/// where it picks one, and the diagram's own otherwise.
fn text_paint(it: &Item) -> &'static str {
    match text_ink(it) {
        "" => match it.style.text {
            Some(p) if p == Paint::FG => "var(--_fg)",
            Some(p) if p == Paint::MUTED => "var(--_m)",
            Some(p) if p == Paint::BG => "var(--_bg)",
            _ => "",
        },
        ink => ink,
    }
}

/// Text as Pele's esc writes it, without the control characters XML does not allow.
fn written(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            !(c < ' ' && !matches!(c, '\t' | '\n' | '\r')
                || matches!(c, '\x7f' | '\u{fffe}' | '\u{ffff}'))
        })
        .collect()
}

/// A scene written as the oracle reads Pele's SVG, every number as JavaScript's String
/// writes it.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let pt = |x: f64, y: f64| format!("{},{}", n(x), n(y));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    // The lines of a label drawn about its point, as Pele's text and labelSvg place them,
    // after prefix: a line of one line of text at its baseline, and a title's lines below
    // each other from where the first goes.
    let label =
        |lines: &mut Vec<String>, prefix: &str, size: Option<String>, weight: &str, it: &Item| {
            let Some(p) = it.label.as_ref().filter(|p| !p.label.is_empty()) else {
                return;
            };
            let l = &p.label;
            let title = it.role.as_str() == ROLE_TITLE;
            let mut top = p.at.y - l.height / 2.0;
            let mut y = if title {
                top + l.line_height / 2.0 + l.size * 0.35
            } else {
                p.at.y + l.size * 0.35
            };
            let size = size.unwrap_or_else(|| n(l.size));
            let anchor = match p.align {
                Align::Middle => "middle",
                Align::Start => "start",
                Align::End => "end",
            };
            for line in &l.lines {
                let at = format!(
                    "{prefix}t {} {size} {anchor} {} {weight} {} ",
                    pt(p.at.x, y),
                    text_paint(it),
                    author(text_view, &it.author)
                );
                if line.is_empty() {
                    lines.push(format!("{at} "));
                }
                for (k, span) in line.iter().enumerate() {
                    if k == 0 {
                        lines.push(format!("{at}{}", written(&span.text)));
                    } else {
                        lines.push(format!("{prefix}+ {}", written(&span.text)));
                    }
                }
                if title {
                    // A title's lines are placed from its top down, as Pele's labelSvg places
                    // them.
                    top += l.line_height;
                    y = top + l.line_height / 2.0 + l.size * 0.35;
                } else {
                    y += l.line_height;
                }
            }
        };
    for it in &sc.items {
        match it.role.as_str() {
            ROLE_TITLE => label(&mut lines, "title ", Some("-".into()), "var(--_tw)", it),
            role @ (ROLE_SECTION | ROLE_LEAF) => {
                let kind = if role == ROLE_LEAF { "leaf" } else { "section" };
                let Outline::Box { rect: r, rounded } = &it.parts[0].outline else {
                    panic!("a box drawn as a path");
                };
                // Pele writes the theme's radius alone as rx:var(--_r), and the radius of a
                // small box, capped, otherwise.
                let rounded = if *rounded && it.style.max_radius.is_none() {
                    "r"
                } else {
                    "-"
                };
                lines.push(format!(
                    "{kind} {} [{}] {},{} {rounded} {} {} {} {}",
                    id(it.id.as_deref().unwrap_or("")),
                    it.class.join(" "),
                    pt(r.x, r.y),
                    pt(r.w, r.h),
                    fill_name(own_fill(it)),
                    stroke_name(it.style.stroke),
                    cap_join(it),
                    author(shape_view, &it.author)
                ));
                if let Some(tip) = &it.tooltip {
                    lines.push(format!("tip {}", written(tip)));
                }
                // A section's name is in the header's weight, and in the size of the group of
                // sections unless its header is short, which Pele writes on the text; a
                // leaf's name in its own size.
                let (mut weight, mut size) = ("-", None);
                if role == ROLE_SECTION {
                    weight = "var(--_hw)";
                    let at = it.label.as_ref().map_or(0.0, |p| p.at.y);
                    if 2.0 * (at - 1.0 - it.bounds.y) >= 23.0 {
                        size = Some(n(round(sc.font_size * 0.875)));
                    }
                }
                label(&mut lines, "", size, weight, it);
            }
            ROLE_VALUE => {
                // A section's value, at the end of its header, is written in the size Pele
                // gives it when no class sets one.
                let end = it.label.as_ref().is_some_and(|p| p.align == Align::End);
                let size = end.then(|| n(round(sc.font_size * 0.75)));
                label(&mut lines, "", size, "-", it);
            }
            role => lines.push(format!("unknown role {role}")),
        }
    }
    lines
}

/// What Pele's render draws of treemaps, read back out of its SVG as a line for each thing
/// drawn, in the order drawn, with every number in full, and the tooltip of each box that has
/// one, against the same lines written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("treemap-render");
    assert!(f.cases.len() >= 5000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a treemap");
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("a treemap that parses");
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            font_size: c.font_size.unwrap_or(16.0),
            padding: c.padding,
            ..Options::default()
        };
        let got = scene_lines(&layout(&m, &p.config, &o));
        let diff = drawings.diff(k, &c.drawing, &got, Some(&author_line));
        failures.check(diff.is_none(), || {
            format!(
                "{:?}, max {:?}, size {:?}, padding {:?}: {}",
                c.src,
                c.max_width,
                c.font_size,
                c.padding,
                diff.unwrap()
            )
        });
    }
    failures.done("renders", 5000);
}
