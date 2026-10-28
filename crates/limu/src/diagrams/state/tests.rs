//! State diagrams parsed and laid out against what Pele makes of the same cases, in the
//! oracle's state and state-render fixtures. The tokens, the DB and the graph are written as
//! the oracle writes Pele's, field by field, so that any field that differs shows.

use limu_fixtures::load;
use serde::Deserialize;

use super::db::{Description, StateDb, StateNote, StateStmt, Stmt};
use super::lexer::tokenize;
use super::parser::parse_state;
use super::render::{
    FOLD, PAD_Y, ROLE_CASING, ROLE_CHOICE, ROLE_COMPOSITE, ROLE_DESCRIPTION, ROLE_DIVIDER,
    ROLE_EDGE_LABEL, ROLE_END, ROLE_FORK, ROLE_JOIN, ROLE_NOTE, ROLE_NOTE_EDGE, ROLE_START,
    ROLE_STATE, ROLE_TITLE, ROLE_TRANSITION,
};
use super::{layout, parse};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::len16;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::Style;
use crate::scene::text::Label;
use crate::scene::{Kind, Outline, Path, Rect, Scene, Step};
use crate::testing::pelesvg::{cap_join, casing_line, id, marker, stroke_ink, text_ink};
use crate::testing::{Enc, Failures, PeleError, pele_d, pele_num, same};
use crate::value::{Map, Value};

#[derive(Deserialize)]
struct Fixture {
    states: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    src: String,
    #[serde(rename = "type")]
    diagram: Option<String>,
    #[serde(flatten)]
    parsed: Parsed,
    direct: Option<Parsed>,
}

/// What Pele made of a text: its tokens, the lexer's own error, and its DB or its error.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Parsed {
    tokens: Option<Enc>,
    lex_error: Option<String>,
    db: Option<Enc>,
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

/// The tokens of a text as the oracle writes Pele's, each its name, its text and the code
/// unit it starts at, and the lexer's own error.
fn dump_tokens(src: &str) -> (Value, Option<String>) {
    let t = tokenize(src);
    let toks = t.toks.iter().map(|t| {
        Value::Array(vec![
            Value::String(t.kind.name().into()),
            Value::String(t.text.to_string()),
            Value::Number(len16(&src[..t.start]) as f64),
        ])
    });
    (Value::Array(toks.collect()), t.error)
}

fn dump_note(n: &StateNote) -> Value {
    let mut o = Object::new();
    o.str("position", &n.position);
    o.str("text", &n.text);
    o.done()
}

fn dump_state(s: &StateStmt) -> Value {
    let mut o = Object::new();
    o.str("stmt", "state");
    o.str("id", &s.id);
    o.some("type", s.kind);
    match &s.description {
        Some(Description::One(d)) => o.str("description", d),
        Some(Description::Many(d)) => o.strs("description", d),
        None => {}
    }
    if let Some(doc) = &s.doc {
        o.set("doc", Value::Array(doc.iter().map(dump_stmt).collect()));
    }
    if let Some(n) = &s.note {
        o.set("note", dump_note(n));
    }
    if let Some(start) = s.start {
        o.set("start", Value::Bool(start));
    }
    if let Some(classes) = &s.classes {
        o.strs("classes", classes);
    }
    o.done()
}

/// A statement as the oracle's dumpStmt writes it.
fn dump_stmt(s: &Stmt) -> Value {
    let mut o = Object::new();
    match s {
        Stmt::Text(t) => return Value::String(t.clone()),
        Stmt::State(s) => return dump_state(s),
        Stmt::Relation {
            state1,
            state2,
            description,
        } => {
            o.str("stmt", "relation");
            o.set("state1", dump_state(state1));
            o.set("state2", dump_state(state2));
            o.some("description", description.as_deref());
        }
        Stmt::ClassDef { id, classes } => {
            o.str("stmt", "classDef");
            o.str("id", id);
            o.str("classes", classes);
        }
        Stmt::Style { id, style_class } => {
            o.str("stmt", "style");
            o.str("id", id);
            o.str("styleClass", style_class);
        }
        Stmt::ApplyClass { id, style_class } => {
            o.str("stmt", "applyClass");
            o.str("id", id);
            o.str("styleClass", style_class);
        }
        Stmt::Dir(value) => {
            o.str("stmt", "dir");
            o.str("value", value);
        }
        Stmt::Click { id, url, tooltip } => {
            o.str("stmt", "click");
            o.set("id", dump_state(id));
            o.str("url", url);
            o.str("tooltip", tooltip);
        }
    }
    o.done()
}

