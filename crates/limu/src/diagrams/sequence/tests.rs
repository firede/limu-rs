//! Sequence diagrams parsed against what Pele makes of the same cases, in the oracle's
//! sequence fixture, and laid out against what it draws of them, in its sequence-render
//! fixture. The tokens and the DB are written as the oracle writes Pele's, field by field,
//! with offsets in UTF-16 code units, so that any field that differs shows.

use limu_fixtures::load;
use serde::Deserialize;

use super::db::{Entries, MessageText, NoteActor, SeqDb};
use super::lexer::tokenize;
use super::parser::parse_sequence;
use super::render::{
    ROLE_ACTIVATION, ROLE_ACTOR, ROLE_BOX, ROLE_CONDITION, ROLE_DESTROY, ROLE_DIVIDER, ROLE_FRAME,
    ROLE_FRAME_KIND, ROLE_LIFELINE, ROLE_MESSAGE, ROLE_MESSAGE_LABEL, ROLE_NOTE, ROLE_NUMBER,
    ROLE_RECT, ROLE_TITLE,
};
use super::{layout, parse};
use crate::css::declarations;
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::len16;
use crate::js::json::{parse_json, stringify};
use crate::js::number::to_string;
use crate::layout::{Monospace, Options};
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::{Cap, Join, Look, Paint, Style};
use crate::scene::{Item, Kind, Op, Outline, Painting, Part, Path, Scene};
use crate::testing::pelesvg::{id, own_fill};
use crate::testing::{Enc, Failures, PeleError, pele_num, same, sorted};
use crate::value::{Map, Value};

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

/// What Pele made of a text: its tokens, and its DB or its error.
#[derive(Deserialize, Default)]
struct Parsed {
    tokens: Option<Enc>,
    db: Option<Enc>,
    error: Option<PeleError>,
}

#[derive(Deserialize)]
struct Case {
    src: String,
    #[serde(rename = "type")]
    diagram: Option<String>,
    tokens: Option<Enc>,
    db: Option<Enc>,
    error: Option<PeleError>,
    direct: Parsed,
    /// Whether the pipeline's DB or error is the direct one.
    #[serde(default)]
    same: bool,
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

    /// A field the oracle leaves out where it is undefined.
    fn some(&mut self, key: &str, s: Option<&str>) {
        if let Some(s) = s {
            self.str(key, s);
        }
    }

    fn done(self) -> Value {
        Value::Object(self.0)
    }
}

fn strs<'a>(s: impl IntoIterator<Item = &'a str>) -> Value {
    Value::Array(s.into_iter().map(|s| Value::String(s.into())).collect())
}

/// The tokens of a text as the oracle writes Pele's, each its name, its text and the code
/// unit it starts at.
fn dump_tokens(src: &str) -> Value {
    let toks = tokenize(src).into_iter().map(|t| {
        Value::Array(vec![
            Value::String(t.kind.name().into()),
            Value::String(t.text.to_string()),
            Value::Number(len16(&src[..t.start]) as f64),
        ])
    });
    Value::Array(toks.collect())
}

/// The entries of a Map, each its key and its value as JSON.stringify writes it.
fn dump_entries(e: &Entries) -> Value {
    let entries =
        e.0.iter()
            .map(|(k, v)| strs([k.as_str(), stringify(v).as_str()]));
    Value::Array(entries.collect())
}

