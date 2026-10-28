//! C4 diagrams read and laid out as Pele reads and draws them, checked against the oracle's c4
//! and c4-render fixtures.

use limu_fixtures::load;
use serde::Deserialize;

use super::db::{C4Db, C4Element, C4Named, Extra};
use super::lexer::{TOKEN_NAMES, tokenize};
use super::parser::parse_c4;
use super::render::{
    PATCH_RADIUS, ROLE_BOUNDARY, ROLE_BOUNDARY_DESCR, ROLE_BOUNDARY_LABEL, ROLE_BOUNDARY_TYPE,
    ROLE_CASING, ROLE_DEPLOYMENT, ROLE_ELEMENT, ROLE_ELEMENT_DESCR, ROLE_ELEMENT_TECHN,
    ROLE_ELEMENT_TYPE, ROLE_REL, ROLE_REL_LABEL, ROLE_REL_TECHN, ROLE_TITLE, layout,
};
use crate::css::declarations;
use crate::detect::DiagramType;
use crate::errors::ErrorCode;
use crate::js::number::to_string;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::{Paint, Source, Style};
use crate::scene::{Align, Item, Kind, Outline, Painting, Rect, Scene};
use crate::testing::pelesvg::{cap_join, id, stroke_ink, text_ink};
use crate::testing::{Enc, Failures, PeleError, same, sorted};
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
    direct: Direct,
    model: Option<Enc>,
    error: Option<PeleError>,
}

#[derive(Deserialize)]
struct Direct {
    tokens: Vec<(String, String)>,
    model: Option<Enc>,
    error: Option<PeleError>,
}

fn object(entries: Vec<(&str, Value)>) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn string(s: &str) -> Value {
    Value::String(s.to_string())
}

fn named(n: &C4Named) -> Value {
    Value::Array(vec![string(&n.key), string(&n.value)])
}

