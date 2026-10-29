//! Use case diagrams parsed against what Pele makes of the same cases, in the oracle's
//! usecase fixture, and laid out against what it draws of them, in its usecase-render
//! fixture. The model and the graph are written as the oracle writes Pele's, field by field,
//! with offsets in UTF-16 code units, so that any field that differs shows.

use limu_fixtures::load;
use serde::Deserialize;

use super::graph::{UsecaseGraph, build_usecase_graph};
use super::parser::parse_usecase;
use super::render::{
    ROLE_ACTOR, ROLE_CASING, ROLE_EDGE_LABEL, ROLE_JSON, ROLE_JSON_KEY, ROLE_JSON_VALUE, ROLE_NOTE,
    ROLE_NOTE_LINK, ROLE_RELATION, ROLE_STEREOTYPE, ROLE_SYSTEM, ROLE_TITLE, ROLE_USECASE,
};
use super::types::{GraphStatement, MetadataOccurrence, Span, UsecaseModel};
use super::{layout, parse};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::len16;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::{Cap, Join, Look, Paint};
use crate::scene::{
    Align, Item, Kind, MARKER_SIZE, Marker, MarkerType, Op, Outline, Painting, Part, Path,
    PictureSource, Placed, Scene,
};
use crate::testing::pelesvg::{id, own_fill};
use crate::testing::{Enc, Failures, PeleError, pele_d, pele_num, same};
use crate::value::{Map, Value};

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Result {
    model: Option<Enc>,
    graph: Option<Enc>,
    error: Option<PeleError>,
}

#[derive(Deserialize)]
struct Case {
    src: String,
    #[serde(rename = "type")]
    diagram: Option<String>,
    direct: Result,
    /// Whether the pipeline's result is the direct one.
    #[serde(default)]
    same: bool,
    model: Option<Enc>,
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

    fn bool(&mut self, key: &str, b: bool) {
        self.set(key, Value::Bool(b));
    }

    fn num(&mut self, key: &str, n: f64) {
        self.set(key, Value::Number(n));
    }

