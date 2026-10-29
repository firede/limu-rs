//! User journeys read and laid out as Pele reads and draws them, checked against the
//! oracle's journey and journey-render fixtures.

use limu_fixtures::load;
use serde::Deserialize;

use super::db::JourneyDb;
use super::lexer::{TOKEN_NAMES, tokenize};
use super::parse;
use super::render::{
    ROLE_ACTOR, ROLE_FACE, ROLE_GRID, ROLE_LEGEND, ROLE_LINE, ROLE_SCALE, ROLE_SCORE, ROLE_SECTION,
    ROLE_SECTION_LABEL, ROLE_STEM, ROLE_TASK, ROLE_TITLE, layout,
};
use crate::detect::DiagramType;
use crate::errors::ErrorCode;
use crate::js::chars::len16;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::{Cap, Join, Paint};
use crate::scene::{Align, Item, Op, Outline, Painting, Part, Path, Scene};
use crate::testing::pelesvg::id;
use crate::testing::{Enc, Failures, PeleError, pele_num, same, sorted};
use crate::value::{Map, Value};

#[derive(Deserialize)]
struct Cases {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    src: String,
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
    tokens: Option<Vec<(String, String, usize)>>,
    model: Option<Enc>,
    error: Option<PeleError>,
}

fn dump_tokens(src: &str) -> Vec<(String, String, usize)> {
    let t = tokenize(src);
    (0..t.types.len())
        .map(|i| {
            let name = TOKEN_NAMES[t.types[i]].to_string();
            (name, t.texts[i].clone(), len16(&src[..t.starts[i]]))
        })
        .collect()
}

/// The db as JSON.stringify writes it, without its type and what is undefined.
fn dump_model(m: &JourneyDb) -> Value {
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
    let strings = |v: &[String]| Value::Array(v.iter().cloned().map(Value::String).collect());
    out.insert("sections", strings(&m.sections));
    let tasks = m.tasks.iter().map(|t| {
        let mut o = Map::new();
        o.insert("task", Value::String(t.task.clone()));
        o.insert("score", Value::Number(t.score));
        o.insert("people", strings(&t.people));
        o.insert("section", Value::String(t.section.clone()));
        Value::Object(o)
    });
    out.insert("tasks", Value::Array(tasks.collect()));
    Value::Object(out)
}

/// Whether the tokens of the text a parser read, and its outcome, are those Pele had.
fn matches(text: &str, got: &Result<JourneyDb, crate::Error>, want: &Outcome) -> bool {
    if want.tokens.as_ref() != Some(&dump_tokens(text)) {
        return false;
    }
    match (got, &want.error, &want.model) {
        (Err(e), Some(want), _) => want.is(e),
        (Ok(m), None, Some(model)) => same(&sorted(&dump_model(m)), &sorted(&model.0)),
        _ => false,
    }
}

#[test]
fn parse_matches_pele() {
    let f: Cases = load("journey");
    let mut failures = Failures::new();
    let mut piped = 0;
    for c in &f.cases {
        let got = parse(&c.src, None);
        failures.check(matches(&c.src, &got, &c.direct), || {
            format!(
                "parse({:?}): {:?} {got:?}, Pele {:?} {:?} {:?}",
                c.src,
                dump_tokens(&c.src),
                c.direct.tokens,
                c.direct.model,
                c.direct.error
            )
        });

        let want = if c.same { &c.direct } else { &c.outcome };
        let p = prepare(&c.src, &ParseOptions::default());
        let ok = match (&p, &want.error) {
            (Err(e), Some(want)) => want.is(e),
            (Err(e), None) => e.code == ErrorCode::UnsupportedDiagram && c.diagram.is_none(),
            (Ok(p), _) if c.diagram.as_deref() != Some(p.found.diagram.name()) => false,
            (Ok(p), _) if p.found.diagram == DiagramType::Journey => {
                piped += 1;
                let mut read = String::new();
                let got = p.parse_with(false, |s| {
                    read = s.to_string();
                    parse(s, p.title.as_deref())
                });
                matches(&read, &got, want)
            }
            (Ok(_), _) => true,
        };
        failures.check(ok, || {
            format!(
                "{:?}: tokens {:?}, model {:?}, error {:?}",
                c.src, want.tokens, want.model, want.error
            )
        });
    }
    assert!(piped >= 3000, "only {piped} cases were journeys");
    failures.done("journey", 2 * 6000);
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
    let Some(p) = p else { return "-".into() };
    for k in 0..8 {
        if p == Paint::series(k) {
            return format!("series-{}", k + 1);
        }
    }
    [
        (Paint::BG, "var(--_bg)"),
        (Paint::MUTED, "var(--_m)"),
        (Paint::LINE, "var(--_l)"),
        (Paint::SURFACE, "var(--_s)"),
        (Paint::SURFACE_ALT, "var(--_a)"),
        (Paint::BORDER, "var(--_b)"),
    ]
    .into_iter()
    .find(|&(q, _)| q == p)
    .map_or("?", |(_, name)| name)
    .into()
}

