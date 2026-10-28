//! Cynefin diagrams read and laid out as Pele reads and draws them, checked against the
//! oracle's cynefin and cynefin-render fixtures.

use limu_fixtures::load;
use serde::Deserialize;

use super::boundaries::{hash_string, seeded_random};
use super::model::CynefinModel;
use super::parse;
use super::parser::{
    CYNEFIN_TOKENS, CynefinAst, T_ACC_DESCR, T_ACC_TITLE, T_ARROW, T_COLON, T_DOMAIN, T_KEYWORD,
    T_KEYWORD_COLON, T_NEWLINE, T_STRING, T_TITLE, parse_cynefin, table,
};
use super::render::{
    ROLE_BACKDROP, ROLE_BOUNDARY, ROLE_CLIFF, ROLE_DESCRIPTION, ROLE_DOMAIN, ROLE_ITEM,
    ROLE_OVERFLOW, ROLE_REGION, ROLE_TITLE, ROLE_TRANSITION, ROLE_TRANSITION_LABEL, layout,
};
use crate::common::tokens::tokenize;
use crate::detect::DiagramType;
use crate::errors::ErrorCode;
use crate::js::number::to_string as n;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::Paint;
use crate::scene::{Item, Op, Outline, Painting, Path, Scene};
use crate::testing::pelesvg::{cap_join, id, own_fill};
use crate::testing::{Enc, Failures, PeleError, same, sorted};
use crate::value::Value;

#[derive(Deserialize)]
struct Cases {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    src: String,
    direct: Direct,
    #[serde(rename = "type")]
    diagram: Option<String>,
    model: Option<Enc>,
    error: Option<PeleError>,
}

#[derive(Deserialize)]
struct Direct {
    tokens: Option<Vec<(String, String)>>,
    ast: Option<Enc>,
    error: Option<PeleError>,
}

/// The kinds of token, numbered by hand, are the indexes of their types in CYNEFIN_TOKENS.
#[test]
fn token_kinds() {
    assert_eq!(CYNEFIN_TOKENS.len(), 14);
    for (kind, name) in [
        (T_KEYWORD_COLON, "cynefin-beta:"),
        (T_KEYWORD, "cynefin-beta"),
        (T_ARROW, "-->"),
        (T_COLON, ":"),
        (T_DOMAIN, "DOMAIN_NAME"),
        (T_ACC_DESCR, "ACC_DESCR"),
        (T_ACC_TITLE, "ACC_TITLE"),
        (T_TITLE, "TITLE"),
        (T_STRING, "STRING"),
        (T_NEWLINE, "NEWLINE"),
    ] {
        assert_eq!(CYNEFIN_TOKENS[kind].name, name, "kind {kind}");
    }
}

/// A string as the oracle writes it, or undefined.
fn enc_str(s: &Option<String>) -> Value {
    match s {
        Some(s) => Value::String(s.clone()),
        None => Value::Object(
            [("$".to_string(), Value::String("undefined".into()))]
                .into_iter()
                .collect(),
        ),
    }
}

fn object(entries: Vec<(&str, Value)>) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn strings(items: &[String]) -> Value {
    Value::Array(items.iter().cloned().map(Value::String).collect())
}

fn dump_ast(t: &CynefinAst) -> Value {
    let domains = t
        .domains
        .iter()
        .map(|d| Value::Array(vec![Value::String(d.domain.clone()), strings(&d.items)]))
        .collect();
    let transitions = t
        .transitions
        .iter()
        .map(|tr| {
            Value::Array(vec![
                Value::String(tr.from.clone()),
                Value::String(tr.to.clone()),
                enc_str(&tr.label),
            ])
        })
        .collect();
    object(vec![
        ("title", enc_str(&t.title)),
        ("accTitle", enc_str(&t.acc_title)),
        ("accDescr", enc_str(&t.acc_descr)),
        ("domains", Value::Array(domains)),
        ("transitions", Value::Array(transitions)),
    ])
}

/// The model as the oracle writes it: its domains a Map's entries, each its key and its value.
fn dump_model(m: &CynefinModel) -> Value {
    let domains = m
        .domains
        .values()
        .map(|d| {
            Value::Array(vec![
                Value::String(d.name.clone()),
                Value::String(d.name.clone()),
                strings(&d.items),
            ])
        })
        .collect();
    let transitions = m
        .transitions
        .iter()
        .map(|t| {
            Value::Array(vec![
                Value::String(t.from.clone()),
                Value::String(t.to.clone()),
                enc_str(&t.label),
            ])
        })
        .collect();
    object(vec![
        ("title", enc_str(&m.title)),
        ("accTitle", enc_str(&m.acc_title)),
        ("accDescr", enc_str(&m.acc_descr)),
        ("domains", Value::Array(domains)),
        ("transitions", Value::Array(transitions)),
    ])
}

