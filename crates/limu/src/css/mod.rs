//! What a diagram's author sets in its text with classDef and style lines, in the
//! declarations of CSS they are written in, as Pele's resolveStyle in src/svg/theme.ts keeps
//! them, and whether Pele's ink in src/svg/ink.ts picks a color for text on the fill they give.
//!
//! Pele writes what it keeps into the SVG for the browser to read, and tests what it wrote.
//! Here [`resolve_style`] writes the same styles, which the tests of ink.ts read as Pele's do,
//! and [`declarations`] reads them as a browser would into a [`Style`], with the weight, slant
//! and size of the text, which layout takes.

#[cfg(test)]
mod tests;

use crate::js::chars::{is_space_char, to_lower, trim, trim_start};
use crate::js::math::{max, min};
use crate::js::number;
use crate::scene::style::{Cap, Color, Join, Paint, Style};

/// The properties of a shape Pele's resolveStyle keeps, as its SHAPE_PROPS has them.
const SHAPE_PROPS: &[&str] = &[
    "fill",
    "fill-opacity",
    "stroke",
    "stroke-width",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-opacity",
    "opacity",
    "rx",
    "ry",
];

/// The properties of text Pele's resolveStyle keeps, as its TEXT_PROPS has them.
const TEXT_PROPS: &[&str] = &[
    "color",
    "font-size",
    "font-family",
    "font-weight",
    "font-style",
    "text-decoration",
    "letter-spacing",
    "word-spacing",
    "opacity",
];

/// The properties declarations keep, each with the last value given it, in the order the
/// properties first came, as the Maps of Pele's resolveStyle hold them: those of a shape, and
/// those of text, with color as fill.
#[derive(Default)]
struct Kept {
    shape: Vec<(String, String)>,
    text: Vec<(String, String)>,
}

impl Kept {
    fn shape(&self, prop: &str) -> Option<&str> {
        get(&self.shape, prop)
    }

    fn text(&self, prop: &str) -> Option<&str> {
        get(&self.text, prop)
    }
}

fn get<'a>(map: &'a [(String, String)], prop: &str) -> Option<&'a str> {
    map.iter().find(|(p, _)| p == prop).map(|(_, v)| v.as_str())
}

/// Sets a property of an ordered map, keeping its place if it has one, as a Map's set does.
fn set(map: &mut Vec<(String, String)>, prop: &str, value: &str) {
    match map.iter_mut().find(|(p, _)| p == prop) {
        Some((_, v)) => *v = value.to_string(),
        None => map.push((prop.to_string(), value.to_string())),
    }
}

/// The declarations Pele's resolveStyle keeps: those with a colon after a property, whose
/// value, without !important in any case and trimmed, is neither empty nor unsafe.
fn keep(decls: &[String]) -> Kept {
    let mut kept = Kept::default();
    for decl in decls {
        let Some(at) = decl.find(':') else {
            continue;
        };
        if at == 0 {
            continue;
        }
        let prop = to_lower(trim(&decl[..at]));
        let without = drop_important(&decl[at + 1..]);
        let value = trim(&without);
        if value.is_empty() || unsafe_value(value) {
            continue;
        }
        if SHAPE_PROPS.contains(&prop.as_str()) {
            set(&mut kept.shape, &prop, value);
        }
        if TEXT_PROPS.contains(&prop.as_str()) {
            let prop = if prop == "color" { "fill" } else { &prop };
            set(&mut kept.text, prop, value);
        }
    }
    kept
}

/// What Pele's resolveStyle makes of declarations: the styles it writes on a shape, on its
/// line, and on its text, and how the text is set.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Resolved {
    /// The style of a shape as Pele writes it into the shape's style attribute, as
    /// "fill:red;stroke:blue;": each property of a shape that is kept, with the last value
    /// given it, in the order the properties first came, with & and " written as entities.
    /// It is empty where none is kept.
    pub(crate) shape: String,
    /// The style of a line, which is the shape's but for its fill and fill-opacity.
    pub(crate) line: String,
    /// The style of text, of each property of text that is kept, with color written as fill.
    pub(crate) text: String,
    /// The stroke, as it was written.
    pub(crate) stroke: Option<String>,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    /// The size of the text in pixels, where it is set to one from more than 0 to 200.
    pub(crate) font_size: Option<f64>,
}

