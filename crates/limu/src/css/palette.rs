//! The series colors a diagram's own theme variables set, as Pele's src/svg/palette.ts reads
//! Mermaid's palette variables: `pie1` to `pie8`, `git0` to `git7`, `cScale0` to `cScale7`,
//! `venn1` to `venn8` and an XY chart's `plotColorPalette`.
//!
//! Pele writes each color it takes into the SVG as `--pele-series-N`, which every part drawn
//! in that series reads. limu-rs reads it here into a [`Paint`] and writes it into the
//! author's style of each item drawn in the series, as it writes what classDef and style
//! lines set, since the palette is the author's too. Of what Pele takes, a color limu-rs can't
//! evaluate, as one of `var()` or `color-mix()`, leaves its series in the theme's color.

use crate::detect::DiagramType;
use crate::js::chars::{is_space_char, to_lower, trim};
use crate::scene::Item;
use crate::scene::style::{Color, Paint, Source, Token};
use crate::value::{Map, Value};

use super::inks;

/// The number of series a palette sets, Pele's SLOTS.
const SLOTS: usize = 8;

/// The functions a color may call, as Pele's FUNCTIONS lists them.
const FUNCTIONS: &[&str] = &[
    "rgb",
    "rgba",
    "hsl",
    "hsla",
    "hwb",
    "lab",
    "lch",
    "oklab",
    "oklch",
    "color",
    "color-mix",
    "light-dark",
    "var",
];

/// Mermaid's palette theme variables of a type of diagram, as Pele's PALETTES names them: the
/// prefix of their names, the number of the variable of the first series, and whether the
/// colors set repeat. A mindmap's branches start at cScale1, as cScale0 is the root's in
/// Mermaid's styles. A Venn diagram's palette repeats the colors it sets, as in Mermaid.
fn palettes(t: DiagramType) -> Option<(&'static str, usize, bool)> {
    Some(match t {
        DiagramType::Pie => ("pie", 1, false),
        DiagramType::GitGraph => ("git", 0, false),
        DiagramType::Timeline | DiagramType::Radar | DiagramType::Treemap => ("cScale", 0, false),
        DiagramType::Mindmap => ("cScale", 1, false),
        DiagramType::Venn => ("venn", 1, true),
        _ => return None,
    })
}

/// The series colors a diagram's theme variables set, and whether Pele's ink picks a color
/// for a label on each, as Pele's Palette holds them.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Palette {
    /// The color of each series a palette sets that limu-rs reads.
    pub(crate) colors: [Option<Paint>; SLOTS],
    /// Whether Pele's ink picks a color for a label on each series, as Pele's inks says.
    inks: [bool; SLOTS],
}

impl Palette {
    /// The ground of a label that lies on series index, where Pele's seriesInkFill gives it a
    /// fill: the palette's color of the series, where the palette sets one that ink picks a
    /// color on. The style picks the label's color on it when the scene is drawn, as it does
    /// on a fill an author sets. Elsewhere it is none, and the label keeps the color the
    /// diagram gives.
    pub(crate) fn ground(&self, index: usize) -> Option<Paint> {
        let k = index % SLOTS;
        self.colors[k].filter(|_| self.inks[k])
    }

    /// Writes the palette's colors into the author's style of each item drawn in a series it
    /// sets, as Pele's `--pele-series-N` gives its color to every part that reads the series'
    /// variable: into the fill, the stroke or the text that the item's own style draws in the
    /// series, at the part of its opacity the diagram draws the series at. The item's own
    /// style keeps the series, so its role in the theme is as it was. What the author's own
    /// style sets wins, as Pele's inline style wins over the variable.
    pub(crate) fn apply(&self, items: &mut [Item]) {
        if self.colors.iter().all(Option::is_none) {
            return;
        }
        let from = |own: Option<Paint>| {
            let p = own?;
            let Source::Theme(Token::Series(k)) = p.source() else {
                return None;
            };
            let color = self.colors[usize::from(k) % SLOTS]?;
            Some(color.with_opacity(f64::from(p.opacity())))
        };
        for it in items {
            let (fill, stroke, text) = (
                from(it.style.fill),
                from(it.style.stroke),
                from(it.style.text),
            );
            let a = &mut it.author;
            a.fill = a.fill.or(fill);
            a.stroke = a.stroke.or(stroke);
            a.text = a.text.or(text);
        }
    }
}

