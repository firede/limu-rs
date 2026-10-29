//! Timelines read and laid out as Pele reads and draws them, checked against the oracle's
//! timeline and timeline-render fixtures.

use limu_fixtures::load;
use serde::Deserialize;

use super::db::TimelineDb;
use super::lexer::tokenize;
use super::parse;
use super::render::{
    ROLE_AXIS, ROLE_CONNECTOR, ROLE_DOT, ROLE_EVENT, ROLE_PERIOD, ROLE_SECTION, ROLE_TITLE, layout,
};
use crate::detect::DiagramType;
use crate::errors::ErrorCode;
use crate::js::chars::len16;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::{Paint, Theme};
use crate::scene::{Align, Item, Outline, Painting, Part, Path, Scene};
use crate::testing::pelesvg::{id, own_fill};
use crate::testing::{Enc, Failures, PeleError, pele_num, same, sorted};
use crate::value::{Map, Value};

#[derive(Deserialize)]
struct Cases {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    src: String,
    tokens: Vec<(usize, String, usize)>,
    direct: Outcome,
    #[serde(rename = "type")]
    diagram: Option<String>,
    #[serde(flatten)]
    outcome: Outcome,
    /// Whether the pipeline's outcome is the direct one.
    #[serde(default)]
    same: bool,
}

#[derive(Deserialize)]
struct Outcome {
    model: Option<Enc>,
    error: Option<PeleError>,
}

fn dump_tokens(src: &str) -> Vec<(usize, String, usize)> {
    let t = tokenize(src);
    (0..t.types.len())
        .map(|i| (t.types[i], t.texts[i].clone(), len16(&src[..t.starts[i]])))
        .collect()
}

/// The db as JSON.stringify writes it, without its type and what is undefined.
fn dump_model(m: &TimelineDb) -> Value {
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
    out.insert("direction", Value::String(m.direction.into()));
    let strings = |v: &[String]| Value::Array(v.iter().cloned().map(Value::String).collect());
    out.insert("sections", strings(&m.sections));
    let periods = m.periods.iter().map(|p| {
        let mut o = Map::new();
        o.insert("text", Value::String(p.text.clone()));
        o.insert("events", strings(&p.events));
        o.insert("section", Value::String(p.section.clone()));
        o.insert("sectionIndex", Value::Number(p.section_index as f64));
        Value::Object(o)
    });
    out.insert("periods", Value::Array(periods.collect()));
    Value::Object(out)
}

/// Whether a parse's outcome is the one Pele had.
fn matches(got: &Result<TimelineDb, crate::Error>, want: &Outcome) -> bool {
    match (got, &want.error, &want.model) {
        (Err(e), Some(want), _) => want.is(e),
        (Ok(m), None, Some(model)) => same(&sorted(&dump_model(m)), &sorted(&model.0)),
        _ => false,
    }
}

#[test]
fn parse_matches_pele() {
    let f: Cases = load("timeline");
    let mut failures = Failures::new();
    for c in &f.cases {
        let tokens = dump_tokens(&c.src);
        failures.check(tokens == c.tokens, || {
            format!("tokens of {:?}: {tokens:?}, Pele {:?}", c.src, c.tokens)
        });
        let got = parse(&c.src, None);
        failures.check(matches(&got, &c.direct), || {
            format!(
                "parse({:?}): {got:?}, Pele {:?} {:?}",
                c.src, c.direct.model, c.direct.error
            )
        });

        let want = if c.same { &c.direct } else { &c.outcome };
        let p = prepare(&c.src, &ParseOptions::default());
        let ok = match (&p, &want.error) {
            (Err(e), Some(want)) => want.is(e),
            (Err(e), None) => e.code == ErrorCode::UnsupportedDiagram && c.diagram.is_none(),
            (Ok(p), _) if c.diagram.as_deref() != Some(p.found.diagram.name()) => false,
            (Ok(p), _) if p.found.diagram == DiagramType::Timeline => {
                matches(&p.parse_with(false, |s| parse(s, p.title.as_deref())), want)
            }
            (Ok(_), _) => true,
        };
        failures.check(ok, || {
            format!(
                "{:?}: model {:?}, error {:?}",
                c.src, want.model, want.error
            )
        });
    }
    failures.done("timeline", 3 * 5000);
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

/// A paint as Pele's SVG names it, "-" for none given. Text in the theme's color of text
/// takes the fill Pele's SVG gives the whole drawing, and is given none of its own.
fn paint_name(p: Option<Paint>) -> String {
    let Some(p) = p.filter(|&p| p != Paint::FG) else {
        return "-".into();
    };
    if p == Paint::NONE {
        return "none".into();
    }
    let token = p.as_token();
    let mut name = [
        (Paint::BG, "var(--_bg)"),
        (Paint::MUTED, "var(--_m)"),
        (Paint::LINE, "var(--_l)"),
        (Paint::BORDER, "var(--_b)"),
    ]
    .into_iter()
    .find(|(q, _)| token.is_some() && q.as_token() == token)
    .map_or("?".to_string(), |(_, name)| name.to_string());
    for k in 0..8 {
        if token.is_some() && token == Paint::series(k).as_token() {
            name = format!("var(--pele-series-{},{})", k + 1, Theme::PELE.series[k]);
        }
    }
    if p.opacity() != 1.0 {
        name += &format!("/{}", pele_num(f64::from(p.opacity())));
    }
    name
}

/// How a part of an item is painted, as the oracle reads Pele's fill, stroke and stroke
/// width.
fn part_paint(it: &Item, part: &Part) -> String {
    let mut s = match part.paint {
        Painting::StrokeOnly => format!("none {}", paint_name(it.style.stroke)),
        Painting::FillOnly => format!("{} -", paint_name(own_fill(it))),
        _ => format!(
            "{} {}",
            paint_name(own_fill(it)),
            paint_name(it.style.stroke)
        ),
    };
    if let Some(w) = it.style.stroke_width {
        s += &format!(" w{}", pele_num(w));
    }
    s
}

/// Text as Pele's SVG holds it, without the control characters XML does not allow.
fn xml_text(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            !(c < ' ' && !matches!(c, '\t' | '\n' | '\r')
                || matches!(c, '\x7f' | '\u{fffe}' | '\u{ffff}'))
        })
        .collect()
}