    /// A field the oracle leaves out where it is undefined or empty.
    fn opt(&mut self, key: &str, s: Option<&str>) {
        if let Some(s) = s.filter(|s| !s.is_empty()) {
            self.str(key, s);
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

/// Byte offsets of a source as UTF-16 code units, as Pele counts them.
struct Units(Vec<usize>);

impl Units {
    fn of(src: &str) -> Units {
        let mut u = vec![0; src.len() + 1];
        let mut n = 0;
        for (i, c) in src.char_indices() {
            u[i] = n;
            n += c.len_utf16();
            u[i + 1..i + c.len_utf8()].fill(n);
        }
        u[src.len()] = len16(src);
        Units(u)
    }

    fn span(&self, s: Span) -> Value {
        Value::Array(vec![
            Value::Number(self.0[s[0]] as f64),
            Value::Number(self.0[s[1]] as f64),
        ])
    }

    fn span_opt(&self, o: &mut Object, key: &str, s: Option<Span>) {
        if let Some(s) = s {
            o.set(key, self.span(s));
        }
    }

    fn spans(&self, ss: &[Span]) -> Value {
        Value::Array(ss.iter().map(|&s| self.span(s)).collect())
    }

    fn metadata(&self, ms: &[MetadataOccurrence]) -> Value {
        Value::Array(
            ms.iter()
                .map(|m| {
                    let mut o = Object::new();
                    o.str("key", &m.key);
                    o.set("span", self.span(m.span));
                    o.set("keySpan", self.span(m.key_span));
                    o.set("valueSpan", self.span(m.value_span));
                    o.done()
                })
                .collect(),
        )
    }

    /// A statement as JSON.stringify writes Pele's, its keys in the order Pele sets them.
    fn statement(&self, s: &GraphStatement) -> Value {
        let mut o = Object::new();
        o.str("kind", s.kind);
        o.set("span", self.span(s.span));
        if let Some(g) = &s.group {
            o.str("group", g);
        }
        self.span_opt(&mut o, "idSpan", s.id_span);
        self.span_opt(&mut o, "titleSpan", s.title_span);
        self.span_opt(&mut o, "endSpan", s.end_span);
        if let Some(r) = &s.reference {
            o.str("ref", r);
        }
        self.span_opt(&mut o, "refSpan", s.ref_span);
        if let Some(nodes) = &s.nodes {
            let nodes = nodes.iter().map(|n| {
                let mut v = Object::new();
                v.str("id", &n.id);
                v.set("span", self.span(n.span));
                v.set("idSpan", self.span(n.id_span));
                self.span_opt(&mut v, "labelSpan", n.label_span);
                if n.defines {
                    v.bool("defines", true);
                }
                self.span_opt(&mut v, "stereotypeSpan", n.stereotype_span);
                if let Some(m) = &n.metadata {
                    v.set("metadata", self.metadata(m));
                }
                if let Some(c) = &n.class_spans {
                    v.set("classSpans", self.spans(c));
                }
                v.done()
            });
            o.set("nodes", Value::Array(nodes.collect()));
        }
        if let Some(c) = &s.class_spans {
            o.set("classSpans", self.spans(c));
        }
        if let Some(children) = &s.children {
            let children = children.iter().map(|c| self.statement(c));
            o.set("children", Value::Array(children.collect()));
        }
        if let Some(m) = &s.metadata {
            o.set("metadata", self.metadata(m));
        }
        if let Some(edges) = &s.edges {
            let edges = edges.iter().map(|e| {
                let mut v = Object::new();
                v.str("id", &e.id);
                v.set("span", self.span(e.span));
                self.span_opt(&mut v, "idSpan", e.id_span);
                self.span_opt(&mut v, "labelSpan", e.label_span);
                if let Some(m) = &e.metadata {
                    v.set("metadata", self.metadata(m));
                }
                v.done()
            });
            o.set("edges", Value::Array(edges.collect()));
        }
        o.done()
    }
}

/// The model as the oracle's dumpModel writes it.
fn dump_model(m: &UsecaseModel) -> Value {
    let u = Units::of(&m.source);
    let actors = m.actors.iter().map(|a| {
        let mut v = Object::new();
        v.str("id", &a.id);
        v.str("label", &a.label);
        v.str("labelType", a.label_type.as_str());
        v.str("type", a.kind);
        v.opt("icon", a.icon.as_deref());
        v.bool("business", a.business);
        v.opt("stereotype", a.stereotype.as_deref());
        v.opt("parentId", a.parent_id.as_deref());
        v.strs("classes", &a.classes);
        v.strs("styles", &a.styles);
        v.done()
    });
    let use_cases = m.use_cases.iter().map(|c| {
        let mut v = Object::new();
        v.str("id", &c.id);
        v.str("label", &c.label);
        v.str("labelType", c.label_type.as_str());
        v.str("shape", c.shape);
        v.bool("business", c.business);
        v.opt("stereotype", c.stereotype.as_deref());
        v.opt("parentId", c.parent_id.as_deref());
        v.strs("classes", &c.classes);
        v.strs("styles", &c.styles);
        v.done()
    });
    let boundaries = m.system_boundaries.iter().map(|b| {
        let mut v = Object::new();
        v.str("id", &b.id);
        v.str("label", &b.label);
        v.str("labelType", b.label_type.as_str());
        v.str("type", b.kind);
        v.strs("members", &b.members);
        v.strs("classes", &b.classes);
        v.strs("styles", &b.styles);
        v.done()
    });
    let relationships = m.relationships.iter().map(|r| {
        let mut v = Object::new();
        v.str("id", &r.id);
        v.bool("explicitId", r.explicit_id);
        v.str("source", &r.source);
        v.str("target", &r.target);
        v.str("type", r.kind);
        v.num("arrowType", r.arrow_type as f64);
        v.num("minlen", r.minlen);
        v.strs("classes", &r.classes);
        v.strs("styles", &r.styles);
        v.bool("animate", r.animate);
        if let Some((label, kind)) = &r.label {
            v.str("label", label);
            v.str("labelType", kind.as_str());
        }
        v.opt("animation", r.animation);
        v.done()
    });
    let notes = m.notes.iter().map(|n| {
        let mut v = Object::new();
        v.str("id", &n.id);
        v.str("target", &n.target);
        v.str("label", &n.label);
        v.str("labelType", n.label_type.as_str());
        v.done()
    });
    let jsons = m.json_nodes.iter().map(|j| {
        let mut v = Object::new();
        v.str("id", &j.id);
        v.set("value", Value::Object(j.value.clone()));
        let order = j
            .property_order
            .iter()
            .map(|(p, keys)| Value::Array(vec![Value::String(p.clone()), strs(keys)]));
        v.set("propertyOrder", Value::Array(order.collect()));
        v.strs("classes", &j.classes);
        v.strs("styles", &j.styles);
        v.done()
    });
    let class_defs = m.class_defs.iter().map(|c| {
        let mut v = Object::new();
        v.str("id", &c.id);
        v.strs("styles", &c.styles);
        v.done()
    });
    let mut o = Object::new();
    o.set("actors", Value::Array(actors.collect()));
    o.set("useCases", Value::Array(use_cases.collect()));
    o.set("boundaries", Value::Array(boundaries.collect()));
    o.set("relationships", Value::Array(relationships.collect()));
    o.set("notes", Value::Array(notes.collect()));
    o.set("jsonNodes", Value::Array(jsons.collect()));
    o.set("classDefs", Value::Array(class_defs.collect()));
    o.str("direction", m.direction);
    o.opt("title", m.title.as_deref());
    o.str("accTitle", &m.acc_title);
    o.str("accDescr", &m.acc_descr);
    o.str("accDescription", &m.acc_description);
    o.set("headerSpan", u.span(m.header_span));
    let statements = m.statements.iter().map(|s| u.statement(s));
    o.set("statements", Value::Array(statements.collect()));
    o.done()
}

/// The graph as the oracle's dumpGraph writes it.
fn dump_graph(g: &UsecaseGraph) -> Value {
    let nodes = g.nodes.iter().map(|n| {
        let mut v = Object::new();
        v.str("id", &n.id);
        v.str("label", &n.label);
        v.str("labelType", n.label_type.as_str());
        v.str("shape", n.shape);
        v.bool("isGroup", n.is_group);
        v.num("padding", n.padding);
        v.str("cssClasses", &n.css_classes);
        v.strs("cssStyles", &n.css_styles);
        v.strs("cssCompiledStyles", &n.css_compiled_styles);
        v.opt("actorType", n.actor_type);
        if let Some(b) = n.business {
            v.bool("business", b);
        }
        v.opt("icon", n.icon.as_deref());
        v.opt("stereotype", n.stereotype.as_deref());
        v.opt("parentId", n.parent_id.as_deref());
        v.opt("noteTarget", n.note_target.as_deref());
        if let Some(rows) = &n.json_rows {
            let rows = rows.iter().map(|r| {
                let mut o = Object::new();
                o.str("key", &r.key);
                o.str("accessibleKey", &r.accessible_key);
                o.str("value", &r.value);
                o.done()
            });
            v.set("jsonRows", Value::Array(rows.collect()));
        }
        v.opt("boundaryType", n.boundary_type);
        v.done()
    });
    let edges = g.edges.iter().map(|e| {
        let mut v = Object::new();
        v.str("id", &e.id);
        v.str("start", &e.start);
        v.str("end", &e.end);
        v.str("relationshipType", e.relationship_type);
        v.str("pattern", e.pattern);
        v.str("arrowTypeStart", e.arrow_type_start);
        v.str("arrowTypeEnd", e.arrow_type_end);
        v.str("classes", &e.classes);
        v.strs("style", &e.style);
        v.strs("cssCompiledStyles", &e.css_compiled_styles);
        v.bool("animate", e.animate);
        v.opt("animation", e.animation);
        v.num("minlen", e.minlen);
        v.bool("explicitId", e.explicit_id);
        v.bool("internal", e.internal);
        v.opt("label", e.label.as_deref());
        v.opt("labelType", e.label_type.map(|t| t.as_str()));
        v.done()
    });
    let mut o = Object::new();
    o.set("nodes", Value::Array(nodes.collect()));
    o.set("edges", Value::Array(edges.collect()));
    o.done()
}

/// Whether what was parsed is what Pele made: the same error, or the same model and graph.
/// It says how they differ where they do.
fn compare(
    got: std::result::Result<UsecaseModel, Error>,
    error: &Option<PeleError>,
    model: &Option<Enc>,
    graph: &Option<Enc>,
) -> std::result::Result<(), String> {
    match (got, error) {
        (Err(e), Some(w)) if w.is(&e) => Ok(()),
        (Err(e), Some(w)) => Err(format!("error {e:?}\nPele {w:?}")),
        (Err(e), None) => Err(format!("error {e:?}, Pele had none")),
        (Ok(_), Some(w)) => Err(format!("no error, Pele {w:?}")),
        (Ok(m), None) => {
            let (d, g) = (dump_model(&m), dump_graph(&build_usecase_graph(&m)));
            let (wd, wg) = (model.as_ref().map(|e| &e.0), graph.as_ref().map(|e| &e.0));
            if !wd.is_some_and(|w| same(&d, w)) {
                return Err(format!("model {d:?}\nPele {wd:?}"));
            }
            if !wg.is_some_and(|w| same(&g, w)) {
                return Err(format!("graph {g:?}\nPele {wg:?}"));
            }
            Ok(())
        }
    }
}

/// A case as the oracle runs it: the parser given the raw text, and then Pele's parseSource
/// for a use case diagram.
fn run(c: &Case) -> std::result::Result<(), String> {
    let d = &c.direct;
    compare(parse_usecase(&c.src, None), &d.error, &d.model, &d.graph)
        .map_err(|e| format!("direct {e}"))?;
    let options = ParseOptions {
        limit: None,
        ..ParseOptions::default()
    };
    let p = match prepare(&c.src, &options) {
        Ok(p) => p,
        Err(e) if e.code == ErrorCode::UnsupportedDiagram && c.error.is_none() => {
            return match &c.diagram {
                Some(t) => Err(format!("no type, Pele {t:?}")),
                None => Ok(()),
            };
        }
        Err(e) => return compare(Err(e), &c.error, &None, &None),
    };
    let ty = p.found.diagram.name();
    if c.diagram.as_deref() != Some(ty) {
        return Err(format!("type {ty}, Pele {:?}", c.diagram));
    }
    if p.found.diagram != DiagramType::UseCase {
        return Ok(());
    }
    let got = p.parse_with(false, |s| parse(s, p.title.as_deref()));
    let (error, model, graph) = if c.same {
        (&d.error, &d.model, &d.graph)
    } else {
        (&c.error, &c.model, &c.graph)
    };
    compare(got, error, model, graph)
}

#[test]
fn diagrams_match_pele() {
    let f: Fixture = load("usecase");
    let mut failures = Failures::new();
    for c in &f.cases {
        let got = run(c);
        failures.check(got.is_ok(), || format!("{:?}: {}", c.src, got.unwrap_err()));
    }
    failures.done("usecase", 5000);
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

/// What Pele's render draws of use case diagrams, read back out of its SVG as one line per
/// thing drawn, in the order drawn, against the same lines written of the scene. Pele draws
/// the inside of a node about the node's middle, and the oracle moves it to where it is on
/// the page, as the scene holds it, before it writes numbers as Pele's SVG writes them.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("usecase-render");
    let (mut failures, mut skipped) = (Failures::new(), 0);
    for (k, c) in f.cases.iter().enumerate() {
        // A Rust string cannot hold a surrogate with no partner, which a JSON escape in the
        // text can write, and which JavaScript measures as wide as it is. The parser holds
        // U+FFFD in its place.
        if lone_surrogate(&c.src) {
            skipped += 1;
            continue;
        }
        let p = prepare(&c.src, &ParseOptions::default()).expect("a use case diagram");
        assert_eq!(p.found.diagram, DiagramType::UseCase, "{:?}", c.src);
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("a use case diagram that parses");
        let mut o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            ..Options::default()
        };
        if let Some(size) = c.font_size {
            o.font_size = size;
        }
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
    assert!(skipped <= 4, "{skipped} cases skipped");
    failures.done("renders", 3000);
}

/// Whether text holds a JSON escape of a surrogate that is not one of a pair.
fn lone_surrogate(s: &str) -> bool {
    let b = s.as_bytes();
    let unit = |i: usize| {
        b.get(i..i + 6)
            .filter(|e| e.starts_with(b"\\u"))
            .and_then(|e| std::str::from_utf8(&e[2..]).ok())
            .and_then(|h| u16::from_str_radix(h, 16).ok())
    };
    let mut i = 0;
    while i < b.len() {
        match unit(i) {
            Some(0xd800..=0xdbff)
                if unit(i + 6).is_some_and(|l| (0xdc00..=0xdfff).contains(&l)) =>
            {
                i += 12
            }
            Some(0xd800..=0xdfff) => return true,
            _ => i += 1,
        }
    }
    false
}

/// A paint as Pele's SVG names it, "-" for none given, and "?" for one it has no name for.
fn paint_name(p: Option<Paint>) -> &'static str {
    let Some(p) = p else { return "-" };
    [
        (Paint::BG, "var(--_bg)"),
        (Paint::FG, "var(--_fg)"),
        (Paint::MUTED, "var(--_m)"),
        (Paint::LINE, "var(--_l)"),
        (Paint::SURFACE, "var(--_s)"),
        (Paint::SURFACE_ALT, "var(--_a)"),
        (Paint::SURFACE_ALT.with_opacity(0.5), "var(--_a)/0.5"),
        (Paint::BORDER, "var(--_b)"),
        (Paint::NONE, "none"),
    ]
    .into_iter()
    .find(|&(q, _)| q == p)
    .map_or("?", |(_, name)| name)
}

/// The color Pele's ink picks, by the theme's paint the default look settles it as.
fn ink_name(p: Option<Paint>) -> &'static str {
    match p {
        Some(p) if p == Paint::FG => "var(--pele-fg-on-light,#1f1f1f)",
        Some(p) if p == Paint::BG => "var(--pele-fg-on-dark,#fff)",
        _ => "",
    }
}

/// The fill Pele writes on an item's text: the color ink picks where the default look picks
/// one, on the item's own fill or on the fill under its text, and the diagram's own
/// otherwise. The diagram's own text is taken as a paint ink never picks, so that a color
/// picked shows even where it is the same as the diagram's.
fn text_paint(it: &Item) -> &'static str {
    let look = Look::default();
    let mut own = it.style.clone();
    own.text = Some(Paint::SURFACE_ALT);
    let mut bare = own.clone();
    (bare.on_fill, bare.ground, bare.text_ground) = (false, None, None);
    let got = look.cascade(it.role.as_str(), &own, &it.author).text;
    if got != look.cascade(it.role.as_str(), &bare, &it.author).text {
        return ink_name(got);
    }
    paint_name(it.style.text)
}