/// The DB as the oracle's dumpDb writes it.
fn dump_db(db: &StateDb) -> Value {
    let mut o = Object::new();
    o.str("direction", db.direction());
    o.opt("title", db.title.as_deref());
    o.opt("accTitle", db.acc_title.as_deref());
    o.opt("accDescr", db.acc_descr.as_deref());
    o.set(
        "rootDoc",
        Value::Array(db.root_doc.iter().map(dump_stmt).collect()),
    );
    let states = db.states.values().map(|s| {
        let mut v = Object::new();
        v.str("id", &s.id);
        v.str("type", s.kind);
        v.strs("descriptions", &s.descriptions);
        if let Some(doc) = s.doc {
            v.set("doc", Value::Number(doc as f64));
        }
        if let Some(n) = &s.note {
            v.set("note", dump_note(n));
        }
        v.strs("classes", &s.classes);
        v.strs("styles", &s.styles);
        v.strs("textStyles", &s.text_styles);
        v.done()
    });
    o.set("states", Value::Array(states.collect()));
    let relations = db.relations.iter().map(|r| {
        let mut v = Object::new();
        v.str("id1", &r.id1);
        v.str("id2", &r.id2);
        v.str("relationTitle", &r.relation_title);
        v.done()
    });
    o.set("relations", Value::Array(relations.collect()));
    let classes = db.classes.values().map(|c| {
        let mut v = Object::new();
        v.str("id", &c.id);
        v.strs("styles", &c.styles);
        v.strs("textStyles", &c.text_styles);
        v.done()
    });
    o.set("classes", Value::Array(classes.collect()));
    let links = db
        .links
        .values()
        .map(|l| strs(&[l.id.clone(), l.url.clone(), l.tooltip.clone()]));
    o.set("links", Value::Array(links.collect()));
    let nodes = db.graph.nodes.iter().map(|n| {
        let mut v = Object::new();
        v.str("id", &n.id);
        v.some("shape", n.shape);
        v.str("label", &n.label);
        v.strs("description", &n.description);
        v.str("cssClasses", &n.css_classes);
        v.strs("cssCompiledStyles", &n.css_compiled_styles);
        v.strs("cssStyles", &n.css_styles);
        v.some("type", n.kind);
        v.set("isGroup", Value::Bool(n.is_group));
        v.some("parentId", n.parent_id.as_deref());
        v.some("dir", n.dir.as_deref());
        v.some("position", n.position.as_deref());
        v.some("noteFor", n.note_for.as_deref());
        if let Some(c) = n.color_index {
            v.set("colorIndex", Value::Number(c as f64));
        }
        v.done()
    });
    let edges = db.graph.edges.iter().map(|e| {
        let mut v = Object::new();
        v.str("id", &e.id);
        v.str("start", &e.start);
        v.str("end", &e.end);
        v.str("label", &e.label);
        v.str("classes", e.classes);
        v.some("pattern", e.pattern);
        v.done()
    });
    let mut graph = Object::new();
    graph.set("nodes", Value::Array(nodes.collect()));
    graph.set("edges", Value::Array(edges.collect()));
    o.set("graph", graph.done());
    o.done()
}

