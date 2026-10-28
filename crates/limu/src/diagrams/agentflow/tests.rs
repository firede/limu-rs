//! Agent flows parsed and laid out against what Pele makes of the same cases, in the oracle's
//! agentflow and agentflow-render fixtures. The DB and the graph are written as the oracle
//! writes Pele's, field by field, so that any field that differs shows.

use limu_fixtures::load;
use serde::Deserialize;

use super::db::{AgentflowDbOptions, Diagnostic, ElementPosition};
use super::graph::{AgentGraph, build_agent_graph};
use super::parser::parse_agentflow;
use super::render::{ROLE_COLLAPSED_DOTS, ROLE_COLLAPSED_RULE};
use super::{AgentflowDb, PALETTE, layout, parse};
use crate::detect::DiagramType;
use crate::draw::draw_shape;
use crate::errors::{Error, ErrorCode};
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::Paint;
use crate::scene::{Item, Kind, Outline, Part, Rect, Scene, Step};
use crate::testing::pelesvg::{
    cap_join, casing_line, classes, id, marker, stroke_ink, tags_last, text_ink,
};
use crate::testing::{Enc, Failures, PeleError, pele_d, pele_num, same};
use crate::value::{Map, Value};

#[derive(Deserialize)]
struct Fixture {
    agentflows: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    src: String,
    max_edges: Option<i64>,
    #[serde(rename = "type")]
    diagram: Option<String>,
    db: Option<Enc>,
    graph: Option<Enc>,
    error: Option<PeleError>,
    direct: Option<Direct>,
}

#[derive(Deserialize)]
struct Direct {
    db: Option<Enc>,
    graph: Option<Enc>,
    error: Option<PeleError>,
}

/// An object of fields, which leaves out the fields Pele leaves undefined.
struct Object(Map);

impl Object {
    fn new() -> Object {
        Object(Map::new())
    }

    fn set(&mut self, key: &str, v: Value) {
        self.0.insert(key, v);
    }

    fn str(&mut self, key: &str, s: &str) {
        self.set(key, Value::String(s.into()));
    }

    /// A field that may be undefined.
    fn some(&mut self, key: &str, s: Option<&str>) {
        if let Some(s) = s {
            self.str(key, s);
        }
    }

    /// A field the oracle leaves out where it is undefined or empty.
    fn opt(&mut self, key: &str, s: Option<&str>) {
        self.some(key, s.filter(|s| !s.is_empty()));
    }

    /// A value the oracle leaves out where it is undefined or null.
    fn val(&mut self, key: &str, v: Option<&Value>) {
        if let Some(v) = v.filter(|v| !v.is_null()) {
            self.set(key, v.clone());
        }
    }

    /// Metadata, which may be undefined.
    fn meta(&mut self, key: &str, m: Option<&Map>) {
        if let Some(m) = m {
            self.set(key, Value::Object(m.clone()));
        }
    }

    fn num(&mut self, key: &str, n: Option<f64>) {
        if let Some(n) = n {
            self.set(key, Value::Number(n));
        }
    }

    fn strs(&mut self, key: &str, s: &[String]) {
        self.set(key, strs(s));
    }

    fn done(self) -> Value {
        Value::Object(self.0)
    }
}

fn strs(s: &[String]) -> Value {
    Value::Array(s.iter().map(|s| Value::String(s.clone())).collect())
}

/// A position as the oracle's dumpPosition writes it. Mermaid's indices are always 0.
fn position(p: &ElementPosition) -> Vec<Value> {
    [p.start_line, p.start_column, p.end_line, p.end_column, 0, 0]
        .into_iter()
        .map(|n| Value::Number(n as f64))
        .collect()
}