/// How a part of an item is painted, as the oracle reads Pele's fill and stroke.
fn part_paint(it: &Item, part: &Part) -> String {
    match part.paint {
        Painting::StrokeOnly => format!("none {}", paint_name(it.style.stroke)),
        Painting::FillOnly => format!("{} -", paint_name(it.style.fill)),
        Painting::Hollow => format!("var(--_bg) {}", paint_name(it.style.stroke)),
        _ => format!(
            "{} {}",
            paint_name(it.style.fill),
            paint_name(it.style.stroke)
        ),
    }
}

/// How an item's lines end and turn their corners, as the oracle reads Pele's
/// stroke-linecap and stroke-linejoin: the author's or the diagram's, or "-" for neither.
fn pen(it: &Item) -> String {
    let s = it.author.over(&it.style);
    let cap = match s.cap {
        None => "-",
        Some(Cap::Butt) => "butt",
        Some(Cap::Round) => "round",
        Some(Cap::Square) => "square",
    };
    let join = match s.join {
        None => "-",
        Some(Join::Round) => "round",
        Some(_) => "?",
    };
    format!("{cap} {join}")
}

/// Text as Pele writes it in its SVG, without the characters XML does not allow.
fn pele_text(s: &str) -> String {
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

/// The subpaths of a path, each the letters of its steps with their points.
fn subpaths(p: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut at = 0;
    for &op in p.ops() {
        let a = &p.args()[at..at + op.args()];
        at += op.args();
        let mut s = "MLQCZ"[op as usize..op as usize + 1].to_string();
        for xy in a.chunks(2) {
            s += &format!(" {}", pt(xy[0], xy[1]));
        }
        match out.last_mut() {
            Some(last) if op != Op::MoveTo => *last += &format!(" {s}"),
            _ => out.push(s),
        }
    }
    out
}

/// The path of a part.
fn path_of(part: &Part) -> &Path {
    match &part.outline {
        Outline::Path(p) => p,
        Outline::Box { .. } => panic!("a box where a path was"),
    }
}

/// The ellipse Path::ellipse draws, by its middle and radii.
fn ellipse(p: &Path) -> String {
    let a = p.args();
    let (cx, cy) = (a[6], a[1]);
    format!("ellipse {} {}", pt(cx, cy), pt(a[0] - cx, a[7] - cy))
}

/// The lines of a scene as the oracle reads Pele's SVG.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    let down = !sc.items.iter().any(|it| it.role == ROLE_GRID);
    // Where Pele's text puts the first line of a label drawn about it, with its fill, its
    // anchor and the text of its lines.
    let text_at = |lines: &mut Vec<String>, it: &Item| {
        let Some(p) = it.label.as_ref().filter(|p| !p.label.lines.is_empty()) else {
            return;
        };
        let l = &p.label;
        let anchor = match p.align {
            Align::Middle => "middle",
            Align::Start => "start",
            Align::End => "end",
        };
        let content: Vec<String> = l
            .lines
            .iter()
            .map(|line| {
                let s: String = if line.is_empty() {
                    " ".into()
                } else {
                    line.iter().map(|span| span.text.as_str()).collect()
                };
                pele_text(&s)
            })
            .collect();
        let y = p.at.y - l.height / 2.0 + l.line_height / 2.0 + l.size * 0.35;
        lines.push(format!(
            "t {} {} {anchor} | {}",
            pt(p.at.x, y),
            paint_name(it.style.text),
            content.join(" / ")
        ));
    };
    let path = |lines: &mut Vec<String>, it: &Item, name: &str| {
        let dash = match &it.style.dash {
            Some(d) => d.iter().map(|&v| pele_num(v)).collect::<Vec<_>>().join(" "),
            None => "-".into(),
        };
        lines.push(format!("{name} {dash}"));
        for s in subpaths(path_of(&it.parts[0])) {
            lines.push(format!(
                "path {s} {} {}",
                part_paint(it, &it.parts[0]),
                pen(it)
            ));
        }
    };
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
    let mut step = 0;
    for it in &sc.items {
        let data = || pele_text(&id(it.id.as_deref().unwrap_or("")));
        match it.role.as_str() {
            ROLE_TITLE | ROLE_SCALE | ROLE_SCORE | ROLE_SECTION_LABEL => text_at(&mut lines, it),
            ROLE_GRID => path(&mut lines, it, "grid"),
            ROLE_STEM => path(&mut lines, it, "stems"),
            ROLE_LINE => path(&mut lines, it, "edge"),
            ROLE_FACE => {
                let shape = it.shape.as_ref().map_or("", |s| s.as_str());
                lines.push(format!("face {shape}"));
                for part in &it.parts[..3] {
                    lines.push(format!(
                        "{} {}",
                        ellipse(path_of(part)),
                        part_paint(it, part)
                    ));
                }
                let mouth = path_of(&it.parts[3]);
                let mut s = format!("path {}", subpaths(mouth)[0]);
                if shape != "neutral" {
                    let sweep = if shape == "sad" { "1" } else { "0" };
                    let end = mouth.last();
                    s = format!(
                        "path M {} A 6,6,0,0,{sweep} {}",
                        pt(mouth.args()[0], mouth.args()[1]),
                        pt(end.x, end.y)
                    );
                }
                lines.push(format!("{s} {} {}", part_paint(it, &it.parts[3]), pen(it)));
            }
            ROLE_SECTION => {
                lines.push(format!("section {}", data()));
                rect(&mut lines, it);
                text_at(&mut lines, it);
            }
            ROLE_TASK => {
                if down {
                    lines.push(format!("step {step}"));
                    step += 1;
                } else {
                    lines.push(format!("task {}", data()));
                }
                rect(&mut lines, it);
                text_at(&mut lines, it);
            }
            role @ (ROLE_ACTOR | ROLE_LEGEND) => {
                if role == ROLE_LEGEND {
                    lines.push(format!("legend {}", data()));
                }
                lines.push(format!(
                    "{} {}",
                    ellipse(path_of(&it.parts[0])),
                    part_paint(it, &it.parts[0])
                ));
                if let Some(tip) = &it.tooltip {
                    lines.push(format!("tip {}", pele_text(&id(tip))));
                }
                text_at(&mut lines, it);
            }
            role => lines.push(format!("unknown role {role}")),
        }
    }
    lines
}

/// What Pele's render draws of user journeys, read back out of its SVG as a line for each
/// thing drawn, in the order drawn, against the same lines written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("journey-render");
    assert!(f.cases.len() >= 3000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    let mut down = 0;
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a journey");
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("a journey that parses");
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            font_size: c.font_size.unwrap_or(16.0),
            ..Options::default()
        };
        let got = scene_lines(&layout(&m, &p.config, &o));
        if got.iter().any(|l| l.starts_with("step ")) {
            down += 1;
        }
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
    assert!(down >= 500, "only {down} cases laid out down");
    failures.done("renders", 3000);
}