fn dump_db(db: &SeqDb) -> Value {
    let boxes = db.boxes.iter().map(|b| {
        let mut v = Object::new();
        v.bool("wrap", b.wrap);
        v.str("fill", &b.fill);
        v.set("actorKeys", strs(b.actor_keys.iter().map(String::as_str)));
        v.some("name", b.name.as_deref());
        v.done()
    });
    let actors = db.actors.iter().map(|a| {
        let mut v = Object::new();
        v.str("id", &a.id);
        v.num("box", a.r#box.map_or(-1.0, |b| b as f64));
        v.str("name", &a.name);
        v.str("description", &a.description);
        v.bool("wrap", a.wrap);
        v.some("prevActor", a.prev_actor.as_deref());
        v.some("nextActor", a.next_actor.as_deref());
        v.set("links", dump_entries(&a.links));
        v.set("properties", dump_entries(&a.properties));
        v.some("details", a.details.as_deref());
        v.str("type", &a.kind);
        v.done()
    });
    let positions = |list: &[(String, usize)]| {
        let list = list
            .iter()
            .map(|(id, i)| Value::Array(vec![Value::String(id.clone()), Value::Number(*i as f64)]));
        Value::Array(list.collect())
    };
    let messages = db.messages.iter().map(|m| {
        let mut v = Object::new();
        v.str("id", &m.id);
        v.some("from", m.from.as_deref());
        v.some("to", m.to.as_deref());
        match &m.message {
            MessageText::Text(t) => v.str("message", t),
            MessageText::Numbering(n) => {
                let mut o = Object::new();
                o.bool("visible", n.visible);
                if let Some(start) = n.start {
                    o.num("start", start);
                }
                if let Some(step) = n.step {
                    o.num("step", step);
                }
                v.set("message", o.done());
            }
        }
        v.bool("wrap", m.wrap);
        v.num("type", f64::from(m.kind));
        if let Some(a) = m.activate {
            v.bool("activate", a);
        }
        if let Some(p) = m.placement {
            v.num("placement", f64::from(p));
        }
        if let Some(c) = m.central_connection {
            v.num("centralConnection", f64::from(c));
        }
        v.done()
    });
    let notes = db.notes.iter().map(|n| {
        let mut v = Object::new();
        v.num("placement", f64::from(n.placement));
        v.str("message", &n.message);
        v.bool("wrap", n.wrap);
        match &n.actor {
            NoteActor::One(a) => v.str("actor", a),
            NoteActor::Pair(a, b) => v.set("actor", strs([a.as_str(), b.as_str()])),
        }
        v.done()
    });
    let mut o = Object::new();
    o.set("actors", Value::Array(actors.collect()));
    o.set("created", positions(&db.created_actors));
    o.set("destroyed", positions(&db.destroyed_actors));
    o.set("boxes", Value::Array(boxes.collect()));
    o.set("messages", Value::Array(messages.collect()));
    o.set("notes", Value::Array(notes.collect()));
    o.some("title", db.title.as_deref().filter(|t| !t.is_empty()));
    o.some("accTitle", db.acc_title.as_deref());
    o.some("accDescr", db.acc_descr.as_deref());
    o.done()
}

/// Whether what a text lexes and parses into is what Pele made of it: the same tokens, and
/// the same error or the same DB, whose keys are compared in any order. It says how they
/// differ where they do.
fn compare(
    lexed: &str,
    got: Result<Value, Error>,
    tokens: Option<&Enc>,
    db: Option<&Enc>,
    error: Option<&PeleError>,
) -> Result<(), String> {
    let toks = dump_tokens(lexed);
    if !tokens.is_some_and(|w| same(&toks, &w.0)) {
        return Err(format!("tokens {toks:?}\nPele {:?}", tokens.map(|w| &w.0)));
    }
    match (got, error) {
        (Err(e), Some(w)) if w.is(&e) => Ok(()),
        (Err(e), Some(w)) => Err(format!("error {e:?}\nPele {w:?}")),
        (Err(e), None) => Err(format!("error {e:?}, Pele had none")),
        (Ok(_), Some(w)) => Err(format!("no error, Pele {w:?}")),
        (Ok(d), None) => {
            let w = db.map(|e| &e.0);
            if w.is_some_and(|w| same(&sorted(&d), &sorted(w))) {
                Ok(())
            } else {
                Err(format!("db {d:?}\nPele {w:?}"))
            }
        }
    }
}

/// A case as the oracle runs it: Pele's parser given the text as it is, and then Pele's
/// parseSource where the text is a sequence diagram.
fn run(c: &Case) -> Result<(), String> {
    let mut db = SeqDb::new(None, false);
    let got = parse_sequence(&c.src, &mut db).map(|()| dump_db(&db));
    let d = &c.direct;
    compare(
        &c.src,
        got,
        d.tokens.as_ref(),
        d.db.as_ref(),
        d.error.as_ref(),
    )
    .map_err(|e| format!("direct {e}"))?;
    let p = match prepare(&c.src, &ParseOptions::default()) {
        Ok(p) => p,
        Err(e) if e.code == ErrorCode::UnsupportedDiagram && c.error.is_none() => return Ok(()),
        Err(e) => {
            return match &c.error {
                Some(w) if w.is(&e) => Ok(()),
                w => Err(format!("error {e:?}\nPele {w:?}")),
            };
        }
    };
    let ty = p.found.diagram.name();
    if c.diagram.as_deref() != Some(ty) {
        return Err(format!("type {ty}, Pele {:?}", c.diagram));
    }
    if p.found.diagram != DiagramType::Sequence {
        return Ok(());
    }
    let got = p
        .parse_with(false, |s| parse(s, &p.config, p.title.as_deref()))
        .map(|db| dump_db(&db));
    let (db, error) = if c.same {
        (d.db.as_ref(), d.error.as_ref())
    } else {
        (c.db.as_ref(), c.error.as_ref())
    };
    compare(&p.source(false), got, c.tokens.as_ref(), db, error)
}

#[test]
fn sequences_match_pele() {
    let f: Fixture = load("sequence");
    let mut failures = Failures::new();
    for c in &f.cases {
        let got = run(c);
        failures.check(got.is_ok(), || format!("{:?}: {}", c.src, got.unwrap_err()));
    }
    failures.done("sequences", 5000);
}

/// Deep nesting is read without recursion, as Pele reads it.
#[test]
fn deep() {
    let src = format!(
        "sequenceDiagram\n{}A->>B: x\n{}",
        "loop a\n".repeat(5000),
        "end\n".repeat(5000)
    );
    let db = parse(&src, &Map::new(), None).unwrap();
    assert_eq!(db.messages.len(), 10001);
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
    /// Pele's monospace option, the parts of diagrams set in the monospace font.
    #[serde(default)]
    monospace: Vec<String>,
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

/// What Pele's render draws of sequence diagrams, read back out of its SVG as one line per
/// thing drawn, in the order drawn, against the same lines written of the scene. Pele draws
/// everything in a group moved by the padding past the diagram's extents. The oracle moves
/// each place by it, as the scene holds it, before it writes numbers as Pele's SVG writes
/// them.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("sequence-render");
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a sequence diagram");
        assert_eq!(p.found.diagram, DiagramType::Sequence, "{:?}", c.src);
        let db = p
            .parse_with(false, |s| parse(s, &p.config, p.title.as_deref()))
            .expect("a sequence diagram that parses");
        let mut o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            ..Options::default()
        };
        if let Some(size) = c.font_size {
            o.font_size = size;
        }
        if c.monospace.iter().any(|m| m == "messages") {
            o.monospace = Monospace::MESSAGES;
        }
        let got = scene_lines(&layout(&db, &p.config, &o));
        let diff = drawings.diff(k, &c.drawing, &got, Some(&author_fill));
        failures.check(diff.is_none(), || {
            format!(
                "{:?}, max {:?}, size {:?}, monospace {:?}: {}",
                c.src,
                c.max_width,
                c.font_size,
                c.monospace,
                diff.unwrap()
            )
        });
    }
    failures.done("renders", 3000);
}