/// How an item's strokes end and turn their corners, after its author's style, as the oracle
/// reads them: " cap=" and " join=" where they are not SVG's initial butt and miter. A
/// marker's own join, where it has one, is over the item's. Without corners there is no join.
fn ends(it: &Item, join: Option<Join>, corners: bool) -> String {
    let mut s = it.author.over(&it.style);
    if join.is_some() {
        s.join = join;
    }
    if !corners {
        s.join = None;
    }
    let mut out = String::new();
    if let Some(cap) = s.cap.filter(|&c| c != Cap::Butt) {
        out.push_str(&format!(" cap={cap}"));
    }
    if let Some(join) = s.join.filter(|&j| j != Join::Miter) {
        out.push_str(&format!(" join={join}"));
    }
    out
}

/// How a part of an item is painted, as the oracle reads Pele's fill and stroke, and where it
/// has a stroke, its ends, and its joins where it has corners.
fn part_paint(it: &Item, part: &Part, corners: bool) -> String {
    let stroke = paint_name(it.style.stroke);
    let mut s = match part.paint {
        Painting::StrokeOnly => format!("none {stroke}"),
        Painting::FillOnly => return format!("{} -", paint_name(own_fill(it))),
        Painting::Hollow => format!("var(--_bg) {stroke}"),
        _ => format!("{} {stroke}", paint_name(own_fill(it))),
    };
    if stroke != "-" && stroke != "none" {
        s.push_str(&ends(it, None, corners));
    }
    s
}

