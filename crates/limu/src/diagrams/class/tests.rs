//! Class diagrams parsed and drawn against what Pele makes of the same cases, in the
//! oracle's class and class-render fixtures. The DB and the graph are written as the oracle
//! writes Pele's, field by field, so that any field that differs shows.

use limu_fixtures::{Drawing, load};
use serde::Deserialize;

use super::db::{ClassDb, RelationEnd};
use super::graph::{ClassGraph, build_class_graph};
use super::members::{ClassMember, MemberType};
use super::parser::parse_class_diagram;
use super::render::*;
use super::{hierarchical, layout, parse};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::layout::{Monospace, Options};
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::{Look, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{Item, Kind, Marker, MarkerType, Outline, Painting, Scene};
use crate::testing::pelesvg::{classes, id, tags_last, text_ink};
use crate::testing::{Enc, Failures, PeleError, pele_d, pele_num, same};
use crate::value::{Map, Value};

#[derive(Deserialize)]
struct Fixture {
    diagrams: Vec<Case>,
    members: Vec<MemberCase>,
}

#[derive(Deserialize)]
struct Case {
    src: String,
    #[serde(rename = "type")]
    diagram: Option<String>,
    db: Option<Enc>,
    graph: Option<Enc>,
    flat: Option<Enc>,
    error: Option<PeleError>,
    direct: Option<Direct>,
}

#[derive(Deserialize)]
struct Direct {
    db: Option<Enc>,
    graph: Option<Enc>,
    flat: Option<Enc>,
    error: Option<PeleError>,
}

#[derive(Deserialize)]
struct MemberCase {
    input: String,
    kind: String,
    member: Enc,
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

    /// A field the oracle leaves out where it is undefined or empty.
    fn opt(&mut self, key: &str, s: Option<&str>) {
        if let Some(s) = s.filter(|s| !s.is_empty()) {
            self.str(key, s);
        }
    }

    fn num(&mut self, key: &str, n: f64) {
        self.set(key, Value::Number(n));
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

/// A member as the oracle's dumpMember writes it.
fn dump_member(m: &ClassMember) -> Value {
    let mut o = Object::new();
    o.str("id", &m.id);
    o.str("memberType", m.member_type.as_str());
    o.str("visibility", m.visibility);
    o.str("classifier", m.classifier);
    o.str("parameters", &m.parameters);
    o.str("returnType", &m.return_type);
    o.str("display", &m.display_text());
    o.done()
}

fn members(list: &[ClassMember]) -> Value {
    Value::Array(list.iter().map(dump_member).collect())
}

fn dump_end(e: RelationEnd) -> Value {
    match e {
        Some(t) => Value::Number(f64::from(t)),
        None => Value::String("none".into()),
    }
}

/// The DB as the oracle's dumpDb writes it.
fn dump_db(db: &ClassDb) -> Value {
    let mut o = Object::new();
    o.str("direction", &db.direction);
    o.opt("title", db.title.as_deref());
    o.opt("accTitle", db.acc_title.as_deref());
    o.opt("accDescr", db.acc_descr.as_deref());
    let classes = db.classes.values().map(|c| {
        let mut v = Object::new();
        v.str("id", &c.id);
        v.str("type", &c.kind);
        v.str("label", &c.label);
        v.str("cssClasses", &c.css_classes);
        v.set("methods", members(&c.methods));
        v.set("members", members(&c.members));
        v.strs("annotations", &c.annotations);
        v.strs("styles", &c.styles);
        v.opt("parent", c.parent.as_deref());
        v.opt("link", c.link.as_deref());
        v.opt("linkTarget", c.link_target.as_deref());
        if c.have_callback {
            v.set("haveCallback", Value::Bool(true));
        }
        v.opt("tooltip", c.tooltip.as_deref());
        v.done()
    });
    o.set("classes", Value::Array(classes.collect()));
    let relations = db.relations.iter().map(|r| {
        let mut v = Object::new();
        v.str("id1", &r.id1);
        v.str("id2", &r.id2);
        v.str("relationTitle1", &r.relation_title1);
        v.str("relationTitle2", &r.relation_title2);
        if let Some(t) = &r.title {
            v.str("title", t);
        }
        v.set("type1", dump_end(r.type1));
        v.set("type2", dump_end(r.type2));
        v.num("lineType", f64::from(r.line_type));
        v.done()
    });
    o.set("relations", Value::Array(relations.collect()));
    let notes = db.notes.values().map(|n| {
        let mut v = Object::new();
        v.str("id", &n.id);
        v.opt("class", n.class.as_deref());
        v.str("text", &n.text);
        v.num("index", n.index as f64);
        v.opt("parent", n.parent.as_deref());
        v.done()
    });
    o.set("notes", Value::Array(notes.collect()));
    let namespaces = db.namespaces.values().map(|ns| {
        let mut v = Object::new();
        v.str("id", &ns.id);
        v.str("label", &ns.label);
        v.strs("classes", &ns.classes);
        v.strs("notes", &ns.notes);
        v.strs("children", &ns.children);
        v.opt("parent", ns.parent.as_deref());
        v.set("explicit", Value::Bool(ns.explicit));
        v.done()
    });
    o.set("namespaces", Value::Array(namespaces.collect()));
    let interfaces = db.interfaces.iter().map(|x| {
        let mut v = Object::new();
        v.str("id", &x.id);
        v.str("label", &x.label);
        v.str("classId", &x.class_id);
        v.done()
    });
    o.set("interfaces", Value::Array(interfaces.collect()));
    let style_classes = db.style_classes.values().map(|s| {
        let mut v = Object::new();
        v.str("id", &s.id);
        v.strs("styles", &s.styles);
        v.strs("textStyles", &s.text_styles);
        v.done()
    });
    o.set("styleClasses", Value::Array(style_classes.collect()));
    o.done()
}

/// The graph as the oracle's dumpGraph writes it.
fn dump_graph(db: &ClassDb, g: &ClassGraph) -> Value {
    let nodes = g.nodes.iter().map(|n| {
        let mut v = Object::new();
        v.str("id", &n.id);
        v.str("label", &n.label);
        v.set("isGroup", Value::Bool(n.is_group));
        v.str("shape", n.shape);
        if let Some(p) = &n.parent_id {
            v.str("parentId", p);
        }
        v.num("parent", f64::from(n.parent));
        if let Some(c) = n.class {
            v.str("classId", &db.classes[c].id);
        }
        if let Some(c) = n.color_index {
            v.num("colorIndex", c as f64);
        }
        v.done()
    });
    let edges = g.edges.iter().map(|e| {
        let mut v = Object::new();
        v.str("id", &e.id);
        v.str("start", &e.start);
        v.str("end", &e.end);
        v.num("source", f64::from(e.source));
        v.num("target", f64::from(e.target));
        if let Some(l) = &e.label {
            v.str("label", l);
        }
        v.str("arrowTypeStart", e.arrow_type_start);
        v.str("arrowTypeEnd", e.arrow_type_end);
        v.str("startLabelRight", &e.start_label_right);
        v.str("endLabelLeft", &e.end_label_left);
        v.str("pattern", e.pattern);
        v.str("classes", e.classes);
        v.done()
    });
    let mut o = Object::new();
    o.set("nodes", Value::Array(nodes.collect()));
    o.set("edges", Value::Array(edges.collect()));
    o.done()
}

/// What the oracle writes of a DB: it, its graph, and, where it has namespaces and nests
/// them, its graph with them not nested.
type Dumped = (Value, Value, Option<Value>);

fn dump(db: &ClassDb, hierarchical: bool) -> Dumped {
    let flat = (hierarchical && db.namespaces.len() > 0)
        .then(|| dump_graph(db, &build_class_graph(db, false)));
    (
        dump_db(db),
        dump_graph(db, &build_class_graph(db, hierarchical)),
        flat,
    )
}

/// Whether what was parsed is what Pele made: the same error, or the same DB and graphs. It
/// says how they differ where they do.
fn compare(
    got: Result<Dumped, Error>,
    error: &Option<PeleError>,
    db: &Option<Enc>,
    graph: &Option<Enc>,
    flat: &Option<Enc>,
) -> Result<(), String> {
    match (got, error) {
        (Err(e), Some(w)) if w.is(&e) => Ok(()),
        (Err(e), Some(w)) => Err(format!("error {e:?}\nPele {w:?}")),
        (Err(e), None) => Err(format!("error {e:?}, Pele had none")),
        (Ok(_), Some(w)) => Err(format!("no error, Pele {w:?}")),
        (Ok((d, g, f)), None) => {
            let want = |e: &Option<Enc>| e.as_ref().map(|e| e.0.clone());
            if !want(db).is_some_and(|w| same(&d, &w)) {
                return Err(format!("db {d:?}\nPele {:?}", want(db)));
            }
            if !want(graph).is_some_and(|w| same(&g, &w)) {
                return Err(format!("graph {g:?}\nPele {:?}", want(graph)));
            }
            let same_flat = match (&f, want(flat)) {
                (None, None) => true,
                (Some(f), Some(w)) => same(f, &w),
                _ => false,
            };
            if !same_flat {
                return Err(format!("flat graph {f:?}\nPele {:?}", want(flat)));
            }
            Ok(())
        }
    }
}

/// What Pele's tests make of text that is no class diagram: the parser given the text as it
/// is.
fn direct(src: &str) -> Result<Dumped, Error> {
    let mut db = ClassDb::default();
    parse_class_diagram(src, &mut db)?;
    Ok(dump(&db, true))
}

/// A case as the oracle runs it: Pele's parseSource for a class diagram, and the graphs its
/// render builds.
fn run(c: &Case) -> Result<(), String> {
    let options = ParseOptions {
        limit: None,
        max_edges: None,
        config: Map::new(),
    };
    let p = match prepare(&c.src, &options) {
        Ok(p) => p,
        Err(e) if e.code == ErrorCode::UnsupportedDiagram => {
            if c.diagram.is_some() {
                return Err(format!("no type, Pele {:?}", c.diagram));
            }
            let d = c.direct.as_ref().ok_or("no direct result")?;
            return compare(direct(&c.src), &d.error, &d.db, &d.graph, &d.flat)
                .map_err(|e| format!("direct {e}"));
        }
        Err(e) => return compare(Err(e), &c.error, &c.db, &c.graph, &c.flat),
    };
    let ty = p.found.diagram.name();
    if c.diagram.as_deref() != Some(ty) {
        return Err(format!("type {ty}, Pele {:?}", c.diagram));
    }
    if p.found.diagram != DiagramType::Class {
        let d = c.direct.as_ref().ok_or("no direct result")?;
        return compare(direct(&c.src), &d.error, &d.db, &d.graph, &d.flat)
            .map_err(|e| format!("direct {e}"));
    }
    let got = p
        .parse_with(false, |s| parse(s, p.title.as_deref()))
        .map(|db| dump(&db, hierarchical(&p.config)));
    compare(got, &c.error, &c.db, &c.graph, &c.flat)
}

#[test]
fn class_diagrams_match_pele() {
    let f: Fixture = load("class");
    let mut failures = Failures::new();
    for c in &f.diagrams {
        let got = run(c);
        failures.check(got.is_ok(), || format!("{:?}: {}", c.src, got.unwrap_err()));
    }
    failures.done("class diagrams", 5000);
}

#[test]
fn members_match_pele() {
    let f: Fixture = load("class");
    let mut failures = Failures::new();
    for c in &f.members {
        let kind = if c.kind == "method" {
            MemberType::Method
        } else {
            MemberType::Attribute
        };
        let got = dump_member(&ClassMember::new(&c.input, kind));
        failures.check(same(&got, &c.member.0), || {
            format!("{} {:?}: {got:?}\nPele {:?}", c.kind, c.input, c.member.0)
        });
    }
    failures.done("members", 1000);
}

/// Lays out a class diagram's text with options.
fn laid_out(src: &str, o: &Options) -> Scene {
    let p = prepare(src, &ParseOptions::default()).expect("a class diagram");
    let db = p
        .parse_with(false, |s| parse(s, p.title.as_deref()))
        .expect("a class diagram that parses");
    layout(&db, &p.config, o)
}

/// A class's dividers take only the stroke its author sets, as Pele draws them, and not the
/// width, the dash or the fill.
#[test]
fn dividers_take_only_the_stroke() {
    let src = "classDiagram\nclass A {\n+int a\n+go()\n}\nstyle A fill:#f00,stroke:#00f,stroke-width:4px,stroke-dasharray:5\n";
    let sc = laid_out(src, &Options::default());
    let class = sc.items.iter().find(|it| it.role == ROLE_CLASS);
    let stroke = class.and_then(|it| it.author.stroke);
    assert!(stroke.is_some(), "a class with the author's stroke");
    let divider = sc.items.iter().find(|it| it.role == ROLE_DIVIDER);
    let divider = divider.expect("a divider");
    assert_eq!(
        divider.author,
        Style {
            stroke,
            ..Style::default()
        }
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
    /// Whether the members are set in the monospace face, as Pele's monospace option lists
    /// them.
    #[serde(default)]
    mono: bool,
    #[serde(flatten)]
    drawing: Drawing,
}

/// What Pele's render draws of class diagrams, read back out of its SVG as a line for each
/// thing drawn, in the order drawn, against the same lines written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("class-render");
    let read = tags_last(&["clickable"]);
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            monospace: if c.mono {
                Monospace::MEMBERS
            } else {
                Monospace::NONE
            },
            ..Options::default()
        };
        let got = class_lines(&laid_out(&c.src, &o));
        let diff = drawings.diff(k, &c.drawing, &got, Some(&read));
        failures.check(diff.is_none(), || {
            format!(
                "{:?}, max {:?}, mono {}: {}",
                c.src,
                c.max_width,
                c.mono,
                diff.unwrap()
            )
        });
    }
    failures.done("renders", 2000);
}

/// Text as Pele writes it into its SVG, the characters XML does not allow dropped.
fn pele_text(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            !(c < ' ' && !matches!(c, '\t' | '\n' | '\r')
                || matches!(c, '\x7f' | '\u{fffe}' | '\u{ffff}'))
        })
        .collect()
}