/// Splits declarations into what applies to a shape and what applies to its text, as Pele's
/// resolveStyle does.
pub(crate) fn resolve_style(decls: &[String]) -> Resolved {
    let kept = keep(decls);
    let mut r = Resolved::default();
    for (prop, value) in &kept.shape {
        write(&mut r.shape, prop, value);
        if prop != "fill" && prop != "fill-opacity" {
            write(&mut r.line, prop, value);
        }
    }
    for (prop, value) in &kept.text {
        write(&mut r.text, prop, value);
    }
    r.stroke = kept.shape("stroke").map(str::to_string);
    (r.bold, r.italic, r.font_size) = font(&kept);
    r
}

/// Writes a declaration as Pele writes it into a style attribute, through its esc. Of what
/// esc changes, only & and " are left in a value that is kept.
fn write(css: &mut String, prop: &str, value: &str) {
    css.push_str(prop);
    css.push(':');
    for c in value.chars() {
        match c {
            '&' => css.push_str("&amp;"),
            '"' => css.push_str("&quot;"),
            _ => css.push(c),
        }
    }
    css.push(';');
}

/// Whether the text is bold and italic, and its size, as Pele's resolveStyle reads them: a
/// size that /^[\d.]+(px)?$/ takes, read with parseFloat, from more than 0 to 200.
fn font(kept: &Kept) -> (bool, bool, Option<f64>) {
    let weight = kept.text("font-weight");
    let bold = weight.is_some_and(|w| w == "bold" || w == "bolder" || number::number(w) >= 600.0);
    let italic = kept.text("font-style") == Some("italic");
    let size = kept.text("font-size").and_then(font_pixels);
    (bold, italic, size.filter(|&px| px > 0.0 && px <= 200.0))
}

/// A font size as Pele's resolveStyle reads one: what /^[\d.]+(px)?$/ takes, read with
/// parseFloat, which stops at a second point.
fn font_pixels(value: &str) -> Option<f64> {
    let body = value.strip_suffix("px").unwrap_or(value);
    if body.is_empty() || !body.bytes().all(|c| c.is_ascii_digit() || c == b'.') {
        return None;
    }
    let body = match body
        .find('.')
        .and_then(|i| body[i + 1..].find('.').map(|j| i + 1 + j))
    {
        Some(second) => &body[..second],
        None => body,
    };
    // Rust reads digits about one point as parseFloat does. A point alone is NaN to it, and
    // NaN is no size.
    body.parse().ok()
}

impl Resolved {
    /// Whether the shape's style sets a fill, whether or not it can be read, as the diagrams
    /// that draw a tint only where the author gave no fill test it with /[";]fill:/.
    pub(crate) fn fill_declared(&self) -> bool {
        declares(&self.shape, "fill")
    }

    /// Whether the text's style sets a font family, as the diagrams that set a part in the
    /// monospace face where the author gave no font of its own test it with
    /// /[";]font-family:/.
    pub(crate) fn font_family_declared(&self) -> bool {
        declares(&self.text, "font-family")
    }

    /// Whether anything of a line is set: a property of a shape other than its fill and fill
    /// opacity, whether or not it can be read.
    pub(crate) fn line_declared(&self) -> bool {
        !self.line.is_empty()
    }

    /// Whether the shape's style paints its fill, as Pele's paints says.
    pub(crate) fn paints(&self) -> bool {
        paints(&self.shape)
    }

    /// Whether Pele's ink picks a color for text on the shape's fill, with no style of text.
    pub(crate) fn inks(&self) -> bool {
        inks(&self.shape, "")
    }

    /// Whether Pele's ink picks a color for text with text's style on the shape's fill.
    pub(crate) fn inks_text(&self, text: &Resolved) -> bool {
        inks(&self.shape, &text.text)
    }
}

/// Where the value of each declaration of prop in a style Pele writes starts, as /[";]prop:/
/// finds it in the attribute: at its start, after its quote, or after a semicolon. An entity
/// Pele writes for a quote or an ampersand in a value ends in a semicolon too, so one may be
/// found inside another's value.
fn values_at<'a>(css: &'a str, prop: &'a str) -> impl Iterator<Item = usize> + 'a {
    std::iter::once(0)
        .chain(css.match_indices(';').map(|(i, _)| i + 1))
        .filter(move |&i| css[i..].starts_with(prop) && css[i + prop.len()..].starts_with(':'))
        .map(move |i| i + prop.len() + 1)
}

