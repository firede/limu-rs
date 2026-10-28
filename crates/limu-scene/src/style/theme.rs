//! Themes: the colors and measures every diagram takes its look from.

use super::color::Color;

/// A color of the theme, by name. A diagram's parts name their colors by token rather than
/// by value, so a change of theme only redraws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Token {
    /// Behind the diagram, and behind labels on lines.
    Bg,
    /// Text.
    Fg,
    /// Secondary text, as the titles of groups.
    Muted,
    /// Edges and other lines.
    Line,
    /// The inside of shapes.
    Surface,
    /// The inside of groups, and of shapes set apart.
    SurfaceAlt,
    /// The outlines of shapes.
    Border,
    /// What a diagram marks out.
    Accent,
    /// What a diagram marks as critical.
    Critical,
    /// The color of a series of a chart, of eight, from 0.
    Series(u8),
}

impl Token {
    /// The token's name, as in the variables the SVG writer writes: "surface-alt",
    /// "series-3", the series counted from 1.
    pub fn name(self) -> &'static str {
        const SERIES: [&str; 8] = [
            "series-1", "series-2", "series-3", "series-4", "series-5", "series-6", "series-7",
            "series-8",
        ];
        match self {
            Token::Bg => "bg",
            Token::Fg => "fg",
            Token::Muted => "muted",
            Token::Line => "line",
            Token::Surface => "surface",
            Token::SurfaceAlt => "surface-alt",
            Token::Border => "border",
            Token::Accent => "accent",
            Token::Critical => "critical",
            Token::Series(n) => SERIES[n as usize % 8],
        }
    }
}

/// The colors and measures diagrams are drawn with.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Theme {
    /// Behind the diagram, and behind labels on lines.
    pub bg: Color,
    /// Text.
    pub fg: Color,
    /// Secondary text, as the titles of groups.
    pub muted: Color,
    /// Edges and other lines.
    pub line: Color,
    /// The inside of shapes.
    pub surface: Color,
    /// The inside of groups, and of shapes set apart.
    pub surface_alt: Color,
    /// The outlines of shapes.
    pub border: Color,
    /// What a diagram marks out.
    pub accent: Color,
    /// What a diagram marks as critical.
    pub critical: Color,
    /// The colors of the series of charts, which repeat after eight.
    pub series: [Color; 8],
    /// The radius of the corners of rounded boxes.
    pub radius: f64,
    /// The width of outlines and lines.
    pub stroke_width: f64,
}

impl Theme {
    /// Pele's light theme.
    pub const PELE: Theme = Theme {
        bg: Color::hex("#fff"),
        fg: Color::hex("#1f1f1f"),
        muted: Color::hex("#6e6e6e"),
        line: Color::hex("#1f1f1f"),
        surface: Color::hex("#f3f3f3"),
        surface_alt: Color::hex("#e6e6e6"),
        border: Color::hex("#8a8a8a"),
        accent: Color::hex("#5b7bd5"),
        critical: Color::hex("#e45756"),
        series: [
            Color::hex("#4c78a8"),
            Color::hex("#f58518"),
            Color::hex("#54a24b"),
            Color::hex("#e45756"),
            Color::hex("#72b7b2"),
            Color::hex("#eeca3b"),
            Color::hex("#b279a2"),
            Color::hex("#9d755d"),
        ],
        radius: 4.0,
        stroke_width: 1.0,
    };

    /// limu's light theme, in the colors of limu kohu, the red limu of Hawaiʻi, in a tide
    /// pool: neutrals of the pool's teal, the limu's red for the accent, and amber for what is
    /// critical, so that it is not taken for the accent. The series are colors of algae.
    pub const KOHU: Theme = Theme {
        bg: Color::hex("#f1f6f5"),
        fg: Color::hex("#0f262a"),
        muted: Color::hex("#536f72"),
        line: Color::hex("#23393d"),
        surface: Color::hex("#e5eeec"),
        surface_alt: Color::hex("#d6e4e2"),
        border: Color::hex("#76939a"),
        accent: Color::hex("#a92e4a"),
        critical: Color::hex("#b0611a"),
        series: [
            Color::hex("#4f8a3a"),
            Color::hex("#b5435c"),
            Color::hex("#23817d"),
            Color::hex("#a8761c"),
            Color::hex("#3d6aa3"),
            Color::hex("#7b5aa0"),
            Color::hex("#c0668a"),
            Color::hex("#7d6a3f"),
        ],
        radius: 4.0,
        stroke_width: 1.0,
    };

    /// limu's dark theme, Kohu's colors in the dark of the pool.
    pub const KOHU_DARK: Theme = Theme {
        bg: Color::hex("#0c1618"),
        fg: Color::hex("#d0dddc"),
        muted: Color::hex("#7f989b"),
        line: Color::hex("#b4c7c7"),
        surface: Color::hex("#142428"),
        surface_alt: Color::hex("#1b2f34"),
        border: Color::hex("#557176"),
        accent: Color::hex("#ee8494"),
        critical: Color::hex("#e3a040"),
        series: [
            Color::hex("#8cbf6a"),
            Color::hex("#e37d90"),
            Color::hex("#5cbcb3"),
            Color::hex("#d9a748"),
            Color::hex("#78a3d9"),
            Color::hex("#ad92d1"),
            Color::hex("#e693b3"),
            Color::hex("#b9a272"),
        ],
        radius: 4.0,
        stroke_width: 1.0,
    };

    /// The theme's color for a token.
    pub fn color(&self, t: Token) -> Color {
        match t {
            Token::Bg => self.bg,
            Token::Fg => self.fg,
            Token::Muted => self.muted,
            Token::Line => self.line,
            Token::Surface => self.surface,
            Token::SurfaceAlt => self.surface_alt,
            Token::Border => self.border,
            Token::Accent => self.accent,
            Token::Critical => self.critical,
            Token::Series(n) => self.series[n as usize % 8],
        }
    }
}

/// Pele's light theme.
impl Default for Theme {
    fn default() -> Theme {
        Theme::PELE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::contrast;

    // Kohu and its dark side keep the contrast limu holds them to: text at 7:1 against the
    // background and surfaces, muted text, lines and the accent at 4.5:1 against the
    // background, the series at 3:1 and outlines at 2.5:1 against the surface.
    #[test]
    fn kohu_contrast() {
        for (name, th) in [("KOHU", Theme::KOHU), ("KOHU_DARK", Theme::KOHU_DARK)] {
            let check = |what: &str, a: Color, b: Color, least: f64| {
                let r = contrast(a, b);
                assert!(
                    r >= least,
                    "{name}: {what} is {r:.2}:1, less than {least}:1"
                );
            };
            check("fg on bg", th.fg, th.bg, 7.0);
            check("fg on surface", th.fg, th.surface, 7.0);
            check("fg on surface_alt", th.fg, th.surface_alt, 7.0);
            check("muted on bg", th.muted, th.bg, 4.5);
            check("line on bg", th.line, th.bg, 4.5);
            check("accent on bg", th.accent, th.bg, 4.5);
            check("border on surface", th.border, th.surface, 2.5);
            for (i, c) in th.series.iter().enumerate() {
                check(&format!("series {} on surface", i + 1), *c, th.surface, 3.0);
            }
        }
    }

    #[test]
    fn tokens() {
        let th = Theme::default();
        assert_eq!(th.color(Token::Series(9)), th.series[1]);
        assert_eq!(Token::Series(2).name(), "series-3");
        assert_eq!(Token::SurfaceAlt.name(), "surface-alt");
    }
}
