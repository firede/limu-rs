//! XY charts parsed and laid out against what Pele makes of the same cases, in the oracle's
//! xychart fixtures. The tokens and the model are written as the oracle writes Pele's, field
//! by field, so that any field that differs shows. The scene is written as the oracle reads
//! Pele's SVG, a line for each thing drawn, its numbers in full.

use limu_fixtures::load;
use serde::Deserialize;

use super::db::{PlotType, XAxis, XyChartDb};
use super::lexer::tokenize;
use super::parser::parse_xy_chart;
use super::render::{
    ROLE_AXIS, ROLE_AXIS_LABEL, ROLE_AXIS_TITLE, ROLE_BARS, ROLE_DATA_LABEL, ROLE_GRID,
    ROLE_LEGEND, ROLE_LINE, ROLE_POINT_LABEL, ROLE_TITLE,
};
use super::{layout, parse};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::len16;
use crate::js::number::to_string;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::{Paint, Theme};
use crate::scene::text::Face;
use crate::scene::{Align, Item, Op, Outline, Painting, Part, Path, Scene};
use crate::testing::pelesvg::id;
use crate::testing::{Enc, Failures, PeleError, pele_num, same, sorted};
use crate::value::{Map, Value};

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    src: String,
    #[serde(rename = "type")]
    diagram: Option<String>,
    /// The tokens of the text as it is.
    tokens: Enc,
    #[serde(flatten)]
    result: Wanted,
    direct: Option<Wanted>,
    /// Whether what the pipeline makes of the text is what the parser makes of it as it is.
    #[serde(default)]
    same: bool,
}

/// What Pele made of a text: its model or its error.
#[derive(Deserialize)]
struct Wanted {
    model: Option<Enc>,
    error: Option<PeleError>,
}

/// The tokens as the oracle's dumpTokens writes them: each one's type by its number, its
/// text, and where it starts, in UTF-16 code units.
fn dump_tokens(src: &str) -> Value {
    let toks = tokenize(src).into_iter().map(|t| {
        Value::Array(vec![
            Value::Number(f64::from(t.kind as u8)),
            Value::String(t.text.into()),
            Value::Number(len16(&src[..t.start]) as f64),
        ])
    });
    Value::Array(toks.collect())
}

fn object(entries: Vec<(&str, Value)>) -> Value {
    let mut m = Map::new();
    for (k, v) in entries {
        m.insert(k, v);
    }
    Value::Object(m)
}

fn string(s: &str) -> Value {
    Value::String(s.into())
}

/// The model as the oracle's dumpModel writes it, its keys sorted, as are the fixture's for
/// the comparison.
fn dump_model(m: &XyChartDb) -> Value {
    let x_axis = match &m.x_axis {
        XAxis::Band(a) => object(vec![
            ("type", string("band")),
            ("title", string(&a.title)),
            (
                "categories",
                Value::Array(a.categories.iter().map(|c| string(c)).collect()),
            ),
        ]),
        XAxis::Linear(a) => object(vec![
            ("type", string("linear")),
            ("title", string(&a.title)),
            ("min", Value::Number(a.min)),
            ("max", Value::Number(a.max)),
        ]),
    };
    let y_axis = object(vec![
        ("type", string("linear")),
        ("title", string(&m.y_axis.title)),
        ("min", Value::Number(m.y_axis.min)),
        ("max", Value::Number(m.y_axis.max)),
    ]);
    let plots = m.plots.iter().map(|p| {
        let data = p.data.iter().map(|(x, v)| {
            let v = v.map_or_else(|| object(vec![("$", string("undefined"))]), Value::Number);
            Value::Array(vec![string(x), v])
        });
        let labels = p.point_labels.as_ref().map_or(Value::Null, |l| {
            Value::Array(l.iter().map(|s| string(s)).collect())
        });
        let kind = match p.kind {
            PlotType::Line => "line",
            PlotType::Bar => "bar",
        };
        object(vec![
            ("type", string(kind)),
            ("title", string(&p.title)),
            ("data", Value::Array(data.collect())),
            ("pointLabels", labels),
        ])
    });
    let mut o = vec![
        ("orientation", m.orientation.map_or(Value::Null, string)),
        ("xAxis", x_axis),
        ("yAxis", y_axis),
        ("hasSetXAxis", Value::Bool(m.has_set_x_axis)),
        ("hasSetYAxis", Value::Bool(m.has_set_y_axis)),
        ("plots", Value::Array(plots.collect())),
    ];
    let opt = [
        ("title", &m.title),
        ("accTitle", &m.acc_title),
        ("accDescr", &m.acc_descr),
    ];
    for (key, v) in opt {
        if let Some(v) = v.as_deref().filter(|v| !v.is_empty()) {
            o.push((key, string(v)));
        }
    }
    sorted(&object(o))
}