/// The text of the first line of a label.
fn spans(l: &Label) -> String {
    l.lines[0].iter().map(|s| s.text.as_str()).collect()
}

/// How an item's lines end and turn their corners, as the oracle reads them, "cap/join": its
/// author's style over its own, and a marker's own join over both.
fn pen(it: &Item, marker: Option<MarkerType>) -> String {
    let r = Look::default().resolve(it.role.as_str(), &it.style, &it.author);
    let join = marker.and_then(MarkerType::join).unwrap_or(r.join);
    format!("{}/{}", r.cap, join)
}

/// The marker Pele draws at an end, as the oracle reads it: how it is painted and its
/// outline, or the middle of a lollipop's ring.
fn pele_marker(m: &Marker) -> String {
    let parts = m.outline();
    if parts.len() != 1 {
        return format!("m unknown {}", m.kind.name());
    }
    let paint = match parts[0].paint {
        Painting::FillOnly => "fill",
        Painting::FillAndStroke => "fill+stroke",
        _ => "stroke",
    };
    if m.kind == MarkerType::ClassLollipop {
        return format!(
            "m {paint} o {},{}",
            pele_num(m.at.x - m.dx * 5.0),
            pele_num(m.at.y - m.dy * 5.0)
        );
    }
    match &parts[0].outline {
        Outline::Path(p) => format!("m {paint} {}", pele_d(p)),
        Outline::Box { .. } => format!("m unknown {}", m.kind.name()),
    }
}