/// The diagnostics as the oracle's dumpDiagnostics writes them.
fn dump_diagnostics(list: &[Diagnostic]) -> Value {
    let list = list.iter().map(|d| {
        let mut v = Object::new();
        v.str("id", d.id.as_str());
        v.str("severity", d.severity.as_str());
        v.str("message", &d.message);
        v.some("nodeId", d.node_id.as_deref());
        v.some("edgeId", d.edge_id.as_deref());
        if let Some(p) = &d.position {
            v.set("position", Value::Array(position(p)));
        }
        v.done()
    });
    Value::Array(list.collect())
}

/// The semantic model as the oracle's dumpSemantic writes it.
fn dump_semantic(db: &AgentflowDb) -> Value {
    let s = db.semantic_model();
    let mut o = Object::new();
    o.some("direction", s.direction);
    let vertices = s.vertices.iter().map(|x| {
        let mut v = Object::new();
        v.str("id", &x.id);
        v.some("label", x.label.as_deref());
        v.some("shape", x.shape.as_deref());
        v.str("vertexKind", x.vertex_kind);
        v.meta("metadata", x.metadata.as_ref());
        v.done()
    });
    o.set("vertices", Value::Array(vertices.collect()));
    let edges = s.edges.iter().map(|e| {
        let mut v = Object::new();
        v.str("start", &e.start);
        v.str("end", &e.end);
        v.str("id", &e.id);
        v.some("label", e.label.as_deref());
        v.some("type", e.kind.as_deref());
        v.some("stroke", e.stroke.as_deref());
        v.some("edgeSemantic", e.edge_semantic);
        v.num("length", e.length);
        v.meta("metadata", e.metadata.as_ref());
        v.done()
    });
    o.set("edges", Value::Array(edges.collect()));
    let sub_graphs = s.sub_graphs.iter().map(|g| {
        let mut v = Object::new();
        v.str("id", &g.id);
        v.str("type", "flow");
        v.str("title", &g.title);
        v.strs("nodes", &g.nodes);
        v.some("direction", g.direction.as_deref());
        v.meta("metadata", g.metadata.as_ref());
        v.done()
    });
    o.set("subGraphs", Value::Array(sub_graphs.collect()));
    let connectors = s.connectors.iter().map(|c| {
        let mut v = Object::new();
        v.str("id", &c.id);
        v.some("title", c.title.as_deref());
        v.meta("metadata", c.metadata.as_ref());
        v.done()
    });
    o.set("connectors", Value::Array(connectors.collect()));
    o.set("diagnostics", dump_diagnostics(s.diagnostics));
    o.done()
}