fn dump_tokens(src: &str) -> Option<Vec<(String, String)>> {
    let t = tokenize(src, table(), DiagramType::Cynefin).ok()?;
    let n = t.kinds.len() - 1;
    Some(
        (0..n)
            .map(|i| {
                let name = CYNEFIN_TOKENS[t.kinds[i]].name.to_string();
                (name, src[t.starts[i]..t.ends[i]].to_string())
            })
            .collect(),
    )
}

#[test]
fn parse_matches_pele() {
    let f: Cases = load("cynefin");
    assert!(f.cases.len() >= 5000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    let (mut models, mut errors) = (0, 0);
    for c in &f.cases {
        let tokens = dump_tokens(&c.src);
        failures.check(tokens == c.direct.tokens, || {
            format!(
                "tokens of {:?}: {tokens:?}, Pele {:?}",
                c.src, c.direct.tokens
            )
        });
        let tree = parse_cynefin(&c.src);
        let ok = match (&tree, &c.direct.error, &c.direct.ast) {
            (Err(e), Some(want), _) => want.is(e),
            (Ok(t), None, Some(want)) => same(&sorted(&dump_ast(t)), &sorted(&want.0)),
            _ => false,
        };
        failures.check(ok, || {
            format!(
                "parse_cynefin({:?}): {tree:?}, Pele {:?} {:?}",
                c.src, c.direct.ast, c.direct.error
            )
        });

        let p = prepare(&c.src, &ParseOptions::default());
        let ok = match (&p, &c.error) {
            (Err(e), Some(want)) => want.is(e),
            (Err(e), None) => e.code == ErrorCode::UnsupportedDiagram && c.diagram.is_none(),
            (Ok(p), _) if c.diagram.as_deref() != Some(p.found.diagram.name()) => false,
            (Ok(p), want) if p.found.diagram == DiagramType::Cynefin => {
                let got = p.parse_with(false, |s| parse(s, p.title.as_deref()));
                match (&got, want, &c.model) {
                    (Err(e), Some(want), _) => {
                        errors += 1;
                        want.is(e)
                    }
                    (Ok(m), None, Some(model)) => {
                        models += 1;
                        same(&sorted(&dump_model(m)), &sorted(&model.0))
                    }
                    _ => false,
                }
            }
            (Ok(_), _) => true,
        };
        failures.check(ok, || {
            format!("{:?}: model {:?}, error {:?}", c.src, c.model, c.error)
        });
    }
    failures.done("cynefin", 3 * 5000);
    assert!(
        models >= 2000 && errors >= 2000,
        "only {models} models and {errors} errors"
    );
}

#[derive(Deserialize)]
struct Renders {
    cases: Vec<Render>,
    randoms: Vec<(f64, f64)>,
    hashes: Vec<(String, f64)>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Render {
    src: String,
    font_size: Option<f64>,
    #[serde(default)]
    icons: bool,
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

/// The icons the oracle's host has, in the cases drawn with icons.
fn host_icons(name: &str) -> bool {
    matches!(name, "star" | "fa:fa-car" | "fa:fa-coffee")
}

/// A path as the test compares paths: each step's letter and its numbers in full.
fn path_data(p: &Path) -> String {
    let mut out = String::new();
    let mut at = 0;
    for &op in &p.ops {
        out.push(match op {
            Op::MoveTo => 'M',
            Op::LineTo => 'L',
            Op::QuadTo => 'Q',
            Op::CubeTo => 'C',
            Op::Close => 'Z',
        });
        let a = &p.args[at..at + op.args()];
        at += op.args();
        for (k, xy) in a.chunks(2).enumerate() {
            if k > 0 {
                out.push(' ');
            }
            out.push_str(&format!("{},{}", n(xy[0]), n(xy[1])));
        }
    }
    out
}

/// A line of the fixture with the data of a path read as the scene would draw it. The oracle
/// keeps in full the lines of paths it reads otherwise than as they are written, those with
/// arcs among them.
fn pele_line(line: &str) -> String {
    if !line.starts_with("path ") {
        return line.to_string();
    }
    // path, class, id, the data, and the six fields of the pen.
    let f: Vec<&str> = line.split(' ').collect();
    let d = f[3..f.len() - 6].join(" ");
    let p = Path::parse(&d).unwrap_or_else(|e| panic!("path {d:?}: {e}"));
    let mut out = f[..3].to_vec();
    let data = path_data(&p);
    out.push(&data);
    out.extend(&f[f.len() - 6..]);
    out.join(" ")
}

/// A paint as the oracle writes it: the theme's by Pele's names for them, and a series by its
/// number, with its opacity after an @.
fn paint_name(p: Option<Paint>) -> String {
    let Some(p) = p else {
        return "-".into();
    };
    let named = [
        (Paint::BG, "var(--_bg)"),
        (Paint::FG, "var(--_fg)"),
        (Paint::MUTED, "var(--_m)"),
        (Paint::LINE, "var(--_l)"),
        (Paint::BORDER, "var(--_b)"),
        (Paint::NONE, "none"),
    ];
    if let Some((_, name)) = named.iter().find(|(q, _)| *q == p) {
        return name.to_string();
    }
    for k in 0..8 {
        for a in [1.0, 0.14, 0.18] {
            if p == Paint::series(k).alpha(a) {
                let mut s = format!("series-{}", k + 1);
                if a != 1.0 {
                    s.push_str(&format!("@{}", n(a)));
                }
                return s;
            }
        }
    }
    "?".into()
}

const FACES: [&str; 4] = ["-", "b", "i", "bi"];

/// The lines of a scene as the oracle reads Pele's SVG.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let pt = |x: f64, y: f64| format!("{},{}", n(x), n(y));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    if let Some(t) = &sc.title {
        lines.push(format!("acc-title {}", id(t)));
    }
    if let Some(d) = &sc.desc {
        lines.push(format!("acc-desc {}", id(d)));
    }
    // What a part is filled and stroked with, the stroke's width, its dash, cap and join.
    let pen = |it: &Item, paint: Painting| {
        let mut fill = paint_name(own_fill(it));
        let mut stroke = paint_name(it.style.stroke);
        match paint {
            Painting::StrokeOnly => fill = "none".into(),
            Painting::FillOnly => stroke = "-".into(),
            _ => {}
        }
        let width = it.style.stroke_width.map_or("-".to_string(), n);
        let dash = match &it.style.dash {
            None => "-".to_string(),
            Some(d) if d.is_empty() => "none".to_string(),
            Some(d) => d.iter().map(|&v| n(v)).collect::<Vec<_>>().join(","),
        };
        format!("{fill} {stroke} {width} {dash} {}", cap_join(it))
    };
    let part = |it: &Item, class: &str, name: &str| {
        let p = &it.parts[0];
        match &p.outline {
            Outline::Box { rect: r, rounded } => format!(
                "rect {},{} {} {}",
                pt(r.x, r.y),
                pt(r.w, r.h),
                if *rounded { "r" } else { "-" },
                pen(it, p.paint)
            ),
            Outline::Path(path) => {
                format!(
                    "path {class} {name} {} {}",
                    path_data(path),
                    pen(it, p.paint)
                )
            }
        }
    };
    // A label as the oracle writes one: its class, size, fill, weight and style as Pele's
    // attributes give them, where it is centered, its measures, and its lines and spans. The
    // size is Pele's font-size, which it sets on all text but titles.
    let label = |it: &Item, class: &str, sized: bool, weight: &str, face: &str| {
        let p = it.label.as_ref()?;
        let l = &p.label;
        if l.is_empty() {
            return None;
        }
        let size = if sized { n(l.size) } else { "-".into() };
        let fill = it
            .style
            .text
            .map_or("-".to_string(), |t| paint_name(Some(t)));
        let spans: Vec<String> = l
            .lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let s: Vec<String> = line
                    .iter()
                    .map(|span| match &span.icon {
                        Some(icon) => format!("i:{icon} {}", n(span.width)),
                        None => format!(
                            "t:{}:{} {}",
                            FACES[usize::from(span.face.bits() & 3)],
                            span.text,
                            n(span.width)
                        ),
                    })
                    .collect();
                format!("{}: {}", n(l.widths[i]), s.join(" + "))
            })
            .collect();
        Some(format!(
            "label {class} {size} {fill} {weight} {face} {} {} {} {} | {}",
            pt(p.at.x, p.at.y),
            pt(l.width, l.height),
            n(l.size),
            n(l.line_height),
            spans.join(" | ")
        ))
    };
    for it in &sc.items {
        let name = id(it.id.as_deref().unwrap_or(""));
        match it.role.as_str() {
            ROLE_TITLE => lines.extend(label(it, "pele-title", false, "var(--_tw)", "-")),
            ROLE_REGION => lines.push(part(it, "pele-cynefin-region", &name)),
            ROLE_BOUNDARY | ROLE_BACKDROP => lines.push(part(it, "-", "-")),
            ROLE_CLIFF => lines.push(part(it, "pele-cynefin-cliff", "-")),
            ROLE_DOMAIN => {
                lines.push(format!("group pele-cynefin-domain {name}"));
                lines.extend(label(it, "pele-label", false, "var(--_hw)", "-"));
            }
            ROLE_DESCRIPTION => {
                lines.extend(label(it, "pele-cynefin-subtitle", true, "-", "italic"));
            }
            ROLE_ITEM => {
                lines.push(format!("group pele-node {name}"));
                lines.push(part(it, "", ""));
                lines.extend(label(it, "pele-label", true, "-", "-"));
            }
            ROLE_OVERFLOW => {
                lines.push("group pele-node pele-cynefin-overflow -".into());
                lines.push(part(it, "", ""));
                lines.extend(label(it, "pele-label", true, "-", "-"));
            }
            ROLE_TRANSITION => {
                lines.push(format!("group pele-edge {name}"));
                lines.push(part(it, "-", "-"));
                let m = it.kind.edge().and_then(|e| e.ends[1]).expect("an arrow");
                lines.push(format!(
                    "marker {} {} {} {}",
                    m.kind.name(),
                    pt(m.at.x, m.at.y),
                    pt(m.dx, m.dy),
                    paint_name(it.style.stroke)
                ));
            }
            ROLE_TRANSITION_LABEL => {
                lines.extend(label(it, "pele-edge-label", true, "-", "-"));
            }
            role => lines.push(format!("unknown role {role}")),
        }
    }
    lines
}

