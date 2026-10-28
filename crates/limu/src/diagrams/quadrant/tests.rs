//! Quadrant charts parsed and laid out against what Pele makes of the same cases, in the
//! oracle's quadrant fixtures. The tokens and the model are written as the oracle writes
//! Pele's, field by field, so that any field that differs shows. The scene is written as the
//! oracle reads Pele's SVG, a line for each thing drawn, its numbers in full.

use limu_fixtures::load;
use serde::Deserialize;

use super::db::{PointStyle, QuadrantDb, QuadrantText};
use super::lexer::tokenize;
use super::parser::parse_quadrant;
use super::render::{ROLE_AXIS, ROLE_POINT, ROLE_POINT_LABEL, ROLE_QUADRANT, ROLE_TITLE};
use super::{layout, parse};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::len16;
use crate::js::number::to_string;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::{Paint, Source};
use crate::scene::text::{Face, Label};
use crate::scene::{Item, Outline, Scene};
use crate::testing::pelesvg::{cap_join, id};
use crate::testing::{Enc, Failures, PeleError, same, sorted};
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
    /// Whether what the pipeline makes of the text is what the parser makes of it as it is.
    #[serde(default)]
    same: bool,
}

/// What Pele made of a text: its tokens, and its model or its error.
#[derive(Deserialize)]
struct Wanted {
    tokens: Option<Enc>,
    model: Option<Enc>,
    error: Option<PeleError>,
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

fn object(entries: Vec<(&str, Value)>) -> Value {
    let mut m = Map::new();
    for (k, v) in entries {
        m.insert(k, v);
    }
    Value::Object(m)
}

fn dump_text(t: Option<&QuadrantText>) -> Value {
    let Some(t) = t else {
        return Value::Null;
    };
    let kind = if t.markdown { "markdown" } else { "text" };
    object(vec![
        ("text", Value::String(t.text.clone())),
        ("type", Value::String(kind.into())),
    ])
}

fn dump_style(s: &PointStyle) -> Value {
    let mut out = Vec::new();
    if let Some(r) = s.radius {
        out.push(("radius", Value::Number(r)));
    }
    let strings = [
        ("color", &s.color),
        ("strokeColor", &s.stroke_color),
        ("strokeWidth", &s.stroke_width),
    ];
    for (key, v) in strings {
        if let Some(v) = v {
            out.push((key, Value::String(v.clone())));
        }
    }
    object(out)
}

/// The model as the oracle's dumpModel writes it, its keys sorted, as are the fixture's for
/// the comparison.
fn dump_model(m: &QuadrantDb) -> Value {
    let quadrants = m.quadrants.iter().map(|q| dump_text(q.as_ref()));
    let points = m.points.iter().map(|p| {
        object(vec![
            ("x", Value::Number(p.x)),
            ("y", Value::Number(p.y)),
            ("text", dump_text(Some(&p.text))),
            ("className", Value::String(p.class_name.clone())),
            ("style", dump_style(&p.style)),
        ])
    });
    let classes = m
        .classes
        .values()
        .map(|(name, style)| Value::Array(vec![Value::String(name.clone()), dump_style(style)]));
    let mut o = vec![
        ("quadrants", Value::Array(quadrants.collect())),
        ("xAxisLeft", dump_text(m.x_axis_left.as_ref())),
        ("xAxisRight", dump_text(m.x_axis_right.as_ref())),
        ("yAxisBottom", dump_text(m.y_axis_bottom.as_ref())),
        ("yAxisTop", dump_text(m.y_axis_top.as_ref())),
        ("points", Value::Array(points.collect())),
        ("classes", Value::Array(classes.collect())),
    ];
    let opt = [
        ("title", &m.title),
        ("accTitle", &m.acc_title),
        ("accDescr", &m.acc_descr),
    ];
    for (key, v) in opt {
        if let Some(v) = v.as_deref().filter(|v| !v.is_empty()) {
            o.push((key, Value::String(v.into())));
        }
    }
    sorted(&object(o))
}

/// Whether what was made of src is what Pele made: the same tokens, and the same error, or
/// the same model. It says how they differ where they do.
fn compare(src: &str, got: Result<QuadrantDb, Error>, want: &Wanted) -> Result<(), String> {
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
        (Ok(m), None) => {
            let got = dump_model(&m);
            let wm = want.model.as_ref().map(|e| sorted(&e.0));
            if !wm.as_ref().is_some_and(|w| same(&got, w)) {
                return Err(format!("model {got:?}\nPele {wm:?}"));
            }
            Ok(())
        }
    }
}

