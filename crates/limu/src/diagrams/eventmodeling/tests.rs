//! Event modeling diagrams read and laid out as Pele reads and draws them, checked against
//! the oracle's eventmodeling and eventmodeling-render fixtures.

use limu_fixtures::load;
use serde::Deserialize;

use super::model::EventModelingModel;
use super::parse;
use super::parser::{
    EVENTMODELING_TOKENS, EventModelAst, T_ACC_DESCR, T_ACC_TITLE, T_ARROW, T_BLOCK, T_CLOSE,
    T_DATA, T_DOT, T_ENTITY, T_FRAME_ID, T_GIVEN, T_GWT, T_ID, T_INLINE, T_KEYWORD, T_NOTE, T_OPEN,
    T_THEN, T_TICK, T_TITLE, T_WHEN, parse_event_model, table,
};
use super::render::{
    ROLE_DATA, ROLE_FRAME_ID, ROLE_LANE, ROLE_LANE_LABEL, ROLE_NOTE, ROLE_NOTE_TEXT, ROLE_RELATION,
    ROLE_SPEC_TEXT, ROLE_SPECIFICATION, ROLE_TITLE, layout,
};
use crate::common::tokens::tokenize;
use crate::detect::DiagramType;
use crate::errors::ErrorCode;
use crate::js::number::to_string as n;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::{Paint, Theme};
use crate::scene::text::{Face, Span};
use crate::scene::{Align, Item, Kind, Op, Outline, Painting, Part, Path, Scene};
use crate::testing::pelesvg::{cap_join, id, own_fill};
use crate::testing::{Enc, Failures, PeleError, same, sorted};
use crate::value::Value;

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

/// The kinds of token, numbered by hand, are the indexes of their types in
/// EVENTMODELING_TOKENS.
#[test]
fn token_kinds() {
    assert_eq!(EVENTMODELING_TOKENS.len(), 47);
    for (kind, name) in [
        (T_KEYWORD, "eventmodeling"),
        (T_ENTITY, "entity"),
        (T_GIVEN, "given"),
        (T_DATA, "data"),
        (T_NOTE, "note"),
        (T_WHEN, "when"),
        (T_THEN, "then"),
        (T_ARROW, "->>"),
        (T_GWT, "gwt"),
        (T_OPEN, "[["),
        (T_CLOSE, "]]"),
        (T_TICK, "`"),
        (T_DOT, "."),
        (T_ID, "EM_ID"),
        (T_FRAME_ID, "EM_FID"),
        (T_INLINE, "EM_DATA_INLINE"),
        (T_BLOCK, "EM_DATA_BLOCK"),
        (T_ACC_DESCR, "EM_ACC_DESCR"),
        (T_ACC_TITLE, "EM_ACC_TITLE"),
        (T_TITLE, "EM_TITLE"),
    ] {
        assert_eq!(EVENTMODELING_TOKENS[kind].name, name, "kind {kind}");
    }
    let names = |kinds: &[usize]| {
        kinds
            .iter()
            .map(|&k| EVENTMODELING_TOKENS[k].name)
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&super::parser::TIME_FRAME), ["tf", "timeframe"]);
    assert_eq!(names(&super::parser::RESET_FRAME), ["rf", "resetframe"]);
    assert_eq!(
        names(&super::parser::ENTITY_TYPE),
        [
            "rmo",
            "readmodel",
            "ui",
            "cmd",
            "command",
            "evt",
            "event",
            "pcr",
            "processor"
        ]
    );
    assert_eq!(
        names(&super::parser::DATA_TYPE),
        [
            "json", "jsobj", "figma", "salt", "uri", "md", "html", "text"
        ]
    );
}

