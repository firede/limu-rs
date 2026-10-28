//! Entity relationship diagrams parsed and laid out against what Pele makes of the same
//! cases, in the oracle's er and er-render fixtures. The DB and the graph are written as the
//! oracle writes Pele's, field by field, so that any field that differs shows.

use limu_fixtures::load;
use serde::Deserialize;

use super::db::{ErAttribute, ErDb};
use super::graph::{ErGraph, build_er_graph};
use super::parser::parse_er;
use super::render::*;
use super::{layout, parse};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::layout::{Monospace, Options};
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::{Cap, Join, Look, Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{Align, Item, Marker, MarkerType, Op, Outline, Painting, Rect, Scene};
use crate::testing::pelesvg::{id, stroke_ink, text_ink};
use crate::testing::{Enc, Failures, PeleError, pele_d, pele_num, same};
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

fn dump_attributes(list: &[ErAttribute]) -> Value {
    let list = list.iter().map(|a| {
        let mut v = Object::new();
        v.str("type", &a.kind);
        v.str("name", &a.name);
        v.strs("keys", &a.keys);
        v.str("comment", &a.comment);
        v.done()
    });
    Value::Array(list.collect())
}

/// The DB as the oracle's dumpDb writes it.
fn dump_db(db: &ErDb) -> Value {
    let mut o = Object::new();
    o.str("direction", db.direction);
    o.opt("title", db.title.as_deref());
    o.opt("accTitle", db.acc_title.as_deref());
    o.opt("accDescr", db.acc_descr.as_deref());
    let entities = db.entities.iter().map(|e| {
        let mut v = Object::new();
        v.str("name", &e.label);
        v.str("id", &e.id);
        v.str("label", &e.label);
        v.str("alias", &e.alias);
        v.set("attributes", dump_attributes(&e.attributes));
        v.str("cssClasses", &e.css_classes);
        v.strs("cssStyles", &e.css_styles);
        v.done()
    });
    o.set("entities", Value::Array(entities.collect()));
    let relationships = db.relationships.iter().map(|r| {
        let mut v = Object::new();
        v.str("entityA", &r.entity_a);
        v.str("roleA", &r.role_a);
        v.str("entityB", &r.entity_b);
        v.str("cardA", r.rel_spec.card_a);
        v.str("relType", r.rel_spec.rel_type);
        v.str("cardB", r.rel_spec.card_b);
        v.done()
    });
    o.set("relationships", Value::Array(relationships.collect()));
    let classes = db.classes.iter().map(|c| {
        let mut v = Object::new();
        v.str("id", &c.id);
        v.strs("styles", &c.styles);
        v.strs("textStyles", &c.text_styles);
        v.done()
    });
    o.set("classes", Value::Array(classes.collect()));
    let subgraphs = db.subgraphs.iter().map(|s| {
        let mut v = Object::new();
        v.str("id", &s.id);
        v.strs("nodes", &s.nodes);
        v.str("title", &s.title);
        v.strs("classes", &s.classes);
        v.strs("cssStyles", &s.css_styles);
        v.some("dir", s.dir);
        v.str("labelType", "markdown");
        v.done()
    });
    o.set("subgraphs", Value::Array(subgraphs.collect()));
    o.done()
}

/// The graph as the oracle's dumpGraph writes it.
fn dump_graph(g: &ErGraph) -> Value {
    let nodes = g.nodes.iter().map(|n| {
        let mut v = Object::new();
        v.str("id", &n.id);
        v.str("label", &n.label);
        v.str("alias", &n.alias);
        v.set("attributes", dump_attributes(&n.attributes));
        v.set("isGroup", Value::Bool(n.is_group));
        v.some("parentId", n.parent_id.as_deref());
        v.strs("cssStyles", &n.css_styles);
        v.strs("cssCompiledStyles", &n.css_compiled_styles);
        v.str("cssClasses", &n.css_classes);
        v.some("dir", n.dir);
        v.str("labelType", "markdown");
        v.done()
    });
    let edges = g.edges.iter().map(|e| {
        let mut v = Object::new();
        v.str("id", &e.id);
        v.str("start", &e.start);
        v.str("end", &e.end);
        v.str("label", &e.label);
        v.str("arrowTypeStart", &e.arrow_type_start);
        v.str("arrowTypeEnd", &e.arrow_type_end);
        v.str("pattern", e.pattern);
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

/// What Pele's tests make of text that is no entity relationship diagram: the parser given
/// the text as it is.
fn direct(src: &str) -> Result<(Value, Value), Error> {
    let mut db = ErDb::default();
    parse_er(src, &mut db)?;
    Ok((dump_db(&db), dump_graph(&build_er_graph(&db))))
}

/// A case as the oracle runs it: Pele's parseSource for an entity relationship diagram, with
/// no limit, and the graph its render builds.
fn run(c: &Case) -> Result<(), String> {
    let options = ParseOptions {
        limit: None,
        max_edges: None,
        config: Map::new(),
    };
    let direct_case = || {
        let d = c.direct.as_ref().ok_or("no direct result")?;
        compare(direct(&c.src), &d.error, &d.db, &d.graph).map_err(|e| format!("direct {e}"))
    };
    let p = match prepare(&c.src, &options) {
        Ok(p) => p,
        Err(e) if e.code == ErrorCode::UnsupportedDiagram => {
            if c.diagram.is_some() {
                return Err(format!("no type, Pele {:?}", c.diagram));
            }
            return direct_case();
        }
        Err(e) => return compare(Err(e), &c.error, &c.db, &c.graph),
    };
    let ty = p.found.diagram.name();
    if c.diagram.as_deref() != Some(ty) {
        return Err(format!("type {ty}, Pele {:?}", c.diagram));
    }
    if p.found.diagram != DiagramType::Er {
        return direct_case();
    }
    let got = p
        .parse_with(false, |s| parse(s, p.title.as_deref()))
        .map(|db| (dump_db(&db), dump_graph(&build_er_graph(&db))));
    compare(got, &c.error, &c.db, &c.graph)
}

#[test]
fn diagrams_match_pele() {
    let f: Fixture = load("er");
    let mut failures = Failures::new();
    for c in &f.cases {
        let got = run(c);
        failures.check(got.is_ok(), || format!("{:?}: {}", c.src, got.unwrap_err()));
    }
    failures.done("er diagrams", 3000);
}

/// Parses a diagram that is one, and lays it out with the options.
fn laid_out(src: &str, o: &Options) -> Scene {
    let p = prepare(src, &ParseOptions::default()).expect("an entity relationship diagram");
    let db = p
        .parse_with(false, |s| parse(s, p.title.as_deref()))
        .expect("a diagram that parses");
    layout(&db, &p.config, o)
}

/// An entity's divider takes only the stroke color the author set, as Pele draws it, and not
/// the width, the dash or the fill.
#[test]
fn divider_takes_only_the_stroke() {
    let src = "erDiagram\nA {\nint a\n}\nstyle A fill:#f00,stroke:#00f,stroke-width:4px,stroke-dasharray:5\n";
    let sc = laid_out(src, &Options::default());
    let entity = sc.items.iter().find(|it| it.role.as_str() == ROLE_ENTITY);
    let stroke = entity.and_then(|it| it.author.stroke);
    let divider = sc.items.iter().find(|it| it.role.as_str() == ROLE_DIVIDER);
    let divider = divider.expect("a divider");
    assert!(stroke.is_some(), "the entity's author sets no stroke");
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
    /// Whether the attributes are set in the monospace face, as Pele's monospace option lists
    /// them.
    #[serde(default)]
    mono: bool,
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

/// What Pele's render draws of entity relationship diagrams, read back out of its SVG as a
/// line for each thing drawn, in the order drawn, against the same lines written of the
/// scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("er-render");
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            monospace: if c.mono {
                Monospace::ATTRIBUTES
            } else {
                Monospace::NONE
            },
            ..Options::default()
        };
        let got = scene_lines(&laid_out(&c.src, &o));
        let diff = drawings.diff(k, &c.drawing, &got, None);
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

/// The box of a part that is one, or the bounds of a path.
fn box_of(outline: &Outline) -> Rect {
    match outline {
        Outline::Box { rect, .. } => *rect,
        Outline::Path(p) => p.bounds(),
    }
}

/// The path of a part that is one.
fn path_of(outline: &Outline) -> &crate::scene::Path {
    match outline {
        Outline::Path(p) => p,
        Outline::Box { .. } => panic!("a box where a path was looked for"),
    }
}

/// Where Pele's text puts the first line of a label centered on cx, cy down.
fn text_at(l: &Label, cx: f64, cy: f64) -> String {
    let shift = l.size * 0.35;
    format!(
        "t {},{}",
        pele_num(cx),
        pele_num(cy - l.height / 2.0 + l.line_height / 2.0 + shift)
    )
}

/// Writes a scene as the oracle reads Pele's SVG of an entity relationship diagram. Pele
/// draws an entity's parts and text in a frame about its center, and the oracle moves them
/// into the diagram's.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let num = pele_num;
    let pt = |x: f64, y: f64| format!("{},{}", num(x), num(y));
    let boxed = |r: Rect| format!("{},{},{}", pt(r.x, r.y), num(r.w), num(r.h));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    // A label with the color ink picks for its item's text.
    let inked_label = |lines: &mut Vec<String>, it: &Item| {
        if let Some(p) = it.label.as_ref().filter(|p| !p.label.lines.is_empty()) {
            lines.push(text_at(&p.label, p.at.x, p.at.y) + &ink(text_ink(it)));
        }
    };
    for (i, it) in sc.items.iter().enumerate() {
        let id = id(it.id.as_deref().unwrap_or(""));
        let classes = it.class.join(" ");
        match it.role.as_str() {
            ROLE_TITLE => {
                let p = it.label.as_ref().expect("a title's label");
                lines.push(format!("title {}", text_at(&p.label, p.at.x, p.at.y)));
            }
            ROLE_CLUSTER => {
                let opaque = if it.style.fill_opacity.is_none() {
                    " opaque"
                } else {
                    ""
                };
                lines.push(format!(
                    "cluster {id} [{classes}] {} {}{opaque}",
                    boxed(it.bounds),
                    pen(it, None)
                ));
                inked_label(&mut lines, it);
            }
            ROLE_CASING => {
                // The casing's path, and the outlines of its relationship's markers, which
                // follows it.
                lines.push(format!("casing {}", pele_d(path_of(&it.parts[0].outline))));
                let edge = sc.items[i + 1..]
                    .iter()
                    .find(|e| e.role.as_str() == ROLE_RELATIONSHIP && e.id == it.id);
                let Some(edge) = edge.and_then(Item::edge) else {
                    lines.push("casing of no relationship".into());
                    continue;
                };
                let mut n = 1;
                for m in edge.ends.iter().flatten() {
                    lines.extend(casing_lines(m));
                    n += m.outline().len();
                }
                if n != it.parts.len() {
                    lines.push("casing's markers differ".into());
                }
            }
            ROLE_RELATIONSHIP => {
                let kind = if it.style.dash.is_some() {
                    "non-identifying dashed"
                } else {
                    "identifying"
                };
                lines.push(format!(
                    "edge {id} {kind} {} {}{}",
                    pele_d(path_of(&it.parts[0].outline)),
                    pen(it, None),
                    ink(stroke_ink(it))
                ));
                let on = it.style.ground.is_some_and(|g| g.visible());
                for m in it.edge().expect("an edge").ends.iter().flatten() {
                    lines.extend(marker_lines(m, &pen(it, Some(m.kind)), on));
                }
            }
            ROLE_RELATIONSHIP_LABEL => {
                lines.push(format!("label {id} {}", boxed(it.bounds)));
                if let Some(p) = it.label.as_ref().filter(|p| !p.label.lines.is_empty()) {
                    lines.push(text_at(&p.label, p.at.x, p.at.y));
                }
            }
            ROLE_ENTITY => {
                lines.push(format!("node {id} [{classes}]"));
                lines.push(format!(
                    "box {} {}",
                    boxed(box_of(&it.parts[0].outline)),
                    pen(it, None)
                ));
                for part in &it.parts[1..] {
                    let rounded = matches!(part.outline, Outline::Box { rounded: true, .. });
                    match part.paint {
                        Painting::Shade => lines.push(format!(
                            "stripe {}{}{}",
                            boxed(box_of(&part.outline)),
                            if rounded { " rounded" } else { "" },
                            shade_name(it)
                        )),
                        _ => {
                            // Pele draws the line under the name, an item of its own after the
                            // entity, before the outline drawn again.
                            if let Some(d) = sc.items.get(i + 1)
                                && d.role.as_str() == ROLE_DIVIDER
                            {
                                lines.push(format!(
                                    "divider {}",
                                    pele_d(path_of(&d.parts[0].outline))
                                ));
                            }
                            lines.push(format!(
                                "outline {} {}",
                                boxed(box_of(&part.outline)),
                                pen(it, None)
                            ));
                        }
                    }
                }
                if let Some(link) = &it.link {
                    lines.push(format!("link {link}"));
                }
                inked_label(&mut lines, it);
            }
            // Written with its entity.
            ROLE_DIVIDER => {}
            ROLE_ATTRIBUTE_TYPE
            | ROLE_ATTRIBUTE_NAME
            | ROLE_ATTRIBUTE_KEYS
            | ROLE_ATTRIBUTE_COMMENT => {
                let mut kind = it.role.as_str()["er.attribute-".len()..].to_string();
                let p = it.label.as_ref().expect("a cell's label");
                if p.align != Align::Start {
                    lines.push(format!("misaligned {kind}"));
                }
                if all_mono(&p.label) {
                    kind.push_str(" mono");
                }
                lines.push(format!(
                    "{kind} {} {}{}",
                    num(p.label.size),
                    cell_at(&p.label, p.at.x, p.at.y),
                    ink(text_ink(it))
                ));
            }
            role => lines.push(format!("unknown role {role}")),
        }
    }
    lines
}

/// What the oracle writes of a color ink picks: " ink=" and the color, or nothing where it
/// picks none.
fn ink(name: &str) -> String {
    if name.is_empty() {
        String::new()
    } else {
        format!(" ink={name}")
    }
}

/// The color Pele's ink names for a paint of the theme's that stands for it.
fn ink_name(p: Paint) -> &'static str {
    if p == Paint::FG {
        "var(--pele-fg-on-light,#1f1f1f)"
    } else if p == Paint::BG {
        "var(--pele-fg-on-dark,#fff)"
    } else {
        "?"
    }
}

/// Whether every span of a label with text is in the monospace face, and there is one: whether
/// Pele's textLeft names the monospace family on the label's text, as it does where Pele's
/// sharedStyle holds Mono.
fn all_mono(l: &Label) -> bool {
    let mut any = false;
    for span in l.lines.iter().flatten() {
        if span.icon.is_some() || span.text.is_empty() {
            continue;
        }
        if !span.face.contains(Face::MONO) {
            return false;
        }
        any = true;
    }
    any
}

/// Where Pele's textLeft puts a cell's text, which starts at x and is centered on y down: on
/// its first span with text, if it has one.
fn cell_at(l: &Label, x: f64, y: f64) -> String {
    for (i, line) in l.lines.iter().enumerate() {
        let mut sx = x;
        for span in line {
            if !span.text.is_empty() || span.icon.is_some() {
                let shift = l.size * 0.35;
                let first = y - l.height / 2.0 + l.line_height / 2.0 + shift;
                return format!(
                    "t {},{}",
                    pele_num(sx),
                    pele_num(first + i as f64 * l.line_height)
                );
            }
            sx += span.width;
        }
    }
    "t none".into()
}

/// How far back along its line the ring of a marker of a type is, for those with one.
fn ring_back(kind: MarkerType) -> Option<f64> {
    match kind {
        MarkerType::ErZeroOrOne => Some(13.5),
        MarkerType::ErZeroOrMore => Some(14.0),
        _ => None,
    }
}

/// A part of a marker's outline as the oracle writes it: a path's d, or a ring's center and
/// radius after "o ", whether it is hollow, and the lines that say a ring is not where Pele
/// draws it.
struct MarkerPart {
    shape: String,
    ring: bool,
    hollow: bool,
    astray: Vec<String>,
}

/// A marker's outline as the oracle reads Pele's. Pele writes a ring as a circle, which limu
/// draws as curves, so the center is worked out as Pele works it out, and the ring's curves
/// must lie about it.
fn outline(m: &Marker) -> Vec<MarkerPart> {
    let mut out = Vec::new();
    for part in m.outline() {
        let p = path_of(&part.outline);
        let hollow = part.paint == Painting::Hollow;
        if let Some(back) = ring_back(m.kind)
            && p.ops.get(1) == Some(&Op::CubeTo)
        {
            let (x, y) = (m.at.x - m.dx * back, m.at.y - m.dy * back);
            let b = p.bounds();
            let mut astray = Vec::new();
            if (b.x + b.w / 2.0 - x).abs() > 1e-9 || (b.y + b.h / 2.0 - y).abs() > 1e-9 || !hollow {
                astray.push("ring astray".to_string());
            }
            out.push(MarkerPart {
                shape: format!("o {},{} {}", pele_num(x), pele_num(y), pele_num(b.w / 2.0)),
                ring: true,
                hollow,
                astray,
            });
            continue;
        }
        out.push(MarkerPart {
            shape: pele_d(p),
            ring: false,
            hollow,
            astray: Vec::new(),
        });
    }
    out
}

/// The lines of a marker's outline, as the oracle reads Pele's: a path's d, " bg" after one
/// filled with the background, or " on" after one filled with the fill the edge lies in,
/// where on says it lies in one, and a ring as its center and radius, " on" after it where
/// it is filled so. Each ends with pen, how the marker ends and turns its lines.
fn marker_lines(m: &Marker, pen: &str, on: bool) -> Vec<String> {
    let mut out = Vec::new();
    for part in outline(m) {
        let hollow = if part.hollow && on {
            " on"
        } else if part.hollow && !part.ring {
            " bg"
        } else {
            ""
        };
        out.extend(part.astray);
        out.push(format!("m {}{hollow} {pen}", part.shape));
    }
    out
}

/// The lines of the outline of a marker in a casing, as the oracle reads Pele's: a path's d
/// or a ring's center and radius, and " bg" after one filled with the background, as the
/// casing fills what the marker fills.
fn casing_lines(m: &Marker) -> Vec<String> {
    let mut out = Vec::new();
    for part in outline(m) {
        out.extend(part.astray);
        let bg = if part.hollow { " bg" } else { "" };
        out.push(format!("casing {}{bg}", part.shape));
    }
    out
}

/// What the oracle writes of the shade of an entity's stripes on a fill ink picks a color
/// on, the other of the two it picks from: " shade=" and that color, or nothing where the
/// stripes are in the background.
fn shade_name(it: &Item) -> String {
    match Look::default().shade(&it.author) {
        (p, true) => format!(" shade={}", ink_name(p)),
        _ => String::new(),
    }
}

/// How an item's lines end and turn their corners, as the oracle reads them, "cap/join": the
/// author's style over the diagram's own, and a marker's own join over both.
fn pen(it: &Item, marker: Option<MarkerType>) -> String {
    let s = it.author.over(&it.style);
    let join = marker.and_then(MarkerType::join).or(s.join);
    format!(
        "{}/{}",
        s.cap.unwrap_or(Cap::Butt),
        join.unwrap_or(Join::Miter)
    )
}