/// Whether a style Pele writes declares prop, as /[";]prop:/ tests it.
pub(crate) fn declares(css: &str, prop: &str) -> bool {
    values_at(css, prop).next().is_some()
}

/// The value of prop in a style Pele writes, trimmed, as Pele's casing takes it with
/// /[";]prop:([^;"]+);/: the first that is followed by a semicolon. It is cut short at an
/// entity Pele wrote for a quote or an ampersand.
pub(crate) fn declared<'a>(css: &'a str, prop: &str) -> Option<&'a str> {
    values_at(css, prop).find_map(|i| {
        let end = css[i..].find([';', '"']).map(|n| i + n)?;
        (end > i && css[end..].starts_with(';')).then(|| trim(&css[i..end]))
    })
}

/// The value RE_FILL, /[";]fill:([^;"]*)/, takes from a style Pele writes.
fn fill_value(css: &str) -> Option<&str> {
    let i = values_at(css, "fill").next()?;
    let end = css[i..].find([';', '"']).map_or(css.len(), |n| i + n);
    Some(&css[i..end])
}

/// How opaque a style's fill-opacity and opacity leave its fill, or NaN where either is not
/// a plain number, as Pele's opacity reads them with /[";](?:fill-)?opacity:([^;"]*)/g. Each
/// value it takes ends where the next can start, at a semicolon, so the two properties are
/// found by their own starts.
fn opacity(css: &str) -> f64 {
    let mut product = 1.0;
    let mut starts: Vec<usize> = values_at(css, "opacity")
        .chain(values_at(css, "fill-opacity"))
        .collect();
    starts.sort_unstable();
    for i in starts {
        let end = css[i..].find([';', '"']).map_or(css.len(), |n| i + n);
        product *= amount(trim(&css[i..end]), 1.0);
    }
    product
}

/// Whether Pele's ink picks a color for text on the fill of a shape's style, as its pick
/// does: where the shape sets a fill whose color it reads, at no less than half its opacity,
/// with opacities it reads, and the text sets no fill of its own. Which color it picks is
/// the style's to settle, by [`Style::on_fill`].
pub(crate) fn inks(shape: &str, text: &str) -> bool {
    let Some(fill) = fill_value(shape) else {
        return false;
    };
    if declares(text, "fill") {
        return false;
    }
    // An opacity that is not a plain number, as calc() or var(), may still be low.
    color_alpha(fill).is_some_and(|alpha| alpha * opacity(shape) >= 0.5)
}

/// Whether a shape's style paints its fill, as Pele's paints says: a fill that is neither
/// none nor transparent, at an opacity above 0. A color or an opacity it cannot read counts
/// as opaque.
pub(crate) fn paints(shape: &str) -> bool {
    let Some(fill) = fill_value(shape) else {
        return false;
    };
    let fill = to_lower(trim(fill));
    if fill.is_empty() || fill == "none" || fill == "transparent" {
        return false;
    }
    let scale = opacity(shape);
    color_alpha(&fill).unwrap_or(1.0) * (if scale.is_nan() { 1.0 } else { scale }) > 0.0
}

