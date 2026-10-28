//! Packet diagrams read and laid out as Pele reads and draws them, checked against the
//! oracle's packet and packet-render fixtures.

use limu_fixtures::load;
use serde::Deserialize;

use super::parser::{PACKET_TOKENS, PacketAst, parse_packet, table};
use super::render::{ROLE_BIT, ROLE_FIELD, ROLE_TITLE, layout};
use super::{PacketModel, parse};
use crate::common::tokens::tokenize;
use crate::detect::DiagramType;
use crate::errors::ErrorCode;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::Paint;
use crate::scene::{Align, Item, Outline, Scene};
use crate::testing::{Enc, Failures, PeleError, pele_num, same, sorted};
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

/// The kinds of token, numbered by hand, are the indexes of their types in PACKET_TOKENS.
#[test]
fn token_kinds() {
    let names: Vec<&str> = PACKET_TOKENS.iter().map(|t| t.name).collect();
    assert_eq!(
        names,
        [
            "packet-beta",
            "packet",
            "-",
            "+",
            ":",
            "ACC_DESCR",
            "ACC_TITLE",
            "TITLE",
            "INT",
            "STRING",
            "NEWLINE",
            "WHITESPACE",
            "YAML",
            "DIRECTIVE",
            "SINGLE_LINE_COMMENT"
        ]
    );
}

/// What the oracle writes of a value Pele leaves undefined.
fn undefined() -> Value {
    Value::Object(
        [("$".to_string(), Value::String("undefined".into()))]
            .into_iter()
            .collect(),
    )
}

fn enc_str(s: &Option<String>) -> Value {
    s.clone().map_or_else(undefined, Value::String)
}

fn enc_num(n: Option<f64>) -> Value {
    n.map_or_else(undefined, Value::Number)
}

fn object(entries: Vec<(&str, Value)>) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn dump_ast(t: &PacketAst) -> Value {
    let blocks = t
        .blocks
        .iter()
        .map(|b| {
            Value::Array(vec![
                enc_num(b.start),
                enc_num(b.end),
                enc_num(b.bits),
                Value::String(b.label.clone()),
            ])
        })
        .collect();
    object(vec![
        ("title", enc_str(&t.title)),
        ("accTitle", enc_str(&t.acc_title)),
        ("accDescr", enc_str(&t.acc_descr)),
        ("blocks", Value::Array(blocks)),
    ])
}

/// The model as JSON.stringify writes it, without its type and what is undefined.
fn dump_model(m: &PacketModel) -> Value {
    let mut out = Map::new();
    for (k, v) in [
        ("title", &m.title),
        ("accTitle", &m.acc_title),
        ("accDescr", &m.acc_descr),
    ] {
        if let Some(v) = v {
            out.insert(k, Value::String(v.clone()));
        }
    }
    out.insert("bitsPerRow", Value::Number(m.bits_per_row));
    let rows = m
        .rows
        .iter()
        .map(|row| {
            Value::Array(
                row.iter()
                    .map(|b| {
                        Value::Array(vec![
                            Value::Number(b.start),
                            Value::Number(b.end),
                            Value::Number(b.bits),
                            Value::String(b.label.clone()),
                        ])
                    })
                    .collect(),
            )
        })
        .collect();
    out.insert("rows", Value::Array(rows));
    Value::Object(out)
}

fn dump_tokens(src: &str) -> Option<Vec<(String, String)>> {
    let t = tokenize(src, table(), DiagramType::Packet).ok()?;
    let n = t.kinds.len() - 1;
    Some(
        (0..n)
            .map(|i| {
                let name = PACKET_TOKENS[t.kinds[i]].name.to_string();
                (name, src[t.starts[i]..t.ends[i]].to_string())
            })
            .collect(),
    )
}

