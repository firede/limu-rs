//! Flowcharts parsed against what Pele makes of the same cases, in the oracle's flowchart
//! fixture. The DB and the graph are written as the oracle writes Pele's, field by field, so
//! that any field that differs shows.

use limu_fixtures::load;
use serde::Deserialize;

use super::db::FlowDbOptions;
use super::graph::FlowGraph;
use super::parser::parse_flowchart;
use super::{FlowDb, build_flow_graph, default_curve, layout, parse};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::testing::pelesvg::{flow_lines, tags_last};
use crate::testing::{Enc, Failures, PeleError, same};
use crate::value::{Map, Value};

#[derive(Deserialize)]
struct Fixture {
    flowcharts: Vec<Case>,
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

/// The DB as the oracle's dumpDb writes it.
fn dump_db(db: &FlowDb) -> Value {
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
            .map(|(k, p)| Value::Array(vec![Value::String(k.into()), p.clone()]));
        v.set("props", Value::Array(props.collect()));
        v.opt("link", n.link.as_deref());
        v.opt("linkTarget", n.link_target.as_deref());
        v.opt("icon", n.icon.as_deref());
        v.opt("form", n.form.as_deref());
        v.opt("pos", n.pos.as_deref());
        v.opt("img", n.img.as_deref());
        v.opt("constraint", n.constraint.as_deref());
        v.num("assetWidth", n.asset_width);
        v.num("assetHeight", n.asset_height);
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
        if let Some(m) = &s.metadata {
            v.set("metadata", Value::Object(m.clone()));
        }
        v.done()
    });
    o.set("subgraphs", Value::Array(subgraphs.collect()));
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
    o.done()
}

/// The graph as the oracle's dumpGraph writes it.
fn dump_graph(g: &FlowGraph) -> Value {
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
        v.opt("icon", n.icon.as_deref());
        v.opt("pos", n.pos.as_deref());
        v.opt("img", n.img.as_deref());
        v.num("assetWidth", n.asset_width);
        v.num("assetHeight", n.asset_height);
        v.opt("constraint", n.constraint.as_deref());
        v.num("colorIndex", n.color_index.map(|i| i as f64));
        if let Some(m) = &n.metadata {
            v.set("metadata", Value::Object(m.clone()));
        }
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

/// What Pele's tests make of text that is no flowchart: the parser given the text as it is,
/// with no limit to its edges.
fn direct(src: &str) -> Result<(Value, Value), Error> {
    let mut db = FlowDb::new(FlowDbOptions::default());
    parse_flowchart(src, &mut db)?;
    Ok((dump_db(&db), dump_graph(&build_flow_graph(&db, None))))
}

/// A case as the oracle runs it: Pele's parseSource for a flowchart, and the graph its
/// render builds.
fn run(c: &Case) -> Result<(), String> {
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
            let d = c.direct.as_ref().ok_or("no direct result")?;
            return compare(direct(&c.src), &d.error, &d.db, &d.graph)
                .map_err(|e| format!("direct {e}"));
        }
        Err(e) => return compare(Err(e), &c.error, &c.db, &c.graph),
    };
    let ty = p.found.diagram.name();
    if c.diagram.as_deref() != Some(ty) {
        return Err(format!("type {ty}, Pele {:?}", c.diagram));
    }
    if p.found.diagram != DiagramType::Flowchart {
        let d = c.direct.as_ref().ok_or("no direct result")?;
        return compare(direct(&c.src), &d.error, &d.db, &d.graph)
            .map_err(|e| format!("direct {e}"));
    }
    let got = p
        .parse_with(false, |s| {
            parse(s, &p.config, p.title.as_deref(), p.max_edges)
        })
        .map(|db| {
            (
                dump_db(&db),
                dump_graph(&build_flow_graph(&db, default_curve(&p.config))),
            )
        });
    compare(got, &c.error, &c.db, &c.graph)
}

#[test]
fn flowcharts_match_pele() {
    let f: Fixture = load("flowchart");
    let mut failures = Failures::new();
    for c in &f.flowcharts {
        let got = run(c);
        failures.check(got.is_ok(), || format!("{:?}: {}", c.src, got.unwrap_err()));
    }
    failures.done("flowcharts", 2000);
}

/// The huge diagram from Mermaid's specs, which Mermaid skips because its parser needs
/// minutes for it, as Pele's parser test has it.
#[test]
fn huge() {
    let body = "A-->B;B-->A;".repeat(415) + "A-->B;";
    let body = body.repeat(57) + &"A-->B;B-->A;".repeat(275);
    let mut db = FlowDb::new(FlowDbOptions::default());
    parse_flowchart(&format!("graph LR;{body}"), &mut db).unwrap();
    assert_eq!(db.edges[0].kind.as_deref(), Some("arrow_point"));
    assert_eq!((db.edges.len(), db.nodes.values().count()), (47917, 2));
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
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

/// What Pele's render draws of flowcharts, read back out of its SVG as a line for each thing
/// drawn, in the order drawn, against the same lines written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("flowchart-render");
    let read = tags_last(&["clickable"]);
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a flowchart");
        let db = p
            .parse_with(false, |s| {
                parse(s, &p.config, p.title.as_deref(), p.max_edges)
            })
            .expect("a flowchart that parses");
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            padding: c.padding.unwrap_or(8.0),
            ..Options::default()
        };
        let got = flow_lines(&layout(&db, &p.config, &o));
        let diff = drawings.diff(k, &c.drawing, &got, Some(&read));
        failures.check(diff.is_none(), || {
            format!(
                "{:?}, max {:?}, padding {:?}: {}",
                c.src,
                c.max_width,
                c.padding,
                diff.unwrap()
            )
        });
    }
    failures.done("renders", 1000);
}