/// The class attribute Pele writes on an item, after its pele- classes: the classes it gives
/// a node or a boundary of its own, which the item's role and shape say, then those of the
/// diagram's author. The class of an edge's animation is left out on both sides.
fn classes(it: &Item) -> String {
    let shape = it.shape.as_ref().map_or("", |s| s.as_str());
    let business = shape.ends_with("-business");
    let shape = shape.trim_end_matches("-business");
    let mut names: Vec<String> = match it.role.as_str() {
        ROLE_SYSTEM => vec!["system-boundary".into(), format!("system-boundary-{shape}")],
        ROLE_ACTOR => {
            let kind = shape.trim_start_matches("actor").trim_start_matches('-');
            let kind = if kind.is_empty() { "normal" } else { kind };
            vec!["usecase-actor".into(), format!("usecase-actor-{kind}")]
        }
        ROLE_USECASE => vec!["usecase-element".into(), format!("usecase-{shape}")],
        ROLE_NOTE => vec!["usecase-note".into()],
        ROLE_JSON => vec!["usecase-json-table".into()],
        _ => Vec::new(),
    };
    if business {
        names.push("usecase-business".into());
    }
    names.extend(
        it.class
            .iter()
            .filter(|c| !c.starts_with("edge-animation-"))
            .cloned(),
    );
    names.join(" ")
}

