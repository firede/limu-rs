//! Info diagrams read and laid out as Pele reads and draws them, checked against the oracle's
//! info and info-render fixtures.

use limu_fixtures::load;
use serde::Deserialize;

use super::parser::{INFO_TOKENS, InfoAst, parse_info, table};
use super::render::{ROLE_TITLE, ROLE_VERSION, VERSION, lay, layout};
use super::{InfoModel, parse};
use crate::common::tokens::tokenize;
use crate::detect::DiagramType;
use crate::errors::ErrorCode;
use crate::js::number::to_string as n;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::Scene;
use crate::scene::style::Paint;
use crate::testing::pelesvg::id;
use crate::testing::{Enc, Failures, PeleError, same};
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

/// The kinds of token, numbered by hand, are the indexes of their types in INFO_TOKENS.
#[test]
fn token_kinds() {
    let names: Vec<&str> = INFO_TOKENS.iter().map(|t| t.name).collect();
    assert_eq!(
        names,
        [
            "showInfo",
            "info",
            "ACC_DESCR",
            "ACC_TITLE",
            "TITLE",
            "NEWLINE",
            "WHITESPACE",
            "YAML",
            "DIRECTIVE",
            "SINGLE_LINE_COMMENT"
        ]
    );
}

/// A string as the oracle writes it, or undefined.
fn enc_str(s: &Option<String>) -> Value {
    match s {
        Some(s) => Value::String(s.clone()),
        None => Value::Object(
            [("$".to_string(), Value::String("undefined".into()))]
                .into_iter()
                .collect(),
        ),
    }
}

fn dump_ast(t: &InfoAst) -> Value {
    Value::Object(
        [
            ("title", enc_str(&t.title)),
            ("accTitle", enc_str(&t.acc_title)),
            ("accDescr", enc_str(&t.acc_descr)),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect(),
    )
}

/// The model as JSON.stringify writes it, without its type and what is undefined.
fn dump_model(m: &InfoModel) -> Value {
    let mut out = Map::new();
    for (k, v) in [
        ("title", &m.title),
        ("accTitle", &m.acc_title),
        ("accDescr", &m.acc_descr),
    ] {
        if let Some(v) = v {
            out.insert(k.to_string(), Value::String(v.clone()));
        }
    }
    Value::Object(out)
}

fn dump_tokens(src: &str) -> Option<Vec<(String, String)>> {
    let t = tokenize(src, table(), DiagramType::Info).ok()?;
    let n = t.kinds.len() - 1;
    Some(
        (0..n)
            .map(|i| {
                let name = INFO_TOKENS[t.kinds[i]].name.to_string();
                (name, src[t.starts[i]..t.ends[i]].to_string())
            })
            .collect(),
    )
}

#[test]
fn parse_matches_pele() {
    let f: Cases = load("info");
    let mut failures = Failures::new();
    for c in &f.cases {
        let tokens = dump_tokens(&c.src);
        failures.check(tokens == c.direct.tokens, || {
            format!(
                "tokens of {:?}: {tokens:?}, Pele {:?}",
                c.src, c.direct.tokens
            )
        });
        let tree = parse_info(&c.src);
        let ok = match (&tree, &c.direct.error, &c.direct.ast) {
            (Err(e), Some(want), _) => want.is(e),
            (Ok(t), None, Some(want)) => same(&dump_ast(t), &want.0),
            _ => false,
        };
        failures.check(ok, || {
            format!(
                "parse_info({:?}): {tree:?}, Pele {:?} {:?}",
                c.src, c.direct.ast, c.direct.error
            )
        });

        let p = prepare(&c.src, &ParseOptions::default());
        let ok = match (&p, &c.error) {
            (Err(e), Some(want)) => want.is(e),
            (Err(e), None) => e.code == ErrorCode::UnsupportedDiagram && c.diagram.is_none(),
            (Ok(p), _) if c.diagram.as_deref() != Some(p.found.diagram.name()) => false,
            (Ok(p), want) if p.found.diagram == DiagramType::Info => {
                let got = p.parse_with(false, |s| parse(s, p.title.as_deref()));
                match (&got, want, &c.model) {
                    (Err(e), Some(want), _) => want.is(e),
                    (Ok(m), None, Some(model)) => same(&dump_model(m), &model.0),
                    _ => false,
                }
            }
            (Ok(_), _) => true,
        };
        failures.check(ok, || {
            format!("{:?}: model {:?}, error {:?}", c.src, c.model, c.error)
        });
    }
    failures.done("info", 2 * 2000);
}

#[derive(Deserialize)]
struct Renders {
    cases: Vec<Render>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Render {
    src: String,
    text: String,
    font_size: Option<f64>,
    #[serde(default)]
    icons: bool,
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

/// The lines of a scene as the oracle reads Pele's SVG, every number as JavaScript's String
/// writes it. Each label is written as the point it is centered on and what is in it.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let pt = |x: f64, y: f64| format!("{},{}", n(x), n(y));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    if let Some(t) = &sc.title {
        lines.push(format!("acc-title {}", id(t)));
    }
    if let Some(d) = &sc.desc {
        lines.push(format!("acc-desc {}", id(d)));
    }
    for it in &sc.items {
        let p = it.label.as_ref().expect("a label");
        let l = &p.label;
        let class = match it.role.as_str() {
            ROLE_TITLE => "pele-title",
            ROLE_VERSION => "pele-version",
            _ => "?",
        };
        let fill = match it.style.text {
            Some(Paint::FG) => "-",
            Some(Paint::MUTED) => "var(--_m)",
            _ => "?",
        };
        let widths: Vec<String> = l.widths.iter().map(|&w| n(w)).collect();
        let spans: Vec<String> = l
            .lines
            .iter()
            .map(|line| {
                line.iter()
                    .map(|s| match &s.icon {
                        Some(icon) => format!("i:{icon}:{}", n(s.width)),
                        None => format!("t:{}:{}:{}", s.face.bits(), s.text, n(s.width)),
                    })
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .collect();
        lines.push(format!(
            "label {class} {fill} {} {},{},{},{} | {} | {}",
            pt(p.at.x, p.at.y),
            n(l.size),
            n(l.line_height),
            n(l.width),
            n(l.height),
            widths.join(","),
            spans.join(" / ")
        ));
    }
    lines
}

/// What Pele's render draws of info diagrams, read back out of its SVG, against the same
/// lines written of the scene. Pele draws in each case, in place of its own name and
/// version, the text limu draws that the case gives.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("info-render");
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("an info diagram");
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("an info diagram that parses");
        let all = |_: &str| true;
        let o = Options {
            font_size: c.font_size.unwrap_or(16.0),
            icons: if c.icons { Some(&all) } else { None },
            ..Options::default()
        };
        let got = scene_lines(&lay(&m, &o, &c.text));
        let diff = drawings.diff(k, &c.drawing, &got, None);
        failures.check(diff.is_none(), || {
            format!(
                "{:?}, {:?}, size {:?}, icons {}: {}",
                c.src,
                c.text,
                c.font_size,
                c.icons,
                diff.unwrap()
            )
        });
    }
    failures.done("renders", 600);
}

/// layout draws limu's name and version.
#[test]
fn layout_draws_version() {
    let sc = layout(&InfoModel::default(), &Map::new(), &Options::default());
    assert_eq!(sc.items.len(), 1);
    assert_eq!(sc.items[0].role.as_str(), ROLE_VERSION);
    let l = &sc.items[0].label.as_ref().unwrap().label;
    assert_eq!(l.lines[0][0].text, VERSION);
    assert!(VERSION.starts_with("limu v0."));
}