/// What Pele's tests make of the text: the parser given it as it is.
fn direct(c: &Case) -> Result<(), String> {
    let d = c.direct.as_ref().ok_or("no direct result")?;
    let mut db = QuadrantDb::default();
    let got = parse_quadrant(&c.src, &mut db).map(|_| db);
    compare(&c.src, got, d).map_err(|e| format!("direct {e}"))
}

/// A case as the oracle runs it: the parser given the text as it is, and Pele's parseSource
/// for a quadrant chart. It is whether the text was a quadrant chart that parsed.
fn run(c: &Case) -> Result<bool, String> {
    direct(c)?;
    let want = if c.same {
        c.direct.as_ref().ok_or("no direct result")?
    } else {
        &c.result
    };
    let p = match prepare(&c.src, &ParseOptions::default()) {
        Ok(p) => p,
        Err(e) if e.code == ErrorCode::UnsupportedDiagram && want.error.is_none() => {
            return Ok(false);
        }
        Err(e) => {
            return match &want.error {
                Some(w) if w.is(&e) => Ok(false),
                w => Err(format!("error {e:?}\nPele {w:?}")),
            };
        }
    };
    let ty = p.found.diagram.name();
    if c.diagram.as_deref() != Some(ty) {
        return Err(format!("type {ty}, Pele {:?}", c.diagram));
    }
    if p.found.diagram != DiagramType::QuadrantChart {
        return Ok(false);
    }
    let got = p.parse_with(false, |s| parse(s, p.title.as_deref()));
    let parsed = got.is_ok();
    compare(&p.source(false), got, want)?;
    Ok(parsed)
}

#[test]
fn quadrants_match_pele() {
    let f: Fixture = load("quadrant");
    let mut failures = Failures::new();
    let mut parsed = 0;
    for c in &f.cases {
        let got = run(c);
        if got == Ok(true) {
            parsed += 1;
        }
        failures.check(got.is_ok(), || format!("{:?}: {}", c.src, got.unwrap_err()));
    }
    failures.done("quadrant charts", 5000);
    assert!(parsed >= 1500, "only {parsed} quadrant charts parsed");
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

/// A paint as Pele's SVG names it, and an author's color as the oracle writes one.
fn paint_name(p: Option<Paint>) -> String {
    let Some(p) = p else {
        return "?".into();
    };
    let named = [
        (Paint::FG, "var(--_fg)"),
        (Paint::MUTED, "var(--_m)"),
        (Paint::SURFACE, "var(--_s)"),
        (Paint::BORDER, "var(--_b)"),
        (Paint::series(0), "var(--pele-series-1,#4c78a8)"),
        (Paint::NONE, "none"),
    ];
    if let Some((_, name)) = named.iter().find(|&&(q, _)| q == p) {
        return (*name).into();
    }
    match p.source() {
        Source::Own(c) if c.a == 255 && p.factor() == 1.0 && p.base().is_none() => {
            format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
        }
        _ => "?".into(),
    }
}

/// Text as Pele's esc writes it, without the control characters XML does not allow.
fn written(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            !(c < ' ' && !matches!(c, '\t' | '\n' | '\r')
                || matches!(c, '\x7f' | '\u{fffe}' | '\u{ffff}'))
        })
        .collect()
}

/// Emphasis as the oracle writes it.
fn face_name(f: Face) -> String {
    let mut s = String::new();
    if f.contains(Face::BOLD) {
        s.push('b');
    }
    if f.contains(Face::ITALIC) {
        s.push('i');
    }
    if s.is_empty() { "-".into() } else { s }
}

/// The emphasis every span of a label with text has, as Pele's sharedStyle finds it.
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