/// A paint as Pele's SVG names it, and "?" for one it has no name for.
fn paint_name(p: Option<Paint>) -> &'static str {
    let Some(p) = p else { return "?" };
    [
        (Paint::BG, "var(--_bg)"),
        (Paint::FG, "var(--_fg)"),
        (Paint::MUTED, "var(--_m)"),
        (Paint::LINE, "var(--_l)"),
        (Paint::SURFACE, "var(--_s)"),
        (Paint::SURFACE_ALT, "var(--_a)"),
        (Paint::SURFACE_ALT.alpha(0.5), "var(--_a)/0.5"),
        (Paint::BORDER, "var(--_b)"),
        (Paint::NONE, "none"),
    ]
    .into_iter()
    .find(|&(q, _)| q == p)
    .map_or("?", |(_, name)| name)
}

/// The color Pele's ink picks for text or lines on a fill, by the theme's paint limu's look
/// picks in its place under the default theme.
fn ink_name(p: Option<Paint>) -> &'static str {
    match p {
        Some(p) if p == Paint::FG => "var(--pele-fg-on-light,#1f1f1f)",
        Some(p) if p == Paint::BG => "var(--pele-fg-on-dark,#fff)",
        _ => "",
    }
}

/// The names of the text and the stroke the default look draws an item in, as the backends
/// cascade its style: those it picks to stand out on a fill under the item, or under its
/// text, by the name Pele's ink writes, and its own otherwise. The item's own are set to a
/// paint no part of a sequence diagram is drawn in, to see whether the look puts a color in
/// their place.
fn inked(it: &Item) -> (&'static str, &'static str) {
    let probe = Paint::CRITICAL;
    let mut own = it.style.clone();
    (own.text, own.stroke) = (Some(probe), Some(probe));
    let s = Look::default().cascade(it.role.as_str(), &own, &it.author);
    let text = if s.text == Some(probe) {
        paint_name(it.style.text)
    } else {
        ink_name(s.text)
    };
    let stroke = if s.stroke == Some(probe) {
        paint_name(it.style.stroke)
    } else {
        ink_name(s.stroke)
    };
    (text, stroke)
}