/// The DB as the oracle's dumpDb writes it.
fn dump_db(db: &AgentflowDb) -> Value {
    let mut o = Object::new();
    o.some("direction", db.direction.as_deref());
    o.opt("title", db.title.as_deref());
    o.opt("accTitle", db.acc_title.as_deref());
    o.opt("accDescr", db.acc_descr.as_deref());
    if let Some(s) = &db.default_edge_style {
        o.strs("defaultEdgeStyle", s);
    }
    o.some("defaultInterpolate", db.default_interpolate.as_deref());
    let nodes = db.nodes.values().map(|n| {
        let mut v = Object::new();
        v.str("id", &n.id);
        v.some("text", n.text.as_deref());
        v.str("labelType", n.label_type.as_str());
        v.some("type", n.kind.as_deref());
        v.strs("styles", &n.styles);
        v.strs("classes", &n.classes);
        v.some("dir", n.dir.as_deref());
        let props = n
            .props
            .iter()
            .flat_map(|m| m.iter())
            .map(|(k, p)| Value::Array(vec![Value::String(k.into()), p.clone()]));
        v.set("props", Value::Array(props.collect()));
        v.opt("link", n.link.as_deref());
        v.opt("linkTarget", n.link_target.as_deref());
        v.meta("metadata", n.metadata.as_ref());
        v.set("isConnector", Value::Bool(n.is_connector));
        v.done()
    });
    o.set("nodes", Value::Array(nodes.collect()));
    let edges = db.edges.iter().map(|e| {
        let mut v = Object::new();
        v.str("start", &e.start);
        v.str("end", &e.end);
        v.some("type", e.kind.as_deref());
        v.str("text", &e.text);
        v.str("labelType", e.label_type.as_str());
        v.some("stroke", e.stroke.as_deref());
        v.some("edgeSemantic", e.edge_semantic);
        v.num("length", e.length);
        if let Some(s) = &e.style {
            v.strs("style", s);
        }
        v.strs("classes", &e.classes);
        v.str("id", &e.id);
        v.set("isUserDefinedId", Value::Bool(e.is_user_defined_id));
        v.val("interpolate", e.interpolate.as_ref());
        v.val("animate", e.animate.as_ref());
        v.val("animation", e.animation.as_ref());
        v.meta("metadata", e.metadata.as_ref());
        v.done()
    });
    o.set("edges", Value::Array(edges.collect()));
    let subgraphs = db.subgraphs.iter().map(|s| {
        let mut v = Object::new();
        v.str("id", &s.id);
        v.strs("nodes", &s.nodes);
        v.str("title", &s.title);
        v.strs("classes", &s.classes);
        v.some("dir", s.dir.as_deref());
        v.str("labelType", s.label_type.as_str());
        v.str("type", "flow");
        v.meta("metadata", s.metadata.as_ref());
        v.done()
    });
    o.set("subgraphs", Value::Array(subgraphs.collect()));
    o.strs("connectors", &db.connectors);
    let classes = db.classes.values().map(|c| {
        let mut v = Object::new();
        v.str("id", &c.id);
        v.strs("styles", &c.styles);
        v.strs("textStyles", &c.text_styles);
        v.done()
    });
    o.set("classes", Value::Array(classes.collect()));
    let tooltips = db
        .tooltips
        .values()
        .map(|(id, t)| strs(&[id.clone(), t.clone()]));
    o.set("tooltips", Value::Array(tooltips.collect()));
    let mappings = db.mappings.iter().map(|m| {
        let mut row = vec![
            Value::String(m.id.clone()),
            Value::String(m.kind.as_str().into()),
        ];
        row.extend(position(&m.position));
        Value::Array(row)
    });
    o.set("mappings", Value::Array(mappings.collect()));
    o.set("diagnostics", dump_diagnostics(&db.diagnostics));
    o.set("semantic", dump_semantic(db));
    o.done()
}

/// The graph as the oracle's dumpGraph writes it.
fn dump_graph(g: &AgentGraph) -> Value {
    let nodes = g.nodes.iter().map(|n| {
        let mut v = Object::new();
        v.str("id", &n.id);
        v.some("label", n.label.as_deref());
        v.str("labelType", n.label_type.as_str());
        v.str("shape", &n.shape);
        v.set("isGroup", Value::Bool(n.is_group));
        v.some("parentId", n.parent_id.as_deref());
        v.strs("cssStyles", &n.css_styles);
        v.strs("cssCompiledStyles", &n.css_compiled_styles);
        v.str("cssClasses", &n.css_classes);
        v.some("dir", n.dir.as_deref());
        v.opt("link", n.link.as_deref());
        v.opt("linkTarget", n.link_target.as_deref());
        v.opt("tooltip", n.tooltip.as_deref());
        v.num("colorIndex", n.color_index.map(|i| i as f64));
        v.some("kind", n.kind);
        v.meta("metadata", n.metadata.as_ref());
        v.done()
    });
    let edges = g.edges.iter().map(|e| {
        let mut v = Object::new();
        v.str("id", &e.id);
        v.set("isUserDefinedId", Value::Bool(e.is_user_defined_id));
        v.str("start", &e.start);
        v.str("end", &e.end);
        v.str("type", &e.kind);
        v.str("label", &e.label);
        v.str("labelType", e.label_type.as_str());
        v.some("thickness", e.thickness.as_deref());
        v.some("pattern", e.pattern.as_deref());
        v.num("minlen", e.minlen);
        v.str("arrowTypeStart", &e.arrow_type_start);
        v.str("arrowTypeEnd", &e.arrow_type_end);
        v.strs("cssCompiledStyles", &e.css_compiled_styles);
        v.strs("style", &e.style);
        v.str("classes", &e.classes);
        v.val("animate", e.animate.as_ref());
        v.val("animation", e.animation.as_ref());
        v.val("curve", e.curve.as_ref().filter(|c| c.as_str() != Some("")));
        v.meta("metadata", e.metadata.as_ref());
        v.done()
    });
    let mut o = Object::new();
    o.set("nodes", Value::Array(nodes.collect()));
    o.set("edges", Value::Array(edges.collect()));
    o.done()
}