/// Whether a parse is what Pele made: the same error, or the same model. It says how they
/// differ where they do.
fn compare(got: Result<XyChartDb, Error>, want: &Wanted) -> Result<(), String> {
    match (got, &want.error) {
        (Err(e), Some(w)) if w.is(&e) => Ok(()),
        (Err(e), Some(w)) => Err(format!("error {e:?}\nPele {w:?}")),
        (Err(e), None) => Err(format!("error {e:?}, Pele had none")),
        (Ok(_), Some(w)) => Err(format!("no error, Pele {w:?}")),
        (Ok(m), None) => {
            let got = dump_model(&m);
            let wm = want.model.as_ref().map(|e| sorted(&e.0));
            if !wm.as_ref().is_some_and(|w| same(&got, w)) {
                return Err(format!("model {got:?}\nPele {wm:?}"));
            }
            Ok(())
        }
    }
}

/// A case as the oracle runs it: the tokens of the text, the parser given it as it is, and
/// Pele's parseSource for an XY chart. It is whether the text was an XY chart that parsed.
fn run(c: &Case) -> Result<bool, String> {
    let tokens = dump_tokens(&c.src);
    if !same(&tokens, &c.tokens.0) {
        return Err(format!("tokens {tokens:?}\nPele {:?}", c.tokens.0));
    }
    let d = c.direct.as_ref().ok_or("no direct result")?;
    let mut db = XyChartDb::default();
    let got = parse_xy_chart(&c.src, &mut db).map(|_| db);
    compare(got, d).map_err(|e| format!("direct {e}"))?;
    let want = if c.same { d } else { &c.result };
    let p = match prepare(&c.src, &ParseOptions::default()) {
        Ok(p) => p,
        // Pele's suite stops where no type is found, with no error.
        Err(e)
            if e.code == ErrorCode::UnsupportedDiagram
                && want.error.is_none()
                && c.diagram.is_none() =>
        {
            return Ok(false);
        }
        Err(e) => {
            return match &want.error {
                Some(w) if w.is(&e) => Ok(false),
                w => Err(format!("error {e:?}\nPele {w:?}")),
            };
        }
    };
    let ty = p.found.diagram.name();
    if c.diagram.as_deref() != Some(ty) {
        return Err(format!("type {ty}, Pele {:?}", c.diagram));
    }
    if p.found.diagram != DiagramType::XyChart {
        return Ok(false);
    }
    let got = p.parse_with(false, |s| parse(s, p.title.as_deref()));
    let parsed = got.is_ok();
    compare(got, want)?;
    Ok(parsed)
}

#[test]
fn xy_charts_match_pele() {
    let f: Fixture = load("xychart");
    let mut failures = Failures::new();
    let mut parsed = 0;
    for c in &f.cases {
        let got = run(c);
        if got == Ok(true) {
            parsed += 1;
        }
        failures.check(got.is_ok(), || format!("{:?}: {}", c.src, got.unwrap_err()));
    }
    failures.done("XY charts", 7000);
    assert!(parsed >= 1500, "only {parsed} XY charts parsed");
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
        Paint::MUTED => "var(--_m)",
        Paint::SURFACE_ALT => "var(--_a)",
        Paint::BORDER => "var(--_b)",
        _ => "?",
    }
    .into()
}

