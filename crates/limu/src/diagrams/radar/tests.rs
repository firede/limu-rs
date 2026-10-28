//! Radar charts read and laid out as Pele reads and draws them, checked against the oracle's
//! radar and radar-render fixtures.

use limu_fixtures::load;
use serde::Deserialize;

use super::model::RadarModel;
use super::parse;
use super::parser::{OptionValue, RADAR_TOKENS, RadarAst, lex_radar, parse_radar};
use super::render::{
    ROLE_AXES, ROLE_AXIS_LABEL, ROLE_CURVE, ROLE_GRATICULE, ROLE_LEGEND, ROLE_TITLE, layout,
};
use crate::detect::DiagramType;
use crate::errors::ErrorCode;
use crate::js::number::to_string as n;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::{Paint, Theme};
use crate::scene::{Item, Op, Outline, Painting, Scene};
use crate::testing::pelesvg::{cap_join, id, own_fill};
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

/// The kinds of token, numbered by hand, are the indexes of their types in RADAR_TOKENS.
#[test]
fn token_kinds() {
    let names: Vec<&str> = RADAR_TOKENS.iter().map(|t| t.name).collect();
    assert_eq!(
        names,
        [
            "radar-beta:",
            "radar-beta",
            "showLegend",
            "graticule",
            "curve",
            "ticks",
            "axis",
            "max",
            "min",
            ":",
            ",",
            "[",
            "]",
            "{",
            "}",
            "GRATICULE",
            "BOOLEAN",
            "ACC_DESCR",
            "ACC_TITLE",
            "TITLE",
            "NUMBER",
            "STRING",
            "ID",
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

fn object(entries: Vec<(&str, Value)>) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn string(v: &str) -> Value {
    Value::String(v.to_string())
}

fn dump_ast(t: &RadarAst) -> Value {
    let axes = t
        .axes
        .iter()
        .map(|a| Value::Array(vec![string(&a.name), enc_str(&a.label)]))
        .collect();
    let curves = t
        .curves
        .iter()
        .map(|c| {
            let entries = c
                .entries
                .iter()
                .map(|e| Value::Array(vec![enc_str(&e.axis), Value::Number(e.value)]))
                .collect();
            Value::Array(vec![
                string(&c.name),
                enc_str(&c.label),
                Value::Array(entries),
            ])
        })
        .collect();
    let options = t
        .options
        .iter()
        .map(|o| {
            let value = match &o.value {
                OptionValue::Bool(b) => Value::Bool(*b),
                OptionValue::Number(v) => Value::Number(*v),
                OptionValue::Word(w) => string(w),
            };
            Value::Array(vec![string(&o.name), value])
        })
        .collect();
    object(vec![
        ("title", enc_str(&t.title)),
        ("accTitle", enc_str(&t.acc_title)),
        ("accDescr", enc_str(&t.acc_descr)),
        ("axes", Value::Array(axes)),
        ("curves", Value::Array(curves)),
        ("options", Value::Array(options)),
    ])
}

/// The model as the oracle writes it, without its type and what is undefined, and with its
/// options in the order Pele declares them.
fn dump_model(m: &RadarModel) -> Value {
    let mut out = Map::new();
    for (k, v) in [
        ("title", &m.title),
        ("accTitle", &m.acc_title),
        ("accDescr", &m.acc_descr),
    ] {
        if let Some(v) = v {
            out.insert(k, string(v));
        }
    }
    let axes = m
        .axes
        .iter()
        .map(|a| Value::Array(vec![string(&a.name), string(&a.label)]))
        .collect();
    let curves = m
        .curves
        .iter()
        .map(|c| {
            let entries = c.entries.iter().map(|&v| Value::Number(v)).collect();
            Value::Array(vec![
                string(&c.name),
                string(&c.label),
                Value::Array(entries),
            ])
        })
        .collect();
    let o = &m.options;
    out.insert("axes", Value::Array(axes));
    out.insert("curves", Value::Array(curves));
    out.insert(
        "options",
        Value::Array(vec![
            Value::Bool(o.show_legend),
            Value::Number(o.ticks),
            o.max.map_or(Value::Null, Value::Number),
            Value::Number(o.min),
            string(&o.graticule),
        ]),
    );
    Value::Object(out)
}

fn dump_tokens(src: &str) -> Option<Vec<(String, String)>> {
    let t = lex_radar(src).ok()?;
    let n = t.kinds.len() - 1;
    Some(
        (0..n)
            .map(|i| {
                let name = RADAR_TOKENS[t.kinds[i]].name.to_string();
                (name, src[t.starts[i]..t.ends[i]].to_string())
            })
            .collect(),
    )
}

#[test]
fn parse_matches_pele() {
    let f: Cases = load("radar");
    assert!(f.cases.len() >= 6000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    let (mut models, mut refused) = (0, 0);
    for c in &f.cases {
        let tokens = dump_tokens(&c.src);
        failures.check(tokens == c.direct.tokens, || {
            format!(
                "tokens of {:?}: {tokens:?}, Pele {:?}",
                c.src, c.direct.tokens
            )
        });
        let tree = parse_radar(&c.src);
        let ok = match (&tree, &c.direct.error, &c.direct.ast) {
            (Err(e), Some(want), _) => want.is(e),
            (Ok(t), None, Some(want)) => same(&sorted(&dump_ast(t)), &sorted(&want.0)),
            _ => false,
        };
        failures.check(ok, || {
            format!(
                "parse_radar({:?}): {tree:?}, Pele {:?} {:?}",
                c.src, c.direct.ast, c.direct.error
            )
        });

        let p = prepare(&c.src, &ParseOptions::default());
        let ok = match (&p, &c.error) {
            (Err(e), Some(want)) => want.is(e),
            (Err(e), None) => e.code == ErrorCode::UnsupportedDiagram && c.diagram.is_none(),
            (Ok(p), _) if c.diagram.as_deref() != Some(p.found.diagram.name()) => false,
            (Ok(p), want) if p.found.diagram == DiagramType::Radar => {
                let got = p.parse_with(false, |s| parse(s, p.title.as_deref()));
                match (&got, want, &c.model) {
                    (Err(e), Some(want), _) => {
                        refused += usize::from(e.code == ErrorCode::Semantic);
                        want.is(e)
                    }
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
    failures.done("radar", 3 * 6000);
    assert!(
        models >= 1500 && refused >= 50,
        "only {models} models and {refused} semantic errors"
    );
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

/// A paint as Pele's SVG names it, with the opacity it is drawn at, "-" for full.
fn paint_name(p: Option<Paint>) -> (String, String) {
    let Some(p) = p else {
        return ("?".into(), "?".into());
    };
    for k in 0..8 {
        for a in [1.0, 0.2, 0.4] {
            if p == Paint::series(k).alpha(a) {
                let opacity = if a == 1.0 { "-".into() } else { n(a) };
                let name = format!("var(--pele-series-{},{})", k + 1, Theme::PELE.series[k]);
                return (name, opacity);
            }
        }
    }
    let name = match p {
        Paint::FG => "var(--_fg)",
        Paint::SURFACE_ALT => "var(--_a)",
        Paint::BORDER => "var(--_b)",
        _ => "?",
    };
    (name.into(), "-".into())
}

/// A scene written as the oracle reads Pele's SVG, every number as JavaScript's String
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
    let pen = |it: &Item, paint: Painting| {
        let (fill, opacity) = if paint == Painting::StrokeOnly {
            ("none".to_string(), "-".to_string())
        } else {
            paint_name(own_fill(it))
        };
        let (stroke, _) = paint_name(it.style.stroke);
        let width = it.style.stroke_width.map_or("-".to_string(), n);
        format!("{fill} {opacity} {stroke} {width} {}", cap_join(it))
    };
    let what = |it: &Item| {
        let name = it.id.as_deref().unwrap_or("");
        match it.role.as_str() {
            ROLE_TITLE => "title -".to_string(),
            ROLE_GRATICULE => "graticule".into(),
            ROLE_AXES => "axes".into(),
            ROLE_CURVE => format!("curve {name}"),
            ROLE_AXIS_LABEL => format!("axis-label {name}"),
            ROLE_LEGEND => format!("legend {name}"),
            role => format!("unknown role {role}"),
        }
    };
    for it in &sc.items {
        for part in &it.parts {
            let path = match &part.outline {
                Outline::Box { rect: r, rounded } => {
                    let radius = if *rounded {
                        format!("r{}", n(it.style.radius.unwrap_or(0.0)))
                    } else {
                        "-".into()
                    };
                    lines.push(format!(
                        "{} rect {},{} {radius} {}",
                        what(it),
                        pt(r.x, r.y),
                        pt(r.w, r.h),
                        pen(it, part.paint)
                    ));
                    continue;
                }
                Outline::Path(p) => p,
            };
            let mut subpaths: Vec<(Vec<String>, Vec<f64>)> = Vec::new();
            let mut at = 0;
            for &op in &path.ops {
                let a = &path.args[at..at + op.args()];
                at += op.args();
                if op == Op::MoveTo {
                    subpaths.push((Vec::new(), Vec::new()));
                }
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
                let last = subpaths.last_mut().expect("a path that starts with a move");
                last.0.push(s);
                last.1.extend_from_slice(a);
            }
            let circle = it.role.as_str() == ROLE_GRATICULE
                && it.shape.as_ref().is_some_and(|s| s.as_str() == "circle");
            for (s, a) in subpaths {
                if circle {
                    // The circle, as Path::ellipse draws it: from its right end, a quarter down
                    // to its bottom first.
                    lines.push(format!(
                        "{} circle {} {} {}",
                        what(it),
                        pt(a[6], a[1]),
                        pt(a[0], a[7]),
                        pen(it, part.paint)
                    ));
                    continue;
                }
                lines.push(format!(
                    "{} path {} {}",
                    what(it),
                    s.join(" "),
                    pen(it, part.paint)
                ));
            }
        }
        if let Some(p) = &it.label {
            let l = &p.label;
            let text: Vec<String> = l
                .lines
                .iter()
                .map(|line| line.iter().map(|s| s.text.as_str()).collect())
                .collect();
            let (fill, _) = paint_name(it.style.text);
            lines.push(format!(
                "{} label {} {} {fill} {} | {}",
                what(it),
                pt(p.at.x, p.at.y),
                pt(l.width, l.height),
                n(l.size),
                text.join(" | ")
            ));
        }
    }
    lines
}

/// What Pele's render draws of radar charts, read back out of its SVG as a line for each
/// thing drawn, in the order drawn, with every number in full, against the same lines
/// written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("radar-render");
    assert!(f.cases.len() >= 3500, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a radar chart");
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("a radar chart that parses");
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
    failures.done("renders", 3500);
}