/// Whether what was parsed is what Pele made: the same error, or the same DB and graph. It
/// says how they differ where they do.
fn compare(
    got: Result<(Value, Value), Error>,
    error: &Option<PeleError>,
    db: &Option<Enc>,
    graph: &Option<Enc>,
) -> Result<(), String> {
    match (got, error) {
        (Err(e), Some(w)) if w.is(&e) => Ok(()),
        (Err(e), Some(w)) => Err(format!("error {e:?}\nPele {w:?}")),
        (Err(e), None) => Err(format!("error {e:?}, Pele had none")),
        (Ok(_), Some(w)) => Err(format!("no error, Pele {w:?}")),
        (Ok((d, g)), None) => {
            let (wd, wg) = (db.as_ref().map(|e| &e.0), graph.as_ref().map(|e| &e.0));
            if !wd.is_some_and(|w| same(&d, w)) {
                return Err(format!("db {d:?}\nPele {wd:?}"));
            }
            if !wg.is_some_and(|w| same(&g, w)) {
                return Err(format!("graph {g:?}\nPele {wg:?}"));
            }
            Ok(())
        }
    }
}

/// What Pele's tests make of text given to the parser as it is: a model with no options, and
/// its graph built twice, as a model drawn twice is, so that the diagnostics of the second
/// are not reported again.
fn direct(src: &str) -> Result<(Value, Value), Error> {
    let mut db = AgentflowDb::new(AgentflowDbOptions::default());
    parse_agentflow(src, &mut db)?;
    build_agent_graph(&mut db, PALETTE, None);
    let graph = build_agent_graph(&mut db, PALETTE, None);
    Ok((dump_db(&db), dump_graph(&graph)))
}

fn compare_direct(src: &str, d: Option<&Direct>) -> Result<(), String> {
    let d = d.ok_or("no direct result")?;
    compare(direct(src), &d.error, &d.db, &d.graph).map_err(|e| format!("direct {e}"))
}

/// A case as the oracle runs it: Pele's parseSource for an agent flow, and the graph its
/// render builds, or the parser given the text as it is.
fn run(c: &Case) -> Result<(), String> {
    if c.diagram.is_none() && c.error.is_none() {
        return compare_direct(&c.src, c.direct.as_ref());
    }
    let options = ParseOptions {
        limit: None,
        max_edges: match c.max_edges {
            Some(m) if m < 0 => None,
            Some(m) => Some(m as usize),
            None => Some(5000),
        },
        config: Map::new(),
    };
    let p = match prepare(&c.src, &options) {
        Ok(p) => p,
        Err(e) if e.code == ErrorCode::UnsupportedDiagram => {
            if c.diagram.is_some() {
                return Err(format!("no type, Pele {:?}", c.diagram));
            }
            return compare_direct(&c.src, c.direct.as_ref());
        }
        Err(e) => return compare(Err(e), &c.error, &c.db, &c.graph),
    };
    let ty = p.found.diagram.name();
    if c.diagram.as_deref() != Some(ty) {
        return Err(format!("type {ty}, Pele {:?}", c.diagram));
    }
    if p.found.diagram != DiagramType::AgentFlow {
        return compare_direct(&c.src, c.direct.as_ref());
    }
    let got = p
        .parse_with(true, |s| {
            parse(s, &p.config, p.title.as_deref(), p.line_offset, p.max_edges)
        })
        .map(|db| (dump_db(&db), dump_graph(db.graph.as_ref().unwrap())));
    compare(got, &c.error, &c.db, &c.graph)
}