/// A point as the oracle writes one.
fn pt(x: f64, y: f64) -> String {
    format!("{},{}", pele_num(x), pele_num(y))
}

/// The subpaths of a path, as the oracle writes them: each its commands, and an ellipse as
/// Path::ellipse draws it, from its right end, four quarters, closed, by its middle and radii.
fn subpaths(p: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Vec<(Op, &[f64])> = Vec::new();
    let flush = |cur: &mut Vec<(Op, &[f64])>, out: &mut Vec<String>| {
        if cur.is_empty() {
            return;
        }
        let ops: Vec<Op> = cur.iter().map(|&(op, _)| op).collect();
        if ops
            == [
                Op::MoveTo,
                Op::CubeTo,
                Op::CubeTo,
                Op::CubeTo,
                Op::CubeTo,
                Op::Close,
            ]
        {
            let (m, c) = (cur[0].1, cur[1].1);
            let (cx, cy) = (c[4], m[1]);
            out.push(format!(
                "ellipse {} {}",
                pt(cx, cy),
                pt(m[0] - cx, c[5] - cy)
            ));
        } else {
            let words: Vec<String> = cur
                .iter()
                .map(|&(op, a)| {
                    let mut s = String::from(match op {
                        Op::MoveTo => "M",
                        Op::LineTo => "L",
                        Op::QuadTo => "Q",
                        Op::CubeTo => "C",
                        Op::Close => "Z",
                    });
                    for xy in a.chunks(2) {
                        s.push(' ');
                        s.push_str(&pt(xy[0], xy[1]));
                    }
                    s
                })
                .collect();
            out.push(words.join(" "));
        }
        cur.clear();
    };
    let mut at = 0;
    for &op in p.ops() {
        if op == Op::MoveTo {
            flush(&mut cur, &mut out);
        }
        cur.push((op, &p.args()[at..at + op.args()]));
        at += op.args();
    }
    flush(&mut cur, &mut out);
    out
}