/// The alpha of a color as Pele's color reads it, where it reads it: a named color, a hex
/// color, or rgb(), rgba(), hsl() or hsla() with the arguments its args takes, each a number
/// it reads. Pele's color works out the color's luminance too, for ink's choice, which is
/// the style's here. Pele's names of colors are those [`Color::parse`] reads, but for
/// transparent.
fn color_alpha(raw: &str) -> Option<f64> {
    let value = to_lower(trim(raw));
    if !value.contains(['#', '(']) {
        let name = value.replacen("grey", "gray", 1);
        return (value != "transparent" && Color::parse(&name).is_some()).then_some(1.0);
    }
    if let Some(h) = value.strip_prefix('#') {
        // /^#(?:[\da-f]{3,4}|[\da-f]{6}|[\da-f]{8})$/i
        if !matches!(h.len(), 3 | 4 | 6 | 8) || !h.bytes().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let alpha = match h.len() {
            4 => h[3..4].repeat(2),
            8 => h[6..8].to_string(),
            _ => return Some(1.0),
        };
        return Some(f64::from(u8::from_str_radix(&alpha, 16).ok()?) / 255.0);
    }
    // /^(rgba?|hsla?)\(([^()]*)\)$/
    let open = value.find('(')?;
    let (name, rest) = (&value[..open], &value[open + 1..]);
    let body = rest.strip_suffix(')')?;
    if !matches!(name, "rgb" | "rgba" | "hsl" | "hsla") || body.contains(['(', ')']) {
        return None;
    }
    let hsl = name.starts_with("hsl");
    let list = args(body, hsl)?;
    let alpha = if list.len() == 4 {
        amount(list[3], 1.0)
    } else {
        1.0
    };
    let unread = if hsl {
        let h = (hue(list[0]) % 360.0 + 360.0) % 360.0;
        h.is_nan() || amount(list[1], 100.0).is_nan() || amount(list[2], 100.0).is_nan()
    } else {
        list[..3].iter().any(|c| amount(c, 255.0).is_nan())
    };
    // Pele's color gives none where its luminance or alpha is NaN, and the luminance is NaN
    // where a channel is.
    (!unread && !alpha.is_nan()).then_some(alpha)
}

/// The arguments of a color function, as Pele's args splits them: the comma syntax takes no
/// none, rgb() channels all or none in percentages, and hsl() saturation and lightness in
/// percentages, and the space syntax takes three and an alpha after a slash.
fn args(body: &str, hsl: bool) -> Option<Vec<&str>> {
    let legacy = body.contains(',');
    let (main, alpha) = if legacy {
        (body, None)
    } else {
        let mut parts = body.split('/');
        let main = parts.next().unwrap_or("");
        let alpha = parts.next();
        if parts.next().is_some() {
            return None;
        }
        (main, alpha)
    };
    let mut list: Vec<&str> = if legacy {
        // split(/\s*,\s*/) of the trimmed arguments, which is each piece between the commas
        // trimmed.
        trim(main).split(',').map(trim).collect()
    } else {
        split_spaces(trim(main))
    };
    let counted = if legacy {
        (3..=4).contains(&list.len())
    } else {
        list.len() == 3
    };
    if !counted {
        return None;
    }
    if let Some(alpha) = alpha {
        list.push(trim(alpha));
    }
    if !legacy {
        return Some(
            list.into_iter()
                .map(|c| if c == "none" { "0" } else { c })
                .collect(),
        );
    }
    let (from, want) = if hsl { (1, 2) } else { (0, 3) };
    let percent = list[from..3].iter().filter(|c| c.ends_with('%')).count();
    (percent == want || !hsl && percent == 0).then_some(list)
}

/// s split on runs of white space, as JavaScript's split(/\s+/) splits it. An empty s is
/// one empty piece.
fn split_spaces(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find(is_space_char) {
        out.push(&rest[..i]);
        rest = trim_start(&rest[i..]);
    }
    out.push(rest);
    out
}

/// A number of full, or a percentage, kept from 0 to 1, as Pele's amount reads it: one that
/// /^[+-]?(?:\d+(?:\.\d+)?|\.\d+)(?:e[+-]?\d+)?(%|deg|grad|rad|turn)?$/i takes, and NaN for
/// anything else, an angle among them.
fn amount(raw: &str, full: f64) -> f64 {
    match css_number(raw) {
        Some((f, "")) => min(max(f / full, 0.0), 1.0),
        Some((f, "%")) => min(max(f / 100.0, 0.0), 1.0),
        _ => f64::NAN,
    }
}

/// A hue in degrees, as Pele's hue reads it, and NaN for a percentage or anything else.
fn hue(raw: &str) -> f64 {
    match css_number(raw) {
        None | Some((_, "%")) => f64::NAN,
        Some((f, "turn")) => f * 360.0,
        Some((f, "rad")) => (f * 180.0) / std::f64::consts::PI,
        Some((f, "grad")) => f * 0.9,
        Some((f, _)) => f,
    }
}

