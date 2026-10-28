//! Venn diagrams read and laid out as Pele reads and draws them, checked against the
//! oracle's venn and venn-render fixtures.

use limu_fixtures::load;
use serde::Deserialize;

use super::db::IndentHost;
use super::lexer::{END, ERROR, TOKEN_NAMES, VennLexer};
use super::render::{ROLE_LABEL, ROLE_OVERLAP, ROLE_SET, ROLE_TEXT, ROLE_TITLE, layout};
use super::{VennDb, parse};
use crate::css::declarations as read_declarations;
use crate::detect::DiagramType;
use crate::errors::ErrorCode;
use crate::js::chars::len16;
use crate::js::number::to_string as n;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::{Look, Paint, Style, Theme};
use crate::scene::text::Face;
use crate::scene::{Item, Op, Outline, Path, Scene};
use crate::testing::pelesvg::{cap_join, id};
use crate::testing::{Enc, Failures, PeleError, same, sorted};
use crate::value::{Map, Value};

#[derive(Deserialize)]
struct Cases {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    src: String,
    tokens: Vec<(String, String, usize)>,
    indented: Vec<(String, String, usize)>,
    direct: Outcome,
    #[serde(rename = "type")]
    diagram: Option<String>,
    #[serde(flatten)]
    outcome: Outcome,
    /// Whether the pipeline's outcome is the direct one.
    #[serde(default)]
    same: bool,
}

#[derive(Deserialize)]
struct Outcome {
    model: Option<Enc>,
    error: Option<PeleError>,
}

/// Answers the lexer's question about indentation always the same, as the oracle's lexer
/// suites do.
struct Fixed(bool);

impl IndentHost for Fixed {
    fn indent_mode(&self) -> bool {
        self.0
    }

    fn set_indent_mode(&mut self, _: bool) {}
}

fn dump_tokens(src: &str, indent: bool) -> Vec<(String, String, usize)> {
    let mut lexer = VennLexer::new(src);
    let mut host = Fixed(indent);
    let mut out = Vec::new();
    loop {
        let kind = lexer.next(&mut host);
        let at = len16(&src[..lexer.start]);
        out.push((TOKEN_NAMES[kind].to_string(), lexer.text.clone(), at));
        if kind == END || kind == ERROR {
            return out;
        }
    }
}

/// The db as the oracle writes Pele's.
fn dump_model(m: &VennDb) -> Value {
    let string = |s: &str| Value::String(s.to_string());
    let strings = |v: &[String]| Value::Array(v.iter().map(|s| string(s)).collect());
    let optional = |s: &Option<String>| s.as_deref().map_or(Value::Null, string);
    let mut out = Map::new();
    out.insert("title", optional(&m.title));
    let subsets = m.subsets.iter().map(|s| {
        let mut o = Map::new();
        o.insert("sets", strings(&s.sets));
        o.insert("size", Value::Number(s.size));
        o.insert("label", optional(&s.label));
        Value::Object(o)
    });
    out.insert("subsets", Value::Array(subsets.collect()));
    let texts = m.text_nodes.iter().map(|t| {
        let mut o = Map::new();
        o.insert("sets", strings(&t.sets));
        o.insert("id", string(&t.id));
        o.insert("label", optional(&t.label));
        Value::Object(o)
    });
    out.insert("textNodes", Value::Array(texts.collect()));
    let styles = m.styles.iter().map(|s| {
        let mut o = Map::new();
        o.insert("targets", strings(&s.targets));
        let decls = s
            .styles
            .iter()
            .map(|(k, v)| Value::Array(vec![string(k), string(v)]));
        o.insert("styles", Value::Array(decls.collect()));
        Value::Object(o)
    });
    out.insert("styles", Value::Array(styles.collect()));
    Value::Object(out)
}

/// Whether a parse's outcome is the one Pele had.
fn matches(got: &Result<VennDb, crate::Error>, want: &Outcome) -> bool {
    match (got, &want.error, &want.model) {
        (Err(e), Some(want), _) => want.is(e),
        (Ok(m), None, Some(model)) => same(&sorted(&dump_model(m)), &sorted(&model.0)),
        _ => false,
    }
}