fn pt(x: f64, y: f64) -> String {
    format!("{},{}", pele_num(x), pele_num(y))
}

/// The steps of a path, each its letter and its points.
fn subpaths(p: &Path) -> String {
    let mut out = Vec::new();
    let mut at = 0;
    for &op in p.ops() {
        let a = &p.args()[at..at + op.args()];
        at += op.args();
        let mut w = "MLQCZ"[op as usize..op as usize + 1].to_string();
        for xy in a.chunks(2) {
            w += &format!(" {}", pt(xy[0], xy[1]));
        }
        out.push(w);
    }
    out.join(" ")
}

/// Where Pele's text puts the first line of a label centered on its point, its paint, its
/// size, and the text of its lines.
fn text_at(it: &Item) -> String {
    let p = it.label.as_ref().expect("a label");
    let l = &p.label;
    let y = p.at.y - l.height / 2.0 + l.line_height / 2.0 + l.size * 0.35;
    let mut s = format!(
        "t {} {} {}",
        pt(p.at.x, y),
        paint_name(it.style.text),
        pele_num(l.size)
    );
    if p.align != Align::Middle {
        s += " not centered";
    }
    for line in &l.lines {
        let text: String = if line.is_empty() {
            " ".into()
        } else {
            line.iter().map(|span| span.text.as_str()).collect()
        };
        s += &format!(" | {}", xml_text(&text));
    }
    s
}

/// The path of an item's first part.
fn path_of(it: &Item) -> &Path {
    match &it.parts[0].outline {
        Outline::Path(p) => p,
        Outline::Box { .. } => panic!("a box where a path was"),
    }
}

/// The lines of a scene as the oracle reads Pele's SVG.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    let rect = |lines: &mut Vec<String>, it: &Item| {
        let part = &it.parts[0];
        let Outline::Box { rect: r, rounded } = part.outline else {
            lines.push("a path where a box was".into());
            return;
        };
        let mut s = format!("rect {},{}", pt(r.x, r.y), pt(r.w, r.h));
        if rounded {
            s += " r";
        }
        lines.push(format!("{s} {}", part_paint(it, part)));
    };
    for it in &sc.items {
        match it.role.as_str() {
            ROLE_TITLE => lines.push(format!("title {}", text_at(it))),
            ROLE_AXIS => {
                lines.push(format!(
                    "pele-axis {} {}",
                    subpaths(path_of(it)),
                    part_paint(it, &it.parts[0])
                ));
                let ends = it.edge().map_or(&[None, None], |e| &e.ends);
                for m in ends.iter().flatten() {
                    for part in m.outline() {
                        let Outline::Path(p) = &part.outline else {
                            continue;
                        };
                        lines.push(format!(
                            "pele-marker {} {} none",
                            subpaths(p),
                            paint_name(it.style.stroke)
                        ));
                    }
                }
            }
            role @ (ROLE_SECTION | ROLE_PERIOD) => {
                let name = role.trim_start_matches("timeline.");
                let data = it.id.as_deref().unwrap_or("");
                lines.push(format!("{name} {}", xml_text(&id(data))));
                rect(&mut lines, it);
                lines.push(text_at(it));
            }
            ROLE_EVENT => {
                lines.push("event".into());
                rect(&mut lines, it);
                lines.push(text_at(it));
            }
            ROLE_CONNECTOR => lines.push(format!(
                "pele-edge {} {}",
                subpaths(path_of(it)),
                part_paint(it, &it.parts[0])
            )),
            ROLE_DOT => {
                // The circle, as Path::ellipse draws it: from its right end, four quarters.
                let a = &path_of(it).args();
                let (cx, cy) = (a[2 + 4], a[1]);
                lines.push(format!(
                    "circle {} {} {}",
                    pt(cx, cy),
                    pele_num(a[0] - cx),
                    part_paint(it, &it.parts[0])
                ));
            }
            role => lines.push(format!("unknown role {role}")),
        }
    }
    lines
}

/// What Pele's render draws of timelines, read back out of its SVG as a line for each thing
/// drawn, in the order drawn, against the same lines written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("timeline-render");
    assert!(f.cases.len() >= 4000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a timeline");
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("a timeline that parses");
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