/// A number as Pele's RE_NUMBER,
/// /^[+-]?(?:\d+(?:\.\d+)?|\.\d+)(?:e[+-]?\d+)?(%|deg|grad|rad|turn)?$/i, takes it, with
/// parseFloat's value and the unit in lower case.
fn css_number(raw: &str) -> Option<(f64, &'static str)> {
    let b = raw.as_bytes();
    let digits = |mut i: usize| {
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        i
    };
    let start = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    let mut i = digits(start);
    if i > start {
        if b.get(i) == Some(&b'.') && digits(i + 1) > i + 1 {
            i = digits(i + 1);
        }
    } else if b.get(i) == Some(&b'.') && digits(i + 1) > i + 1 {
        i = digits(i + 1);
    } else {
        return None;
    }
    // The exponent is optional, so where no digits follow its e it gives way, and the e is
    // read as the start of a unit.
    if matches!(b.get(i), Some(b'e' | b'E')) {
        let j = i + 1 + usize::from(matches!(b.get(i + 1), Some(b'+' | b'-')));
        if digits(j) > j {
            i = digits(j);
        }
    }
    let unit = ["", "%", "deg", "grad", "rad", "turn"]
        .into_iter()
        .find(|u| raw[i..].eq_ignore_ascii_case(u))?;
    // Rust reads what the expression takes as parseFloat does, a number too large for an f64
    // as infinite.
    Some((raw[..i].parse().ok()?, unit))
}

/// What a diagram's author set in its text, with classDef and style lines, as a browser
/// reads the styles Pele writes of them: the style they give a thing, and the weight, slant
/// and size of its text, which layout takes.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Author {
    pub(crate) style: Style,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    /// The size of the text in pixels, where it is set.
    pub(crate) font_size: Option<f64>,
}

/// Reads Mermaid's style declarations, as "fill:#f9f" and "stroke-width:4px", as a browser
/// reads the styles Pele's resolveStyle writes of them. Pele keeps the last value of each
/// property alone, so one that cannot be read leaves the property unset, as a browser drops
/// what it cannot take. Of the properties, fill, stroke, color, the widths, dashes, caps and
/// joins, opacities and rx or ry are read, and the font's weight, style and size. A
/// fill-opacity and a stroke-opacity are the style's, apart from the paints, as CSS cascades
/// them.
pub(crate) fn declarations(decls: &[String]) -> Author {
    let kept = keep(decls);
    let mut s = Style {
        text: kept.text("fill").and_then(paint),
        ..Style::default()
    };
    for (prop, value) in &kept.shape {
        let value = value.as_str();
        match prop.as_str() {
            "fill" => s.fill = paint(value),
            "stroke" => s.stroke = paint(value),
            "fill-opacity" => s.fill_opacity = opacity_of(value),
            "stroke-opacity" => s.stroke_opacity = opacity_of(value),
            "opacity" => s.opacity = opacity_of(value),
            "stroke-width" => s.stroke_width = length(value).filter(|&f| f >= 0.0),
            "stroke-dasharray" => s.dash = dashes(value),
            "stroke-linecap" => s.cap = cap(value),
            "stroke-linejoin" => s.join = join(value),
            _ => {}
        }
    }
    // A box's ry is its rx where it has none, and its rx is its ry. The style has one
    // radius, so a box Pele would round more across than down is rounded by the rx.
    let radius = |prop| kept.shape(prop).and_then(length).filter(|&f| f >= 0.0);
    s.radius = radius("rx").or_else(|| radius("ry"));
    let (bold, italic, font_size) = font(&kept);
    Author {
        style: s,
        bold,
        italic,
        font_size,
    }
}

/// A paint of a color, none for none, or unset for one a browser cannot read.
fn paint(value: &str) -> Option<Paint> {
    if value.eq_ignore_ascii_case("none") {
        return Some(Paint::NONE);
    }
    let c = Color::parse(value)?;
    Some(if c.a == 0 { Paint::NONE } else { Paint::own(c) })
}

/// An opacity, as 0.5 or 50%, kept from 0 to 1, or unset for one a browser cannot read.
fn opacity_of(value: &str) -> Option<f64> {
    let (f, unit) = css_number(value)?;
    let f = match unit {
        "" => f,
        "%" => f / 100.0,
        _ => return None,
    };
    (!f.is_infinite()).then(|| min(max(f, 0.0), 1.0))
}

/// A number of pixels, with or without px, as a browser reads a CSS number.
fn length(value: &str) -> Option<f64> {
    let v = css_trim(value);
    let v = match v.len().checked_sub(2) {
        Some(n) if v.is_char_boundary(n) && v[n..].eq_ignore_ascii_case("px") => &v[..n],
        _ => v,
    };
    match css_number(v)? {
        (f, "") if !f.is_infinite() => Some(f),
        _ => None,
    }
}