#[test]
fn parse_matches_pele() {
    let f: Cases = load("venn");
    let mut failures = Failures::new();
    let mut parsed = 0;
    for c in &f.cases {
        let tokens = dump_tokens(&c.src, false);
        failures.check(tokens == c.tokens, || {
            format!("tokens of {:?}: {tokens:?}, Pele {:?}", c.src, c.tokens)
        });
        let indented = dump_tokens(&c.src, true);
        failures.check(indented == c.indented, || {
            format!(
                "indented tokens of {:?}: {indented:?}, Pele {:?}",
                c.src, c.indented
            )
        });
        let got = parse(&c.src, None);
        failures.check(matches(&got, &c.direct), || {
            format!(
                "parse({:?}): {got:?}, Pele {:?} {:?}",
                c.src, c.direct.model, c.direct.error
            )
        });

        let want = if c.same { &c.direct } else { &c.outcome };
        let p = prepare(&c.src, &ParseOptions::default());
        let ok = match (&p, &want.error) {
            (Err(e), Some(want)) => want.is(e),
            (Err(e), None) => e.code == ErrorCode::UnsupportedDiagram && c.diagram.is_none(),
            (Ok(p), _) if c.diagram.as_deref() != Some(p.found.diagram.name()) => false,
            (Ok(p), _) if p.found.diagram == DiagramType::Venn => {
                let got = p.parse_with(false, |s| parse(s, p.title.as_deref()));
                parsed += usize::from(got.is_ok());
                matches(&got, want)
            }
            (Ok(_), _) => true,
        };
        failures.check(ok, || {
            format!(
                "{:?}: model {:?}, error {:?}",
                c.src, want.model, want.error
            )
        });
    }
    failures.done("venn", 4 * 9000);
    assert!(parsed >= 1800, "only {parsed} venn diagrams parsed");
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

/// The series a color Pele writes is.
fn series_of(color: &str) -> Option<Paint> {
    (0..8).find_map(|k| (color == series_name(k)).then(|| Paint::series(k)))
}

/// The color of series k as Pele writes it.
fn series_name(k: usize) -> String {
    format!("var(--pele-series-{},{})", k + 1, Theme::PELE.series[k])
}

/// The declarations of the style Pele writes on an element, as the oracle writes it,
/// encoded in a URL's way.
fn declarations(field: &str) -> Vec<String> {
    let s = field.strip_prefix("css=").unwrap_or(field).as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'%' && i + 2 < s.len() {
            let hex = std::str::from_utf8(&s[i + 1..i + 3]).expect("hex");
            out.push(u8::from_str_radix(hex, 16).expect("hex"));
            i += 3;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    let css = String::from_utf8(out).expect("UTF-8");
    css.split(';').map(str::to_string).collect()
}

/// The value of the last declaration of an opacity among decls that a browser takes, a
/// number or a percentage, kept from 0 to 1, if there is one.
fn last_opacity(decls: &[String], prop: &str) -> Option<f64> {
    let value = decls
        .iter()
        .filter_map(|d| d.split_once(':'))
        .filter(|(p, _)| p.trim() == prop)
        .map(|(_, v)| v.trim())
        .next_back()?;
    let (value, scale) = match value.strip_suffix('%') {
        Some(v) => (v, 100.0),
        None => (value, 1.0),
    };
    let f: f64 = value.parse().ok()?;
    Some((f / scale).clamp(0.0, 1.0))
}

/// decls without the declarations of prop.
fn without(decls: &[String], prop: &str) -> Vec<String> {
    decls
        .iter()
        .filter(|d| d.split_once(':').map_or(d.as_str(), |(p, _)| p).trim() != prop)
        .cloned()
        .collect()
}

/// A style as the test compares it.
fn view(s: &Style) -> String {
    format!("{s:?}")
}

/// The style an author gave a text: as "author=" and its view, or as "css=", as Pele writes
/// no style, where the author gave none. A text of Pele's with no style is so read as it is,
/// and the oracle keeps only the texts with one in full.
fn author(s: &Style) -> String {
    if *s != Style::default() {
        format!("author={}", view(s))
    } else {
        "css=".into()
    }
}

/// Where the part of a line after its style begins, as an id before it may hold anything.
fn after_css(line: &str) -> usize {
    let css = line.find(" css=").expect("a style") + 1;
    let at = line[..css].rfind(' ').map_or(0, |i| i + 1);
    at + line[at..].find(" | ").expect("a bar")
}

/// A path Pele writes, M, L, A and Z with every number in full, drawn as the scene draws it.
fn pele_outline(d: &str) -> Path {
    let f: Vec<&str> = d.split_ascii_whitespace().collect();
    let num = |i: usize| -> f64 { f[i].parse().expect("a number") };
    let mut p = Path::new();
    let mut i = 0;
    while i < f.len() {
        match f[i] {
            "M" => {
                p.move_to(num(i + 1), num(i + 2));
                i += 3;
            }
            "L" => {
                p.line_to(num(i + 1), num(i + 2));
                i += 3;
            }
            "A" => {
                let (large, sweep) = (f[i + 4] == "1", f[i + 5] == "1");
                p.arc_to(
                    num(i + 1),
                    num(i + 2),
                    num(i + 3),
                    large,
                    sweep,
                    num(i + 6),
                    num(i + 7),
                );
                i += 8;
            }
            "Z" => {
                p.close();
                i += 1;
            }
            c => panic!("unknown command {c}"),
        }
    }
    p
}

/// A path's commands, every number in full.
fn path_line(p: &Path) -> String {
    let mut out = Vec::new();
    let mut at = 0;
    for &op in &p.ops {
        let mut s = match op {
            Op::MoveTo => "M",
            Op::LineTo => "L",
            Op::QuadTo => "Q",
            Op::CubeTo => "C",
            Op::Close => "Z",
        }
        .to_string();
        for &v in &p.args[at..at + op.args()] {
            s.push_str(&format!(" {}", n(v)));
        }
        at += op.args();
        out.push(s);
    }
    out.join(" ")
}

/// A line of the fixture with the style Pele writes on an element read as what a browser
/// draws: of a circle, its fill and stroke from the author's style or Pele's attributes,
/// each at the opacity the author's style or Pele's attributes give, under the rest of the
/// author's style, of an overlap, the author's style over no fill or stroke, and of a text,
/// the color and the opacity the author gave. An overlap's outline is drawn again as the
/// scene draws arcs.
fn pele_line(line: &str) -> String {
    let fields: Vec<&str> = line.split(' ').collect();
    match fields[0] {
        "set" => {
            // set id cx,cy r fill fill-opacity stroke cap join css=
            let k = fields.len();
            let decls = declarations(fields[k - 1]);
            let (fill, stroke) = (series_of(fields[k - 6]), series_of(fields[k - 4]));
            let mut opacity: f64 = fields[k - 5].parse().expect("an opacity");
            let fill_opacity = last_opacity(&decls, "fill-opacity");
            if let Some(f) = fill_opacity {
                opacity = f;
            }
            let stroke_opacity = last_opacity(&decls, "stroke-opacity");
            let rest = without(&without(&decls, "fill-opacity"), "stroke-opacity");
            let mut s = read_declarations(&rest).style;
            s.text = None;
            if fill_opacity.is_none() && stroke_opacity.is_none() && s == Style::default() {
                // A style that sets nothing a browser draws is read as no style, and the line
                // as Pele writes a set the author gave none, as scene_lines writes it too.
                return format!("{} css=", fields[..k - 1].join(" "));
            }
            s.fill = s.fill.or(fill);
            s.fill_opacity = Some(opacity);
            s.stroke = s.stroke.or(stroke);
            if stroke_opacity.is_some() {
                s.stroke_opacity = stroke_opacity;
            }
            format!(
                "{} {} style={}",
                fields[..k - 6].join(" "),
                fields[k - 3..k - 1].join(" "),
                view(&s)
            )
        }
        "overlap" => {
            // overlap id fill cap join css= | d
            let bar = after_css(line);
            let head: Vec<&str> = line[..bar].split(' ').collect();
            let k = head.len();
            let mut s = read_declarations(&declarations(head[k - 1])).style;
            s.text = None;
            let s = s.over(&Style {
                fill: Some(Paint::NONE),
                stroke: Some(Paint::NONE),
                ..Style::default()
            });
            format!(
                "{} {} style={} | {}",
                head[..k - 4].join(" "),
                head[k - 3..k - 1].join(" "),
                view(&s),
                path_line(&pele_outline(&line[bar + 3..]))
            )
        }
        "title" | "label" | "text" => {
            // kind id t x,y fill size weight style css= | lines
            let bar = after_css(line);
            let head: Vec<&str> = line[..bar].split(' ').collect();
            let k = head.len();
            let decls: Vec<String> = declarations(head[k - 1])
                .into_iter()
                .map(|d| match d.strip_prefix("fill:") {
                    Some(v) => format!("color:{v}"),
                    None => d,
                })
                .collect();
            let s = read_declarations(&decls).style;
            let given = Style {
                text: s.text,
                opacity: s.opacity,
                ..Style::default()
            };
            format!(
                "{} {}{}",
                head[..k - 1].join(" "),
                author(&given),
                &line[bar..]
            )
        }
        _ => line.to_string(),
    }
}

/// A set's fill, fill opacity and stroke as Pele writes them on its circle, where the author
/// gave the set no style and its own style is no more than those. A set of Pele's with no
/// style is so read as it is, and the oracle keeps only the sets with one in full.
fn set_attributes(it: &Item) -> Option<String> {
    let s = &it.style;
    let opacity = s.fill_opacity?;
    let plain = Style {
        fill: s.fill,
        fill_opacity: s.fill_opacity,
        stroke: s.stroke,
        ..Style::default()
    };
    if it.author != Style::default() || *s != plain {
        return None;
    }
    let name = |p: Option<Paint>| {
        (0..8)
            .find(|&k| p == Some(Paint::series(k)))
            .map(series_name)
    };
    Some(format!(
        "{} {} {}",
        name(s.fill)?,
        n(opacity),
        name(s.stroke)?
    ))
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

/// The fill Pele writes on a text: the color ink picks on the ground under it, unless its
/// author sets one, and otherwise its own.
fn text_paint(it: &Item) -> &'static str {
    if it.style.text_ground.is_some() && it.author.text.is_none() {
        let s = Look::default().cascade(it.role.as_str(), &it.style, &it.author);
        return match s.text {
            Some(p) if p == Paint::FG => "var(--pele-fg-on-light,#1f1f1f)",
            Some(p) if p == Paint::BG => "var(--pele-fg-on-dark,#fff)",
            _ => "?",
        };
    }
    match it.style.text {
        Some(p) if p == Paint::FG => "var(--_fg)",
        Some(p) if p == Paint::MUTED => "var(--_m)",
        _ => "?",
    }
}

/// The lines of a scene as the oracle reads Pele's SVG.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let pt = |x: f64, y: f64| format!("{},{}", n(x), n(y));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    // A label as Pele's labelSvg writes it about its point: one text with no spans when it is
    // one line of one span of plain text, and otherwise spans, with the emphasis every span
    // has written on the text, unless the attributes give a weight of their own, and the rest
    // on each span.
    let label = |lines: &mut Vec<String>, kind: &str, id: &str, it: &Item| {
        let p = it.label.as_ref().expect("a label");
        let l = &p.label;
        let y = p.at.y - l.height / 2.0 + l.line_height / 2.0 + l.size * 0.35;
        let plain = l.lines.len() == 1
            && l.lines[0].len() == 1
            && l.lines[0][0].icon.is_none()
            && l.lines[0][0].face == Face::REGULAR;
        let both = Face::BOLD.bits() | Face::ITALIC.bits();
        let mut shared = both;
        let mut any = false;
        for span in l.lines.iter().flatten() {
            if span.icon.is_some() || span.text.is_empty() {
                continue;
            }
            shared &= span.face.bits();
            any = true;
        }
        if !any || plain {
            shared = 0;
        }
        let weight = if it.role == ROLE_TITLE {
            "var(--_tw)"
        } else if shared & Face::BOLD.bits() != 0 {
            "var(--_w)"
        } else {
            "-"
        };
        let italic = if shared & Face::ITALIC.bits() != 0 {
            "italic"
        } else {
            "-"
        };
        let content: Vec<String> = l
            .lines
            .iter()
            .map(|line| {
                if line.is_empty() {
                    return " ".to_string();
                }
                let mut s = String::new();
                for span in line {
                    let own = span.face.bits() & !shared;
                    let mut marks = String::new();
                    if !plain {
                        if own & Face::BOLD.bits() != 0 {
                            marks.push('b');
                        }
                        if own & Face::ITALIC.bits() != 0 {
                            marks.push('i');
                        }
                    }
                    if !marks.is_empty() {
                        s.push_str(&format!("[{marks}]"));
                    }
                    s.push_str(&written(&span.text));
                }
                s
            })
            .collect();
        lines.push(format!(
            "{kind} {id} t {} {} {} {weight} {italic} {} | {}",
            pt(p.at.x, y),
            text_paint(it),
            n(l.size),
            author(&it.author),
            content.join(" | ")
        ));
    };
    for it in &sc.items {
        let name = id(it.id.as_deref().unwrap_or(""));
        let path = match it.parts.first().map(|p| &p.outline) {
            Some(Outline::Path(p)) => Some(p),
            _ => None,
        };
        match it.role.as_str() {
            ROLE_TITLE => label(&mut lines, "title", "-", it),
            ROLE_SET => {
                // The circle, as Path::ellipse draws it: from its right end, four quarters,
                // its radius half its box's width.
                let p = path.expect("a circle");
                let (cx, cy, r) = (p.args[6], p.args[1], it.bounds.w / 2.0);
                let mut circle = Path::new();
                circle.ellipse(cx, cy, r, r);
                if *p != circle {
                    lines.push("set drawn other than as its circle".into());
                }
                match set_attributes(it) {
                    Some(attrs) => lines.push(format!(
                        "set {name} {} {} {attrs} {} css=",
                        pt(cx, cy),
                        n(r),
                        cap_join(it)
                    )),
                    None => lines.push(format!(
                        "set {name} {} {} {} style={}",
                        pt(cx, cy),
                        n(r),
                        cap_join(it),
                        view(&it.author.over(&it.style))
                    )),
                }
            }
            ROLE_OVERLAP => lines.push(format!(
                "overlap {name} {} style={} | {}",
                cap_join(it),
                view(&it.author.over(&it.style)),
                path_line(path.expect("an outline"))
            )),
            ROLE_LABEL => label(&mut lines, "label", &name, it),
            ROLE_TEXT => label(&mut lines, "text", &name, it),
            role => lines.push(format!("unknown role {role}")),
        }
    }
    lines
}

/// What Pele's render draws of venn diagrams, read back out of its SVG as one line per thing
/// drawn, in the order drawn, with every number in full, against the same lines written of
/// the scene. Where Pele writes the style an author gives a circle, an overlap or a text,
/// the test works out what a browser draws from it and from the attributes Pele writes
/// beside it, reading the declarations as limu-rs reads them, and compares that with the
/// item's style under its author's.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("venn-render");
    assert!(f.cases.len() >= 6000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    let mut overlaps = 0;
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a venn diagram");
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("a venn diagram that parses");
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            font_size: c.font_size.unwrap_or(16.0),
            ..Options::default()
        };
        let got = scene_lines(&layout(&m, &p.config, &o));
        overlaps += got.iter().filter(|l| l.starts_with("overlap ")).count();
        let diff = drawings.diff(k, &c.drawing, &got, Some(&pele_line));
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
    failures.done("renders", 6000);
    assert!(overlaps >= 500, "only {overlaps} overlaps drawn");
}