/// The palette a diagram's theme variables set for its series, as Pele's palette reads it
/// from the configuration. An entry that isn't a usable color leaves its series in the
/// theme's color.
pub(crate) fn palette(t: DiagramType, config: &Map) -> Palette {
    let mut out = Palette::default();
    let vars = record(config.get("themeVariables"));
    let Some((list, repeat)) = entries(t, vars) else {
        return out;
    };
    if list.is_empty() {
        return out;
    }
    for i in 0..SLOTS {
        if !repeat && i >= list.len() {
            break;
        }
        let Some(color) = list[i % list.len()].as_deref().and_then(safe_color) else {
            continue;
        };
        out.colors[i] = read(color);
        out.inks[i] = inks(&format!(";fill:{color}"), "");
    }
    out
}

/// A value Pele's palette takes as a color, read as limu-rs reads one: a hex code, a named
/// color, rgb(), rgba(), hsl() or hsla() as [`Color::parse`] reads it, transparent as a paint
/// that draws nothing, and currentColor as the theme's foreground, since Pele's root sets
/// its color to it. It is none for what limu-rs can't evaluate, as var() or color-mix().
pub(crate) fn read(color: &str) -> Option<Paint> {
    let value = to_lower(trim(color));
    match value.as_str() {
        "transparent" => return Some(Paint::NONE),
        "currentcolor" => return Some(Paint::FG),
        _ => {}
    }
    let name = value.replacen("grey", "gray", 1);
    let c = Color::parse(&name)?;
    Some(if c.a == 0 { Paint::NONE } else { Paint::own(c) })
}

/// The value of an object, or an empty one, as Pele's record reads it.
fn record(value: Option<&Value>) -> &Map {
    static EMPTY: std::sync::OnceLock<Map> = std::sync::OnceLock::new();
    match value {
        Some(Value::Object(m)) => m,
        _ => EMPTY.get_or_init(Map::new),
    }
}

/// The values of a palette's variables, each a string or none for a value of another type,
/// and whether they repeat, as Pele's entries lists them, or none where the type has no
/// palette. An XY chart's palette is one string, its colors split at the commas between
/// them, and repeats.
fn entries(t: DiagramType, vars: &Map) -> Option<(Vec<Option<String>>, bool)> {
    if t == DiagramType::XyChart {
        let list = record(vars.get("xyChart"))
            .get("plotColorPalette")?
            .as_str()?;
        return Some((split_list(list).into_iter().map(Some).collect(), true));
    }
    let (prefix, first, repeat) = palettes(t)?;
    let mut list: Vec<Option<&Value>> = (0..SLOTS)
        .map(|i| vars.get(&format!("{prefix}{}", first + i)))
        .collect();
    if repeat {
        list.retain(|v| match v {
            None | Some(Value::Null) => false,
            Some(Value::String(s)) => !s.is_empty(),
            Some(_) => true,
        });
    }
    let list = list
        .into_iter()
        .map(|v| v.and_then(Value::as_str).map(str::to_string))
        .collect();
    Some((list, repeat))
}

/// Splits a list at the commas between colors, not those inside rgb() and other functions,
/// and trims each, as Pele's splitList does.
fn split_list(value: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in value.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(trim(&value[start..i]).to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(trim(&value[start..]).to_string());
    parts
}

/// Whether a character is one of \w, an ASCII letter, digit or underscore.
fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// A color that can't load a resource, end its declaration or read the page's own variables,
/// trimmed, as Pele's safeColor takes it. A value outside functions is checked as a color,
/// so a typo keeps the theme's color.
fn safe_color(value: &str) -> Option<&str> {
    let color = trim(value);
    // /^[\w\s#%.,+\-/()]{1,200}$/, whose characters are each one code unit.
    let n = color.chars().count();
    if !(1..=200).contains(&n)
        || !color
            .chars()
            .all(|c| is_word(c) || is_space_char(c) || "#%.,+-/()".contains(c))
    {
        return None;
    }
    if !color.contains('(') {
        return plain_color(color).then_some(color);
    }
    let mut depth = 0;
    for c in color.chars() {
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
            if depth < 0 {
                return None;
            }
        }
    }
    if depth != 0 {
        return None;
    }
    if !functions(color).all(|name| FUNCTIONS.contains(&to_lower(name).as_str())) {
        return None;
    }
    if !vars(color).all(|name| name.starts_with("--pele-")) {
        return None;
    }
    Some(color)
}

/// The run of characters from i that keep, and where it ends.
fn run(s: &str, i: usize, keep: impl Fn(char) -> bool) -> usize {
    s[i..].find(|c| !keep(c)).map_or(s.len(), |n| i + n)
}