/// Whether what a text lexes and parses into is what Pele made of it: the same tokens and
/// lexical error, and the same error, or the same DB. It says how they differ where they do.
fn compare(lexed: &str, got: Result<Value, Error>, want: &Parsed) -> Result<(), String> {
    let (tokens, lex_error) = dump_tokens(lexed);
    if !want.tokens.as_ref().is_some_and(|w| same(&tokens, &w.0)) {
        return Err(format!(
            "tokens {tokens:?}\nPele {:?}",
            want.tokens.as_ref().map(|w| &w.0)
        ));
    }
    if lex_error != want.lex_error {
        return Err(format!(
            "lexical error {lex_error:?}\nPele {:?}",
            want.lex_error
        ));
    }
    match (got, &want.error) {
        (Err(e), Some(w)) if w.is(&e) => Ok(()),
        (Err(e), Some(w)) => Err(format!("error {e:?}\nPele {w:?}")),
        (Err(e), None) => Err(format!("error {e:?}, Pele had none")),
        (Ok(_), Some(w)) => Err(format!("no error, Pele {w:?}")),
        (Ok(d), None) => {
            let w = want.db.as_ref().map(|e| &e.0);
            if w.is_some_and(|w| same(&d, w)) {
                Ok(())
            } else {
                Err(format!("db {d:?}\nPele {w:?}"))
            }
        }
    }
}

/// What Pele's tests make of text that is no state diagram: the parser given the text as it
/// is.
fn direct(src: &str, want: &Option<Parsed>) -> Result<(), String> {
    let want = want.as_ref().ok_or("no direct result")?;
    let mut db = StateDb::default();
    let got = parse_state(src, &mut db).map(|()| dump_db(&db));
    compare(src, got, want).map_err(|e| format!("direct {e}"))
}

/// A case as the oracle runs it: Pele's parseSource for a state diagram.
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
            return direct(&c.src, &c.direct);
        }
        Err(e) => {
            return match &c.parsed.error {
                Some(w) if w.is(&e) => Ok(()),
                w => Err(format!("error {e:?}\nPele {w:?}")),
            };
        }
    };
    let ty = p.found.diagram.name();
    if c.diagram.as_deref() != Some(ty) {
        return Err(format!("type {ty}, Pele {:?}", c.diagram));
    }
    if p.found.diagram != DiagramType::State {
        return direct(&c.src, &c.direct);
    }
    let got = p
        .parse_with(false, |s| parse(s, p.title.as_deref()))
        .map(|db| dump_db(&db));
    compare(&p.source(false), got, &c.parsed)
}

#[test]
fn states_match_pele() {
    let f: Fixture = load("state");
    let mut failures = Failures::new();
    for c in &f.states {
        let got = run(c);
        failures.check(got.is_ok(), || format!("{:?}: {}", c.src, got.unwrap_err()));
    }
    failures.done("states", 3000);
}

/// Deep nesting is read without recursion, as Pele reads it.
#[test]
fn deep() {
    let src = format!(
        "stateDiagram-v2\n{}x\n{}",
        "state a {\n".repeat(5000),
        "}\n".repeat(5000)
    );
    let mut db = parse(&src, None).unwrap();
    assert_eq!(db.graph.nodes.len(), 2);
    // The nested documents are taken apart without recursion too.
    let mut stack = std::mem::take(&mut db.root_doc);
    while let Some(item) = stack.pop() {
        if let Stmt::State(s) = item
            && let Some(doc) = s.doc
        {
            stack.extend(doc);
        }
    }
}

