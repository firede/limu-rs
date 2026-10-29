//! Wardley maps read and laid out as Pele reads and draws them, checked against the oracle's
//! wardley and wardley-render fixtures.

use limu_fixtures::load;
use serde::Deserialize;

use super::model::WardleyModel;
use super::parse;
use super::parser::{
    LabelAst, PlacedAst, T_ACC_DESCR, T_ACC_TITLE, T_ACCELERATOR, T_ANCHOR, T_ANNOTATION,
    T_ANNOTATIONS, T_ARROW, T_AT, T_COMMA, T_COMMENT, T_COMPONENT, T_DEACCELERATOR, T_DIRECTIVE,
    T_EVOLUTION, T_EVOLVE, T_ID, T_INERTIA, T_INT, T_LABEL, T_LBRACE, T_LBRACKET, T_LINK_ARROW,
    T_LINK_LABEL, T_LINK_PORT, T_LPAREN, T_MINUS, T_NAME, T_NEWLINE, T_NOTE, T_NUMBER, T_PIPELINE,
    T_RBRACE, T_RBRACKET, T_RPAREN, T_SIZE, T_SLASH, T_STRATEGY, T_STRING, T_TITLE, T_WARDLEY,
    T_WHITESPACE, T_WS, T_YAML, WARDLEY_TOKENS, WardleyAst, parse_wardley, table,
};
use super::render::{
    ROLE_ACCELERATOR, ROLE_ANCHOR, ROLE_ANNOTATION, ROLE_ANNOTATION_TEXT, ROLE_ANNOTATIONS,
    ROLE_AXIS, ROLE_AXIS_LABEL, ROLE_COMPONENT, ROLE_DEACCELERATOR, ROLE_GRID, ROLE_INERTIA,
    ROLE_LABEL, ROLE_LINK, ROLE_LINK_LABEL, ROLE_NOTE, ROLE_PIPELINE, ROLE_PIPELINE_COMPONENT,
    ROLE_PIPELINE_LINE, ROLE_PIPELINE_PARENT, ROLE_STAGE, ROLE_STAGE_LABEL, ROLE_TITLE, ROLE_TREND,
    ROLE_TREND_TARGET, layout,
};
use crate::common::tokens::tokenize;
use crate::detect::DiagramType;
use crate::errors::ErrorCode;
use crate::js::number::to_string as n;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::Paint;
use crate::scene::{Align, Item, Kind, Op, Outline, Painting, Part, Scene};
use crate::testing::pelesvg::{cap_join, id};
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

/// The kinds of token, numbered by hand, are the indexes of their types in WARDLEY_TOKENS.
#[test]
fn token_kinds() {
    assert_eq!(WARDLEY_TOKENS.len(), 43);
    for (kind, name) in [
        (T_LBRACKET, "["),
        (T_COMMA, ","),
        (T_RBRACKET, "]"),
        (T_AT, "@"),
        (T_SLASH, "/"),
        (T_LPAREN, "("),
        (T_RPAREN, ")"),
        (T_MINUS, "-"),
        (T_LBRACE, "{"),
        (T_RBRACE, "}"),
        (T_NUMBER, "WARDLEY_NUMBER"),
        (T_ARROW, "ARROW"),
        (T_LINK_PORT, "LINK_PORT"),
        (T_LINK_ARROW, "LINK_ARROW"),
        (T_LINK_LABEL, "LINK_LABEL"),
        (T_STRATEGY, "STRATEGY"),
        (T_WARDLEY, "KW_WARDLEY"),
        (T_SIZE, "KW_SIZE"),
        (T_EVOLUTION, "KW_EVOLUTION"),
        (T_ANCHOR, "KW_ANCHOR"),
        (T_COMPONENT, "KW_COMPONENT"),
        (T_LABEL, "KW_LABEL"),
        (T_INERTIA, "KW_INERTIA"),
        (T_EVOLVE, "KW_EVOLVE"),
        (T_PIPELINE, "KW_PIPELINE"),
        (T_NOTE, "KW_NOTE"),
        (T_ANNOTATIONS, "KW_ANNOTATIONS"),
        (T_ANNOTATION, "KW_ANNOTATION"),
        (T_ACCELERATOR, "KW_ACCELERATOR"),
        (T_DEACCELERATOR, "KW_DEACCELERATOR"),
        (T_NAME, "NAME_WITH_SPACES"),
        (T_WS, "WS"),
        (T_ACC_DESCR, "ACC_DESCR"),
        (T_ACC_TITLE, "ACC_TITLE"),
        (T_TITLE, "TITLE"),
        (T_INT, "INT"),
        (T_STRING, "STRING"),
        (T_ID, "ID"),
        (T_NEWLINE, "NEWLINE"),
        (T_WHITESPACE, "WHITESPACE"),
        (T_YAML, "YAML"),
        (T_DIRECTIVE, "DIRECTIVE"),
        (T_COMMENT, "SINGLE_LINE_COMMENT"),
    ] {
        assert_eq!(WARDLEY_TOKENS[kind].name, name, "kind {kind}");
    }
}