/// The path of an item's part.
fn path(it: &Item, i: usize) -> String {
    match &it.parts[i].outline {
        Outline::Path(p) => pele_d(p),
        Outline::Box { .. } => "a box".into(),
    }
}

/// Writes the scene of a class diagram as the render oracle reads Pele's SVG of one: a line
/// for each thing drawn, in the order drawn, with numbers as Pele's SVG writes them.
fn class_lines(sc: &Scene) -> Vec<String> {
    let num = pele_num;
    let pt = |x: f64, y: f64| format!("{},{}", num(x), num(y));
    let boxed = |r: crate::scene::Rect| format!("{},{},{}", pt(r.x, r.y), num(r.w), num(r.h));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    // Where Pele's labelSvg puts the lines of a label centered on cx, cy, and the runs of a
    // line with an icon in it, and then the icons.
    let text_at = |l: &Label, cx: f64, cy: f64, icons: &mut Vec<String>| {
        let shift = l.size * 0.35;
        let mut top = cy - l.height / 2.0;
        let mut at = Vec::new();
        for (i, line) in l.lines.iter().enumerate() {
            let y = top + l.line_height / 2.0 + shift;
            if line.iter().all(|s| s.icon.is_none()) {
                at.push(pt(cx, y));
            } else {
                let mut x = cx - l.widths[i] / 2.0;
                for s in line {
                    if let Some(icon) = &s.icon {
                        let side = l.size * 1.1;
                        icons.push(format!(
                            "icon {icon} {},{},{}",
                            pt(x, y - shift - side / 2.0),
                            num(side),
                            num(side)
                        ));
                    } else if !s.text.is_empty() {
                        at.push(pt(x, y));
                    }
                    x += s.width;
                }
            }
            top += l.line_height;
        }
        at.join(" ")
    };
    let ink = |it: &Item| {
        let s = text_ink(it);
        if s.is_empty() {
            String::new()
        } else {
            format!(" ink={s}")
        }
    };
    // A class's name is written after its annotations, which are items of their own after
    // it.
    let mut held: Option<Vec<String>> = None;
    for (i, it) in sc.items.iter().enumerate() {
        let role = it.role.as_str();
        if held.is_some() && role != ROLE_ANNOTATION && role != ROLE_DIVIDER {
            lines.extend(held.take().unwrap_or_default());
        }
        // Pele writes a member starting at its x, with no anchor, and every other label with
        // labelSvg, centered on its point. The oracle reads only where a text is, so the
        // alignment is checked here.
        let align = if role == ROLE_ATTRIBUTE || role == ROLE_METHOD {
            crate::scene::Align::Start
        } else {
            crate::scene::Align::Middle
        };
        if let Some(p) = &it.label
            && !p.label.is_empty()
            && p.align != align
        {
            lines.push(format!("misaligned {role}"));
        }
        let id = id(it.id.as_deref().unwrap_or(""));
        let mut icons = Vec::new();
        let mut label = |lines: &mut Vec<String>| {
            if let Some(p) = it.label.as_ref().filter(|p| !p.label.is_empty()) {
                lines.push(format!(
                    "t {}",
                    text_at(&p.label, p.at.x, p.at.y, &mut icons)
                ));
                lines.append(&mut icons);
            }
        };
        let placed = it.label.as_ref();
        match role {
            ROLE_TITLE => {
                let p = placed.expect("a title");
                let mut none = Vec::new();
                lines.push(format!(
                    "title {}",
                    text_at(&p.label, p.at.x, p.at.y, &mut none)
                ));
            }
            ROLE_NAMESPACE => {
                lines.push(format!("ns {id} {}", boxed(it.bounds)));
                label(&mut lines);
            }
            ROLE_RELATION | ROLE_NOTE_EDGE => {
                let mut kind = if role == ROLE_NOTE_EDGE {
                    "note"
                } else {
                    "relation"
                }
                .to_string();
                match it.style.dash.as_deref() {
                    Some([6.0, _]) => kind.push_str(" dashed"),
                    Some([2.0, _]) => kind.push_str(" dotted"),
                    _ => {}
                }
                lines.push(format!(
                    "edge {id} {kind} {} {}",
                    path(it, 0),
                    pen(it, None)
                ));
                if let Kind::Edge(e) = &it.kind {
                    for m in e.ends.iter().flatten() {
                        lines.push(format!("{} {}", pele_marker(m), pen(it, Some(m.kind))));
                    }
                }
            }
            ROLE_NAMESPACE_TITLE => {
                lines.push(format!("struck {id}"));
                label(&mut lines);
            }
            ROLE_EDGE_LABEL => {
                lines.push(format!("label {id} {}", boxed(it.bounds)));
                label(&mut lines);
            }
            ROLE_CARDINALITY => {
                let p = placed.expect("a cardinality");
                let mut none = Vec::new();
                lines.push(format!(
                    "card {} {}",
                    pele_text(&spans(&p.label)),
                    text_at(&p.label, p.at.x, p.at.y, &mut none)
                ));
            }
            ROLE_CLASS => {
                lines.push(format!("class {id} [{}]", classes(it)));
                if let Some(link) = &it.link {
                    lines.push(format!("link {link}"));
                }
                if let Some(tip) = &it.tooltip {
                    lines.push(format!("tip {}", pele_text(tip)));
                }
                let rect = match &it.parts[0].outline {
                    Outline::Box { rect, .. } => boxed(*rect),
                    Outline::Path(_) => "a path".into(),
                };
                lines.push(format!("rect {rect} {}", pen(it, None)));
                // Pele draws the lines under the name and the attributes after the class's
                // box, before its label.
                if let Some(next) = sc.items.get(i + 1).filter(|n| n.role == ROLE_DIVIDER) {
                    lines.push(format!("divider {}", path(next, 0)));
                }
                let mut name = Vec::new();
                if let Some(p) = placed.filter(|p| !p.label.is_empty()) {
                    name.push(format!(
                        "t {}{}",
                        text_at(&p.label, p.at.x, p.at.y, &mut icons),
                        ink(it)
                    ));
                    name.append(&mut icons);
                }
                held = Some(name);
            }
            ROLE_DIVIDER => {}
            ROLE_ANNOTATION => {
                let p = placed.expect("an annotation");
                let mut none = Vec::new();
                lines.push(format!(
                    "annotation {} {}{}",
                    pele_text(&spans(&p.label)),
                    text_at(&p.label, p.at.x, p.at.y, &mut none),
                    ink(it)
                ));
            }
            ROLE_ATTRIBUTE | ROLE_METHOD => {
                let p = placed.expect("a member");
                let face = p.label.lines[0][0].face;
                let mut kind = role.trim_start_matches("class.").to_string();
                if face.contains(Face::UNDERLINE) {
                    kind.push_str(" static");
                } else if face.contains(Face::ITALIC) {
                    kind.push_str(" abstract");
                }
                if face.contains(Face::MONO) {
                    kind.push_str(" mono");
                }
                let mut none = Vec::new();
                lines.push(format!(
                    "member {kind} {} {}{}",
                    text_at(&p.label, p.at.x, p.at.y, &mut none),
                    pele_text(&spans(&p.label)),
                    ink(it)
                ));
            }
            ROLE_NOTE => {
                lines.push(format!("note {id}"));
                lines.push(format!("path {} {}", path(it, 0), pen(it, None)));
                lines.push(format!("fold {} {}", path(it, 1), pen(it, None)));
                label(&mut lines);
            }
            ROLE_INTERFACE => {
                lines.push(format!("interface {id}"));
                label(&mut lines);
            }
            role => lines.push(format!("unknown role {role}")),
        }
    }
    lines.extend(held.unwrap_or_default());
    lines
}