#[test]
fn agentflows_match_pele() {
    let f: Fixture = load("agentflow");
    let mut failures = Failures::new();
    for c in &f.agentflows {
        let got = run(c);
        failures.check(got.is_ok(), || format!("{:?}: {}", c.src, got.unwrap_err()));
    }
    failures.done("agent flows", 4000);
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
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

/// What Pele's render draws of agent flows, read back out of its SVG as a line for each thing
/// drawn, in the order drawn, against the same lines written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("agentflow-render");
    let read = tags_last(&["clickable"]);
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("an agent flow");
        let db = p
            .parse_with(true, |s| {
                parse(s, &p.config, p.title.as_deref(), p.line_offset, p.max_edges)
            })
            .expect("an agent flow that parses");
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            ..Options::default()
        };
        let got = scene_lines(&layout(&db, &p.config, &o));
        let diff = drawings.diff(k, &c.drawing, &got, Some(&read));
        failures.check(diff.is_none(), || {
            format!("{:?}, max {:?}: {}", c.src, c.max_width, diff.unwrap())
        });
    }
    failures.done("renders", 2000);
}

/// The palette slot an item is drawn in, as the oracle reads it: the series color of its
/// outline.
fn slot_of(it: &Item) -> String {
    (0..8)
        .find(|&n| it.style.stroke == Some(Paint::series(n)))
        .map_or("no slot".into(), |n| format!("s{n}"))
}

/// The box of a part.
fn box_of(part: &Part) -> Rect {
    match &part.outline {
        Outline::Box { rect, .. } => *rect,
        Outline::Path(p) => p.bounds(),
    }
}

/// The parts of an outline, numbers as Pele writes them: the center of a node, taken from
/// its box, may be off the one it was drawn about in the last bit.
fn parts_of(parts: &[Part]) -> String {
    let num = pele_num;
    let mut out = String::new();
    for p in parts {
        let r = box_of(p);
        let rounded = matches!(p.outline, Outline::Box { rounded: true, .. });
        out.push_str(&format!(
            "{},{},{},{} {rounded} {:?}",
            num(r.x),
            num(r.y),
            num(r.w),
            num(r.h),
            p.paint
        ));
        if let Outline::Path(path) = &p.outline {
            out.push(' ');
            out.push_str(&pele_d(path));
        }
        out.push_str("; ");
    }
    out
}