fn undefined() -> Value {
    Value::Object(
        [("$".to_string(), Value::String("undefined".into()))]
            .into_iter()
            .collect(),
    )
}

/// A value as the oracle writes it, or undefined.
fn opt<T>(v: Option<T>, f: impl FnOnce(T) -> Value) -> Value {
    v.map_or_else(undefined, f)
}

fn string(s: &str) -> Value {
    Value::String(s.to_string())
}

fn num(v: f64) -> Value {
    Value::Number(v)
}

fn array(items: Vec<Value>) -> Value {
    Value::Array(items)
}

fn object(entries: Vec<(&str, Value)>) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn dump_label(l: Option<&LabelAst>) -> Value {
    opt(l, |l| {
        array(vec![
            Value::Bool(l.neg_x),
            num(l.offset_x),
            Value::Bool(l.neg_y),
            num(l.offset_y),
        ])
    })
}

fn dump_placed(list: &[PlacedAst]) -> Value {
    array(
        list.iter()
            .map(|p| array(vec![string(&p.name), num(p.x), num(p.y)]))
            .collect(),
    )
}

fn dump_ast(t: &WardleyAst) -> Value {
    let stages = t.evolution.as_ref().map(|stages| {
        array(
            stages
                .iter()
                .map(|s| {
                    array(vec![
                        string(&s.name),
                        opt(s.boundary, num),
                        opt(s.second_name.as_deref(), string),
                    ])
                })
                .collect(),
        )
    });
    let components = t
        .components
        .iter()
        .map(|c| {
            array(vec![
                string(&c.name),
                num(c.visibility),
                num(c.evolution),
                dump_label(c.label.as_ref()),
                opt(c.decorator.as_deref(), string),
                Value::Bool(c.inertia),
            ])
        })
        .collect();
    let links = t
        .links
        .iter()
        .map(|l| {
            array(vec![
                string(&l.from),
                opt(l.from_port.as_deref(), string),
                opt(l.arrow.as_deref(), string),
                string(&l.to),
                opt(l.to_port.as_deref(), string),
                opt(l.link_label.as_deref(), string),
            ])
        })
        .collect();
    let pipelines = t
        .pipelines
        .iter()
        .map(|p| {
            let components = p
                .components
                .iter()
                .map(|c| {
                    array(vec![
                        string(&c.name),
                        num(c.evolution),
                        dump_label(c.label.as_ref()),
                    ])
                })
                .collect();
            array(vec![string(&p.parent), array(components)])
        })
        .collect();
    object(vec![
        ("title", opt(t.title.as_deref(), string)),
        ("accTitle", opt(t.acc_title.as_deref(), string)),
        ("accDescr", opt(t.acc_descr.as_deref(), string)),
        ("size", opt(t.size, |(w, h)| array(vec![num(w), num(h)]))),
        ("evolution", stages.unwrap_or_else(undefined)),
        (
            "anchors",
            array(
                t.anchors
                    .iter()
                    .map(|a| array(vec![string(&a.name), num(a.visibility), num(a.evolution)]))
                    .collect(),
            ),
        ),
        ("components", array(components)),
        ("links", array(links)),
        (
            "evolves",
            array(
                t.evolves
                    .iter()
                    .map(|e| array(vec![string(&e.component), num(e.target)]))
                    .collect(),
            ),
        ),
        ("pipelines", array(pipelines)),
        (
            "notes",
            array(
                t.notes
                    .iter()
                    .map(|a| array(vec![string(&a.text), num(a.visibility), num(a.evolution)]))
                    .collect(),
            ),
        ),
        (
            "annotations",
            array(
                t.annotations
                    .iter()
                    .map(|&(x, y)| array(vec![num(x), num(y)]))
                    .collect(),
            ),
        ),
        (
            "annotation",
            array(
                t.annotation
                    .iter()
                    .map(|a| array(vec![num(a.number), num(a.x), num(a.y), string(&a.text)]))
                    .collect(),
            ),
        ),
        ("accelerators", dump_placed(&t.accelerators)),
        ("deaccelerators", dump_placed(&t.deaccelerators)),
    ])
}