/// The names of the functions a color calls, as Pele's RE_FUNCTION, /([\w-]*)\s*\(/g, finds
/// them: each match starts where the run of word characters and hyphens before a
/// parenthesis, and the white space after it, can start, past the match before.
fn functions(s: &str) -> impl Iterator<Item = &str> {
    let mut at = 0;
    std::iter::from_fn(move || {
        while at <= s.len() {
            let name_end = run(s, at, |c| is_word(c) || c == '-');
            let open = run(s, name_end, is_space_char);
            if s[open..].starts_with('(') {
                let name = &s[at..name_end];
                at = open + 1;
                return Some(name);
            }
            // The match fails here, and the next is tried a character on.
            at += s[at..].chars().next().map_or(1, char::len_utf8);
        }
        None
    })
}

/// The names of the variables a color reads, as Pele's RE_VAR, /var\(\s*([\w-]*)/gi, finds
/// them.
fn vars(s: &str) -> impl Iterator<Item = &str> {
    let mut at = 0;
    std::iter::from_fn(move || {
        while at + 4 <= s.len() {
            if s.as_bytes()[at..at + 4].eq_ignore_ascii_case(b"var(") {
                let start = run(s, at + 4, is_space_char);
                let end = run(s, start, |c| is_word(c) || c == '-');
                at = end;
                return Some(&s[start..end]);
            }
            at += 1;
        }
        None
    })
}