/// The elements Pele's drawShape writes for the shapes agent flows draw, as the oracle reads
/// them.
fn shape_lines(shape: &str, w: f64, h: f64) -> Vec<String> {
    let num = pele_num;
    let rect = |rx: &str| {
        format!(
            "rect {},{},{},{} {rx}",
            num(-w / 2.0),
            num(-h / 2.0),
            num(w),
            num(h)
        )
    };
    let poly = |xy: &[f64]| {
        let points: Vec<String> = xy
            .chunks(2)
            .map(|p| format!("{},{}", num(p[0]), num(p[1])))
            .collect();
        format!("polygon {}", points.join(" "))
    };
    match shape {
        "rect" | "connector" => vec![rect("")],
        "rounded" => vec![rect(&num(crate::js::math::min(12.0, h / 2.0)))],
        "fr-rect" => {
            let x = w / 2.0 - 8.0;
            vec![
                rect(""),
                format!(
                    "path M{},{}V{}M{},{}V{}",
                    num(-x),
                    num(-h / 2.0),
                    num(h / 2.0),
                    num(x),
                    num(-h / 2.0),
                    num(h / 2.0)
                ),
            ]
        }
        "diam" => vec![poly(&[
            0.0,
            -h / 2.0,
            w / 2.0,
            0.0,
            0.0,
            h / 2.0,
            -w / 2.0,
            0.0,
        ])],
        "hex" => {
            let m = h / 4.0;
            vec![poly(&[
                -w / 2.0 + m,
                -h / 2.0,
                w / 2.0 - m,
                -h / 2.0,
                w / 2.0,
                0.0,
                w / 2.0 - m,
                h / 2.0,
                -w / 2.0 + m,
                h / 2.0,
                -w / 2.0,
                0.0,
            ])]
        }
        "lean-r" => {
            let s = h / 4.0;
            vec![poly(&[
                -w / 2.0 + s,
                -h / 2.0,
                w / 2.0,
                -h / 2.0,
                w / 2.0 - s,
                h / 2.0,
                -w / 2.0,
                h / 2.0,
            ])]
        }
        "lin-doc" => {
            let (x, top, base, q) = (w / 2.0, -h / 2.0, h / 2.0 - 10.0, w / 4.0);
            vec![
                format!(
                    "path M{},{}H{}V{}q{},-12 {},0t{},0Z",
                    num(-x),
                    num(top),
                    num(x),
                    num(base),
                    num(-q),
                    num(-2.0 * q),
                    num(-2.0 * q)
                ),
                format!(
                    "path M{},{}V{}",
                    num(-w / 2.0 + 8.0),
                    num(-h / 2.0),
                    num(h / 2.0 - 6.0)
                ),
            ]
        }
        _ => vec![format!("shape {shape} is not one agent flows draw")],
    }
}

/// The path of a part that is one.
fn path_of(part: &Part) -> &crate::scene::Path {
    match &part.outline {
        Outline::Path(p) => p,
        Outline::Box { .. } => panic!("a box where a path was looked for"),
    }
}