/// The path of a part that is one.
fn path_of(part: &Part) -> &Path {
    match &part.outline {
        Outline::Path(p) => p,
        Outline::Box { .. } => panic!("a box where a path was looked for"),
    }
}

/// The outline Pele's marker draws, as the oracle reads it, or none for a marker it draws no
/// outline of.
fn marker(m: &Marker) -> Option<String> {
    let (x, y, dx, dy) = (m.at.x, m.at.y, m.dx, m.dy);
    let (nx, ny) = (-dy, dx);
    match m.kind {
        MarkerType::ArrowPoint => {
            let (bx, by) = (x - dx * MARKER_SIZE, y - dy * MARKER_SIZE);
            let half = MARKER_SIZE * 0.42;
            Some(format!(
                "M {} L {} L {} Z",
                pt(x, y),
                pt(bx + nx * half, by + ny * half),
                pt(bx - nx * half, by - ny * half)
            ))
        }
        MarkerType::ArrowCircle => Some(format!("o {}", pt(x - dx * 4.0, y - dy * 4.0))),
        MarkerType::ArrowCross => {
            let (cx, cy) = (x - dx * 4.0, y - dy * 4.0);
            const A: f64 = 3.5;
            let (ux, uy) = ((dx + nx) * A, (dy + ny) * A);
            let (vx, vy) = ((dx - nx) * A, (dy - ny) * A);
            Some(format!(
                "M {} L {} M {} L {}",
                pt(cx - ux, cy - uy),
                pt(cx + ux, cy + uy),
                pt(cx - vx, cy - vy),
                pt(cx + vx, cy + vy)
            ))
        }
        // The scene's own outline, which ports Pele's extension.
        MarkerType::UsecaseExtension => Some(
            m.outline()
                .iter()
                .flat_map(|part| subpaths(path_of(part)))
                .collect::<Vec<_>>()
                .join(" "),
        ),
        _ => None,
    }
}