/// Writes a scene as the oracle reads Pele's SVG of a quadrant chart.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let n = to_string;
    let pt = |x: f64, y: f64| format!("{},{}", n(x), n(y));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    // The lines of a label drawn about cx, cy, as Pele's labelSvg writes them: a label of a
    // single span with no emphasis as one text, and any other as a tspan for each line and
    // each span. A title's weight is set on its text.
    let label = |lines: &mut Vec<String>, it: &Item, cx: f64, cy: f64, title: bool| {
        let Some(p) = it.label.as_ref().filter(|p| !p.label.is_empty()) else {
            return;
        };
        let l = &p.label;
        let mut line_top = cy - l.height / 2.0;
        let mut y = line_top + l.line_height / 2.0 + l.size * 0.35;
        let head = |y: f64, face: Face| {
            format!(
                "t {} {} {} {} ",
                pt(cx, y),
                n(l.size),
                paint_name(it.style.text),
                face_name(face)
            )
        };
        if l.lines.len() == 1
            && l.lines[0].len() == 1
            && l.lines[0][0].icon.is_none()
            && l.lines[0][0].face == Face::REGULAR
            && !title
        {
            lines.push(head(y, Face::REGULAR) + &written(&l.lines[0][0].text));
            return;
        }
        let share = shared_face(l);
        let mut top = share;
        if title {
            lines.push("title".into());
            top = Face::BOLD | (share & Face::ITALIC);
        }
        for line in &l.lines {
            for (k, s) in line.iter().enumerate() {
                let face = top | s.face.without(share);
                if k == 0 {
                    lines.push(head(y, face) + &written(&s.text));
                } else {
                    lines.push(format!("+ {} {}", face_name(face), written(&s.text)));
                }
            }
            if line.is_empty() {
                lines.push(head(y, top) + " ");
            }
            line_top += l.line_height;
            y = line_top + l.line_height / 2.0 + l.size * 0.35;
        }
    };
    for it in &sc.items {
        let at = it.label.as_ref().map(|p| p.at);
        match it.role.as_str() {
            ROLE_TITLE => {
                let at = at.expect("a title's place");
                label(&mut lines, it, at.x, at.y, true);
            }
            ROLE_QUADRANT => {
                let Outline::Box { rect: r, rounded } = it.parts[0].outline else {
                    lines.push("quadrant of a path".into());
                    continue;
                };
                lines.push(format!("quadrant {}", it.id.as_deref().unwrap_or("")));
                lines.push(format!(
                    "rect {},{} {} {} {} {}",
                    pt(r.x, r.y),
                    pt(r.w, r.h),
                    if rounded { "r" } else { "-" },
                    paint_name(it.style.fill),
                    paint_name(it.style.stroke),
                    cap_join(it)
                ));
                if let Some(at) = at {
                    label(&mut lines, it, at.x, at.y, false);
                }
            }
            ROLE_AXIS => {
                let p = it.label.as_ref().expect("an axis label");
                if p.rotate != 0.0 {
                    lines.push(format!("turn {} {}", pt(p.at.x, p.at.y), n(p.rotate)));
                    label(&mut lines, it, 0.0, 0.0, false);
                } else {
                    label(&mut lines, it, p.at.x, p.at.y, false);
                }
            }
            ROLE_POINT => {
                let Outline::Path(path) = &it.parts[0].outline else {
                    lines.push("point of a box".into());
                    continue;
                };
                // A circle, as Path::ellipse draws it: from its right end, four quarters about
                // its middle. Its first quarter ends below the middle.
                let (cx, cy, r) = (path.args[6], path.args[1], it.bounds.w / 2.0);
                let mut s = format!(
                    "point {} [{}] {} {}",
                    id(it.id.as_deref().unwrap_or("")),
                    it.class.join(" "),
                    pt(cx, cy),
                    n(r)
                );
                if path.args[0] != cx + r || it.bounds.x != cx - r {
                    s.push_str(" not a circle about its box");
                }
                let pen = it.author.over(&it.style);
                let width = it.author.stroke_width.map_or("-".into(), n);
                lines.push(format!(
                    "{s} {} {} {width} {}",
                    paint_name(pen.fill),
                    paint_name(pen.stroke),
                    cap_join(it)
                ));
            }
            ROLE_POINT_LABEL => {
                let at = at.expect("a point label's place");
                label(&mut lines, it, at.x, at.y, false);
            }
            role => lines.push(format!("unknown role {role}")),
        }
    }
    lines
}

/// What Pele's render draws of quadrant charts, read back out of its SVG as a line for each
/// thing drawn, in the order drawn, against the same lines written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("quadrant-render");
    let mut failures = Failures::new();
    let mut points = 0;
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a quadrant chart");
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("a quadrant chart that parses");
        points += m.points.len();
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            font_size: c.font_size.unwrap_or(16.0),
            ..Options::default()
        };
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
    failures.done("renders", 4000);
    assert!(points >= 10000, "only {points} points drawn");
}