/// Writes a scene of an agent flow as the oracle reads Pele's SVG of one: a line for each
/// thing drawn, in the order drawn, with numbers as Pele's SVG writes them.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let num = pele_num;
    let pt = |x: f64, y: f64| format!("{},{}", num(x), num(y));
    let boxed =
        |r: Rect, cx: f64, cy: f64| format!("{},{},{}", pt(r.x - cx, r.y - cy), num(r.w), num(r.h));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    // Where Pele's text puts the first line of a label centered on cx, cy.
    let text_at = |l: &crate::scene::text::Label, cx: f64, cy: f64| {
        let shift = l.size * 0.35;
        format!(
            "t {}",
            pt(cx, cy - l.height / 2.0 + l.line_height / 2.0 + shift)
        )
    };
    let label_of = |it: &Item| it.label.clone().filter(|p| !p.label.lines.is_empty());
    let mut i = 0;
    while i < sc.items.len() {
        let it = &sc.items[i];
        let id = id(it.id.as_deref().unwrap_or(""));
        let classes = classes(it);
        let label = |lines: &mut Vec<String>, cx: f64, cy: f64| {
            if let Some(p) = label_of(it) {
                lines.push(text_at(&p.label, p.at.x - cx, p.at.y - cy));
            }
        };
        // The color the item's label takes on a fill a style sets, if it takes one.
        let ink = |lines: &mut Vec<String>| {
            if label_of(it).is_some() {
                let s = text_ink(it);
                if !s.is_empty() {
                    lines.push(format!("ink {s}"));
                }
            }
        };
        let kind = it
            .role
            .as_str()
            .strip_prefix("agentflow.")
            .unwrap_or(it.role.as_str());
        match kind {
            "title" => {
                if let Some(p) = label_of(it) {
                    lines.push(format!("title {}", text_at(&p.label, p.at.x, p.at.y)));
                }
            }
            "flow" => {
                lines.push(format!(
                    "flow {id} [{classes}] {} {}",
                    boxed(it.bounds, 0.0, 0.0),
                    slot_of(it)
                ));
                label(&mut lines, 0.0, 0.0);
                ink(&mut lines);
            }
            "casing" => lines.push(casing_line(it)),
            "sequence" | "reference" | "failure" => {
                let flags = if it.style.dash.is_some() {
                    " dotted"
                } else {
                    ""
                };
                let class = format!("pele-edge-{kind} {classes}");
                lines.push(format!(
                    "edge {id} [{}]{flags} {}",
                    class.trim(),
                    pele_d(path_of(&it.parts[0]))
                ));
                lines.push(format!("stroke {}", cap_join(it)));
                let s = stroke_ink(it);
                if !s.is_empty() {
                    lines.push(format!("ink {s}"));
                }
                if let Kind::Edge(e) = &it.kind {
                    for m in e.ends.iter().flatten() {
                        if let Some(s) = marker(m) {
                            lines.push(format!("m {s}"));
                        }
                    }
                }
            }
            "edge-label" => {
                lines.push(format!("label {id} {}", boxed(it.bounds, 0.0, 0.0)));
                label(&mut lines, 0.0, 0.0);
            }
            "collapsed" => {
                let b = it.bounds;
                let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
                lines.push(format!(
                    "node {id} collapsed [{classes}] translate({}) {}",
                    pt(cx, cy),
                    slot_of(it)
                ));
                if let Some(link) = &it.link {
                    lines.push(format!("link {link}"));
                }
                lines.push(format!("rect {} ", boxed(b, cx, cy)));
                if it.parts.len() != 1
                    || !matches!(it.parts[0].outline, Outline::Box { rect, rounded: true } if rect == b)
                {
                    lines.push("parts differ from the box".into());
                }
                let rule = sc.items.get(i + 1);
                let dots = sc.items.get(i + 2);
                match (rule, dots) {
                    (Some(rule), Some(dots))
                        if rule.role.as_str() == ROLE_COLLAPSED_RULE
                            && dots.role.as_str() == ROLE_COLLAPSED_DOTS =>
                    {
                        let mut d = String::new();
                        for step in path_of(&rule.parts[0]).steps() {
                            match step {
                                Step::MoveTo(p) => d += &format!("M{}", pt(p.x - cx, p.y - cy)),
                                Step::LineTo(p) => d += &format!("H{}", num(p.x - cx)),
                                _ => d += "?",
                            }
                        }
                        lines.push(format!("path {d} dashed"));
                        let mut d = String::new();
                        for step in path_of(&dots.parts[0]).steps() {
                            if let Step::MoveTo(p) = step {
                                d += &format!(
                                    "M{}a1.75,1.75 0 1 0 3.5,0a1.75,1.75 0 1 0 -3.5,0Z",
                                    pt(p.x - cx, p.y - cy)
                                );
                            }
                        }
                        lines.push(format!("path {d}"));
                        i += 2;
                    }
                    _ => lines.push("no rule and dots".into()),
                }
                label(&mut lines, cx, cy);
                ink(&mut lines);
            }
            "tool" | "task" | "decision" | "input" | "refdoc" | "connector" | "action" => {
                let b = it.bounds;
                let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
                let shape = it.shape.as_ref().map_or("", |s| s.as_str());
                let class = format!("pele-kind-{kind} {classes}");
                lines.push(format!(
                    "node {id} {shape} [{}] translate({}) {}",
                    class.trim(),
                    pt(cx, cy),
                    slot_of(it)
                ));
                if let Some(link) = &it.link {
                    lines.push(format!("link {link}"));
                }
                lines.extend(shape_lines(shape, b.w, b.h));
                if parts_of(&it.parts) != parts_of(&draw_shape(shape, cx, cy, b.w, b.h)) {
                    lines.push("parts differ from the outline".into());
                }
                label(&mut lines, cx, cy);
                ink(&mut lines);
            }
            _ => lines.push(format!("unknown role {}", it.role.as_str())),
        }
        i += 1;
    }
    lines
}
