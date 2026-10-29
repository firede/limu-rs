//! Sankey diagrams read and laid out as Pele reads and draws them, checked against the
//! oracle's sankey and sankey-render fixtures.

use std::collections::HashMap;

use limu_fixtures::load;
use serde::Deserialize;

use super::db::SankeyDb;
use super::lexer::tokenize;
use super::parse;
use super::parser::{parse_sankey, prepare_text_for_parsing};
use super::render::{ROLE_LABEL, ROLE_LINK, ROLE_NODE, ROLE_TITLE, ROLE_VALUE, layout};
use crate::css::declarations;
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::len16;
use crate::js::math::round;
use crate::js::number::to_string as n;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::{Paint, Style, Theme};
use crate::scene::{Align, Item, Op, Outline, Painting, Scene};
use crate::testing::pelesvg::{cap_join, id, own_fill};
use crate::testing::{Enc, Failures, PeleError, same, sorted};
use crate::value::{Map, Value};

#[derive(Deserialize)]
struct Cases {
    cases: Vec<Case>,
}

#[derive(Deserialize, Default)]
struct Outcome {
    model: Option<Enc>,
    error: Option<PeleError>,
}

#[derive(Deserialize)]
struct Case {
    src: String,
    #[serde(rename = "type")]
    diagram: Option<String>,
    tokens: Enc,
    prepared: String,
    direct: Outcome,
    model: Option<Enc>,
    error: Option<PeleError>,
    /// Whether the pipeline's result is the direct one.
    #[serde(default)]
    same: bool,
}

fn dump_tokens(src: &str) -> Value {
    let t = tokenize(src);
    let tokens = (0..t.types.len())
        .map(|i| {
            Value::Array(vec![
                Value::Number(t.types[i] as f64),
                Value::String(t.texts[i].to_string()),
                Value::Number(len16(&src[..t.starts[i]]) as f64),
            ])
        })
        .collect();
    Value::Array(tokens)
}

/// The model as the oracle writes it, without its type and what is undefined.
fn dump_model(m: &SankeyDb) -> Value {
    let mut out = Map::new();
    if let Some(t) = &m.title {
        out.insert("title", Value::String(t.clone()));
    }
    let nodes = m
        .nodes
        .iter()
        .map(|n| {
            Value::Array(vec![
                Value::String(n.id.clone()),
                Value::Number(n.index as f64),
            ])
        })
        .collect();
    let links = m
        .links
        .iter()
        .map(|l| {
            Value::Array(vec![
                Value::Number(m.nodes[l.source].index as f64),
                Value::Number(m.nodes[l.target].index as f64),
                Value::Number(l.value),
            ])
        })
        .collect();
    out.insert("nodes", Value::Array(nodes));
    out.insert("links", Value::Array(links));
    Value::Object(out)
}

/// Whether a parse gives what Pele's does.
fn check(got: &Result<SankeyDb, Error>, model: &Option<Enc>, error: &Option<PeleError>) -> bool {
    match (got, error, model) {
        (Err(e), Some(want), _) => want.is(e),
        (Ok(m), None, Some(want)) => same(&sorted(&dump_model(m)), &sorted(&want.0)),
        _ => false,
    }
}