/// Whether a label is one line of one span of plain text.
fn plain(it: &Item) -> bool {
    let l = &it.label.as_ref().expect("a label").label;
    l.lines.len() == 1
        && l.lines[0].len() == 1
        && l.lines[0][0].icon.is_none()
        && l.lines[0][0].face == Face::REGULAR
}

/// Writes a scene as the oracle reads Pele's SVG of an XY chart, every number as
/// JavaScript's String writes it.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let n = to_string;
    let pt = |x: f64, y: f64| format!("{},{}", n(x), n(y));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    if let Some(t) = sc.title.as_deref().filter(|t| !t.is_empty()) {
        lines.push(format!("acc-title {}", id(t)));
    }
    if let Some(d) = sc.desc.as_deref().filter(|d| !d.is_empty()) {
        lines.push(format!("acc-desc {}", id(d)));
    }
    let context = |it: &Item| match it.role.as_str() {
        ROLE_TITLE => "title",
        ROLE_AXIS_LABEL => "axis-labels",
        ROLE_AXIS_TITLE => "axis-titles",
        ROLE_DATA_LABEL => "data-labels",
        ROLE_POINT_LABEL => "point-labels",
        _ => "legend",
    };
    // Where Pele's text puts the first line of a label, with its fill, anchor and size and
    // the text of its lines: a plain one-line label that Pele's aligned writes anchored at
    // its point, and any other as Pele's labelSvg centers it.
    let text_at = |lines: &mut Vec<String>, it: &Item| {
        let p = it.label.as_ref().expect("a label");
        let l = &p.label;
        let (mut x, mut y) = (p.at.x, p.at.y);
        if p.rotate != 0.0 {
            lines.push(format!("rotate {} {}", pt(x, y), n(p.rotate)));
            (x, y) = (0.0, 0.0);
        }
        let mut anchor = "middle";
        let aligned = matches!(
            it.role.as_str(),
            ROLE_AXIS_LABEL | ROLE_POINT_LABEL | ROLE_DATA_LABEL
        );
        if aligned && plain(it) {
            anchor = match p.align {
                Align::Middle => "middle",
                Align::Start => "start",
                Align::End => "end",
            };
            y += l.size * 0.35;
        } else {
            y = y - l.height / 2.0 + l.line_height / 2.0 + l.size * 0.35;
        }
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
        lines.push(format!(
            "{} t {} {} {anchor} {} | {}",
            context(it),
            pt(x, y),
            paint_name(it.style.text),
            n(l.size),
            content.join(" | ")
        ));
    };
    let pen = |it: &Item| {
        let s = it.author.over(&it.style);
        let width = s.stroke_width.map_or("-".to_string(), n);
        let cap = s.cap.map_or("-".to_string(), |c| c.to_string());
        let join = s.join.map_or("-".to_string(), |j| j.to_string());
        format!("{} {width} {cap} {join}", paint_name(s.stroke))
    };
    let path = |lines: &mut Vec<String>, it: &Item, context: &str, p: &Path| {
        let mut subpaths: Vec<String> = Vec::new();
        let mut at = 0;
        for &op in &p.ops {
            let a = &p.args[at..at + op.args()];
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
                s.push_str(&format!(" {}", pt(xy[0], xy[1])));
            }
            match subpaths.last_mut() {
                Some(last) if op != Op::MoveTo => {
                    last.push(' ');
                    last.push_str(&s);
                }
                _ => subpaths.push(s),
            }
        }
        for s in subpaths {
            lines.push(format!("{context} path {s} {}", pen(it)));
        }
    };
    let rect = |lines: &mut Vec<String>, it: &Item, part: &Part| {
        let Outline::Box { rect: r, rounded } = part.outline else {
            lines.push("rect of a path".into());
            return;
        };
        let radius = if rounded {
            format!("r{}", n(it.style.radius.unwrap_or(0.0)))
        } else {
            "-".into()
        };
        lines.push(format!(
            "rect {},{} {radius} {}",
            pt(r.x, r.y),
            pt(r.w, r.h),
            paint_name(it.style.fill)
        ));
    };
    let path_of = |part: &Part| match &part.outline {
        Outline::Path(p) => Some(p.clone()),
        Outline::Box { .. } => None,
    };
    let series_id = |it: &Item| {
        it.id
            .as_deref()
            .filter(|t| !t.is_empty())
            .map_or("-".into(), id)
    };
    for it in &sc.items {
        match it.role.as_str() {
            ROLE_TITLE | ROLE_AXIS_LABEL | ROLE_AXIS_TITLE | ROLE_DATA_LABEL | ROLE_POINT_LABEL => {
                text_at(&mut lines, it)
            }
            ROLE_GRID | ROLE_AXIS => {
                let context = if it.role.as_str() == ROLE_GRID {
                    "grid"
                } else {
                    "axis"
                };
                let p = path_of(&it.parts[0]).expect("a path");
                path(&mut lines, it, context, &p);
            }
            ROLE_BARS => {
                lines.push(format!(
                    "bars {} {}",
                    series_id(it),
                    paint_name(it.style.fill)
                ));
                for part in &it.parts {
                    rect(&mut lines, it, part);
                }
            }
            ROLE_LINE => {
                lines.push(format!(
                    "line {} {}",
                    series_id(it),
                    paint_name(it.style.fill)
                ));
                for part in &it.parts {
                    let p = path_of(part).expect("a path");
                    if part.paint == Painting::StrokeOnly {
                        path(&mut lines, it, "line", &p);
                        continue;
                    }
                    // The circle, as Path::ellipse draws it: from its right end, four
                    // quarters. Its first quarter ends below the middle.
                    let (cx, cy) = (p.args[6], p.args[1]);
                    lines.push(format!(
                        "circle {} {} {}",
                        pt(cx, cy),
                        pele_num(p.args[0] - cx),
                        paint_name(it.style.fill)
                    ));
                }
            }
            ROLE_LEGEND => {
                lines.push(format!("legend {}", id(it.id.as_deref().unwrap_or(""))));
                let part = &it.parts[0];
                match path_of(part) {
                    Some(p) => path(&mut lines, it, "legend", &p),
                    None => rect(&mut lines, it, part),
                }
                if it.label.is_some() {
                    text_at(&mut lines, it);
                }
            }
            role => lines.push(format!("unknown role {role}")),
        }
    }
    lines
}