/// What Pele's render draws of Cynefin diagrams, read back out of its SVG as a line for each
/// thing drawn, in the order drawn, with every number in full, against the same lines
/// written of the scene. Pele's path data, arcs and all, is read with Path::parse and written
/// out again as the scene's paths are.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("cynefin-render");
    assert!(f.cases.len() >= 1800, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    let (mut items, mut transitions) = (0, 0);
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a Cynefin diagram");
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("a Cynefin diagram that parses");
        let o = Options {
            font_size: c.font_size.unwrap_or(16.0),
            icons: if c.icons { Some(&host_icons) } else { None },
            ..Options::default()
        };
        let sc = layout(&m, &p.config, &o);
        for it in &sc.items {
            match it.role.as_str() {
                ROLE_ITEM | ROLE_OVERFLOW => items += 1,
                ROLE_TRANSITION => transitions += 1,
                _ => {}
            }
        }
        let got = scene_lines(&sc);
        let diff = drawings.diff(k, &c.drawing, &got, Some(&pele_line));
        failures.check(diff.is_none(), || {
            format!(
                "{:?}, size {:?}, icons {}: {}",
                c.src,
                c.font_size,
                c.icons,
                diff.unwrap()
            )
        });
    }
    failures.done("renders", 1800);
    assert!(
        items >= 8000 && transitions >= 1800,
        "only {items} items and {transitions} transitions drawn"
    );
}

/// The seeded numbers and the hashes of strings are Pele's.
#[test]
fn seeds_match_pele() {
    let (f, _): (Renders, _) = limu_fixtures::load_drawings("cynefin-render");
    assert!(
        f.randoms.len() >= 2000 && f.hashes.len() >= 10,
        "only {} seeds and {} strings",
        f.randoms.len(),
        f.hashes.len()
    );
    let mut failures = Failures::new();
    for &(seed, want) in &f.randoms {
        let got = seeded_random(seed);
        failures.check(got.to_bits() == want.to_bits(), || {
            format!("seeded_random({seed}) = {got}, Pele {want}")
        });
    }
    for (s, want) in &f.hashes {
        let got = hash_string(s);
        failures.check(f64::from(got) == *want, || {
            format!("hash_string({s:?}) = {got}, Pele {want}")
        });
    }
    failures.done("seeds", 2010);
}
