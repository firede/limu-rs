//! Requirement diagrams parsed and laid out against what Pele makes of the same cases, in the
//! oracle's requirement fixtures. The tokens, the DB and the graph are written as the oracle
//! writes Pele's, field by field, so that any field that differs shows. The scene is written
//! as the oracle reads Pele's SVG, a line for each thing drawn.

use limu_fixtures::load;
use serde::Deserialize;

use super::db::RequirementDb;
use super::graph::{RequirementGraph, build_requirement_graph};
use super::lexer::tokenize;
use super::parser::parse_requirement;
use super::render::{
    ROLE_ELEMENT, ROLE_FIELD, ROLE_RELATIONSHIP, ROLE_RELATIONSHIP_LABEL, ROLE_REQUIREMENT,
    ROLE_TITLE, ROLE_VALUE,
};
use super::{layout, parse};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::len16;
use crate::js::math::max;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::Look;
use crate::scene::text::{Face, Label};
use crate::scene::{Align, Item, MarkerType, Op, Outline, Path, Rect, Scene};
use crate::testing::pelesvg::{id, text_ink};
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
    #[serde(flatten)]
    result: Wanted,
    direct: Option<Wanted>,
}

/// What Pele made of a text: its tokens, and its DB and graph or its error.
#[derive(Deserialize)]
struct Wanted {
    tokens: Option<Enc>,
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

/// An entry of a Map as the oracle writes it: its key and its value.
fn entry(key: &str, v: Value) -> Value {
    Value::Array(vec![Value::String(key.into()), v])
}

/// The tokens as the oracle's dumpTokens writes them: each one's name, its text, and where
/// it starts, in UTF-16 code units.
fn dump_tokens(src: &str) -> Value {
    let toks = tokenize(src).into_iter().map(|t| {
        Value::Array(vec![
            Value::String(t.kind.name().into()),
            Value::String(t.text.into()),
            Value::Number(len16(&src[..t.start]) as f64),
        ])
    });
    Value::Array(toks.collect())
}

/// The DB as the oracle's dumpDb writes it.
fn dump_db(db: &RequirementDb) -> Value {
    let mut o = Object::new();
    o.str("direction", &db.direction);
    o.opt("title", db.title.as_deref());
    o.opt("accTitle", db.acc_title.as_deref());
    o.opt("accDescr", db.acc_descr.as_deref());
    let requirements = db.requirements.values().map(|r| {
        let mut v = Object::new();
        v.str("name", &r.name);
        v.str("type", &r.kind);
        v.str("requirementId", &r.requirement_id);
        v.str("text", &r.text);
        v.str("risk", &r.risk);
        v.str("verifyMethod", &r.verify_method);
        v.strs("cssStyles", &r.css_styles);
        v.strs("classes", &r.classes);
        entry(&r.name, v.done())
    });
    o.set("requirements", Value::Array(requirements.collect()));
    let elements = db.elements.values().map(|e| {
        let mut v = Object::new();
        v.str("name", &e.name);
        v.str("type", &e.kind);
        v.str("docRef", &e.doc_ref);
        v.strs("cssStyles", &e.css_styles);
        v.strs("classes", &e.classes);
        entry(&e.name, v.done())
    });
    o.set("elements", Value::Array(elements.collect()));
    let relations = db.relations.iter().map(|r| {
        let mut v = Object::new();
        v.str("type", &r.kind);
        v.str("src", &r.src);
        v.str("dst", &r.dst);
        v.done()
    });
    o.set("relations", Value::Array(relations.collect()));
    let classes = db.classes.values().map(|c| {
        let mut v = Object::new();
        v.str("id", &c.id);
        v.strs("styles", &c.styles);
        v.strs("textStyles", &c.text_styles);
        entry(&c.id, v.done())
    });
    o.set("classes", Value::Array(classes.collect()));
    o.done()
}

/// The graph as the oracle's dumpGraph writes it.
fn dump_graph(g: &RequirementGraph) -> Value {
    let nodes = g.nodes.iter().map(|n| {
        let mut v = Object::new();
        v.str("id", n.id);
        if let Some(r) = n.requirement {
            v.str("requirement", &r.name);
        }
        if let Some(e) = n.element {
            v.str("element", &e.name);
        }
        v.strs("cssStyles", n.css_styles);
        v.str("cssClasses", &n.css_classes);
        v.done()
    });
    let edges = g.edges.iter().map(|e| {
        let mut v = Object::new();
        v.str("id", &e.id);
        if let Some(s) = e.start {
            v.str("start", s);
        }
        if let Some(s) = e.end {
            v.str("end", s);
        }
        v.str("type", e.kind);
        v.set("contains", Value::Bool(e.contains));
        v.done()
    });
    let mut o = Object::new();
    o.str("direction", g.direction);
    o.set("nodes", Value::Array(nodes.collect()));
    o.set("edges", Value::Array(edges.collect()));
    o.done()
}

/// Whether what was made of src is what Pele made: the same tokens, and the same error, or
/// the same DB and graph. It says how they differ where they do.
fn compare(src: &str, got: Result<(Value, Value), Error>, want: &Wanted) -> Result<(), String> {
    let tokens = dump_tokens(src);
    let wt = want.tokens.as_ref().map(|e| &e.0);
    if !wt.is_some_and(|w| same(&tokens, w)) {
        return Err(format!("tokens {tokens:?}\nPele {wt:?}"));
    }
    match (got, &want.error) {
        (Err(e), Some(w)) if w.is(&e) => Ok(()),
        (Err(e), Some(w)) => Err(format!("error {e:?}\nPele {w:?}")),
        (Err(e), None) => Err(format!("error {e:?}, Pele had none")),
        (Ok(_), Some(w)) => Err(format!("no error, Pele {w:?}")),
        (Ok((d, g)), None) => {
            let wd = want.db.as_ref().map(|e| &e.0);
            let wg = want.graph.as_ref().map(|e| &e.0);
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

/// The DB and the graph of a parse.
fn dumps(db: Result<RequirementDb, Error>) -> Result<(Value, Value), Error> {
    db.map(|db| (dump_db(&db), dump_graph(&build_requirement_graph(&db))))
}

/// What Pele's tests make of text that is no requirement diagram: the parser given the text
/// as it is.
fn direct(c: &Case) -> Result<(), String> {
    let d = c.direct.as_ref().ok_or("no direct result")?;
    let mut db = RequirementDb::default();
    let got = dumps(parse_requirement(&c.src, &mut db).map(|_| db));
    compare(&c.src, got, d).map_err(|e| format!("direct {e}"))
}

/// A case as the oracle runs it: Pele's parseSource for a requirement diagram, and the graph
/// its render builds.
fn run(c: &Case) -> Result<(), String> {
    let p = match prepare(&c.src, &ParseOptions::default()) {
        Ok(p) => p,
        Err(e) if e.code == ErrorCode::UnsupportedDiagram => {
            if c.diagram.is_some() {
                return Err(format!("no type, Pele {:?}", c.diagram));
            }
            return direct(c);
        }
        Err(e) => {
            return match &c.result.error {
                Some(w) if w.is(&e) => Ok(()),
                w => Err(format!("error {e:?}\nPele {w:?}")),
            };
        }
    };
    let ty = p.found.diagram.name();
    if c.diagram.as_deref() != Some(ty) {
        return Err(format!("type {ty}, Pele {:?}", c.diagram));
    }
    if p.found.diagram != DiagramType::Requirement {
        return direct(c);
    }
    let got = dumps(p.parse_with(false, |s| parse(s, p.title.as_deref())));
    compare(&p.source(false), got, &c.result)
}

#[test]
fn requirements_match_pele() {
    let f: Fixture = load("requirement");
    let mut failures = Failures::new();
    for c in &f.cases {
        let got = run(c);
        failures.check(got.is_ok(), || format!("{:?}: {}", c.src, got.unwrap_err()));
    }
    failures.done("requirements", 3000);
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

/// What Pele's render draws of requirement diagrams, read back out of its SVG as a line for
/// each thing drawn, in the order drawn, against the same lines written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("requirement-render");
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a requirement diagram");
        let db = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("a requirement diagram that parses");
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
    failures.done("renders", 4000);
}

/// The path of an item's part that is one.
fn path_of(it: &Item, i: usize) -> &Path {
    match &it.parts[i].outline {
        Outline::Path(p) => p,
        Outline::Box { .. } => panic!("a box where a path was looked for"),
    }
}

/// The label of an item.
fn label_of(it: &Item) -> &Label {
    &it.label.as_ref().expect("a label").label
}

/// Writes a scene as the oracle reads Pele's SVG of a requirement diagram.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let num = pele_num;
    let pt = |x: f64, y: f64| format!("{},{}", num(x), num(y));
    let boxed = |r: Rect| format!("{},{},{}", pt(r.x, r.y), num(r.w), num(r.h));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    if let Some(t) = sc.title.as_deref().filter(|t| !t.is_empty()) {
        lines.push(format!("acc-title {}", id(t)));
    }
    if let Some(d) = sc.desc.as_deref().filter(|d| !d.is_empty()) {
        lines.push(format!("acc-desc {}", id(d)));
    }
    // An item's label, drawn about x, y, as the oracle reads Pele's text element.
    let text = |lines: &mut Vec<String>,
                it: &Item,
                class: &str,
                size: &str,
                weight: &str,
                x: f64,
                y: f64| {
        let p = it.label.as_ref().expect("a label");
        if let Some(s) = text_line(&p.label, p.align, class, size, weight, x, y) {
            let ink = text_ink(it);
            if ink.is_empty() {
                lines.push(s);
            } else {
                lines.push(format!("{s} ink={ink}"));
            }
        }
    };
    let mut labels = false;
    let mut i = 0;
    while i < sc.items.len() {
        let it = &sc.items[i];
        match it.role.as_str() {
            ROLE_TITLE => {
                let at = it.label.as_ref().expect("a label").at;
                text(&mut lines, it, "pele-title", "", "var(--_tw)", at.x, at.y);
            }
            ROLE_RELATIONSHIP => {
                let ends = &it.edge().expect("an edge").ends;
                let contains = ends[0].is_some_and(|m| m.kind == MarkerType::RequirementContains);
                let kind = if contains { "contains" } else { "arrow" };
                lines.push(format!(
                    "edge {} {kind}",
                    id(it.id.as_deref().unwrap_or(""))
                ));
                let dashed = if it.style.dash.is_some() {
                    " dashed"
                } else {
                    ""
                };
                lines.push(format!(
                    "path {}{dashed} {}",
                    pele_d(path_of(it, 0)),
                    pen(it, None)
                ));
                for m in ends.iter().flatten() {
                    for part in m.outline() {
                        let Outline::Path(p) = &part.outline else {
                            panic!("a box in a marker");
                        };
                        lines.push(format!("m {} {}", marker_part(p), pen(it, Some(m.kind))));
                    }
                }
            }
            ROLE_RELATIONSHIP_LABEL => {
                let l = label_of(it);
                if !labels {
                    lines.push(format!("labels {}", num(l.size)));
                    labels = true;
                }
                lines.push(format!("label {}", id(it.id.as_deref().unwrap_or(""))));
                lines.push(format!("rect {} {}", boxed(it.bounds), pen(it, None)));
                let at = it.label.as_ref().expect("a label").at;
                text(&mut lines, it, "-", "", "", at.x, at.y);
            }
            ROLE_REQUIREMENT | ROLE_ELEMENT => {
                // Pele draws a box about its middle, which it moves to with a translate. The
                // scene places its parts where they end up, so the numbers Pele writes are
                // worked out here as its render.ts works them out, from the sizes of the box
                // and its texts, and the scene is held to placing each part exactly there.
                // Taking the middle off where the scene places a part would be off by a bit
                // in the last place, which shows when a number falls on a half of a
                // hundredth.
                let name = it.label.as_ref().expect("a name");
                let (cx, cy) = (name.at.x, it.bounds.y + it.bounds.h / 2.0);
                let kind = it.role.as_str().trim_start_matches("requirement.");
                let (w, h) = (it.bounds.w, it.bounds.h);
                let (left, top) = (-w / 2.0, -h / 2.0);
                let place =
                    |lines: &mut Vec<String>, what: &str, x: f64, y: f64, rx: f64, ry: f64| {
                        if x != cx + rx || y != cy + ry {
                            lines.push(format!(
                                "the scene places the {what} at {x},{y}, not {},{}",
                                cx + rx,
                                cy + ry
                            ));
                        }
                    };
                lines.push(format!(
                    "node {} {kind} [{}] translate({})",
                    id(it.id.as_deref().unwrap_or("")),
                    it.class.join(" "),
                    pt(cx, cy)
                ));
                lines.push(format!(
                    "rect {},{},{} {}",
                    pt(left, top),
                    num(w),
                    num(h),
                    pen(it, None)
                ));
                place(&mut lines, "box", it.bounds.x, it.bounds.y, left, top);
                i += 1;
                let k = &sc.items[i];
                let kl = label_of(k);
                let kind_y = top + 10.0 + kl.height / 2.0;
                let ka = k.label.as_ref().expect("a label").at;
                place(&mut lines, "kind", ka.x, ka.y, 0.0, kind_y);
                text(
                    &mut lines,
                    k,
                    "pele-stereotype",
                    &num(kl.size),
                    "",
                    0.0,
                    kind_y,
                );
                let name_y = top + 10.0 + kl.height + name.label.height / 2.0;
                place(&mut lines, "name", name.at.x, name.at.y, 0.0, name_y);
                text(
                    &mut lines,
                    it,
                    "pele-label",
                    &num(name.label.size),
                    "var(--_hw)",
                    0.0,
                    name_y,
                );
                let mut fields = Vec::new();
                while i + 1 < sc.items.len()
                    && matches!(sc.items[i + 1].role.as_str(), ROLE_FIELD | ROLE_VALUE)
                {
                    i += 1;
                    fields.push(&sc.items[i]);
                }
                if it.parts.len() > 1 {
                    let divider = top + (20.0 + kl.height + name.label.height);
                    let a = &path_of(it, 1).args;
                    place(&mut lines, "divider", a[0], a[1], left, divider);
                    place(&mut lines, "divider's end", a[2], a[3], -left, divider);
                    lines.push(format!(
                        "divider M{}H{} {}",
                        pt(left, divider),
                        num(-left),
                        pen(it, None)
                    ));
                    lines.push(format!("fields {}", num(label_of(fields[0]).size)));
                    let key_width = fields
                        .iter()
                        .step_by(2)
                        .fold(0.0, |w, f| max(w, label_of(f).width));
                    let mut at = divider + 10.0;
                    for &[key, value] in fields.as_chunks::<2>().0 {
                        let (kl, vl) = (label_of(key), label_of(value));
                        let (key_x, value_x) = (left + 14.0, left + 14.0 + key_width + 12.0);
                        let (kp, vp) = (
                            key.label.as_ref().unwrap().at,
                            value.label.as_ref().unwrap().at,
                        );
                        place(&mut lines, "field", kp.x, kp.y, key_x, at + kl.height / 2.0);
                        place(
                            &mut lines,
                            "value",
                            vp.x,
                            vp.y,
                            value_x,
                            at + vl.height / 2.0,
                        );
                        text(
                            &mut lines,
                            key,
                            "pele-field",
                            "",
                            "",
                            key_x,
                            at + kl.height / 2.0,
                        );
                        text(
                            &mut lines,
                            value,
                            "pele-value",
                            "",
                            "",
                            value_x,
                            at + vl.height / 2.0,
                        );
                        at += max(kl.height, vl.height) + 2.0;
                    }
                }
            }
            role => lines.push(format!("unknown role {role}")),
        }
        i += 1;
    }
    lines
}

/// How an item's lines end and turn their corners, as the oracle reads them, "cap/join": the
/// author's style over the diagram's own, and a marker's own join over both.
fn pen(it: &Item, marker: Option<MarkerType>) -> String {
    let r = Look::default().resolve(it.role.as_str(), &it.style, &it.author);
    let join = marker.and_then(MarkerType::join).unwrap_or(r.join);
    format!("{}/{join}", r.cap)
}

/// A part of a marker as the oracle reads Pele's: a circle by its middle and radius, a path
/// by its d.
fn marker_part(p: &Path) -> String {
    if p.ops.len() > 1 && p.ops[1] == Op::CubeTo {
        // An ellipse: it starts at the right of its middle, and its first curve ends below it.
        let (cx, cy) = (p.args[6], p.args[1]);
        return format!(
            "o {},{},{}",
            pele_num(cx),
            pele_num(cy),
            pele_num(p.args[0] - cx)
        );
    }
    pele_d(p)
}

/// The emphasis every run of a label has, as Pele's sharedStyle finds it.
fn shared_face(l: &Label) -> Face {
    let mut shared = Face::BOLD | Face::ITALIC;
    let mut any = false;
    for s in l.lines.iter().flatten() {
        if s.icon.is_some() || s.text.is_empty() {
            continue;
        }
        shared = shared & s.face;
        any = true;
    }
    if any { shared } else { Face::REGULAR }
}

/// The characters XML does not allow dropped, as Pele's esc drops them.
fn drop_controls(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            !(c < ' ' && !matches!(c, '\t' | '\n' | '\r')
                || matches!(c, '\x7f' | '\u{fffe}' | '\u{ffff}'))
        })
        .collect()
}

/// A string as JSON.stringify writes it.
fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Writes a label as the oracle reads the text element Pele writes for it: centered on x, y
/// as Pele's labelSvg writes it, or with its lines starting at x, as its startText does.
fn text_line(
    l: &Label,
    align: Align,
    class: &str,
    size: &str,
    weight: &str,
    x: f64,
    y: f64,
) -> Option<String> {
    let num = pele_num;
    let shared = shared_face(l);
    let mut italic = shared.contains(Face::ITALIC);
    let weight = match weight {
        "" if shared.contains(Face::BOLD) => "var(--_w)",
        "" => "-",
        w => w,
    };
    let size = if size.is_empty() { "-" } else { size };
    let anchor = if align == Align::Start {
        "start"
    } else {
        "middle"
    };
    let mut spans = Vec::new();
    let mut span = |pos: String, f: Face, s: &str| {
        let mut pos = pos;
        if f.contains(Face::BOLD) {
            pos.push_str("+b");
        }
        if f.contains(Face::ITALIC) {
            pos.push_str("+i");
        }
        spans.push(format!("{pos}:{}", quote(&drop_controls(s))));
    };
    let shift = l.size * 0.35;
    let mut top = y - l.height / 2.0;
    let mut ly = top + l.line_height / 2.0 + shift;
    let at = |x: f64, y: f64| format!("@{},{}", num(x), num(y));
    if align == Align::Start {
        for (i, line) in l.lines.iter().enumerate() {
            // startText places each line by its middle, as Pele's lineMiddle gives it.
            ly = top + (i as f64 + 0.5) * l.line_height + shift;
            let mut first = true;
            for s in line {
                if s.icon.is_some() || s.text.is_empty() {
                    continue;
                }
                let pos = if first { at(x, ly) } else { String::new() };
                span(pos, s.face.without(shared), &s.text);
                first = false;
            }
        }
    } else if l.lines.len() == 1
        && l.lines[0].len() == 1
        && l.lines[0][0].icon.is_none()
        && l.lines[0][0].face == Face::REGULAR
    {
        // Pele writes a label of one plain run without a tspan, and no emphasis.
        span(at(x, ly), Face::REGULAR, &l.lines[0][0].text);
        italic = false;
    } else {
        for line in &l.lines {
            for (j, s) in line.iter().enumerate() {
                let pos = if j == 0 { at(x, ly) } else { String::new() };
                span(pos, s.face.without(shared), &s.text);
            }
            if line.is_empty() {
                span(at(x, ly), Face::REGULAR, " ");
            }
            top += l.line_height;
            ly = top + l.line_height / 2.0 + shift;
        }
    }
    if spans.is_empty() {
        return None;
    }
    Some(format!(
        "text {class} fs={size} w={weight} i={} a={anchor} {}",
        u8::from(italic),
        spans.join(" ")
    ))
}
