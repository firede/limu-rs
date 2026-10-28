//! Pie charts read and laid out as Pele reads and draws them, checked against the oracle's
//! pie and pie-render fixtures.

use limu_fixtures::load;
use serde::Deserialize;

use super::parser::{PIE_TOKENS, PieAst, parse_pie, table};
use super::render::{ROLE_LEGEND, ROLE_SLICE, ROLE_SLICE_LABEL, ROLE_TITLE, layout};
use super::{PieModel, parse};
use crate::common::tokens::tokenize;
use crate::detect::DiagramType;
use crate::errors::ErrorCode;
use crate::js::number::to_string as n;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::{Paint, Theme};
use crate::scene::{Item, Op, Outline, Painting, Path, Point, Scene};
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

/// The kinds of token, numbered by hand, are the indexes of their types in PIE_TOKENS.
#[test]
fn token_kinds() {
    let names: Vec<&str> = PIE_TOKENS.iter().map(|t| t.name).collect();
    assert_eq!(
        names,
        [
            "showData",
            "pie",
            ":",
            "NUMBER_PIE",
            "ACC_DESCR",
            "ACC_TITLE",
            "TITLE",
            "STRING",
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

fn dump_sections(sections: &[(String, f64)]) -> Value {
    Value::Array(
        sections
            .iter()
            .map(|(label, value)| {
                Value::Array(vec![Value::String(label.clone()), Value::Number(*value)])
            })
            .collect(),
    )
}

fn dump_ast(t: &PieAst) -> Value {
    object(vec![
        ("showData", Value::Bool(t.show_data)),
        ("title", enc_str(&t.title)),
        ("accTitle", enc_str(&t.acc_title)),
        ("accDescr", enc_str(&t.acc_descr)),
        ("sections", dump_sections(&t.sections)),
    ])
}

/// The model as JSON.stringify writes it, without its type and what is undefined.
fn dump_model(m: &PieModel) -> Value {
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
    out.insert("showData", Value::Bool(m.show_data));
    out.insert("sections", dump_sections(&m.sections));
    Value::Object(out)
}

fn dump_tokens(src: &str) -> Option<Vec<(String, String)>> {
    let t = tokenize(src, table(), DiagramType::Pie).ok()?;
    let n = t.kinds.len() - 1;
    Some(
        (0..n)
            .map(|i| {
                let name = PIE_TOKENS[t.kinds[i]].name.to_string();
                (name, src[t.starts[i]..t.ends[i]].to_string())
            })
            .collect(),
    )
}

#[test]
fn parse_matches_pele() {
    let f: Cases = load("pie");
    let mut failures = Failures::new();
    for c in &f.cases {
        let tokens = dump_tokens(&c.src);
        failures.check(tokens == c.direct.tokens, || {
            format!(
                "tokens of {:?}: {tokens:?}, Pele {:?}",
                c.src, c.direct.tokens
            )
        });
        let tree = parse_pie(&c.src);
        let ok = match (&tree, &c.direct.error, &c.direct.ast) {
            (Err(e), Some(want), _) => want.is(e),
            (Ok(t), None, Some(want)) => same(&sorted(&dump_ast(t)), &sorted(&want.0)),
            _ => false,
        };
        failures.check(ok, || {
            format!(
                "parse_pie({:?}): {tree:?}, Pele {:?} {:?}",
                c.src, c.direct.ast, c.direct.error
            )
        });

        let p = prepare(&c.src, &ParseOptions::default());
        let ok = match (&p, &c.error) {
            (Err(e), Some(want)) => want.is(e),
            (Err(e), None) => e.code == ErrorCode::UnsupportedDiagram && c.diagram.is_none(),
            (Ok(p), _) if c.diagram.as_deref() != Some(p.found.diagram.name()) => false,
            (Ok(p), want) if p.found.diagram == DiagramType::Pie => {
                let got = p.parse_with(false, |s| parse(s, p.title.as_deref()));
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
    failures.done("pie", 3 * 4000);
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
    padding: Option<f64>,
    font_size: Option<f64>,
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

/// A paint as Pele's SVG names it, "-" for none given.
fn paint_name(p: Option<Paint>) -> String {
    let Some(p) = p.filter(|&p| p != Paint::FG) else {
        return "-".into();
    };
    for k in 0..8 {
        if p == Paint::series(k) {
            return format!("var(--pele-series-{},{})", k + 1, Theme::PELE.series[k]);
        }
    }
    match p {
        Paint::BG => "var(--_bg)",
        Paint::BORDER => "var(--_b)",
        Paint::NONE => "none",
        _ => "?",
    }
    .into()
}

/// An arc of Pele's path, as the oracle writes it: its radii, turn and flags, and where it
/// ends, with the text of its numbers.
struct Arc {
    rx: f64,
    ry: f64,
    rot: f64,
    large: bool,
    sweep: bool,
    to: Point,
    text: String,
}

/// The arcs of a line of Pele's, in order.
fn arcs(line: &str) -> Vec<Arc> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let mut out = Vec::new();
    for (i, &f) in fields.iter().enumerate() {
        if f != "A" || i + 2 >= fields.len() {
            continue;
        }
        let a: Vec<f64> = fields[i + 1]
            .split(',')
            .chain(fields[i + 2].split(','))
            .map(|s| s.parse().unwrap_or(0.0))
            .collect();
        if a.len() != 7 {
            continue;
        }
        out.push(Arc {
            rx: a[0],
            ry: a[1],
            rot: a[2],
            large: a[3] == 1.0,
            sweep: a[4] == 1.0,
            to: Point::new(a[5], a[6]),
            text: fields[i + 1].to_string(),
        });
    }
    out
}

/// A path as the oracle reads Pele's. The curves an arc was drawn with are written as the arc
/// of Pele's line at the same place, where arc_to draws that arc with exactly those curves.
/// Elsewhere they are written as curves, which no line of Pele's has.
fn path_line(p: &Path, mut hints: &[Arc]) -> String {
    let pt = |x: f64, y: f64| format!("{},{}", n(x), n(y));
    let mut out: Vec<String> = Vec::new();
    let mut last = Point::new(0.0, 0.0);
    let (ops, args) = (&p.ops, &p.args);
    let (mut i, mut at) = (0, 0);
    while i < ops.len() {
        let op = ops[i];
        if op != Op::CubeTo {
            let a = &args[at..at + op.args()];
            match op {
                Op::MoveTo => out.push(format!("M {}", pt(a[0], a[1]))),
                Op::LineTo => out.push(format!("L {}", pt(a[0], a[1]))),
                Op::Close => out.push("Z".into()),
                _ => out.push("?".into()),
            }
            if matches!(op, Op::MoveTo | Op::LineTo) {
                last = Point::new(a[0], a[1]);
            }
            i += 1;
            at += op.args();
            continue;
        }
        // A run of curves: the arc of the hint at this place, if arc_to draws it so. The run
        // may hold several arcs, and each hint takes the curves up to the first that ends
        // where its arc does, if arc_to draws the arc with exactly those.
        let (mut j, mut end) = (i, at);
        while j < ops.len() && ops[j] == Op::CubeTo {
            end += 6;
            j += 1;
        }
        let mut curves = &args[at..end];
        while !curves.is_empty() {
            let mut taken = 0;
            if let Some((h, rest)) = hints.split_first() {
                hints = rest;
                let mut k = 6;
                while k <= curves.len() {
                    if curves[k - 2] != h.to.x || curves[k - 1] != h.to.y {
                        k += 6;
                        continue;
                    }
                    let mut q = Path::new();
                    q.move_to(last.x, last.y)
                        .arc_to(h.rx, h.ry, h.rot, h.large, h.sweep, h.to.x, h.to.y);
                    if q.args[2..] == curves[..k] && !q.ops[1..].contains(&Op::LineTo) {
                        out.push(format!("A {} {}", h.text, pt(h.to.x, h.to.y)));
                        taken = k;
                    }
                    break;
                }
            }
            if taken == 0 {
                let mut s = String::from("C");
                for xy in curves.chunks(2) {
                    s.push_str(&format!(" {}", pt(xy[0], xy[1])));
                }
                out.push(s);
                taken = curves.len();
            }
            last = Point::new(curves[taken - 2], curves[taken - 1]);
            curves = &curves[taken..];
        }
        (i, at) = (j, end);
    }
    out.join(" ")
}

/// The lines of a scene as the oracle reads Pele's SVG, every number as JavaScript's String
/// writes it. want is the line Pele drew at an index, or "" where the fixture does not hold
/// it. The oracle keeps the lines of paths and circles in full.
fn scene_lines(sc: &Scene, want: &dyn Fn(usize) -> String) -> Vec<String> {
    let pt = |x: f64, y: f64| format!("{},{}", n(x), n(y));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    if let Some(t) = &sc.title {
        lines.push(format!("acc-title {}", id(t)));
    }
    if let Some(d) = &sc.desc {
        lines.push(format!("acc-desc {}", id(d)));
    }
    let context = |it: &Item| match it.role.as_str() {
        ROLE_TITLE => "title",
        ROLE_SLICE_LABEL => "slice-labels",
        _ => "legend",
    };
    // Where Pele's text puts the first line of a label, with its fill, anchor and size and
    // the text of its lines: a percentage at its point, and any other as Pele's labelSvg
    // centers it.
    let text_at = |it: &Item| {
        let p = it.label.as_ref().expect("a label");
        let l = &p.label;
        let x = p.at.x;
        let y = if it.role.as_str() == ROLE_SLICE_LABEL {
            p.at.y + l.size * 0.35
        } else {
            p.at.y - l.height / 2.0 + l.line_height / 2.0 + l.size * 0.35
        };
        let content: Vec<String> = l
            .lines
            .iter()
            .map(|line| {
                if line.is_empty() {
                    return id(" ");
                }
                id(&line.iter().map(|s| s.text.as_str()).collect::<String>())
            })
            .collect();
        format!(
            "{} t {} {} middle {} | {}",
            context(it),
            pt(x, y),
            paint_name(it.style.text),
            n(l.size),
            content.join(" | ")
        )
    };
    let pen = |it: &Item| {
        let width = it.style.stroke_width.map_or("-".to_string(), n);
        format!("{} {width} {}", paint_name(it.style.stroke), cap_join(it))
    };
    for it in &sc.items {
        match it.role.as_str() {
            ROLE_TITLE | ROLE_SLICE_LABEL => lines.push(text_at(it)),
            ROLE_SLICE => {
                let part = &it.parts[0];
                let Outline::Path(path) = &part.outline else {
                    lines.push("slice of a box".into());
                    continue;
                };
                if part.paint == Painting::StrokeOnly {
                    // The circle of a pie with no slice, as Path::ellipse draws it about the
                    // pie's center, with the radius of Pele's circle where that draws exactly
                    // it. The center is where the ellipse's first curve ends across and where
                    // it starts down.
                    let (cx, cy) = (path.args[6], path.args[1]);
                    let hint = want(lines.len());
                    let f: Vec<&str> = hint.split_whitespace().collect();
                    let mut r = "?".to_string();
                    if f.len() > 2
                        && f[0] == "circle"
                        && let Ok(v) = f[2].parse::<f64>()
                    {
                        let mut q = Path::new();
                        q.ellipse(cx, cy, v, v);
                        if q.args == path.args {
                            r = n(v);
                        }
                    }
                    lines.push(format!("circle {} {r} none {}", pt(cx, cy), pen(it)));
                    continue;
                }
                let name = id(it.id.as_deref().unwrap_or(""));
                lines.push(format!(
                    "slice {name} {} {}",
                    paint_name(it.style.fill),
                    pen(it)
                ));
                let hints = arcs(&want(lines.len()));
                lines.push(format!("path {}", path_line(path, &hints)));
            }
            ROLE_LEGEND => {
                lines.push(format!("legend {}", id(it.id.as_deref().unwrap_or(""))));
                let Outline::Box { rect: r, .. } = it.parts[0].outline else {
                    lines.push("swatch of a path".into());
                    continue;
                };
                lines.push(format!(
                    "rect {},{} r{} {}",
                    pt(r.x, r.y),
                    pt(r.w, r.h),
                    n(it.style.radius.unwrap_or(0.0)),
                    paint_name(it.style.fill)
                ));
                if it.label.is_some() {
                    lines.push(text_at(it));
                }
            }
            role => lines.push(format!("unknown role {role}")),
        }
    }
    lines
}

/// What Pele's render draws of pie charts, read back out of its SVG as a line for each thing
/// drawn, in the order drawn, with every number in full and moved to where the scene holds
/// it, against the same lines written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("pie-render");
    assert!(f.cases.len() >= 4000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    let (mut outside, mut empty) = (0, 0);
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a pie chart");
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("a pie chart that parses");
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            padding: c.padding,
            font_size: c.font_size.unwrap_or(16.0),
            ..Options::default()
        };
        let sc = layout(&m, &p.config, &o);
        let got = scene_lines(&sc, &|i| c.drawing.line(i).to_string());
        let diff = drawings.diff(k, &c.drawing, &got, None);
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
        if sc
            .items
            .iter()
            .any(|it| it.role.as_str() == ROLE_SLICE_LABEL && it.style.text == Some(Paint::FG))
        {
            outside += 1;
        }
        if sc
            .items
            .iter()
            .any(|it| it.role.as_str() == ROLE_SLICE && it.id.is_none())
        {
            empty += 1;
        }
    }
    failures.done("renders", 4000);
    assert!(
        outside >= 200 && empty >= 50,
        "only {outside} pies with percentages outside, {empty} with no slice"
    );
}