/// How a stroke ends and turns its corners, as the oracle reads them: " cap=" and " join="
/// where they are not SVG's initial butt and miter, and nothing where nothing is stroked. A
/// marker's own join, when it has one, is over the style's.
fn ends(mut s: Style, stroke: &str, join: Option<Join>) -> String {
    if stroke == "none" {
        return String::new();
    }
    if join.is_some() {
        s.join = join;
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

/// How a part of an item is painted, as the oracle reads Pele's fill and stroke, with the
/// width of the stroke where it is not the theme's, its ends and corners, and the fill the
/// author set, where they set one.
fn part_paint(it: &Item, paint: Painting) -> String {
    let (_, mut stroke) = inked(it);
    let mut fill = paint_name(own_fill(it));
    match paint {
        Painting::StrokeOnly => fill = "none",
        Painting::FillOnly => stroke = "none",
        Painting::Dot => (fill, stroke) = (stroke, "none"),
        _ => {}
    }
    let mut s = format!("{fill} {stroke}");
    if let Some(w) = it.style.stroke_width {
        s.push_str(&format!(" w={}", pele_num(w)));
    }
    s.push_str(&ends(it.author.over(&it.style), stroke, None));
    if let Some(fill) = it.author.fill {
        s.push_str(&format!(" author={fill:?}"));
    }
    s
}

/// Writes the fill an author gave in a line of Pele's as the paint limu's style reads it as.
/// Pele writes whatever fill the author gave into the style. A browser drops one it cannot
/// read, and draws the theme's paint, as limu does with a fill it cannot read.
fn author_fill(line: &str) -> String {
    let Some(at) = line.find(" author=\"") else {
        return line.to_string();
    };
    let start = at + " author=".len();
    // The end of the JSON string that starts at start.
    let b = line.as_bytes();
    let mut end = start + 1;
    while end < b.len() && b[end] != b'"' {
        end += if b[end] == b'\\' { 2 } else { 1 };
    }
    let end = (end + 1).min(b.len());
    let Some(Value::String(value)) = parse_json(&line[start..end]) else {
        return line.to_string();
    };
    let rest = &line[end..];
    match declarations(&[format!("fill:{value}")]).style.fill {
        Some(fill) => format!("{} author={fill:?}{rest}", &line[..at]),
        None => format!("{}{rest}", &line[..at]),
    }
}

/// How a part of a marker is painted, in the color of its edge's line.
fn marker_paint(it: &Item, paint: Painting, join: Option<Join>) -> String {
    let (_, line) = inked(it);
    if paint == Painting::FillOnly {
        return format!("{line} none");
    }
    format!("none {line}{}", ends(it.author.over(&it.style), line, join))
}

/// Drops what XML does not allow, as Pele's esc does.
fn clean(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            !(c < ' ' && !matches!(c, '\t' | '\n' | '\r')
                || matches!(c, '\x7f' | '\u{fffe}' | '\u{ffff}'))
        })
        .collect()
}

