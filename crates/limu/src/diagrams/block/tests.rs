//! Block diagrams read and laid out as Pele reads and draws them, checked against the
//! oracle's block and block-render fixtures.

use limu_fixtures::load;
use serde::Deserialize;

use super::db::{Block, BlockDb, ROOT};
use super::lexer::tokenize;
use super::parser::parse_block;
use super::render::{
    ROLE_BLOCK, ROLE_CASING, ROLE_COMPOSITE, ROLE_EDGE, ROLE_EDGE_LABEL, ROLE_TITLE, layout,
};
use crate::detect::DiagramType;
use crate::draw::draw_shape;
use crate::errors::ErrorCode;
use crate::js::chars::len16;
use crate::js::number::to_string as n;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::Paint;
use crate::scene::{Item, Kind, Marker, MarkerType, Op, Outline, Part, Path, Placed, Point, Scene};
use crate::testing::pelesvg::{cap_join, id, stroke_ink, text_ink};
use crate::testing::{Enc, Failures, PeleError, pele_d, pele_num, same, sorted};
use crate::value::{Map, Value};

#[derive(Deserialize)]
struct Cases {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    src: String,
    #[serde(rename = "type")]
    diagram: Option<String>,
    tokens: Vec<(usize, String, usize)>,
    direct: Outcome,
    #[serde(flatten)]
    piped: Outcome,
    /// Whether the pipeline's result is the direct one.
    #[serde(default)]
    same: bool,
}

#[derive(Deserialize)]
struct Outcome {
    model: Option<Enc>,
    error: Option<PeleError>,
}

fn string(s: &str) -> Value {
    Value::String(s.to_string())
}

fn undefined() -> Value {
    Value::Object(
        [("$".to_string(), string("undefined"))]
            .into_iter()
            .collect(),
    )
}

/// An optional number as the oracle writes it.
fn num(v: Option<f64>) -> Value {
    v.map_or_else(undefined, Value::Number)
}

/// An optional string as the oracle writes it.
fn text(s: &Option<String>) -> Value {
    s.as_deref().map_or_else(undefined, string)
}

/// An optional list of strings as the oracle writes it.
fn list(s: &Option<Vec<String>>) -> Value {
    s.as_ref().map_or_else(undefined, |s| {
        Value::Array(s.iter().map(|s| string(s)).collect())
    })
}

fn dump_tokens(src: &str) -> Vec<(usize, String, usize)> {
    let t = tokenize(src);
    t.types
        .iter()
        .enumerate()
        .map(|(i, &k)| (k, t.texts[i].clone(), len16(&src[..t.starts[i]])))
        .collect()
}