/// Whether a value is a hex color or one of CSS's named colors, transparent or currentColor,
/// as Pele's plainColor says. Pele's named colors are those [`Color::parse`] reads, with grey
/// read as gray.
fn plain_color(raw: &str) -> bool {
    let value = to_lower(trim(raw));
    let name = value.replacen("grey", "gray", 1);
    let hex = value.strip_prefix('#').is_some_and(|h| {
        matches!(h.len(), 3 | 4 | 6 | 8) && h.bytes().all(|c| c.is_ascii_hexdigit())
    });
    let named = !name.is_empty()
        && name.bytes().all(|c| c.is_ascii_lowercase())
        && name != "transparent"
        && Color::parse(&name).is_some();
    hex || named || value == "transparent" || value == "currentcolor"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prepare::{ParseOptions, prepare};

    /// The configuration a directive of init sets.
    fn config(json: &str) -> Map {
        let src = format!("%%{{init: {json}}}%%\npie\n  \"A\": 1");
        prepare(&src, &ParseOptions::default())
            .expect("a pie chart")
            .config
    }

    fn colors(t: DiagramType, json: &str) -> Vec<(usize, Option<Paint>)> {
        palette(t, &config(json))
            .colors
            .iter()
            .enumerate()
            .filter(|(_, c)| c.is_some())
            .map(|(i, c)| (i + 1, *c))
            .collect()
    }

    fn own(s: &str) -> Option<Paint> {
        Some(Paint::own(Color::parse(s).unwrap()))
    }

    // The cases of Pele's tests/render/palette.test.ts, as limu-rs reads the colors.
    #[test]
    fn maps_each_palette_onto_the_series() {
        let red_blue = vec![(1, own("red")), (2, own("blue"))];
        for (t, vars) in [
            (DiagramType::Pie, r#"{"pie1": "red", "pie2": "blue"}"#),
            (DiagramType::GitGraph, r#"{"git0": "red", "git1": "blue"}"#),
            (
                DiagramType::Timeline,
                r#"{"cScale0": "red", "cScale1": "blue"}"#,
            ),
            (
                DiagramType::Radar,
                r#"{"cScale0": "red", "cScale1": "blue"}"#,
            ),
            (
                DiagramType::Treemap,
                r#"{"cScale0": "red", "cScale1": "blue"}"#,
            ),
            (
                DiagramType::Mindmap,
                r#"{"cScale0": "green", "cScale1": "red", "cScale2": "blue"}"#,
            ),
        ] {
            let json = format!(r#"{{"themeVariables": {vars}}}"#);
            assert_eq!(colors(t, &json), red_blue, "{t:?}");
        }
        let json = r#"{"themeVariables": {"venn2": "red", "venn4": "blue"}}"#;
        let got = colors(DiagramType::Venn, json);
        assert_eq!(got.len(), 8);
        for (i, c) in got {
            assert_eq!(c, if i % 2 == 1 { own("red") } else { own("blue") });
        }
        let json = r#"{"themeVariables": {"xyChart": {"plotColorPalette": "rgb(0, 0, 0), hsl(0 50% 50% / 0.5), #00FF00"}}}"#;
        let got = colors(DiagramType::XyChart, json);
        assert_eq!(got[0], (1, own("#000")));
        assert_eq!(got[1], (2, own("hsl(0 50% 50% / 0.5)")));
        assert_eq!(got[2], (3, own("#0f0")));
        assert_eq!(got[3], (4, own("#000")));
        assert_eq!(got.len(), 8);
    }

    #[test]
    fn passes_over_other_palettes() {
        let json = r#"{"themeVariables": {"cScale0": "red", "primaryColor": "red"}}"#;
        assert!(colors(DiagramType::Pie, json).is_empty());
        let json = r#"{"themeVariables": {"pie1": "red"}}"#;
        assert!(colors(DiagramType::Flowchart, json).is_empty());
    }

    #[test]
    fn takes_safe_colors_only() {
        for value in [
            "url(https://example.com/a.svg)",
            "url(//example.com/a.svg)",
            "image-set(a 1x)",
            "red;stroke:blue",
            "\"><script>alert(1)</script>",
            "expression(alert(1))",
            "rgb(0, 0",
            "red)",
            "red}",
            "javascript:alert(1)",
            "/*",
            "reddish",
            "#ff00000",
            "var(--page-color)",
            "color-mix(in srgb, var(--page-color) 50%, red)",
            "",
        ] {
            assert_eq!(safe_color(value), None, "{value:?}");
        }
        for value in [
            "var(--pele-accent)",
            "light-dark(#000, #fff)",
            "color-mix(in srgb, red 40%, blue)",
            " MidnightBlue ",
            "Grey",
            "currentColor",
            "oklch(70% 0.1 200)",
        ] {
            assert_eq!(safe_color(value), Some(trim(value)), "{value:?}");
        }
    }

    // What Pele takes and limu-rs can't evaluate leaves the series in the theme's color.
    #[test]
    fn reads_what_it_can_evaluate() {
        assert_eq!(read("transparent"), Some(Paint::NONE));
        assert_eq!(read("currentColor"), Some(Paint::FG));
        assert_eq!(read("darkgrey"), own("darkgray"));
        assert_eq!(read("#11223380"), own("#11223380"));
        for value in [
            "var(--pele-accent)",
            "color-mix(in srgb, red, blue)",
            "light-dark(red, blue)",
            "oklch(0.5 0.1 200)",
        ] {
            assert_eq!(read(value), None, "{value:?}");
        }
    }

    // A label on a series is set on its color where ink picks one: a color it reads, at no
    // less than half its opacity.
    #[test]
    fn grounds_labels_where_ink_picks_a_color() {
        let json = r##"{"themeVariables": {"pie1": "#ffeb3b", "pie2": "navy", "pie3": "var(--pele-accent)", "pie4": "rgb(0 0 0 / 0.3)", "pie5": "transparent"}}"##;
        let p = palette(DiagramType::Pie, &config(json));
        assert_eq!(p.ground(0), own("#ffeb3b"));
        assert_eq!(p.ground(1), own("navy"));
        assert_eq!(p.ground(9), own("navy"));
        for i in [2, 3, 4, 5] {
            assert_eq!(p.ground(i), None, "{i}");
        }
    }

    // The palette's color goes into the author's style of what is drawn in its series, at
    // the opacity the diagram draws the series at, under what the author's own style sets.
    #[test]
    fn applies_as_the_authors_colors() {
        use crate::scene::style::Style;
        use crate::scene::{Item, Kind};
        let json = r##"{"themeVariables": {"pie1": "red", "pie3": "navy"}}"##;
        let p = palette(DiagramType::Pie, &config(json));
        let item = |own: Style, author: Style| {
            let mut it = Item::new(Kind::Mark, "pie.slice");
            it.style = own;
            it.author = author;
            it
        };
        let mut items = vec![
            item(
                Style {
                    fill: Some(Paint::series(8).with_opacity(0.5)),
                    stroke: Some(Paint::series(1)),
                    text: Some(Paint::BG),
                    ..Style::default()
                },
                Style::default(),
            ),
            item(
                Style {
                    fill: Some(Paint::series(2)),
                    stroke: Some(Paint::series(2)),
                    ..Style::default()
                },
                Style {
                    fill: own("#123456"),
                    ..Style::default()
                },
            ),
        ];
        p.apply(&mut items);
        assert_eq!(
            items[0].author,
            Style {
                fill: own("red").map(|c| c.with_opacity(0.5)),
                ..Style::default()
            }
        );
        assert_eq!(
            items[1].author,
            Style {
                fill: own("#123456"),
                stroke: own("navy"),
                ..Style::default()
            }
        );
        assert_eq!(
            items[0].style.fill,
            Some(Paint::series(0).with_opacity(0.5))
        );
    }
}