fn dump_model(m: &WardleyModel) -> Value {
    let nodes = m
        .nodes
        .iter()
        .map(|n| {
            array(vec![
                string(&n.id),
                string(&n.label),
                num(n.x),
                num(n.y),
                opt(n.class_name, string),
                opt(n.label_offset_x, num),
                opt(n.label_offset_y, num),
                opt(n.in_pipeline.then_some(true), Value::Bool),
                opt(n.is_pipeline_parent.then_some(true), Value::Bool),
                opt(n.inertia, Value::Bool),
                opt(n.source_strategy.as_deref(), string),
            ])
        })
        .collect();
    let links = m
        .links
        .iter()
        .map(|l| {
            array(vec![
                string(&l.source),
                string(&l.target),
                Value::Bool(l.dashed),
                opt(l.label.as_deref(), string),
                opt(l.flow, string),
            ])
        })
        .collect();
    let forces = |list: &[super::builder::WardleyForce]| {
        array(
            list.iter()
                .map(|f| array(vec![string(&f.name), num(f.x), num(f.y)]))
                .collect(),
        )
    };
    let axes = &m.axes;
    object(vec![
        ("title", opt(m.title.as_deref(), string)),
        ("accTitle", opt(m.acc_title.as_deref(), string)),
        ("accDescr", opt(m.acc_descr.as_deref(), string)),
        ("nodes", array(nodes)),
        ("links", array(links)),
        (
            "trends",
            array(
                m.trends
                    .iter()
                    .map(|t| array(vec![string(&t.node_id), num(t.target_x), num(t.target_y)]))
                    .collect(),
            ),
        ),
        (
            "pipelines",
            array(
                m.pipelines
                    .iter()
                    .map(|p| {
                        let ids = p.component_ids.iter().map(|s| string(s)).collect();
                        array(vec![string(&p.node_id), array(ids)])
                    })
                    .collect(),
            ),
        ),
        (
            "annotations",
            array(
                m.annotations
                    .iter()
                    .map(|a| {
                        let places = a
                            .coordinates
                            .iter()
                            .map(|&(x, y)| array(vec![num(x), num(y)]))
                            .collect();
                        array(vec![num(a.number), array(places), string(&a.text)])
                    })
                    .collect(),
            ),
        ),
        (
            "notes",
            array(
                m.notes
                    .iter()
                    .map(|n| array(vec![string(&n.text), num(n.x), num(n.y)]))
                    .collect(),
            ),
        ),
        ("accelerators", forces(&m.accelerators)),
        ("deaccelerators", forces(&m.deaccelerators)),
        (
            "annotationsBox",
            opt(m.annotations_box, |(x, y)| array(vec![num(x), num(y)])),
        ),
        (
            "axes",
            array(vec![
                opt(axes.x_label.as_deref(), string),
                opt(axes.y_label.as_deref(), string),
                opt(axes.stages.as_ref(), |s| {
                    array(s.iter().map(|s| string(s)).collect())
                }),
                opt(axes.stage_boundaries.as_ref(), |b| {
                    array(b.iter().map(|&b| num(b)).collect())
                }),
            ]),
        ),
        ("size", opt(m.size, |(w, h)| array(vec![num(w), num(h)]))),
    ])
}