/// s without the white space of CSS about it: spaces, tabs, line feeds, carriage returns and
/// form feeds. A browser reads a value Pele writes with these and no others. What
/// JavaScript's trim takes off besides, as a no-break space, was taken off before.
fn css_trim(s: &str) -> &str {
    s.trim_matches([' ', '\t', '\n', '\r', '\x0c'])
}

/// The lengths of a dash array, none of them for none, or unset for one a browser cannot
/// read.
fn dashes(value: &str) -> Option<Vec<f64>> {
    if value.eq_ignore_ascii_case("none") {
        return Some(Vec::new());
    }
    let out: Vec<f64> = value
        .split([',', ' ', '\t', '\n', '\r', '\x0c'])
        .filter(|f| !f.is_empty())
        .map(|f| length(f).filter(|&n| n >= 0.0))
        .collect::<Option<_>>()?;
    (!out.is_empty()).then_some(out)
}

// The keywords of stroke-linecap and stroke-linejoin. Pele passes the properties through to
// the browser, which takes these in either case of their ASCII letters, initial for the
// property's initial value, and inherit or unset for what the element would inherit, which
// is what an unset style takes from under it here. It takes no others. SVG 2's miter-clip
// and arcs are not drawn by browsers, and a value they do not take is dropped.

fn cap(value: &str) -> Option<Cap> {
    [
        ("butt", Cap::Butt),
        ("round", Cap::Round),
        ("square", Cap::Square),
        ("initial", Cap::Butt),
    ]
    .into_iter()
    .find(|(k, _)| value.eq_ignore_ascii_case(k))
    .map(|(_, c)| c)
}

fn join(value: &str) -> Option<Join> {
    [
        ("miter", Join::Miter),
        ("round", Join::Round),
        ("bevel", Join::Bevel),
        ("initial", Join::Miter),
    ]
    .into_iter()
    .find(|(k, _)| value.eq_ignore_ascii_case(k))
    .map(|(_, j)| j)
}

/// Whether a value is one Pele's resolveStyle drops: one RE_UNSAFE,
/// /url\s*\(|expression\s*\(|javascript:|[<>{};\\]|@import|image-set|\/\*/i, finds, of what
/// in a value could load or run something, or one with a character RE_INVALID_XML finds, as
/// leaving it out could make what is left unsafe: a control character but the tab, line feed
/// and carriage return, DEL, U+FFFE or U+FFFF. A Rust string holds no lone surrogate. The i
/// flag, without u, folds the case of ASCII letters only, and \s is JavaScript's white space.
fn unsafe_value(value: &str) -> bool {
    if value.contains(['<', '>', '{', '}', ';', '\\']) {
        return true;
    }
    let invalid = |c: char| {
        c < ' ' && !matches!(c, '\t' | '\n' | '\r') || matches!(c, '\x7f' | '\u{fffe}' | '\u{ffff}')
    };
    if value.contains(invalid) {
        return true;
    }
    let v = value.to_ascii_lowercase();
    if ["javascript:", "@import", "image-set", "/*"]
        .iter()
        .any(|bad| v.contains(bad))
    {
        return true;
    }
    ["url", "expression"].iter().any(|f| {
        v.match_indices(f)
            .any(|(i, _)| trim_start(&v[i + f.len()..]).starts_with('('))
    })
}

/// s with each !important in it, in either case of its ASCII letters, taken out, as
/// `replace(/!important/gi, '')` takes them. The i flag, without u, folds ASCII letters only.
fn drop_important(s: &str) -> String {
    const OLD: &[u8] = b"!important";
    let mut out = String::new();
    let (mut last, mut i) = (0, 0);
    while let Some(n) = s[i..].find('!') {
        let at = i + n;
        if s.as_bytes()[at..].len() >= OLD.len()
            && s.as_bytes()[at..at + OLD.len()].eq_ignore_ascii_case(OLD)
        {
            out.push_str(&s[last..at]);
            last = at + OLD.len();
            i = last;
        } else {
            i = at + 1;
        }
    }
    out.push_str(&s[last..]);
    out
}