#[test]
fn parse_matches_pele() {
    let f: Cases = load("sankey");
    assert!(f.cases.len() >= 7000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    let mut models = 0;
    for c in &f.cases {
        let tokens = dump_tokens(&c.src);
        failures.check(same(&tokens, &c.tokens.0), || {
            format!("tokens of {:?}: {tokens:?}, Pele {:?}", c.src, c.tokens.0)
        });
        let prepared = prepare_text_for_parsing(&c.src, None);
        failures.check(prepared == c.prepared, || {
            format!(
                "prepared text of {:?}: {prepared:?}, Pele {:?}",
                c.src, c.prepared
            )
        });
        let mut db = SankeyDb::default();
        let direct = parse_sankey(&c.src, &mut db, &c.src).map(|()| db);
        failures.check(check(&direct, &c.direct.model, &c.direct.error), || {
            format!(
                "parse_sankey({:?}): {direct:?}, Pele {:?} {:?}",
                c.src, c.direct.model, c.direct.error
            )
        });

        let (model, error) = if c.same {
            (&c.direct.model, &c.direct.error)
        } else {
            (&c.model, &c.error)
        };
        let p = prepare(&c.src, &ParseOptions::default());
        let ok = match (&p, error) {
            (Err(e), Some(want)) => want.is(e),
            (Err(e), None) => e.code == ErrorCode::UnsupportedDiagram && c.diagram.is_none(),
            (Ok(p), _) if c.diagram.as_deref() != Some(p.found.diagram.name()) => false,
            (Ok(p), _) if p.found.diagram == DiagramType::Sankey => {
                let got = p.parse_with(false, |s| parse(s, p.title.as_deref()));
                models += usize::from(got.is_ok());
                check(&got, model, error)
            }
            (Ok(_), _) => true,
        };
        failures.check(ok, || {
            format!("{:?}: model {model:?}, error {error:?}", c.src)
        });
    }
    failures.done("sankey", 4 * 7000);
    assert!(models >= 1000, "only {models} cases parse");
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
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

/// An author's style: as "author=" and the style, or as "css=", as Pele writes no style,
/// where the author set none. A line of Pele's with no style is so read as it is.
fn author(s: &Style) -> String {
    if *s == Style::default() {
        "css=".into()
    } else {
        format!("author={s:?}")
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

/// A line of the fixture with the style Pele writes, "css=", read as limu reads
/// declarations, "author=".
fn author_line(line: &str) -> String {
    let Some(at) = line.find(" css=") else {
        return line.to_string();
    };
    let end = line[at + 1..].find(' ').map_or(line.len(), |e| e + at + 1);
    let Some(written) = unescape(&line[at + " css=".len()..end]) else {
        return line.to_string();
    };
    let decls: Vec<String> = written.split(';').map(str::to_string).collect();
    format!(
        "{} {}{}",
        &line[..at],
        author(&declarations(&decls).style),
        &line[end..]
    )
}

/// A paint as Pele's SVG names it, "-" for the text's own color, which Pele leaves to the
/// root, and a paint at an opacity other than factor "?".
fn paint_name(p: Option<Paint>, factor: f32) -> String {
    let Some(p) = p.filter(|p| p.opacity() == factor) else {
        return "?".into();
    };
    if p == Paint::FG {
        return "-".into();
    }
    for k in 0..8 {
        if p.as_token() == Paint::series(k).as_token() {
            return format!("var(--pele-series-{},{})", k + 1, Theme::PELE.series[k]);
        }
    }
    match p {
        Paint::MUTED => "var(--_m)",
        Paint::BG => "var(--_bg)",
        _ => "?",
    }
    .into()
}

/// Text as Pele's esc writes it, without the control characters XML does not allow.
fn xml_text(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            !(c < ' ' && !matches!(c, '\t' | '\n' | '\r')
                || matches!(c, '\x7f' | '\u{fffe}' | '\u{ffff}'))
        })
        .collect()
}

fn label_text(it: &Item) -> String {
    let l = &it.label.as_ref().expect("a label").label;
    xml_text(&l.lines[0][0].text)
}

/// A scene written as the oracle reads Pele's SVG, every number as JavaScript's String
/// writes it.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let pt = |x: f64, y: f64| format!("{},{}", n(x), n(y));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    if sc.title.is_some() || sc.desc.is_some() {
        lines.push("acc".into());
    }
    let mut items = &sc.items[..];
    if let Some(it) = items.first().filter(|it| it.role.as_str() == ROLE_TITLE) {
        let p = it.label.as_ref().expect("a label");
        let l = &p.label;
        let y = p.at.y - l.height / 2.0 + l.line_height / 2.0 + l.size * 0.35;
        let content: Vec<String> = l
            .lines
            .iter()
            .map(|line| {
                if line.is_empty() {
                    return " ".into();
                }
                xml_text(&line.iter().map(|s| s.text.as_str()).collect::<String>())
            })
            .collect();
        let size = if l.size == sc.font_size {
            "-".into()
        } else {
            n(l.size)
        };
        lines.push(format!(
            "title {} {size} | {}",
            pt(p.at.x, y),
            content.join(" | ")
        ));
        items = &items[1..];
    }
    lines.push("links 0.4".into());
    // The labels' size, which Pele writes on their group: that of every label, which shrinks
    // with a narrow or crowded chart, or with none, the size it starts from.
    let label_size = items
        .iter()
        .find(|it| it.role.as_str() == ROLE_LABEL)
        .map_or(round(sc.font_size * 0.875), |it| {
            it.label.as_ref().expect("a label").label.size
        });
    let labels_line = format!("labels {}", n(label_size));
    let mut labelled = false;
    // Where each node's bar starts, by its id.
    let mut bars: HashMap<&str, f64> = HashMap::new();
    let mut i = 0;
    while i < items.len() {
        let it = &items[i];
        let name = it.id.as_deref().unwrap_or("");
        match it.role.as_str() {
            ROLE_LINK => {
                let Outline::Path(path) = &it.parts[0].outline else {
                    lines.push("link of a box".into());
                    i += 1;
                    continue;
                };
                let mut d = Vec::new();
                let mut at = 0;
                for &op in path.ops() {
                    let a = &path.args()[at..at + op.args()];
                    at += op.args();
                    let mut s = match op {
                        Op::MoveTo => "M",
                        Op::LineTo => "L",
                        Op::QuadTo => "Q",
                        Op::CubeTo => "C",
                        Op::Close => "Z",
                    }
                    .to_string();
                    for xy in a.chunks(2) {
                        s += &format!(" {}", pt(xy[0], xy[1]));
                    }
                    d.push(s);
                }
                lines.push(format!(
                    "link {} {} {} {} {} | {}",
                    id(name),
                    d.join(" "),
                    paint_name(own_fill(it), 0.4),
                    author(&it.author),
                    cap_join(it),
                    id(it.tooltip.as_deref().unwrap_or(""))
                ));
            }
            ROLE_NODE => {
                let part = &it.parts[0];
                let Outline::Box { rect: r, rounded } = part.outline else {
                    lines.push("node of a path".into());
                    i += 1;
                    continue;
                };
                let radius = if rounded {
                    format!("r{}", n(it.style.radius.unwrap_or(0.0)))
                } else {
                    "-".into()
                };
                let stroke = if it.style.stroke.is_some() || part.paint != Painting::FillOnly {
                    "?"
                } else {
                    "-"
                };
                bars.insert(name, r.x);
                lines.push(format!(
                    "node {} {},{} {radius} {} {stroke} {} {}",
                    id(name),
                    pt(r.x, r.y),
                    pt(r.w, r.h),
                    paint_name(it.style.fill, 1.0),
                    author(&it.author),
                    cap_join(it)
                ));
            }
            ROLE_LABEL => {
                if !labelled {
                    labelled = true;
                    lines.push(labels_line.clone());
                }
                // The name and the value of a node, as Pele writes them in one text:
                // anchored where the name starts when the label is right of the bar, and
                // where the value ends when it is left of it.
                let value = items
                    .get(i + 1)
                    .filter(|v| v.role.as_str() == ROLE_VALUE && v.id == it.id);
                if value.is_some() {
                    i += 1;
                }
                let p = it.label.as_ref().expect("a label");
                let (anchor, at) = match value {
                    None if p.align == Align::Start => ("start", it),
                    None => ("end", it),
                    Some(_) if p.at.x > bars.get(name).copied().unwrap_or(f64::NAN) => {
                        ("start", it)
                    }
                    Some(v) => ("end", v),
                };
                let at = at.label.as_ref().expect("a label").at;
                let mut line = format!(
                    "label {} {} {anchor} {} | {}",
                    id(name),
                    pt(at.x, at.y),
                    paint_name(it.style.text, 1.0),
                    label_text(it)
                );
                if let Some(v) = value {
                    line += &format!(" | {} {}", paint_name(v.style.text, 1.0), label_text(v));
                    let vp = v.label.as_ref().expect("a label");
                    if v.style.halo != it.style.halo
                        || v.style.halo_width != it.style.halo_width
                        || n(vp.at.y) != n(p.at.y)
                    {
                        line += " | value apart";
                    }
                }
                if let Some(halo) = it.style.halo {
                    line += &format!(
                        " | halo none {} {} round",
                        paint_name(Some(halo), 1.0),
                        n(it.style.halo_width.unwrap_or(0.0))
                    );
                }
                lines.push(line);
            }
            role => lines.push(format!("unknown role {role}")),
        }
        i += 1;
    }
    if !labelled {
        lines.push(labels_line);
    }
    lines
}

/// What Pele's render draws of sankey diagrams, read back out of its SVG as a line for each
/// thing drawn, in the order drawn, with every number in full, and the errors of the
/// diagrams it refuses, against the same lines written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("sankey-render");
    assert!(f.cases.len() >= 5000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    let mut refused = 0;
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a sankey diagram");
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("a sankey diagram that parses");
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            font_size: c.font_size.unwrap_or(16.0),
            ..Options::default()
        };
        let got = match layout(&m, &p.config, &o) {
            Ok(sc) => scene_lines(&sc),
            Err(e) => {
                refused += 1;
                // Entity codes are written back as written, as Diagram::layout writes them.
                let e = e.tagged(p.found);
                vec![format!("error {} {}", e.code.as_str(), e.message)]
            }
        };
        let diff = drawings.diff(k, &c.drawing, &got, Some(&author_line));
        failures.check(diff.is_none(), || {
            format!(
                "{:?}, max {:?}, size {:?}: {}",
                c.src,
                c.max_width,
                c.font_size,
                diff.unwrap()
            )
        });
    }
    failures.done("renders", 5000);
    assert!(refused >= 20, "only {refused} diagrams refused");
}