/// Writes a string as JSON.stringify does, which leaves <, >, & and the separators of lines
/// and paragraphs as they are. What clean leaves of the controls is a tab or an end of line.
fn stringified(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A point as the oracle writes one.
fn pt(x: f64, y: f64) -> String {
    format!("{},{}", pele_num(x), pele_num(y))
}

/// The subpaths of a path, with the curves an arc was drawn with written as the arc, by
/// where it ends.
fn subpaths(p: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    let mut curve = false;
    let mut at = 0;
    for &op in &p.ops {
        let a = &p.args[at..at + op.args()];
        at += op.args();
        if op == Op::MoveTo && !cur.is_empty() {
            out.push(cur.join(" "));
            cur.clear();
        }
        match op {
            Op::CubeTo => {
                let s = format!("A {}", pt(a[4], a[5]));
                if curve {
                    *cur.last_mut().unwrap() = s;
                } else {
                    cur.push(s);
                }
            }
            Op::Close => cur.push("Z".into()),
            _ => {
                let letter = match op {
                    Op::MoveTo => "M",
                    Op::LineTo => "L",
                    _ => "Q",
                };
                cur.push(format!("{letter} {}", pt(a[a.len() - 2], a[a.len() - 1])));
            }
        }
        curve = op == Op::CubeTo;
    }
    if !cur.is_empty() {
        out.push(cur.join(" "));
    }
    out
}

/// An ellipse, as Path::ellipse draws it: from its right end, four quarters, closed.
fn ellipse(p: &Path) -> Option<String> {
    let ops = [
        Op::MoveTo,
        Op::CubeTo,
        Op::CubeTo,
        Op::CubeTo,
        Op::CubeTo,
        Op::Close,
    ];
    if p.ops != ops {
        return None;
    }
    let (cx, cy) = (p.args[6], p.args[1]);
    Some(format!(
        "ellipse {} {}",
        pt(cx, cy),
        pele_num(p.args[0] - cx)
    ))
}

/// Writes a scene as the oracle reads Pele's SVG.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    let hash = |s: &Option<String>| format!(" #{}", id(s.as_deref().unwrap_or("")));
    let content = |l: &crate::scene::text::Label| {
        let out: Vec<String> = l
            .lines
            .iter()
            .map(|line| {
                let mut s: String = line.iter().map(|span| span.text.as_str()).collect();
                if line.is_empty() {
                    s.push(' ');
                }
                clean(&s)
            })
            .collect();
        stringified(&out.join("|"))
    };
    // The text of a label centered on its point, its first line where Pele's labelSvg puts
    // it. A tab's kind and a message's number Pele writes as one line by its own sum.
    let label = |lines: &mut Vec<String>, it: &Item| {
        let Some(p) = it.label.as_ref().filter(|p| !p.label.lines.is_empty()) else {
            return;
        };
        let l = &p.label;
        let shift = l.size * 0.35;
        let y = if it.role.as_str() == ROLE_FRAME_KIND || it.role.as_str() == ROLE_NUMBER {
            p.at.y + shift
        } else {
            p.at.y - l.height / 2.0 + l.line_height / 2.0 + shift
        };
        let (text, _) = inked(it);
        lines.push(format!("text {} {text} {}", pt(p.at.x, y), content(l)));
    };
    let part = |lines: &mut Vec<String>, it: &Item, pa: &Part, suffix: &str| match &pa.outline {
        Outline::Box { rect: r, rounded } => {
            let radius = match (rounded, it.style.radius) {
                (false, _) => "-".to_string(),
                (true, None) => "r".to_string(),
                (true, Some(v)) => format!("rx={}", to_string(v)),
            };
            lines.push(format!(
                "rect {},{} {radius} {}{suffix}",
                pt(r.x, r.y),
                pt(r.w, r.h),
                part_paint(it, pa.paint)
            ));
        }
        Outline::Path(p) => {
            if let Some(s) = ellipse(p) {
                lines.push(format!("{s} {}{suffix}", part_paint(it, pa.paint)));
                return;
            }
            let dash = it.style.dash.as_ref().map_or(String::new(), |d| {
                let d: Vec<String> = d.iter().map(|&v| pele_num(v)).collect();
                format!(" dash={}", d.join(" "))
            });
            for s in subpaths(p) {
                lines.push(format!(
                    "path {s} {}{dash}{suffix}",
                    part_paint(it, pa.paint)
                ));
            }
        }
    };
    for (i, it) in sc.items.iter().enumerate() {
        match it.role.as_str() {
            ROLE_TITLE => label(&mut lines, it),
            ROLE_BOX => {
                let name = if it.id.as_deref().unwrap_or("").is_empty() {
                    String::new()
                } else {
                    hash(&it.id)
                };
                lines.push(format!("g pele-cluster pele-box{name}"));
                part(&mut lines, it, &it.parts[0], "");
                label(&mut lines, it);
            }
            ROLE_RECT | ROLE_DIVIDER => part(&mut lines, it, &it.parts[0], ""),
            ROLE_LIFELINE | ROLE_ACTIVATION | ROLE_DESTROY => {
                part(&mut lines, it, &it.parts[0], &hash(&it.id));
            }
            ROLE_FRAME => {
                let shape = it.shape.as_ref().map_or("", |s| s.as_str());
                lines.push(format!("g pele-frame pele-frame-{shape}"));
                part(&mut lines, it, &it.parts[0], "");
            }
            ROLE_CONDITION | ROLE_FRAME_KIND => {
                for pa in &it.parts {
                    part(&mut lines, it, pa, "");
                }
                label(&mut lines, it);
            }
            ROLE_MESSAGE => {
                lines.push(format!("g pele-edge pele-message{}", hash(&it.id)));
                part(&mut lines, it, &it.parts[0], "");
                if let Kind::Edge(e) = &it.kind {
                    for m in e.ends.iter().flatten() {
                        for pa in m.outline() {
                            let Outline::Path(p) = &pa.outline else {
                                continue;
                            };
                            for s in subpaths(p) {
                                lines.push(format!(
                                    "path {s} {}",
                                    marker_paint(it, pa.paint, m.kind.join())
                                ));
                            }
                        }
                    }
                }
                for pa in &it.parts[1..] {
                    part(&mut lines, it, pa, "");
                }
            }
            ROLE_NUMBER => {
                lines.push("g pele-sequence-number".into());
                part(&mut lines, it, &it.parts[0], "");
                label(&mut lines, it);
            }
            ROLE_MESSAGE_LABEL => {
                lines.push(format!("g pele-edge-label{}", hash(&it.id)));
                for pa in &it.parts {
                    part(&mut lines, it, pa, "");
                }
                label(&mut lines, it);
            }
            ROLE_NOTE => {
                lines.push(format!("g pele-note{}", hash(&it.id)));
                part(&mut lines, it, &it.parts[0], "");
                label(&mut lines, it);
            }
            ROLE_ACTOR => {
                let kind = match it.shape.as_ref().map_or("", |s| s.as_str()) {
                    "rect" => "participant",
                    k => k,
                };
                let mut cls = format!("pele-node pele-actor pele-actor-{kind}");
                // A participant drawn at the foot of its lifeline comes before the one at its
                // head.
                if sc.items[i + 1..]
                    .iter()
                    .any(|o| o.role.as_str() == ROLE_ACTOR && o.id == it.id)
                {
                    cls.push_str(" pele-actor-footer");
                }
                for c in &it.class {
                    cls.push(' ');
                    cls.push_str(c);
                }
                lines.push(format!("g {cls}{}", hash(&it.id)));
                for pa in &it.parts {
                    part(&mut lines, it, pa, "");
                }
                label(&mut lines, it);
            }
            role => lines.push(format!("unknown role {role}")),
        }
    }
    lines
}