fn dump_tokens(src: &str) -> Option<Vec<(String, String)>> {
    let t = tokenize(src, table(), DiagramType::Wardley).ok()?;
    let n = t.kinds.len() - 1;
    Some(
        (0..n)
            .map(|i| {
                let name = WARDLEY_TOKENS[t.kinds[i]].name.to_string();
                (name, src[t.starts[i]..t.ends[i]].to_string())
            })
            .collect(),
    )
}

#[test]
fn parse_matches_pele() {
    let f: Cases = load("wardley");
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
        let tree = parse_wardley(&c.src);
        let ok = match (&tree, &c.direct.error, &c.direct.ast) {
            (Err(e), Some(want), _) => want.is(e),
            (Ok(t), None, Some(want)) => same(&sorted(&dump_ast(t)), &sorted(&want.0)),
            _ => false,
        };
        failures.check(ok, || {
            format!(
                "parse_wardley({:?}): {tree:?}, Pele {:?} {:?}",
                c.src, c.direct.ast, c.direct.error
            )
        });

        let p = prepare(&c.src, &ParseOptions::default());
        let ok = match (&p, &c.error) {
            (Err(e), Some(want)) => want.is(e),
            (Err(e), None) => e.code == ErrorCode::UnsupportedDiagram && c.diagram.is_none(),
            (Ok(p), _) if c.diagram.as_deref() != Some(p.found.diagram.name()) => false,
            (Ok(p), want) if p.found.diagram == DiagramType::Wardley => {
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
            let got = p
                .as_ref()
                .map(|p| p.parse_with(false, |s| parse(s, p.title.as_deref())));
            format!(
                "{:?}: got {got:?}, model {:?}, error {:?}",
                c.src, c.model, c.error
            )
        });
    }
    failures.done("wardley", 3 * 6000);
    assert!(
        models >= 2000 && refused >= 20,
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
    #[serde(default)]
    icons: bool,
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

/// The icons the oracle's host has, in the cases drawn with icons.
fn host_icons(name: &str) -> bool {
    matches!(name, "star" | "fa:fa-car" | "fa:fa-coffee")
}

/// A paint as Pele's SVG names it, "?" for one it does not.
fn paint_name(p: Option<Paint>) -> &'static str {
    match p {
        Some(Paint::BG) => "var(--_bg)",
        Some(Paint::FG) => "var(--_fg)",
        Some(Paint::MUTED) => "var(--_m)",
        Some(Paint::LINE) => "var(--_l)",
        Some(Paint::SURFACE) => "var(--_s)",
        Some(Paint::SURFACE_ALT) => "var(--_a)",
        Some(Paint::BORDER) => "var(--_b)",
        Some(Paint::ACCENT) => "var(--_c)",
        Some(Paint::NONE) => "none",
        _ => "?",
    }
}

/// The class Pele gives the text of an item of a role.
fn text_class(role: &str) -> &'static str {
    match role {
        ROLE_TITLE => "pele-title",
        ROLE_STAGE_LABEL => "pele-wardley-stage-label",
        ROLE_AXIS_LABEL => "pele-wardley-axis-label",
        ROLE_LINK_LABEL => "pele-edge-label",
        ROLE_LABEL => "pele-label",
        ROLE_NOTE => "pele-wardley-note",
        _ => "-",
    }
}

/// The class of Pele's group of a node of a role.
fn node_class(role: &str) -> &'static str {
    match role {
        ROLE_ANCHOR => "pele-wardley-anchor",
        ROLE_COMPONENT => "pele-wardley-component",
        ROLE_PIPELINE_PARENT => "pele-wardley-pipeline-parent",
        _ => "pele-wardley-pipeline-component",
    }
}