/// Sets a field Pele may leave undefined, where it is set.
fn opt(o: &mut Vec<(&'static str, Value)>, key: &'static str, v: &Option<String>) {
    if let Some(v) = v {
        o.push((key, string(v)));
    }
}

fn extra(o: &mut Vec<(&'static str, Value)>, extra: &Extra) {
    if let Some(list) = extra {
        let pairs = list
            .iter()
            .map(|(k, v)| Value::Array(vec![string(k), string(v)]))
            .collect();
        o.push(("extra", Value::Array(pairs)));
    }
}

/// An element as the oracle writes Pele's: every field that is defined, a parent by its
/// place among the boundaries.
fn element(e: &C4Element) -> Vec<(&'static str, Value)> {
    let parent = e.parent.map_or(Value::Null, |p| Value::Number(p as f64));
    let mut o = vec![
        ("alias", string(&e.alias)),
        ("label", string(&e.label)),
        ("parent", parent),
    ];
    if let Some(n) = &e.alias_attr {
        o.push(("aliasAttr", named(n)));
    }
    if let Some(n) = &e.label_attr {
        o.push(("labelAttr", named(n)));
    }
    for (key, v) in [
        ("type", &e.r#type),
        ("descr", &e.descr),
        ("techn", &e.techn),
        ("sprite", &e.sprite),
        ("tags", &e.tags),
        ("link", &e.link),
        ("bgColor", &e.bg_color),
        ("fontColor", &e.font_color),
        ("borderColor", &e.border_color),
        ("shadowing", &e.shadowing),
        ("shape", &e.shape),
        ("legendText", &e.legend_text),
        ("legendSprite", &e.legend_sprite),
    ] {
        opt(&mut o, key, v);
    }
    extra(&mut o, &e.extra);
    o
}

/// The model as the oracle writes Pele's.
fn dump_model(db: &C4Db) -> Value {
    let shapes = db
        .shapes
        .iter()
        .map(|s| {
            let mut o = element(&s.el);
            o.push(("kind", string(&s.kind)));
            object(o)
        })
        .collect();
    let boundaries = db
        .boundaries
        .iter()
        .map(|b| {
            let mut o = element(&b.el);
            opt(&mut o, "nodeType", &b.node_type);
            object(o)
        })
        .collect();
    let rels = db
        .rels
        .iter()
        .map(|r| {
            let mut o = vec![
                ("type", string(&r.r#type)),
                ("from", string(&r.from)),
                ("to", string(&r.to)),
                ("label", string(&r.label)),
            ];
            for (key, n) in [
                ("fromAttr", &r.from_attr),
                ("toAttr", &r.to_attr),
                ("labelAttr", &r.label_attr),
            ] {
                if let Some(n) = n {
                    o.push((key, named(n)));
                }
            }
            for (key, v) in [
                ("techn", &r.techn),
                ("descr", &r.descr),
                ("sprite", &r.sprite),
                ("tags", &r.tags),
                ("link", &r.link),
                ("textColor", &r.text_color),
                ("lineColor", &r.line_color),
            ] {
                opt(&mut o, key, v);
            }
            for (key, v) in [("offsetX", r.offset_x), ("offsetY", r.offset_y)] {
                if let Some(v) = v {
                    o.push((key, Value::Number(v)));
                }
            }
            extra(&mut o, &r.extra);
            object(o)
        })
        .collect();
    let mut o = vec![
        ("c4Type", db.c4_type.as_deref().map_or(Value::Null, string)),
        ("shapes", Value::Array(shapes)),
        ("boundaries", Value::Array(boundaries)),
        ("rels", Value::Array(rels)),
        ("shapeInRow", Value::Number(db.shape_in_row)),
        ("boundaryInRow", Value::Number(db.boundary_in_row)),
    ];
    opt(&mut o, "title", &db.title);
    opt(&mut o, "accDescr", &db.acc_descr);
    object(o)
}

fn tokens(src: &str) -> Vec<(String, String)> {
    let t = tokenize(src);
    t.types
        .iter()
        .enumerate()
        .map(|(i, &k)| {
            (
                TOKEN_NAMES[k].to_string(),
                src[t.starts[i]..t.ends[i]].to_string(),
            )
        })
        .collect()
}

/// Whether what was parsed is what Pele had: its model, or its error.
fn matches(
    got: Result<C4Db, crate::Error>,
    model: &Option<Enc>,
    error: &Option<PeleError>,
) -> bool {
    match (got, model, error) {
        (Err(e), _, Some(want)) => want.is(&e),
        (Ok(db), Some(want), None) => same(&sorted(&dump_model(&db)), &sorted(&want.0)),
        _ => false,
    }
}

fn parse_direct(src: &str) -> Result<C4Db, crate::Error> {
    let mut db = C4Db::default();
    parse_c4(src, &mut db).map(|()| db)
}

#[test]
fn parse_matches_pele() {
    let f: Cases = load("c4");
    assert!(f.cases.len() >= 5000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    let mut piped = 0;
    for c in &f.cases {
        let got = tokens(&c.src);
        failures.check(got == c.direct.tokens, || {
            format!("tokens of {:?}: {got:?}, Pele {:?}", c.src, c.direct.tokens)
        });
        let ok = matches(parse_direct(&c.src), &c.direct.model, &c.direct.error);
        failures.check(ok, || {
            format!(
                "parse_c4({:?}): {:?}, Pele {:?} {:?}",
                c.src,
                parse_direct(&c.src).map(|db| dump_model(&db)),
                c.direct.model,
                c.direct.error
            )
        });

        let p = prepare(&c.src, &ParseOptions::default());
        let ok = match (&p, &c.error) {
            (Err(e), Some(want)) => want.is(e),
            (Err(e), None) => e.code == ErrorCode::UnsupportedDiagram && c.diagram.is_none(),
            (Ok(p), _) if c.diagram.as_deref() != Some(p.found.diagram.name()) => false,
            (Ok(p), _) if p.found.diagram == DiagramType::C4 => {
                piped += 1;
                let got = p.parse_with(false, |s| super::parse(s, p.title.as_deref()));
                matches(got, &c.model, &c.error)
            }
            (Ok(_), _) => true,
        };
        failures.check(ok, || {
            format!("{:?}: model {:?}, error {:?}", c.src, c.model, c.error)
        });
    }
    assert!(piped >= 5000, "only {piped} cases were C4 diagrams");
    failures.done("c4", 3 * 8000);
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
    error: Option<PeleError>,
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

/// A paint as Pele's SVG names the theme's, "-" for none given and "?" for one it does not
/// name.
fn paint_name(p: Option<Paint>) -> &'static str {
    let Some(p) = p else {
        return "-";
    };
    if p == Paint::NONE {
        return "none";
    }
    [
        (Paint::BG, "var(--_bg)"),
        (Paint::FG, "var(--_fg)"),
        (Paint::MUTED, "var(--_m)"),
        (Paint::LINE, "var(--_l)"),
        (Paint::SURFACE, "var(--_s)"),
        (Paint::BORDER, "var(--_b)"),
    ]
    .into_iter()
    .find(|(q, _)| *q == p)
    .map_or("?", |(_, name)| name)
}

/// An author's fill and stroke as the test writes them, and nothing where the author set
/// neither, as Pele writes no style. A line of Pele's with no style is so read as it is.
fn colors(fill: Option<Paint>, stroke: Option<Paint>) -> String {
    if fill.is_none() && stroke.is_none() {
        return String::new();
    }
    let paint = |p: Option<Paint>| match p {
        None => "-".to_string(),
        Some(p) if p == Paint::NONE => "none".to_string(),
        Some(p) => {
            let c = match p.source() {
                Source::Own(c) => format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a),
                Source::Theme(t) => t.name().to_string(),
                Source::None => "none".to_string(),
            };
            format!("{c}/{}", p.factor())
        }
    };
    format!("fill={} stroke={}", paint(fill), paint(stroke))
}

/// Writes the style at the end of one of the oracle's lines as the author's colors limu reads
/// of it.
fn read_css(line: &str) -> String {
    let Some(at) = line.find(" css=") else {
        return line.to_string();
    };
    let decls: Vec<String> = line[at + 5..].split(';').map(str::to_string).collect();
    let style = declarations(&decls).style;
    format!("{} css={}", &line[..at], colors(style.fill, style.stroke))
}

/// The fill Pele writes on an item's text: the color ink picks where the look picks one, the
/// diagram's own otherwise, and "-" for the theme's text color.
fn text_paint(it: &Item) -> &'static str {
    let ink = text_ink(it);
    if !ink.is_empty() {
        return ink;
    }
    if it.style.text == Some(Paint::FG) {
        return "-";
    }
    paint_name(it.style.text)
}

/// The stroke Pele writes on an item: the color ink picks where the look picks one, and the
/// diagram's own otherwise.
fn stroke_paint(it: &Item) -> &'static str {
    let ink = stroke_ink(it);
    if !ink.is_empty() {
        return ink;
    }
    paint_name(it.style.stroke)
}

fn dash(st: &Style) -> String {
    match &st.dash {
        Some(d) if !d.is_empty() => d
            .iter()
            .map(|&v| to_string(v))
            .collect::<Vec<_>>()
            .join(" "),
        _ => "-".into(),
    }
}

fn pt(x: f64, y: f64) -> String {
    format!("{},{}", to_string(x), to_string(y))
}

fn boxed(r: Rect) -> String {
    format!("{},{}", pt(r.x, r.y), pt(r.w, r.h))
}

/// A label as the oracle writes one: where it is drawn about, its size, the size of its type
/// and its lines, and the paint of its text. A label whose lines start at the point and hold
/// no text Pele writes nothing of.
fn label(lines: &mut Vec<String>, it: &Item, extra: &str) {
    let Some(p) = &it.label else {
        return;
    };
    let l = &p.label;
    let kind = if p.align == Align::Start {
        if l.lines.iter().flatten().all(|s| s.text.is_empty()) {
            return;
        }
        "l"
    } else {
        "t"
    };
    let text: Vec<String> = l
        .lines
        .iter()
        .map(|line| {
            line.iter()
                .map(|s| format!("{}:{}", s.face.bits(), s.text))
                .collect::<Vec<_>>()
                .join("+")
        })
        .collect();
    lines.push(format!(
        "{kind} {} {} {} | {} {extra}{} css={}",
        pt(p.at.x, p.at.y),
        pt(l.width, l.height),
        to_string(l.size),
        text.join(" | "),
        text_paint(it),
        colors(it.author.text, None)
    ));
}

/// Writes a scene as the oracle reads Pele's SVG.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    if let Some(desc) = &sc.desc {
        lines.push(format!("acc-desc {desc}"));
    }
    let items = &sc.items;
    let same_id = |i: usize, it: &Item| i < items.len() && items[i].id == it.id;
    let mut i = 0;
    while i < items.len() {
        let it = &items[i];
        let data = id(it.id.as_deref().unwrap_or(""));
        match it.role.as_str() {
            ROLE_TITLE => label(&mut lines, it, "title "),
            ROLE_BOUNDARY | ROLE_DEPLOYMENT => {
                let kind = if it.role.as_str() == ROLE_DEPLOYMENT {
                    "node"
                } else {
                    "boundary"
                };
                lines.push(format!("cluster {kind} {data}"));
                let Outline::Box { rect, rounded } = &it.parts[0].outline else {
                    panic!("a boundary that is not a box");
                };
                lines.push(format!(
                    "rect {}{} {} {} {} css={}",
                    boxed(*rect),
                    if *rounded { " r" } else { "" },
                    paint_name(it.style.fill),
                    paint_name(it.style.stroke),
                    dash(&it.style),
                    colors(it.author.fill, it.author.stroke)
                ));
                let mut linked = false;
                while same_id(i + 1, it)
                    && [ROLE_BOUNDARY_LABEL, ROLE_BOUNDARY_TYPE, ROLE_BOUNDARY_DESCR]
                        .contains(&items[i + 1].role.as_str())
                {
                    let next = &items[i + 1];
                    if let Some(link) = &next.link
                        && !linked
                    {
                        lines.push(format!("link {link}"));
                        linked = true;
                    }
                    label(&mut lines, next, "");
                    i += 1;
                }
            }
            ROLE_CASING => {
                // Pele's casing is a path and the outlines of the relation's markers, outside
                // any group the oracle names.
                for part in &it.parts {
                    lines.push(match part.outline {
                        Outline::Box { .. } => "rect outside".into(),
                        Outline::Path(_) => "path outside".into(),
                    });
                }
            }
            ROLE_REL => {
                lines.push(format!(
                    "edge {data} {} {} {} css={}",
                    paint_name(it.style.fill),
                    stroke_paint(it),
                    cap_join(it),
                    colors(None, it.author.stroke)
                ));
                let Outline::Path(p) = &it.parts[0].outline else {
                    panic!("a relation that is not a path");
                };
                let mut steps = Vec::new();
                let mut at = 0;
                for &op in &p.ops {
                    let mut s = "MLQCZ"[op as usize..op as usize + 1].to_string();
                    for xy in p.args[at..at + op.args()].chunks(2) {
                        s += &format!(" {}", pt(xy[0], xy[1]));
                    }
                    at += op.args();
                    steps.push(s);
                }
                lines.push(format!("path {}", steps.join(" ")));
                if let Kind::Edge(e) = &it.kind {
                    for m in e.ends.iter().flatten() {
                        lines.push(format!(
                            "m {} {} {}",
                            m.kind.name(),
                            pt(m.at.x, m.at.y),
                            pt(m.dx, m.dy)
                        ));
                    }
                }
            }
            ROLE_ELEMENT => {
                let shape = it.shape.as_ref().map_or("", |s| s.as_str());
                let mut classes = format!("pele-shape-{shape} pele-c4-{}", it.tags[0]);
                if it.tags.len() > 1 {
                    classes += " pele-c4-external";
                }
                // The middle, which Pele moves the shape's inside by, is checked through the
                // box about it and through where the inside is drawn.
                lines.push(format!("node {data} {classes} box {}", boxed(it.bounds)));
                if let Some(link) = &it.link {
                    lines.push(format!("link {link}"));
                }
                for part in &it.parts {
                    let (fill, author) = if part.paint == Painting::StrokeOnly {
                        ("none", colors(None, it.author.stroke))
                    } else {
                        (
                            paint_name(it.style.fill),
                            colors(it.author.fill, it.author.stroke),
                        )
                    };
                    let what = match &part.outline {
                        Outline::Box {
                            rect,
                            rounded: true,
                        } => format!("rect {}", boxed(*rect)),
                        _ => "shape".into(),
                    };
                    lines.push(format!(
                        "part {what} {fill} {} {} {} css={author}",
                        paint_name(it.style.stroke),
                        dash(&it.style),
                        cap_join(it)
                    ));
                }
                // The texts that follow the shape in the scene, in the order Pele writes them:
                // the type, the name, the technology, the description.
                let (mut kind, mut techn, mut descr) = (None, None, None);
                while same_id(i + 1, it) {
                    let next = &items[i + 1];
                    match next.role.as_str() {
                        ROLE_ELEMENT_TYPE => kind = Some(next),
                        ROLE_ELEMENT_TECHN => techn = Some(next),
                        ROLE_ELEMENT_DESCR => descr = Some(next),
                        _ => break,
                    }
                    i += 1;
                }
                for x in [kind, Some(it), techn, descr].into_iter().flatten() {
                    label(&mut lines, x, "");
                }
            }
            ROLE_REL_LABEL => {
                lines.push(format!("label {data}"));
                if let Some(tip) = it.tooltip.as_deref().filter(|t| !t.is_empty()) {
                    lines.push(format!("tip {tip}"));
                }
                lines.push(format!(
                    "patch {} {} {} - -",
                    boxed(it.bounds),
                    PATCH_RADIUS,
                    paint_name(it.style.fill)
                ));
                if let Some(link) = &it.link {
                    lines.push(format!("link {link}"));
                }
                label(&mut lines, it, "");
                if same_id(i + 1, it) && items[i + 1].role.as_str() == ROLE_REL_TECHN {
                    i += 1;
                    label(&mut lines, &items[i], "");
                }
            }
            role => lines.push(format!("unknown role {role}")),
        }
        i += 1;
    }
    lines
}

/// What Pele's render draws of C4 diagrams, read back out of its SVG as a line for each thing
/// drawn, in the order drawn, against the same lines written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("c4-render");
    assert!(f.cases.len() >= 3000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a C4 diagram");
        let m = p
            .parse_with(false, |s| super::parse(s, p.title.as_deref()))
            .expect("a C4 diagram that parses");
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            font_size: c.font_size.unwrap_or(16.0),
            ..Options::default()
        };
        match (layout(&m, &Map::new(), &o), &c.error) {
            (Err(e), Some(want)) => {
                failures.check(want.is(&e), || format!("{:?}: {e:?}, Pele {want:?}", c.src));
            }
            (Err(e), None) => failures.check(false, || format!("{:?}: {e:?}", c.src)),
            (Ok(_), Some(want)) => {
                failures.check(false, || format!("{:?}: no error, Pele {want:?}", c.src));
            }
            (Ok(sc), None) => {
                let got = scene_lines(&sc);
                let diff = drawings.diff(k, &c.drawing, &got, Some(&read_css));
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
    failures.done("renders", 5000);
}