/// Writes a scene as the oracle reads Pele's SVG.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    // Where Pele's text puts the first line of a label, and the paint the item that holds it
    // gives its text. Pele centers every text of a use case diagram.
    let text_at = |owner: &Item, p: &Placed| {
        let l = &p.label;
        let shift = l.size * 0.35;
        let mut s = format!(
            "t {} {}",
            pt(
                p.at.x,
                p.at.y - l.height / 2.0 + l.line_height / 2.0 + shift
            ),
            text_paint(owner)
        );
        if p.align != Align::Middle {
            s.push_str(" not centered");
        }
        s
    };
    let label = |lines: &mut Vec<String>, owner: &Item| {
        if let Some(p) = owner.label.as_ref().filter(|p| !p.label.lines.is_empty()) {
            lines.push(text_at(owner, p));
        }
    };
    let shape = |lines: &mut Vec<String>, it: &Item, part: &Part| match &part.outline {
        Outline::Box { rect: r, rounded } => {
            let mut s = format!("rect {},{}", pt(r.x, r.y), pt(r.w, r.h));
            if *rounded {
                s.push_str(" r");
            }
            lines.push(format!("{s} {}", part_paint(it, part, true)));
        }
        Outline::Path(p) => {
            for s in subpaths(p) {
                // A circle and a single straight line have no corners.
                let ellipse = s.starts_with("ellipse");
                let single =
                    s.starts_with("M ") && s.matches(' ').count() == 3 && s.contains(" L ");
                let paint = part_paint(it, part, !ellipse && !single);
                if ellipse {
                    lines.push(format!("{s} {paint}"));
                } else {
                    lines.push(format!("path {s} {paint}"));
                }
            }
        }
    };
    let mut i = 0;
    while i < sc.items.len() {
        let it = &sc.items[i];
        let id = id(it.id.as_deref().unwrap_or(""));
        let shape_name = it.shape.as_ref().map_or("", |s| s.as_str());
        match it.role.as_str() {
            ROLE_TITLE => {
                if let Some(p) = &it.label {
                    lines.push(format!("title {}", text_at(it, p)));
                }
            }
            ROLE_SYSTEM => {
                lines.push(format!("cluster {id} [{}]", classes(it)));
                for part in &it.parts {
                    shape(&mut lines, it, part);
                }
                label(&mut lines, it);
            }
            ROLE_CASING => {
                // Pele's casings follow its boundaries, and the oracle reads them with the
                // last: the path, in the casings' paint, and the outlines of the relation's
                // markers, filled and stroked in the background, as a group with round ends
                // and corners draws them, but for a cross, which is stroked alone.
                for s in subpaths(path_of(&it.parts[0])) {
                    lines.push(format!("path {s} - -"));
                }
                for part in &it.parts[1..] {
                    for s in subpaths(path_of(part)) {
                        if part.paint == Painting::StrokeOnly {
                            lines.push(format!("path {s} - -"));
                        } else if s.starts_with("ellipse") {
                            lines.push(format!("{s} var(--_bg) var(--_bg) cap=round"));
                        } else {
                            lines.push(format!(
                                "path {s} var(--_bg) var(--_bg) cap=round join=round"
                            ));
                        }
                    }
                }
            }
            ROLE_RELATION | ROLE_NOTE_LINK => {
                let dash = it.style.dash.as_ref().map_or("-".to_string(), |d| {
                    d.iter().map(|&v| pele_num(v)).collect::<Vec<_>>().join(" ")
                });
                if (it.role.as_str() == ROLE_NOTE_LINK) != (shape_name == "note") {
                    lines.push(format!(
                        "role {} of a line of shape {shape_name}",
                        it.role.as_str()
                    ));
                }
                let mut ink = String::new();
                let s = Look::default().cascade(it.role.as_str(), &it.style, &it.author);
                if s.stroke != it.author.over(&it.style).stroke {
                    // The color ink picks on the fill the edge lies in.
                    ink = format!(" ink={}", ink_name(s.stroke));
                }
                lines.push(format!(
                    "edge {id} {shape_name} [{}] {dash}{ink}",
                    classes(it)
                ));
                for s in subpaths(path_of(&it.parts[0])) {
                    lines.push(format!("path {s}{}", ends(it, None, true)));
                }
                if let Kind::Edge(e) = &it.kind {
                    for m in e.ends.iter().flatten() {
                        if let Some(mut s) = marker(m) {
                            if matches!(
                                m.kind,
                                MarkerType::ArrowCross | MarkerType::UsecaseExtension
                            ) {
                                // The markers Pele strokes.
                                s.push_str(&ends(it, m.kind.join(), true));
                            }
                            lines.push(format!("m {s}"));
                        }
                    }
                }
            }
            ROLE_EDGE_LABEL => {
                let r = it.bounds;
                let mut patch = Path::new();
                patch.round_rect(r, 3.0);
                if pele_d(path_of(&it.parts[0])) != pele_d(&patch) {
                    lines.push("patch not the label's box".into());
                }
                lines.push(format!("label {id} {},{}", pt(r.x, r.y), pt(r.w, r.h)));
                label(&mut lines, it);
            }
            ROLE_ACTOR | ROLE_USECASE | ROLE_NOTE | ROLE_JSON => {
                // Where the node's middle is, which Pele's SVG moves what is in the node by,
                // is checked through where that is drawn.
                lines.push(format!("node {id} [{}]", classes(it)));
                // What follows a node in the scene and is drawn inside it in Pele's SVG.
                let mut stereotype = None;
                let mut cells = Vec::new();
                while let Some(next) = sc.items.get(i + 1) {
                    match next.role.as_str() {
                        ROLE_STEREOTYPE => stereotype = Some(next),
                        ROLE_JSON_KEY | ROLE_JSON_VALUE => cells.push(next),
                        _ => break,
                    }
                    i += 1;
                }
                for part in &it.parts {
                    shape(&mut lines, it, part);
                }
                if let Some(pic) = &it.picture
                    && let PictureSource::Icon(name) = &pic.source
                {
                    let r = pic.bounds;
                    lines.push(format!(
                        "icon {} {},{}",
                        self::id(name),
                        pt(r.x, r.y),
                        pt(r.w, r.h)
                    ));
                }
                if let Some(s) = stereotype {
                    label(&mut lines, s);
                }
                label(&mut lines, it);
                for c in cells {
                    label(&mut lines, c);
                }
            }
            role => lines.push(format!("unknown role {role}")),
        }
        i += 1;
    }
    lines
}