/// What Pele's render draws of XY charts, read back out of its SVG as a line for each thing
/// drawn, in the order drawn, and the errors of the charts it refuses, against the same lines
/// written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("xychart-render");
    let mut failures = Failures::new();
    let mut refused = 0;
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("an XY chart");
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("an XY chart that parses");
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            font_size: c.font_size.unwrap_or(16.0),
            ..Options::default()
        };
        let got = match layout(&m, &p.config, &o) {
            Ok(sc) => scene_lines(&sc),
            Err(e) => {
                refused += 1;
                vec![format!("error {} {}", e.code.as_str(), e.message)]
            }
        };
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
    assert!(refused >= 20, "only {refused} charts refused");
}

/// A chart with no series parses, and its layout is refused with an error of its type, as
/// Pele's render refuses it.
#[test]
fn a_chart_with_no_series_is_refused() {
    let d = crate::parse("xychart\n  title \"Sales\"", &ParseOptions::default())
        .expect("an XY chart that parses");
    let e = d.layout(&Options::default()).expect_err("no series");
    assert_eq!(e.code, ErrorCode::Semantic);
    assert_eq!(
        e.message,
        "No Plot to render, please provide a plot with some data"
    );
    assert_eq!(e.diagram, Some(DiagramType::XyChart));
    assert_eq!(e.mermaid_type, Some("xychart"));
}