/// A note right after its colon loses its first code unit. Where that is the first half of a
/// character beyond the Basic Multilingual Plane, the half Pele keeps is U+FFFD.
#[test]
fn note_after_colon() {
    let text = |src: &str| {
        let t = tokenize(src);
        let note = t
            .toks
            .iter()
            .find(|t| t.kind == super::tokens::T::NOTE_TEXT);
        note.map(|t| t.text.to_string())
    };
    assert_eq!(
        text("stateDiagram-v2\nnote right of a:xy \n").as_deref(),
        Some("y")
    );
    assert_eq!(
        text("stateDiagram-v2\nnote right of a:\u{1f600}y \n").as_deref(),
        Some("\u{fffd}y")
    );
    assert_eq!(
        text("stateDiagram-v2\nnote right of a : \u{1f600}y\n").as_deref(),
        Some("\u{1f600}y")
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
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

/// The kind Pele gives a node of each role among its classes, less its "pele-".
fn node_kind(role: &str) -> Option<&'static str> {
    Some(match role {
        ROLE_STATE => "state",
        ROLE_START => "state-start",
        ROLE_END => "state-end",
        ROLE_CHOICE => "state-choice",
        ROLE_FORK => "state-fork",
        ROLE_JOIN => "state-join",
        ROLE_NOTE => "note",
        _ => return None,
    })
}

/// Text as Pele's esc writes it, less the escapes: without the characters XML does not
/// allow.
fn pele_text(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            !(c < ' ' && !matches!(c, '\t' | '\n' | '\r')
                || matches!(c, '\x7f' | '\u{fffe}' | '\u{ffff}'))
        })
        .collect()
}

fn path_of(part: &crate::scene::Part) -> Option<&Path> {
    match &part.outline {
        Outline::Path(p) => Some(p),
        Outline::Box { .. } => None,
    }
}

fn box_of(part: &crate::scene::Part) -> Option<Rect> {
    match &part.outline {
        Outline::Box { rect, .. } => Some(*rect),
        Outline::Path(_) => None,
    }
}