/// The slash of a business use case starts where it meets the oval, the middle's y and the
/// oval's half height times a root, each rounded apart, as JavaScript works it out. The
/// render suite writes the slash to two decimals, as Pele does, and so cannot see the last
/// bit a fused multiply-add would change. This checks it to the bit, for labels where fusing
/// would change it.
#[test]
fn business_slash_keeps_the_product_apart() {
    let mut reached = 0;
    for name in [
        "Use",
        "Pay the bill",
        "Return goods now",
        "Order",
        "Check out the cart",
        "W",
    ] {
        let src = format!("usecase-beta\nU({name})@{{ business: true }}\n");
        let p = prepare(&src, &ParseOptions::default()).unwrap();
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .unwrap();
        let sc = layout(&m, &p.config, &Options::default());
        for it in &sc.items {
            if it.shape.as_ref().map(|s| s.as_str()) != Some("ellipse-business") {
                continue;
            }
            let (w, h) = (it.bounds.w, it.bounds.h);
            // The oval starts at its right end, level with its middle.
            let y = path_of(&it.parts[0]).args()[1];
            let q = (w / 2.0 - 15.0) / (w / 2.0);
            let root = (1.0 - q * q).sqrt();
            let want = y + (h / 2.0) * root;
            let got = path_of(&it.parts[1]).args()[1];
            assert_eq!(
                got.to_bits(),
                want.to_bits(),
                "{name:?}: slash starts at y {got}"
            );
            if (h / 2.0).mul_add(root, y) != want {
                reached += 1;
            }
        }
    }
    assert!(
        reached > 0,
        "no label where a fused multiply-add would differ"
    );
}