const FACES: [&str; 4] = ["-", "b", "i", "bi"];

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
    // What a part is filled and stroked with, its dash, cap and join.
    let pen = |it: &Item, paint: Painting| {
        let (mut fill, mut stroke) = (paint_name(it.style.fill), paint_name(it.style.stroke));
        match paint {
            Painting::StrokeOnly => fill = "none",
            Painting::FillOnly => stroke = "-",
            Painting::Hollow => fill = paint_name(Some(Paint::BG)),
            _ => {}
        }
        let dash = match &it.style.dash {
            None => "-".to_string(),
            Some(d) => d.iter().map(|&v| n(v)).collect::<Vec<_>>().join(" "),
        };
        format!("{fill} {stroke} {dash} {}", cap_join(it))
    };
    let part = |lines: &mut Vec<String>, it: &Item, p: &Part| {
        let path = match &p.outline {
            Outline::Box { rect: r, rounded } => {
                lines.push(format!(
                    "rect {},{} {} {}",
                    pt(r.x, r.y),
                    pt(r.w, r.h),
                    if *rounded { "r" } else { "-" },
                    pen(it, p.paint)
                ));
                return;
            }
            Outline::Path(path) => path,
        };
        let a = &path.args;
        if path.ops.len() == 6 && path.ops[1] == Op::CubeTo {
            // A circle, as Path::ellipse draws it: from its right end, four quarters about its
            // middle, the first to its bottom.
            lines.push(format!(
                "circle {} {} {}",
                pt(a[6], a[1]),
                pt(a[0], a[7]),
                pen(it, p.paint)
            ));
            return;
        }
        let mut steps = Vec::new();
        let mut at = 0;
        for &op in &path.ops {
            steps.push(match op {
                Op::MoveTo => format!("M {}", pt(a[at], a[at + 1])),
                Op::LineTo => format!("L {}", pt(a[at], a[at + 1])),
                Op::Close => "Z".to_string(),
                _ => "?".to_string(),
            });
            at += op.args();
        }
        lines.push(format!("path {} {}", steps.join(" "), pen(it, p.paint)));
    };
    // A label as the oracle writes one: where it is drawn about, its class, size, fill and
    // weight as Pele's attributes give them, its measures, and its spans.
    let label = |lines: &mut Vec<String>, it: &Item, class: &str, weight: &str| {
        let Some(p) = &it.label else {
            return;
        };
        let l = &p.label;
        if l.is_empty() {
            return;
        }
        let mut kind = "label";
        if p.align == Align::Start {
            kind = "start";
            // Pele writes nothing of a label whose lines start at a point and hold only
            // icons, which it leaves out of such a label.
            if l.lines.iter().flatten().all(|s| s.icon.is_some()) {
                return;
            }
        }
        let size = if it.role == ROLE_TITLE {
            "-".to_string()
        } else {
            n(l.size)
        };
        let fill = match it.style.text {
            Some(t) if t != Paint::FG => paint_name(Some(t)),
            _ => "-",
        };
        let turn = if p.rotate != 0.0 {
            format!(" turn {}", n(p.rotate))
        } else {
            String::new()
        };
        let spans: Vec<String> = l
            .lines
            .iter()
            .map(|line| {
                line.iter()
                    .map(|s| match &s.icon {
                        Some(icon) => format!("i:{icon}"),
                        None => format!("t:{}:{}", FACES[usize::from(s.face.bits() & 3)], s.text),
                    })
                    .collect::<Vec<_>>()
                    .join(" + ")
            })
            .collect();
        lines.push(format!(
            "{kind} {class} {size} {fill} {weight}{turn} {} {} {} {} | {}",
            pt(p.at.x, p.at.y),
            pt(l.width, l.height),
            n(l.size),
            n(l.line_height),
            spans.join(" | ")
        ));
    };
    let mut node = "";
    for it in &sc.items {
        let role = it.role.as_str();
        let heading = matches!(
            role,
            ROLE_AXIS_LABEL | ROLE_NOTE | ROLE_ANNOTATION | ROLE_ACCELERATOR | ROLE_DEACCELERATOR
        ) || role == ROLE_LABEL && node == ROLE_ANCHOR;
        let weight = if heading { "var(--_hw)" } else { "-" };
        let class = text_class(role);
        let ident = id(it.id.as_deref().unwrap_or(""));
        match role {
            ROLE_TITLE => label(&mut lines, it, class, "var(--_tw)"),
            ROLE_STAGE | ROLE_GRID | ROLE_AXIS | ROLE_TREND => part(&mut lines, it, &it.parts[0]),
            ROLE_PIPELINE_LINE => {
                // Pele leaves the fill of the line unset, which a straight line does not show.
                let s = lines.len();
                part(&mut lines, it, &it.parts[0]);
                lines[s] = lines[s].replacen(" none ", " - ", 1);
            }
            ROLE_STAGE_LABEL | ROLE_AXIS_LABEL | ROLE_LINK_LABEL | ROLE_LABEL | ROLE_NOTE
            | ROLE_ANNOTATION_TEXT => label(&mut lines, it, class, weight),
            ROLE_PIPELINE => {
                lines.push(format!("group pele-wardley-pipeline {ident}"));
                part(&mut lines, it, &it.parts[0]);
            }
            ROLE_LINK => {
                let dashed = if it.style.dash.is_some() {
                    " pele-wardley-dashed"
                } else {
                    ""
                };
                lines.push(format!("group pele-edge{dashed} {ident}"));
                part(&mut lines, it, &it.parts[0]);
            }
            ROLE_TREND_TARGET | ROLE_INERTIA => part(&mut lines, it, &it.parts[0]),
            ROLE_ANCHOR | ROLE_COMPONENT | ROLE_PIPELINE_PARENT | ROLE_PIPELINE_COMPONENT => {
                node = role;
                lines.push(format!("group pele-node {} {ident}", node_class(role)));
                for p in &it.parts {
                    part(&mut lines, it, p);
                }
            }
            ROLE_ANNOTATION => {
                lines.push(format!("group pele-wardley-annotation {ident}"));
                part(&mut lines, it, &it.parts[0]);
                label(&mut lines, it, class, weight);
            }
            ROLE_ANNOTATIONS => {
                lines.push("group pele-wardley-annotations -".to_string());
                part(&mut lines, it, &it.parts[0]);
            }
            ROLE_ACCELERATOR | ROLE_DEACCELERATOR => {
                let kind = role.trim_start_matches("wardley.");
                lines.push(format!("group pele-wardley-{kind} {ident}"));
                part(&mut lines, it, &it.parts[0]);
                label(&mut lines, it, class, weight);
            }
            role => lines.push(format!("unknown role {role}")),
        }
        // Pele draws the marker at the end of a line first.
        if let Kind::Edge(e) = &it.kind {
            for m in [e.ends[1], e.ends[0]].into_iter().flatten() {
                lines.push(format!(
                    "marker {} {} {} {}",
                    m.kind.name(),
                    pt(m.at.x, m.at.y),
                    pt(m.dx, m.dy),
                    paint_name(it.style.stroke)
                ));
            }
        }
    }
    lines
}

/// What Pele's render draws of Wardley maps, read back out of its SVG as a line for each
/// thing drawn, in the order drawn, with every number in full, against the same lines
/// written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("wardley-render");
    assert!(f.cases.len() >= 4000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    let mut nodes = 0;
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a Wardley map");
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("a Wardley map that parses");
        nodes += m.nodes.len();
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            font_size: c.font_size.unwrap_or(16.0),
            icons: if c.icons { Some(&host_icons) } else { None },
            ..Options::default()
        };
        let got = scene_lines(&layout(&m, &p.config, &o));
        let diff = drawings.diff(k, &c.drawing, &got, None);
        failures.check(diff.is_none(), || {
            format!(
                "{:?}, max {:?}, size {:?}, icons {}: {}",
                c.src,
                c.max_width,
                c.font_size,
                c.icons,
                diff.unwrap()
            )
        });
    }
    failures.done("renders", 4000);
    assert!(nodes >= 10000, "only {nodes} nodes drawn");
}