/// A string as the oracle writes it, or undefined.
fn enc_str(s: Option<&str>) -> Value {
    match s {
        Some(s) => Value::String(s.to_string()),
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

fn strings(items: &[String]) -> Value {
    Value::Array(items.iter().cloned().map(Value::String).collect())
}

fn string(s: &str) -> Value {
    Value::String(s.to_string())
}

fn dump_ast(t: &EventModelAst) -> Value {
    let frames = t
        .frames
        .iter()
        .map(|f| {
            Value::Array(vec![
                string(if f.reset {
                    "EmResetFrame"
                } else {
                    "EmTimeFrame"
                }),
                string(&f.name),
                string(&f.model_entity_type),
                string(&f.entity_identifier),
                strings(&f.source_frames),
                enc_str(f.data_reference.as_deref()),
                enc_str(f.data_type.as_deref()),
                enc_str(f.data_inline_value.as_deref()),
            ])
        })
        .collect();
    let data = t
        .data_entities
        .iter()
        .map(|d| {
            Value::Array(vec![
                string(&d.name),
                enc_str(d.data_type.as_deref()),
                string(&d.data_block_value),
            ])
        })
        .collect();
    let notes = t
        .note_entities
        .iter()
        .map(|n| {
            Value::Array(vec![
                string(&n.source_frame),
                enc_str(n.data_type.as_deref()),
                string(&n.data_block_value),
            ])
        })
        .collect();
    let gwts = t
        .gwt_entities
        .iter()
        .map(|g| {
            Value::Array(vec![
                string(&g.source_frame),
                strings(&g.given_statements),
                strings(&g.when_statements),
                strings(&g.then_statements),
            ])
        })
        .collect();
    object(vec![
        ("title", enc_str(t.title.as_deref())),
        ("accTitle", enc_str(t.acc_title.as_deref())),
        ("accDescr", enc_str(t.acc_descr.as_deref())),
        ("entities", strings(&t.model_entities)),
        ("frames", Value::Array(frames)),
        ("data", Value::Array(data)),
        ("notes", Value::Array(notes)),
        ("gwts", Value::Array(gwts)),
    ])
}

/// The model as the oracle writes it. A title the model leaves undefined or empty is left
/// out.
fn dump_model(m: &EventModelingModel) -> Value {
    let lanes = m
        .lanes
        .iter()
        .map(|l| {
            Value::Array(vec![
                Value::Number(l.index as f64),
                string(&l.label),
                string(l.group.name()),
                enc_str(l.namespace.as_deref()),
            ])
        })
        .collect();
    let boxes = m
        .boxes
        .iter()
        .map(|b| {
            Value::Array(vec![
                string(&b.id),
                string(b.kind.name()),
                string(&b.entity),
                string(&b.name),
                enc_str(b.namespace.as_deref()),
                Value::Bool(b.reset),
                Value::Number(b.lane as f64),
                enc_str(b.data.as_deref()),
                enc_str(b.data_type.as_deref()),
            ])
        })
        .collect();
    let relations = m
        .relations
        .iter()
        .map(|r| {
            Value::Array(vec![
                string(&m.boxes[r.source].id),
                string(&m.boxes[r.target].id),
            ])
        })
        .collect();
    let notes = m
        .notes
        .iter()
        .map(|n| {
            Value::Array(vec![
                string(&n.frame),
                string(&n.text),
                enc_str(n.data_type.as_deref()),
            ])
        })
        .collect();
    let specs = m
        .specifications
        .iter()
        .map(|s| {
            Value::Array(vec![
                string(&s.frame),
                strings(&s.given),
                strings(&s.when),
                strings(&s.then),
            ])
        })
        .collect();
    let mut entries = Vec::new();
    for (key, v) in [
        ("title", &m.title),
        ("accTitle", &m.acc_title),
        ("accDescr", &m.acc_descr),
    ] {
        if let Some(v) = v.as_deref().filter(|v| !v.is_empty()) {
            entries.push((key, string(v)));
        }
    }
    entries.extend([
        ("lanes", Value::Array(lanes)),
        ("boxes", Value::Array(boxes)),
        ("relations", Value::Array(relations)),
        ("notes", Value::Array(notes)),
        ("specifications", Value::Array(specs)),
        ("entities", strings(&m.entities)),
    ]);
    object(entries)
}

fn dump_tokens(src: &str) -> Option<Vec<(String, String)>> {
    let t = tokenize(src, table(), DiagramType::EventModeling).ok()?;
    let n = t.kinds.len() - 1;
    Some(
        (0..n)
            .map(|i| {
                let name = EVENTMODELING_TOKENS[t.kinds[i]].name.to_string();
                (name, src[t.starts[i]..t.ends[i]].to_string())
            })
            .collect(),
    )
}

#[test]
fn parse_matches_pele() {
    let f: Cases = load("eventmodeling");
    assert!(f.cases.len() >= 7000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    let (mut models, mut errors) = (0, 0);
    for c in &f.cases {
        let tokens = dump_tokens(&c.src);
        failures.check(tokens == c.direct.tokens, || {
            format!(
                "tokens of {:?}: {tokens:?}, Pele {:?}",
                c.src, c.direct.tokens
            )
        });
        let tree = parse_event_model(&c.src);
        let ok = match (&tree, &c.direct.error, &c.direct.ast) {
            (Err(e), Some(want), _) => want.is(e),
            (Ok(t), None, Some(want)) => same(&sorted(&dump_ast(t)), &sorted(&want.0)),
            _ => false,
        };
        failures.check(ok, || {
            format!(
                "parse_event_model({:?}): {tree:?}, Pele {:?} {:?}",
                c.src, c.direct.ast, c.direct.error
            )
        });

        let p = prepare(&c.src, &ParseOptions::default());
        let ok = match (&p, &c.error) {
            (Err(e), Some(want)) => want.is(e),
            (Err(e), None) => e.code == ErrorCode::UnsupportedDiagram && c.diagram.is_none(),
            (Ok(p), _) if c.diagram.as_deref() != Some(p.found.diagram.name()) => false,
            (Ok(p), want) if p.found.diagram == DiagramType::EventModeling => {
                let got = p.parse_with(true, |s| parse(s, p.title.as_deref()));
                match (&got, want, &c.model) {
                    (Err(e), Some(want), _) => {
                        errors += 1;
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
            let got = p
                .as_ref()
                .map(|p| p.parse_with(true, |s| parse(s, p.title.as_deref())));
            format!(
                "{:?}: got {got:?}, model {:?}, error {:?}",
                c.src, c.model, c.error
            )
        });
    }
    failures.done("eventmodeling", 3 * 7000);
    assert!(
        models >= 2000 && errors >= 500,
        "only {models} models and {errors} errors"
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
    auto_direction: Option<bool>,
    direction_breakpoint: Option<f64>,
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

/// A paint as Pele's SVG names it, "-" for none given.
fn paint_name(p: Option<Paint>) -> String {
    let Some(p) = p.filter(|&p| p != Paint::FG) else {
        return "-".into();
    };
    for k in 0..8 {
        if p.as_token().is_some() && p.as_token() == Paint::series(k).as_token() {
            return format!("var(--pele-series-{},{})", k + 1, Theme::PELE.series[k]);
        }
    }
    match p {
        Paint::BG => "var(--_bg)",
        Paint::MUTED => "var(--_m)",
        Paint::LINE => "var(--_l)",
        Paint::SURFACE => "var(--_s)",
        Paint::BORDER => "var(--_b)",
        _ => "?",
    }
    .into()
}

/// The class Pele gives the text of an item of a role.
fn class_of(it: &Item) -> &'static str {
    match it.role.as_str() {
        ROLE_TITLE => "pele-title",
        ROLE_LANE | ROLE_LANE_LABEL => "pele-cluster-label",
        ROLE_DATA => "pele-em-data",
        ROLE_FRAME_ID => "pele-em-frame",
        ROLE_NOTE | ROLE_SPECIFICATION => "-",
        _ if it.kind == Kind::Node => "pele-label",
        _ => "-",
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

/// The text of a line of a label, as Pele writes it.
fn line_text(line: &[Span]) -> String {
    clean(&line.iter().map(|s| s.text.as_str()).collect::<String>())
}

/// A path as the oracle reads Pele's: a string for each subpath, each step its letter and
/// its points.
fn subpaths(p: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut at = 0;
    for &op in p.ops() {
        let a = &p.args()[at..at + op.args()];
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
            s.push_str(&format!(" {},{}", n(xy[0]), n(xy[1])));
        }
        match out.last_mut() {
            Some(last) if op != Op::MoveTo => {
                last.push(' ');
                last.push_str(&s);
            }
            _ => out.push(s),
        }
    }
    out
}

/// The lines of a scene as the oracle reads Pele's SVG, every number as JavaScript's String
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
    // Where Pele's text puts a label: centered on its point as labelSvg writes it, its lines
    // starting at it as lines writes them, or a line of its own each, as leftLabelSvg does,
    // and what the oracle writes of it after where it sits.
    let label = |lines: &mut Vec<String>, it: &Item| {
        let Some(p) = &it.label else {
            return;
        };
        let l = &p.label;
        if l.lines.is_empty() {
            return;
        }
        let anchor = match p.align {
            Align::Middle => "middle",
            Align::Start => "start",
            Align::End => "end",
        };
        let first = l.lines[0].first().map_or(Face::REGULAR, |s| s.face);
        let weight = if first.contains(Face::BOLD) { "b" } else { "-" };
        let family = if first.contains(Face::MONO) {
            "mono"
        } else {
            "-"
        };
        let look = |at: String| {
            format!(
                "t {} {at} {anchor} {} {} {weight} {family}",
                class_of(it),
                n(l.size),
                paint_name(it.style.text)
            )
        };
        let y = p.at.y - l.height / 2.0 + l.line_height / 2.0 + l.size * 0.35;
        if it.role == ROLE_LANE_LABEL {
            let top = p.at.y - l.height / 2.0;
            for (i, line) in l.lines.iter().enumerate() {
                let s = line_text(line);
                if !s.is_empty() {
                    let at = pt(
                        p.at.x,
                        top + (i as f64 + 0.5) * l.line_height + l.size * 0.35,
                    );
                    lines.push(format!("{} | {s}", look(at)));
                }
            }
        } else if p.align == Align::Start {
            let mut texts = Vec::new();
            let mut first = None;
            for (i, line) in l.lines.iter().enumerate() {
                let s = line_text(line);
                if !s.is_empty() {
                    first.get_or_insert(i);
                    texts.push(s);
                }
            }
            let at = pt(p.at.x, y + first.unwrap_or(0) as f64 * l.line_height);
            lines.push(format!("{} | {}", look(at), texts.join(" | ")));
        } else {
            let texts: Vec<String> = l
                .lines
                .iter()
                .map(|line| {
                    if line.is_empty() {
                        " ".to_string()
                    } else {
                        line_text(line)
                    }
                })
                .collect();
            lines.push(format!("{} | {}", look(pt(p.at.x, y)), texts.join(" | ")));
        }
    };
    let rect = |lines: &mut Vec<String>, it: &Item, part: &Part| {
        let Outline::Box { rect: r, rounded } = &part.outline else {
            panic!("a box where a path was looked for");
        };
        let mut fill = own_fill(it);
        let mut stroke = paint_name(it.style.stroke);
        match part.paint {
            Painting::Shade => (fill, stroke) = (Some(Paint::BG), "-".into()),
            Painting::FillOnly => stroke = "-".into(),
            _ => {}
        }
        let factor = fill.map_or(1.0, |f| f.opacity());
        let opacity = if factor < 1.0 {
            factor.to_string()
        } else {
            "-".into()
        };
        lines.push(format!(
            "rect {},{} {} {} {opacity} {stroke}",
            pt(r.x, r.y),
            pt(r.w, r.h),
            if *rounded { "r" } else { "-" },
            paint_name(fill)
        ));
    };
    for it in &sc.items {
        let ident = it.id.clone().unwrap_or_default();
        match it.role.as_str() {
            ROLE_TITLE | ROLE_DATA | ROLE_FRAME_ID | ROLE_NOTE_TEXT | ROLE_SPEC_TEXT => {
                label(&mut lines, it);
            }
            ROLE_LANE_LABEL => {
                lines.push(format!("lane {ident}"));
                label(&mut lines, it);
            }
            ROLE_LANE => {
                lines.push(format!("lane {ident}"));
                rect(&mut lines, it, &it.parts[0]);
                label(&mut lines, it);
            }
            ROLE_NOTE | ROLE_SPECIFICATION => {
                let kind = it.role.as_str().trim_start_matches("eventmodeling.");
                lines.push(format!("{kind} {ident}"));
                rect(&mut lines, it, &it.parts[0]);
                label(&mut lines, it);
            }
            ROLE_RELATION => {
                lines.push(format!("edge {ident}"));
                let Outline::Path(path) = &it.parts[0].outline else {
                    panic!("an edge of a box");
                };
                let stroke = paint_name(it.style.stroke);
                for s in subpaths(path) {
                    lines.push(format!("path {s} {stroke} none {}", cap_join(it)));
                }
                let ends = it.edge().expect("an edge").ends;
                for m in ends.iter().flatten() {
                    if let Some(Part {
                        outline: Outline::Path(p),
                        ..
                    }) = m.outline().first()
                    {
                        lines.push(format!("marker {} {stroke}", subpaths(p).join(" ")));
                    }
                }
            }
            "eventmodeling.ui"
            | "eventmodeling.command"
            | "eventmodeling.event"
            | "eventmodeling.readmodel"
            | "eventmodeling.processor" => {
                let mut s = format!(
                    "node {ident} {}",
                    it.role.as_str().trim_start_matches("eventmodeling.")
                );
                for tag in &it.tags {
                    s.push(' ');
                    s.push_str(tag);
                }
                lines.push(s);
                for part in &it.parts {
                    rect(&mut lines, it, part);
                }
                label(&mut lines, it);
            }
            role => lines.push(format!("unknown role {role}")),
        }
    }
    lines
}

/// What Pele's render draws of event modeling diagrams, read back out of its SVG as a line
/// for each thing drawn, in the order drawn, with every number in full, against the same
/// lines written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("eventmodeling-render");
    assert!(f.cases.len() >= 4000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    let mut down = 0;
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("an event modeling diagram");
        let m = p
            .parse_with(true, |s| parse(s, p.title.as_deref()))
            .expect("an event modeling diagram that parses");
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            font_size: c.font_size.unwrap_or(16.0),
            keep_direction: c.auto_direction == Some(false),
            direction_breakpoint: c.direction_breakpoint.unwrap_or(0.0),
            ..Options::default()
        };
        let sc = layout(&m, &p.config, &o);
        if sc.items.iter().any(|it| it.role == ROLE_LANE_LABEL) {
            down += 1;
        }
        let got = scene_lines(&sc);
        let diff = drawings.diff(k, &c.drawing, &got, None);
        failures.check(diff.is_none(), || {
            format!(
                "{:?}, max {:?}, size {:?}, auto {:?}, breakpoint {:?}: {}",
                c.src,
                c.max_width,
                c.font_size,
                c.auto_direction,
                c.direction_breakpoint,
                diff.unwrap()
            )
        });
    }
    failures.done("renders", 4000);
    assert!(down >= 500, "only {down} laid out down");
}