#[test]
fn parse_matches_pele() {
    let f: Cases = load("packet");
    let mut failures = Failures::new();
    for c in &f.cases {
        let tokens = dump_tokens(&c.src);
        failures.check(tokens == c.direct.tokens, || {
            format!(
                "tokens of {:?}: {tokens:?}, Pele {:?}",
                c.src, c.direct.tokens
            )
        });
        let tree = parse_packet(&c.src);
        let ok = match (&tree, &c.direct.error, &c.direct.ast) {
            (Err(e), Some(want), _) => want.is(e),
            (Ok(t), None, Some(want)) => same(&sorted(&dump_ast(t)), &sorted(&want.0)),
            _ => false,
        };
        failures.check(ok, || {
            format!(
                "parse_packet({:?}): {tree:?}, Pele {:?} {:?}",
                c.src, c.direct.ast, c.direct.error
            )
        });

        let p = prepare(&c.src, &ParseOptions::default());
        let ok = match (&p, &c.error) {
            (Err(e), Some(want)) => want.is(e),
            (Err(e), None) => e.code == ErrorCode::UnsupportedDiagram && c.diagram.is_none(),
            (Ok(p), _) if c.diagram.as_deref() != Some(p.found.diagram.name()) => false,
            (Ok(p), want) if p.found.diagram == DiagramType::Packet => {
                let got = p.parse_with(false, |s| parse(s, &p.config, p.title.as_deref()));
                match (&got, want, &c.model) {
                    (Err(e), Some(want), _) => want.is(e),
                    (Ok(m), None, Some(model)) => same(&sorted(&dump_model(m)), &sorted(&model.0)),
                    _ => false,
                }
            }
            (Ok(_), _) => true,
        };
        failures.check(ok, || {
            format!("{:?}: model {:?}, error {:?}", c.src, c.model, c.error)
        });
    }
    failures.done("packet", 3 * 4000);
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

/// A paint of the theme as Pele's SVG names it.
fn paint_name(p: Option<Paint>) -> &'static str {
    match p {
        Some(Paint::FG) => "var(--_fg)",
        Some(Paint::MUTED) => "var(--_m)",
        Some(Paint::SURFACE) => "var(--_s)",
        Some(Paint::BORDER) => "var(--_b)",
        _ => "?",
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

/// The lines of a scene as the oracle reads Pele's SVG.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let num = pele_num;
    let pt = |x: f64, y: f64| format!("{},{}", num(x), num(y));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    // The lines of a label drawn about its point, as Pele's text and labelSvg place them,
    // after prefix, and with what the oracle writes of the size and paint of its type.
    let label = |lines: &mut Vec<String>, prefix: &str, kind: &str, it: &Item| {
        let Some(p) = it.label.as_ref().filter(|p| !p.label.is_empty()) else {
            return;
        };
        let l = &p.label;
        let anchor = match p.align {
            Align::Middle => "middle",
            Align::Start => "start",
            Align::End => "end",
        };
        let mut top = p.at.y - l.height / 2.0;
        for line in &l.lines {
            let y = top + l.line_height / 2.0 + l.size * 0.35;
            let at = format!("{prefix}t {} {kind} {anchor} ", pt(p.at.x, y));
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
            top += l.line_height;
        }
    };
    for it in &sc.items {
        match it.role.as_str() {
            ROLE_FIELD => {
                let Outline::Box { rect: r, rounded } = it.parts[0].outline else {
                    lines.push("field of a path".into());
                    continue;
                };
                lines.push(format!(
                    "field {} {},{} {} {} {}",
                    it.id.as_deref().unwrap_or(""),
                    pt(r.x, r.y),
                    pt(r.w, r.h),
                    if rounded { "r" } else { "-" },
                    paint_name(it.style.fill),
                    paint_name(it.style.stroke)
                ));
                if let Some(p) = &it.label {
                    label(&mut lines, "", &num(p.label.size), it);
                }
            }
            ROLE_BIT => {
                let size = it.label.as_ref().map_or(0.0, |p| p.label.size);
                let kind = format!("{} {}", num(size), paint_name(it.style.text));
                label(&mut lines, "bit ", &kind, it);
            }
            ROLE_TITLE => label(&mut lines, "title ", "-", it),
            role => lines.push(format!("unknown role {role}")),
        }
    }
    lines
}

/// What Pele's render draws of packet diagrams, read back out of its SVG as a line for each
/// thing drawn, in the order drawn, against the same lines written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("packet-render");
    assert!(f.cases.len() >= 4000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a packet diagram");
        let m = p
            .parse_with(false, |s| parse(s, &p.config, p.title.as_deref()))
            .expect("a packet diagram that parses");
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            font_size: c.font_size.unwrap_or(16.0),
            ..Options::default()
        };
        let got = scene_lines(&layout(&m, &p.config, &o));
        let diff = drawings.diff(k, &c.drawing, &got, None);
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
    failures.done("renders", 4000);
}