/// Writes the scene of a state diagram as the render oracle reads Pele's SVG of one: a line
/// for each thing drawn, in the order drawn, with numbers as Pele's SVG writes them. What a
/// node draws is written from its middle, as Pele's transform places it, and as Pele works it
/// out from the node's size, and the parts are checked to be where that puts them.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let num = pele_num;
    let pt = |x: f64, y: f64| format!("{},{}", num(x), num(y));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    // Where Pele's text puts the first line of a label centered on cx, cy.
    let text_at = |l: &Label, cx: f64, cy: f64| {
        let shift = l.size * 0.35;
        format!(
            "t {}",
            pt(cx, cy - l.height / 2.0 + l.line_height / 2.0 + shift)
        )
    };
    let radius = |s: &Style| num(s.radius.unwrap_or(0.0));
    let rect = |kind: &str, r: Rect, rx: &str| {
        format!("{kind} {},{},{} {rx}", pt(r.x, r.y), num(r.w), num(r.h))
    };
    // Notes a part that is not where Pele draws it.
    let check = |lines: &mut Vec<String>, what: &str, got: f64, want: f64| {
        if (got - want).abs() > 1e-6 {
            lines.push(format!("mismatch {what} {got} {want}"));
        }
    };
    let check_path = |lines: &mut Vec<String>, p: &Path, cx: f64, cy: f64, want: &[f64]| {
        let mut k = 0;
        for step in p.steps() {
            let to = match step {
                Step::Close => continue,
                Step::MoveTo(to)
                | Step::LineTo(to)
                | Step::QuadTo(_, to)
                | Step::CubeTo(_, _, to) => to,
            };
            if k + 1 >= want.len() {
                break;
            }
            check(lines, "path", to.x - cx, want[k]);
            check(lines, "path", to.y - cy, want[k + 1]);
            k += 2;
        }
    };
    let (mut cx, mut cy, mut desc_at) = (0.0, 0.0, 0.0);
    for it in &sc.items {
        let id = id(it.id.as_deref().unwrap_or(""));
        let classes = it.class.join(" ");
        let placed = it.label.as_ref().filter(|p| !p.label.lines.is_empty());
        let label = |lines: &mut Vec<String>, cx: f64, cy: f64| {
            if let Some(p) = placed {
                lines.push(text_at(&p.label, cx, cy));
            }
        };
        // The color an item's label takes on a fill a style sets, if it takes one.
        let ink = |lines: &mut Vec<String>| {
            if placed.is_some() {
                let s = text_ink(it);
                if !s.is_empty() {
                    lines.push(format!("ink {s}"));
                }
            }
        };
        let linked = |lines: &mut Vec<String>| {
            if let Some(link) = &it.link {
                lines.push(format!("link {link}"));
            }
            if let Some(tip) = &it.tooltip {
                lines.push(format!("tip {}", pele_text(tip)));
            }
        };
        match it.role.as_str() {
            ROLE_TITLE => {
                if let Some(p) = placed {
                    lines.push(format!("title {}", text_at(&p.label, p.at.x, p.at.y)));
                }
            }
            ROLE_COMPOSITE => {
                lines.push(format!("composite {id} [{classes}]"));
                linked(&mut lines);
                let b = box_of(&it.parts[0]).expect("a box");
                lines.push(rect("rect", b, &radius(&it.style)));
                if it.style.fill_opacity.is_none() {
                    lines.push("opaque".into());
                }
                for part in &it.parts[1..] {
                    lines.push(format!("path {}", pele_d(path_of(part).expect("a path"))));
                }
                if let Some(p) = placed {
                    lines.push(text_at(&p.label, p.at.x, p.at.y));
                }
                ink(&mut lines);
            }
            ROLE_CASING => lines.push(casing_line(it)),
            ROLE_DIVIDER => match &it.parts[0].outline {
                Outline::Path(p) => lines.push(format!("divider {}", pele_d(p))),
                Outline::Box { rect: r, .. } => {
                    lines.push(rect("divider-rect", *r, &radius(&it.style)));
                }
            },
            ROLE_TRANSITION | ROLE_NOTE_EDGE => {
                let kind = if it.role.as_str() == ROLE_NOTE_EDGE {
                    "note-edge"
                } else {
                    "transition"
                };
                lines.push(format!("edge {id} {kind}"));
                let s = stroke_ink(it);
                if !s.is_empty() {
                    lines.push(format!("ink {s}"));
                }
                lines.push(format!(
                    "path {}",
                    pele_d(path_of(&it.parts[0]).expect("a path"))
                ));
                lines.push(format!("stroke {}", cap_join(it)));
                if let Kind::Edge(e) = &it.kind {
                    for m in e.ends.iter().flatten() {
                        if let Some(s) = marker(m) {
                            lines.push(format!("m {s}"));
                        }
                    }
                }
            }
            ROLE_EDGE_LABEL => {
                lines.push(format!("label {id}"));
                lines.push(rect("rect", it.bounds, "3"));
                if let Some(p) = placed {
                    lines.push(text_at(&p.label, p.at.x, p.at.y));
                }
            }
            ROLE_DESCRIPTION => {
                let p = it.label.as_ref().expect("a description");
                if !p.label.lines.is_empty() {
                    let y = desc_at + p.label.height / 2.0;
                    check(&mut lines, "description", p.at.x - cx, 0.0);
                    check(&mut lines, "description", p.at.y - cy, y);
                    lines.push(text_at(&p.label, 0.0, y));
                    ink(&mut lines);
                }
                desc_at += p.label.height;
            }
            role => {
                let Some(kind) = node_kind(role) else {
                    lines.push(format!("unknown role {role}"));
                    continue;
                };
                let (w, h) = (it.bounds.w, it.bounds.h);
                let described = role == ROLE_STATE && it.parts.len() == 2;
                (cx, cy) = (it.bounds.x + w / 2.0, it.bounds.y + h / 2.0);
                if role == ROLE_NOTE || role == ROLE_STATE && !described {
                    let at = it.label.as_ref().expect("a label").at;
                    (cx, cy) = (at.x, at.y);
                }
                lines.push(format!("node {id} {kind} [{classes}] {}", pt(cx, cy)));
                linked(&mut lines);
                let boxed = |lines: &mut Vec<String>| {
                    if box_of(&it.parts[0]) != Some(it.bounds) {
                        lines.push("mismatch box".into());
                    }
                    lines.push(format!(
                        "rect {},{} {}",
                        pt(-w / 2.0, -h / 2.0),
                        pt(w, h),
                        radius(&it.style)
                    ));
                };
                match role {
                    ROLE_START | ROLE_END => {
                        for part in &it.parts {
                            let p = path_of(part).expect("a circle");
                            let r = p.bounds().w / 2.0;
                            let want = [r, 0.0, 0.0, r, -r, 0.0, 0.0, -r, r, 0.0];
                            check_path(&mut lines, p, cx, cy, &want);
                            lines.push(format!("circle {}", num(r)));
                        }
                    }
                    ROLE_CHOICE => {
                        let p = path_of(&it.parts[0]).expect("a diamond");
                        let want = [0.0, -h / 2.0, w / 2.0, 0.0, 0.0, h / 2.0, -w / 2.0, 0.0];
                        check_path(&mut lines, p, cx, cy, &want);
                        lines.push(format!(
                            "polygon 0,{} {},0 0,{} {},0",
                            num(-h / 2.0),
                            num(w / 2.0),
                            num(h / 2.0),
                            num(-w / 2.0)
                        ));
                    }
                    ROLE_FORK | ROLE_JOIN => boxed(&mut lines),
                    ROLE_NOTE => {
                        let outline = path_of(&it.parts[0]).expect("an outline");
                        let ear = path_of(&it.parts[1]).expect("a dog-ear");
                        let (r, b) = (w / 2.0, h / 2.0);
                        let (f, c) = (w / 2.0 - FOLD, FOLD - h / 2.0);
                        check_path(
                            &mut lines,
                            outline,
                            cx,
                            cy,
                            &[-r, -b, f, -b, r, c, r, b, -r, b],
                        );
                        check_path(&mut lines, ear, cx, cy, &[f, -b, f, c, r, c]);
                        let (r, b, f, c) = (num(r), num(b), num(f), num(c));
                        lines.push(format!(
                            "path M-{r},-{b}L{f},-{b}L{r},{c}L{r},{b}L-{r},{b}Z"
                        ));
                        lines.push(format!("path M{f},-{b}L{f},{c}L{r},{c}"));
                        label(&mut lines, 0.0, 0.0);
                    }
                    _ => {
                        boxed(&mut lines);
                        if !described {
                            label(&mut lines, 0.0, 0.0);
                            ink(&mut lines);
                            continue;
                        }
                        let p = placed.expect("a label");
                        let at = PAD_Y - h / 2.0 + p.label.height;
                        let rule = path_of(&it.parts[1]).expect("a rule");
                        check_path(
                            &mut lines,
                            rule,
                            cx,
                            cy,
                            &[-w / 2.0, at + 6.0, w / 2.0, at + 6.0],
                        );
                        lines.push(format!(
                            "path M{}L{}",
                            pt(-w / 2.0, at + 6.0),
                            pt(w / 2.0, at + 6.0)
                        ));
                        let y = at - p.label.height / 2.0;
                        check(&mut lines, "label", p.at.x - cx, 0.0);
                        check(&mut lines, "label", p.at.y - cy, y);
                        label(&mut lines, 0.0, y);
                        ink(&mut lines);
                        desc_at = at + 14.0;
                    }
                }
            }
        }
    }
    lines
}

/// What Pele's render draws of state diagrams, read back out of its SVG as a line for each
/// thing drawn, in the order drawn, against the same lines written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("state-render");
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a state diagram");
        let db = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("a state diagram that parses");
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            ..Options::default()
        };
        let got = scene_lines(&layout(&db, &p.config, &o));
        let diff = drawings.diff(k, &c.drawing, &got, None);
        failures.check(diff.is_none(), || {
            format!("{:?}, max {:?}: {}", c.src, c.max_width, diff.unwrap())
        });
    }
    failures.done("renders", 2500);
}