/// A block's own fields, as the oracle writes them in turn.
fn flat(b: &Block) -> Vec<Value> {
    vec![
        text(&b.id),
        string(&b.r#type),
        text(&b.label),
        num(b.columns),
        num(b.width_in_columns),
        num(b.width),
        list(&b.directions),
        list(&b.classes),
        list(&b.styles),
        num(b.color_index.map(|c| c as f64)),
    ]
}

fn tree(db: &BlockDb, b: usize) -> Value {
    let block = &db.arena[b];
    let mut o = flat(block);
    o.push(block.children.as_ref().map_or_else(undefined, |children| {
        Value::Array(children.iter().map(|&c| tree(db, c)).collect())
    }));
    Value::Array(o)
}

/// The model as the oracle writes Pele's.
fn dump_model(db: &BlockDb) -> Value {
    let mut o = Map::new();
    o.insert("root", tree(db, ROOT));
    if let Some(title) = db.title.as_deref().filter(|t| !t.is_empty()) {
        o.insert("title", string(title));
    }
    let blocks = db.blocks.iter().map(|&b| text(&db.arena[b].id)).collect();
    o.insert("blocks", Value::Array(blocks));
    let edges = db
        .edges
        .iter()
        .map(|&e| {
            let e = &db.arena[e];
            Value::Array(vec![
                text(&e.id),
                text(&e.start),
                text(&e.end),
                text(&e.label),
                string(&e.r#type),
                text(&e.thickness),
                text(&e.pattern),
                list(&e.directions),
                text(&e.arrow_type_end),
                text(&e.arrow_type_start),
            ])
        })
        .collect();
    o.insert("edges", Value::Array(edges));
    let classes = db
        .classes
        .values()
        .map(|(k, c)| {
            let strings = |s: &[String]| Value::Array(s.iter().map(|s| string(s)).collect());
            Value::Array(vec![
                string(k),
                string(&c.id),
                strings(&c.styles),
                strings(&c.text_styles),
            ])
        })
        .collect();
    o.insert("classes", Value::Array(classes));
    let by_id = db
        .by_id
        .values()
        .map(|(k, b)| {
            let block = &db.arena[*b];
            let mut row = vec![string(k)];
            row.extend(flat(block));
            row.push(block.children.as_ref().map_or_else(undefined, |children| {
                Value::Array(children.iter().map(|&c| text(&db.arena[c].id)).collect())
            }));
            Value::Array(row)
        })
        .collect();
    o.insert("byId", Value::Array(by_id));
    Value::Object(o)
}

/// Whether what was parsed is what Pele had: its model, or its error.
fn matches(got: Result<BlockDb, crate::Error>, want: &Outcome) -> bool {
    match (got, &want.model, &want.error) {
        (Err(e), _, Some(want)) => want.is(&e),
        (Ok(db), Some(want), None) => same(&sorted(&dump_model(&db)), &sorted(&want.0)),
        _ => false,
    }
}

fn parse_direct(src: &str) -> Result<BlockDb, crate::Error> {
    let mut db = BlockDb::default();
    parse_block(src, &mut db).map(|()| db)
}

#[test]
fn parse_matches_pele() {
    let f: Cases = load("block");
    assert!(f.cases.len() >= 7000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    let mut piped = 0;
    for c in &f.cases {
        let got = dump_tokens(&c.src);
        failures.check(got == c.tokens, || {
            format!("tokens of {:?}: {got:?}, Pele {:?}", c.src, c.tokens)
        });
        failures.check(matches(parse_direct(&c.src), &c.direct), || {
            format!(
                "parse_block({:?}): {:?}, Pele {:?} {:?}",
                c.src,
                parse_direct(&c.src).map(|db| dump_model(&db)),
                c.direct.model,
                c.direct.error
            )
        });

        let want = if c.same { &c.direct } else { &c.piped };
        let p = prepare(&c.src, &ParseOptions::default());
        let ok = match (&p, &want.error) {
            (Err(e), Some(want)) => want.is(e),
            // Pele's suite stops where no type is found, with no error.
            (Err(e), None) => e.code == ErrorCode::UnsupportedDiagram && c.diagram.is_none(),
            (Ok(p), _) if c.diagram.as_deref() != Some(p.found.diagram.name()) => false,
            (Ok(p), _) if p.found.diagram == DiagramType::Block => {
                piped += 1;
                matches(
                    p.parse_with(false, |s| super::parse(s, p.title.as_deref())),
                    want,
                )
            }
            (Ok(_), _) => true,
        };
        failures.check(ok, || {
            let got = p.and_then(|p| p.parse_with(false, |s| super::parse(s, p.title.as_deref())));
            format!(
                "{:?}: {:?}, Pele {:?} {:?}",
                c.src,
                got.map(|db| dump_model(&db)),
                want.model,
                want.error
            )
        });
    }
    assert!(piped >= 6500, "only {piped} cases were block diagrams");
    failures.done("block", 3 * 7000);
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
    /// The message of the error Pele throws instead of drawing.
    error: Option<String>,
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

fn pt(x: f64, y: f64) -> String {
    format!("{},{}", n(x), n(y))
}

/// A paint as the oracle writes it: its name in Pele's SVG, and its opacity after a slash
/// where it has one; "-" for none given and "?" for one Pele's SVG does not name.
fn paint_name(p: Option<Paint>) -> String {
    let Some(p) = p else {
        return "-".into();
    };
    if p == Paint::NONE {
        return "none".into();
    }
    let name = [
        (Paint::BG, "var(--_bg)"),
        (Paint::MUTED, "var(--_m)"),
        (Paint::LINE, "var(--_l)"),
        (Paint::SURFACE, "var(--_s)"),
        (Paint::SURFACE_ALT, "var(--_a)"),
        (Paint::BORDER, "var(--_b)"),
    ]
    .into_iter()
    .find(|(q, _)| q.as_token().is_some() && q.as_token() == p.as_token() && p.base().is_none())
    .map(|(_, name)| name);
    match (name, p.factor()) {
        (None, _) => "?".into(),
        (Some(name), 1.0) => name.into(),
        (Some(name), f) => format!("{name}/{f}"),
    }
}

/// How a part of an item is painted, as the oracle reads Pele's fill and stroke: the
/// diagram's own at the opacities the author's style laid over the diagram's gives.
fn part_paint(it: &Item, part: &Part) -> String {
    let s = it.author.over(&it.style);
    let stroke = paint_name(
        it.style
            .stroke
            .map(|p| p.alpha(s.stroke_opacity.unwrap_or(1.0))),
    );
    let fill = paint_name(
        it.style
            .fill
            .map(|p| p.alpha(s.fill_opacity.unwrap_or(1.0))),
    );
    match part.paint {
        crate::scene::Painting::StrokeOnly => format!("none {stroke}"),
        crate::scene::Painting::FillOnly => format!("{fill} -"),
        _ => format!("{fill} {stroke}"),
    }
}

/// The fill the oracle reads of an item's text: the color ink picks where the look picks
/// one, and the diagram's own otherwise.
fn text_paint(it: &Item) -> String {
    let ink = text_ink(it);
    if !ink.is_empty() {
        return ink.into();
    }
    paint_name(it.style.text)
}

/// The line the oracle writes of a label: the point it is drawn about, its size, the fill of
/// its text, and its spans.
fn label_line(it: &Item, p: &Placed) -> String {
    let l = &p.label;
    let lines: Vec<String> = l
        .lines
        .iter()
        .map(|line| {
            line.iter()
                .map(|s| match &s.icon {
                    Some(icon) => format!("i:{icon}:{}", n(s.width)),
                    None => format!("{}:{}:{}", s.face.bits(), s.text, n(s.width)),
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    format!(
        "t {} {} {} | {}",
        pt(p.at.x, p.at.y),
        pt(l.width, l.height),
        text_paint(it),
        lines.join(" | ")
    )
}

/// The outline Pele's marker draws, every number in full, as the oracle reads it, filled and
/// stroked as given where it is filled.
fn marker(m: &Marker, fill: &str, stroke: &str) -> Option<String> {
    let arrow = crate::scene::MARKER_SIZE;
    let (x, y, dx, dy) = (m.at.x, m.at.y, m.dx, m.dy);
    let (nx, ny) = (-dy, dx);
    match m.kind {
        MarkerType::ArrowPoint => {
            let (bx, by) = (x - dx * arrow, y - dy * arrow);
            let half = arrow * 0.42;
            Some(format!(
                "M {} L {} L {} Z {fill} {stroke}",
                pt(x, y),
                pt(bx + nx * half, by + ny * half),
                pt(bx - nx * half, by - ny * half)
            ))
        }
        MarkerType::ArrowCircle => Some(format!(
            "o {} 3.5 {fill} {stroke}",
            pt(x - dx * 4.0, y - dy * 4.0)
        )),
        MarkerType::ArrowCross => {
            let (cx, cy) = (x - dx * 4.0, y - dy * 4.0);
            let a = 3.5;
            let (ux, uy) = ((dx + nx) * a, (dy + ny) * a);
            let (vx, vy) = ((dx - nx) * a, (dy - ny) * a);
            Some(format!(
                "M {} L {} M {} L {} - -",
                pt(cx - ux, cy - uy),
                pt(cx + ux, cy + uy),
                pt(cx - vx, cy - vy),
                pt(cx + vx, cy + vy)
            ))
        }
        _ => None,
    }
}

/// An arc of Pele's path, as the oracle writes it: its radii, turn and flags, and where it
/// ends, with the text of its numbers.
struct Arc {
    rx: f64,
    ry: f64,
    rot: f64,
    large: bool,
    sweep: bool,
    to: Point,
    text: String,
}

/// The arcs of a line of Pele's, in order.
fn arcs(line: &str) -> Vec<Arc> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let mut out = Vec::new();
    for (i, &f) in fields.iter().enumerate() {
        if f != "A" || i + 2 >= fields.len() {
            continue;
        }
        let a: Vec<f64> = fields[i + 1]
            .split(',')
            .chain(fields[i + 2].split(','))
            .map(|s| s.parse().unwrap_or(0.0))
            .collect();
        if a.len() != 7 {
            continue;
        }
        out.push(Arc {
            rx: a[0],
            ry: a[1],
            rot: a[2],
            large: a[3] == 1.0,
            sweep: a[4] == 1.0,
            to: Point::new(a[5], a[6]),
            text: fields[i + 1].to_string(),
        });
    }
    out
}

/// A path as the oracle reads Pele's. A run of curves is written as the arc of Pele's line at
/// the same place, where arc_to draws that arc with exactly those curves, and as curves
/// elsewhere, which no line of Pele's has.
fn path_line(p: &Path, mut hints: &[Arc]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut last = Point::new(0.0, 0.0);
    let (ops, args) = (p.ops(), p.args());
    let (mut i, mut at) = (0, 0);
    while i < ops.len() {
        let op = ops[i];
        if op != Op::CubeTo {
            let a = &args[at..at + op.args()];
            let mut s = "MLQCZ"[op as usize..op as usize + 1].to_string();
            for xy in a.chunks(2) {
                s += &format!(" {}", pt(xy[0], xy[1]));
            }
            if a.len() >= 2 {
                last = Point::new(a[a.len() - 2], a[a.len() - 1]);
            }
            out.push(s);
            i += 1;
            at += op.args();
            continue;
        }
        let (mut j, mut end) = (i, at);
        while j < ops.len() && ops[j] == Op::CubeTo {
            end += 6;
            j += 1;
        }
        let curves = &args[at..end];
        let mut s = String::new();
        if let Some((h, rest)) = hints.split_first() {
            hints = rest;
            let mut q = Path::new();
            q.move_to(last.x, last.y)
                .arc_to(h.rx, h.ry, h.rot, h.large, h.sweep, h.to.x, h.to.y);
            if q.args()[2..] == *curves && !q.ops()[1..].contains(&Op::LineTo) {
                s = format!("A {} {}", h.text, pt(h.to.x, h.to.y));
            }
        }
        if s.is_empty() {
            s = "C".into();
            for xy in curves.chunks(2) {
                s += &format!(" {}", pt(xy[0], xy[1]));
            }
        }
        out.push(s);
        last = Point::new(curves[curves.len() - 2], curves[curves.len() - 1]);
        (i, at) = (j, end);
    }
    out.join(" ")
}

/// The parts of an outline, with numbers as Pele writes them: the center of a block, taken
/// from its box, may be off the one it was drawn about in the last bit.
fn parts_of(parts: &[Part]) -> String {
    let mut out = String::new();
    for p in parts {
        match &p.outline {
            Outline::Box { rect, rounded } => {
                out += &format!(
                    "{},{},{},{} {rounded}",
                    pele_num(rect.x),
                    pele_num(rect.y),
                    pele_num(rect.w),
                    pele_num(rect.h)
                )
            }
            Outline::Path(path) => out += &pele_d(path),
        }
        out += &format!(" {:?}; ", p.paint);
    }
    out
}

fn path_of(part: &Part) -> &Path {
    match &part.outline {
        Outline::Path(p) => p,
        Outline::Box { .. } => panic!("a box where a path was looked for"),
    }
}

/// Writes a scene as the oracle reads Pele's SVG. want is the line Pele drew at an index, or
/// "" where the fixture does not hold it, and the arcs of its lines are hints for writing the
/// scene's curves. The oracle keeps the lines of paths with arcs in full.
fn scene_lines(sc: &Scene, want: &dyn Fn(usize) -> String) -> Vec<String> {
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    for (i, it) in sc.items.iter().enumerate() {
        let data = id(it.id.as_deref().unwrap_or(""));
        let classes = it.class.join(" ");
        match it.role.as_str() {
            ROLE_TITLE => {
                let p = it.label.as_ref().expect("a title's label");
                lines.push(format!("title {}", label_line(it, p)));
            }
            ROLE_COMPOSITE => {
                let Outline::Box { rect: r, rounded } = it.parts[0].outline else {
                    panic!("a composite that is not a box");
                };
                let mut s = format!(
                    "cluster {data} [{classes}] {},{}",
                    pt(r.x, r.y),
                    pt(r.w, r.h)
                );
                if rounded {
                    s += " r";
                }
                if r != it.bounds {
                    s += " box differs";
                }
                lines.push(format!("{s} {}", part_paint(it, &it.parts[0])));
                lines.push(format!("pen {}", cap_join(it)));
                if let Some(p) = &it.label {
                    lines.push(label_line(it, p));
                }
            }
            ROLE_BLOCK => {
                let r = it.bounds;
                let shape = it.shape.as_ref().map_or("", |s| s.as_str());
                lines.push(format!(
                    "node {data} {shape} [{classes}] {},{}",
                    pt(r.x, r.y),
                    pt(r.w, r.h)
                ));
                for part in &it.parts {
                    lines.push(format!("part {}", part_paint(it, part)));
                    if shape == "block-arrow" {
                        let p = path_of(part);
                        let mut points = Vec::new();
                        let mut at = 0;
                        for &op in p.ops() {
                            if op != Op::Close {
                                points.push(pt(p.args()[at], p.args()[at + 1]));
                            }
                            at += op.args();
                        }
                        lines.push(format!("polygon {}", points.join(" ")));
                    }
                }
                if shape != "block-arrow" {
                    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
                    if parts_of(&it.parts) != parts_of(&draw_shape(shape, cx, cy, r.w, r.h)) {
                        lines.push("parts differ from the outline".into());
                    }
                }
                if !it.parts.is_empty() {
                    lines.push(format!("pen {}", cap_join(it)));
                }
                if let Some(p) = &it.label {
                    lines.push(label_line(it, p));
                }
            }
            ROLE_EDGE => {
                let dash = match &it.style.dash {
                    Some(d) if !d.is_empty() => {
                        d.iter().map(|&v| n(v)).collect::<Vec<_>>().join(" ")
                    }
                    _ => "-".into(),
                };
                let width = it.style.stroke_width.map_or("-".into(), n);
                let ink = stroke_ink(it);
                let (over, color) = if ink.is_empty() {
                    ("-", "var(--_l)")
                } else {
                    (ink, ink)
                };
                lines.push(format!("edge {data} {dash} {width} {over}"));
                let hints = arcs(&want(lines.len()));
                lines.push(format!("path {}", path_line(path_of(&it.parts[0]), &hints)));
                lines.push(format!("pen {}", cap_join(it)));
                if let Kind::Edge(e) = &it.kind {
                    for m in e.ends.iter().flatten() {
                        if let Some(s) = marker(m, color, "none") {
                            lines.push(format!("m {s}"));
                        }
                    }
                }
            }
            ROLE_CASING => {
                // A casing is written as the oracle reads Pele's: its width where it is not
                // the casings' 5, its path, and the markers of its edge, drawn in the
                // background.
                let w = it.style.stroke_width.unwrap_or(1.0);
                let mut width = if w == 5.0 { "-".into() } else { n(w) };
                if it.style.opacity.is_some() || it.style.stroke_opacity.is_some() {
                    width += " opacity";
                }
                lines.push(format!("casing {width} -"));
                let hints = arcs(&want(lines.len()));
                lines.push(format!("path {}", path_line(path_of(&it.parts[0]), &hints)));
                let edge = sc.items[i + 1..]
                    .iter()
                    .find(|e| e.role.as_str() == ROLE_EDGE && e.id == it.id);
                let Some(Item {
                    kind: Kind::Edge(edge),
                    ..
                }) = edge
                else {
                    lines.push("casing of no edge".into());
                    continue;
                };
                let mut outline = Vec::new();
                for m in edge.ends.iter().flatten() {
                    outline.extend(m.outline());
                    if let Some(s) = marker(m, "var(--_bg)", "var(--_bg)") {
                        lines.push(format!("cm {s}"));
                    }
                }
                if outline.len() != it.parts.len() - 1
                    || outline
                        .iter()
                        .zip(&it.parts[1..])
                        .any(|(a, b)| pele_d(path_of(a)) != pele_d(path_of(b)))
                {
                    lines.push("casing's markers differ".into());
                }
            }
            ROLE_EDGE_LABEL => {
                let r = it.bounds;
                let mut patch = Path::new();
                patch.round_rect(r, 3.0);
                if pele_d(path_of(&it.parts[0])) != pele_d(&patch) {
                    lines.push("patch not the label's box".into());
                }
                lines.push(format!(
                    "elabel {data} {},{} {}",
                    pt(r.x, r.y),
                    pt(r.w, r.h),
                    paint_name(it.style.fill)
                ));
                let p = it.label.as_ref().expect("an edge label's label");
                let l = &p.label;
                let shift = l.size * 0.35;
                lines.push(format!(
                    "t {}",
                    pt(
                        p.at.x,
                        p.at.y - l.height / 2.0 + l.line_height / 2.0 + shift
                    )
                ));
            }
            role => lines.push(format!("unknown role {role}")),
        }
    }
    lines
}

/// What Pele's render draws of block diagrams, read back out of its SVG as a line for each
/// thing drawn, in the order drawn, against the same lines written of the scene, and the
/// errors of those it does not draw.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("block-render");
    assert!(f.cases.len() >= 4000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    let mut errors = 0;
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a block diagram");
        let m = p
            .parse_with(false, |s| super::parse(s, p.title.as_deref()))
            .expect("a block diagram that parses");
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            font_size: c.font_size.unwrap_or(16.0),
            ..Options::default()
        };
        match (layout(&m, &p.config, &o), &c.error) {
            (Err(e), Some(want)) => {
                errors += 1;
                failures.check(e.message == *want, || {
                    format!("{:?}: {e:?}, Pele {want:?}", c.src)
                });
            }
            (Err(e), None) => failures.check(false, || format!("{:?}: {e:?}", c.src)),
            (Ok(_), Some(want)) => {
                failures.check(false, || format!("{:?}: no error, Pele {want:?}", c.src));
            }
            (Ok(sc), None) => {
                let got = scene_lines(&sc, &|i| c.drawing.line(i).to_string());
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
        }
    }
    failures.done("renders", 4000);
    assert!(errors >= 10, "only {errors} cases Pele could not draw");
}
