//! How a diagram looks, at three levels.
//!
//! A [`Theme`] is the first: a handful of colors and measures every diagram is drawn from, as
//! Pele's `--pele-*` variables are. A diagram's parts name their colors by [`Token`] rather
//! than by value, so a change of theme only redraws.
//!
//! [`Overrides`] are the second: a [`Style`] for each role of the parts of diagrams, such as
//! "flowchart.node" or "sequence.actor", that sets some of what the theme gives.
//!
//! The third, drawing a part some way of one's own, is a backend's hook.
//!
//! What changes where things are, as the font and the sizes of text, is not style. It is
//! given to layout, and a change of it lays the diagram out again.

mod color;
mod theme;

use std::collections::HashMap;
use std::fmt;

pub use color::Color;
pub use theme::{Theme, Token};

/// Where a paint's color comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Source {
    /// Nothing: the paint draws nothing.
    None,
    /// A color of the theme.
    Theme(Token),
    /// A color of its own.
    Own(Color),
}

/// What a part is filled, stroked or written with: a color of the theme, a color of its own,
/// or nothing, at a part of its opacity, and laid over a color of the theme where it must
/// hide what is under it.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Paint {
    source: Source,
    // A factor on the color's alpha, from 0 to 1. It is kept in an f32, as limu keeps it, so
    // that the two settle a color's alpha alike.
    alpha: f32,
    over: Option<Token>,
}

impl Paint {
    /// The paint that draws nothing.
    pub const NONE: Paint = Paint {
        source: Source::None,
        alpha: 1.0,
        over: None,
    };
    /// The theme's background.
    pub const BG: Paint = Paint::token(Token::Bg);
    /// The theme's color of text.
    pub const FG: Paint = Paint::token(Token::Fg);
    /// The theme's color of secondary text.
    pub const MUTED: Paint = Paint::token(Token::Muted);
    /// The theme's color of lines.
    pub const LINE: Paint = Paint::token(Token::Line);
    /// The theme's color of the inside of shapes.
    pub const SURFACE: Paint = Paint::token(Token::Surface);
    /// The theme's color of the inside of groups.
    pub const SURFACE_ALT: Paint = Paint::token(Token::SurfaceAlt);
    /// The theme's color of outlines.
    pub const BORDER: Paint = Paint::token(Token::Border);
    /// The theme's color of what a diagram marks out.
    pub const ACCENT: Paint = Paint::token(Token::Accent);
    /// The theme's color of what a diagram marks as critical.
    pub const CRITICAL: Paint = Paint::token(Token::Critical);

    /// The theme's color for a token.
    pub const fn token(t: Token) -> Paint {
        Paint {
            source: Source::Theme(t),
            alpha: 1.0,
            over: None,
        }
    }

    /// The color of the nth series of a chart, of eight that repeat.
    pub const fn series(n: usize) -> Paint {
        Paint::token(Token::Series((n % 8) as u8))
    }

    /// A color of its own.
    pub const fn own(c: Color) -> Paint {
        Paint {
            source: Source::Own(c),
            alpha: 1.0,
            over: None,
        }
    }

    /// The paint at a part of its opacity. At no opacity or less, it draws nothing.
    pub fn alpha(self, a: f64) -> Paint {
        if a <= 0.0 {
            return Paint::NONE;
        }
        let mut p = self;
        if a < 1.0 {
            p.alpha *= a as f32;
        }
        p
    }

    /// The paint laid over a color of the theme: an opaque color, the paint's color at its
    /// opacity mixed into base's, as CSS's color-mix does in sRGB. It is for what must hide
    /// what is under it in the color it would seem to have there, as a title's halo over a
    /// group.
    pub fn over(self, base: Token) -> Paint {
        let mut p = self;
        if p.visible() {
            p.over = Some(base);
        }
        p
    }

    /// Where the paint's color comes from.
    pub fn source(&self) -> Source {
        self.source
    }

    /// The theme's token the paint is, if it is one.
    pub fn as_token(&self) -> Option<Token> {
        match self.source {
            Source::Theme(t) => Some(t),
            _ => None,
        }
    }

    /// The token of the color the paint is laid over, if it is.
    pub fn base(&self) -> Option<Token> {
        self.over
    }

    /// Whether the paint draws anything.
    pub fn visible(&self) -> bool {
        self.source != Source::None
    }

    /// The part of its opacity the paint is drawn at.
    pub fn factor(&self) -> f32 {
        self.alpha
    }

    /// The color the paint draws with in a theme. Its alpha is 0 when it draws nothing.
    pub fn color(&self, th: &Theme) -> Color {
        let mut c = match self.source {
            Source::None => return Color::default(),
            Source::Theme(t) => th.color(t),
            Source::Own(c) => c,
        };
        if self.alpha != 1.0 {
            c.a = (f32::from(c.a) * self.alpha + 0.5) as u8;
        }
        if let Some(base) = self.over {
            let b = th.color(base);
            let t = f64::from(c.a) / 255.0;
            let mix = |x: u8, y: u8| (f64::from(x) * t + f64::from(y) * (1.0 - t) + 0.5) as u8;
            c = Color::rgb(mix(c.r, b.r), mix(c.g, b.g), mix(c.b, b.b));
        }
        c
    }
}

/// How the ends of a stroked line are drawn, as SVG's stroke-linecap: the ends of a line and
/// of each of its dashes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Cap {
    /// Flat, at the end.
    #[default]
    Butt,
    /// A half circle about the end.
    Round,
    /// Flat, half the line's width past the end.
    Square,
}

impl fmt::Display for Cap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Cap::Butt => "butt",
            Cap::Round => "round",
            Cap::Square => "square",
        })
    }
}

/// How the corners of a stroked line or outline are drawn, as SVG's stroke-linejoin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Join {
    /// The two sides run on until they meet, as far as the miter limit lets them.
    #[default]
    Miter,
    /// An arc about the corner.
    Round,
    /// Straight across, from one side's end to the other's.
    Bevel,
}

impl fmt::Display for Join {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Join::Miter => "miter",
            Join::Round => "round",
            Join::Bevel => "bevel",
        })
    }
}

/// How a part is painted. What a style leaves unset, as `None`, comes from under it.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Style {
    /// The inside of the part.
    pub fill: Option<Paint>,
    /// Outlines and lines, and the markers at their ends.
    pub stroke: Option<Paint>,
    /// The part's text.
    pub text: Option<Paint>,
    /// The opacity the fill is drawn at, from 0 to 1, as SVG's fill-opacity: apart from the
    /// paint, so that each is laid over what is under it on its own, as CSS cascades them. A
    /// fill the author sets over a part the diagram draws at an opacity is drawn at that
    /// opacity. It does not reach the part's text.
    pub fill_opacity: Option<f64>,
    /// The opacity the stroke is drawn at, as SVG's stroke-opacity. Markers are filled in
    /// the stroke's color without it and stroked with it, as Pele draws them.
    pub stroke_opacity: Option<f64>,
    /// The width of the part's strokes.
    pub stroke_width: Option<f64>,
    /// The radius of the corners of a box that takes the theme's.
    pub radius: Option<f64>,
    /// Caps the theme's radius where no level sets one, as a treemap's small boxes are
    /// rounded by min(var(--_r), cap) in Pele.
    pub max_radius: Option<f64>,
    /// The lengths of the dashes and gaps of the part's strokes in turn. None of them is a
    /// solid line.
    pub dash: Option<Vec<f64>>,
    /// The ends of the part's lines and of their dashes.
    pub cap: Option<Cap>,
    /// The corners of the part's lines and outlines.
    pub join: Option<Join>,
    /// The opacity of the whole part.
    pub opacity: Option<f64>,
    /// What is drawn about the letters of the part's text, `halo_width` wide, so that lines
    /// under the text stop short of it, as the title of a group an edge runs through.
    pub halo: Option<Paint>,
    /// The width of the halo.
    pub halo_width: Option<f64>,
    /// Whether the part's text is set on its fill. Where an override or the part's author
    /// sets a fill over the diagram's own, and neither sets a color for the text, the text is
    /// drawn in the theme's foreground or background, whichever stands out more against that
    /// fill, as Pele's ink picks a color for a label on a fill a style sets. A diagram sets it
    /// on the parts whose labels Pele draws so.
    pub on_fill: bool,
    /// The fill under the part where it lies inside a fill its author sets on another part,
    /// as an edge inside a group with a fill of its own. Unless an override or the part's
    /// author sets them, the part's stroke and text are drawn in the theme's foreground or
    /// background, whichever stands out more against the ground, as Pele draws an edge or a
    /// label inside one fill in the color of that fill's labels, and what it paints
    /// [`Hollow`](crate::Painting::Hollow) is filled with the ground. A diagram sets it only
    /// on a fill ink can pick a color on.
    pub ground: Option<Paint>,
    /// The fill under the part's text alone, where the text lies inside a fill its author
    /// sets on another part and the rest of the part keeps its own colors, as the name under
    /// a sequence diagram's figure in a box. Unless an override or the part's author sets
    /// one, the text is drawn in the theme's foreground or background, whichever stands out
    /// more against it.
    pub text_ground: Option<Paint>,
}

impl Style {
    /// The style laid over base: what it sets, and what base sets where it does not.
    pub fn over(&self, base: &Style) -> Style {
        Style {
            fill: self.fill.or(base.fill),
            stroke: self.stroke.or(base.stroke),
            text: self.text.or(base.text),
            fill_opacity: self.fill_opacity.or(base.fill_opacity),
            stroke_opacity: self.stroke_opacity.or(base.stroke_opacity),
            stroke_width: self.stroke_width.or(base.stroke_width),
            radius: self.radius.or(base.radius),
            max_radius: self.max_radius.or(base.max_radius),
            dash: self.dash.clone().or_else(|| base.dash.clone()),
            cap: self.cap.or(base.cap),
            join: self.join.or(base.join),
            opacity: self.opacity.or(base.opacity),
            halo: self.halo.or(base.halo),
            halo_width: self.halo_width.or(base.halo_width),
            on_fill: self.on_fill || base.on_fill,
            ground: self.ground.or(base.ground),
            text_ground: self.text_ground.or(base.text_ground),
        }
    }
}

/// Styles for the roles of diagrams' parts, by the names of their roles, as
/// "flowchart.node".
pub type Overrides = HashMap<String, Style>;

/// A theme and the overrides on it: all of style a backend is given.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Look {
    /// The theme. Its default is Pele's light theme.
    pub theme: Theme,
    /// The styles laid over the diagram's own for parts of a role.
    pub overrides: Overrides,
}

impl Look {
    /// The look of a theme with no overrides.
    pub fn new(theme: Theme) -> Look {
        Look {
            theme,
            overrides: Overrides::new(),
        }
    }

    /// The style of a part of a role: the theme's measures under the diagram's own style for
    /// the part, under the overrides for its role, under the style its author gave it in the
    /// diagram's text. Text is written in the foreground unless something sets another paint.
    pub fn cascade(&self, role: &str, own: &Style, author: &Style) -> Style {
        let th = &self.theme;
        let none = Style::default();
        let over = self.overrides.get(role).unwrap_or(&none);
        let mut s = author.over(&over.over(own));
        let set = author.over(over);
        if s.on_fill
            && set.text.is_none()
            && let Some(text) = ink(th, &set)
        {
            s.text = Some(text);
        }
        // A diagram sets a ground only on a fill ink picks a color on, as it reads the
        // author's declarations, so the color is picked whatever its alpha, which holds less
        // than they do.
        if let Some(ground) = s.ground.filter(Paint::visible) {
            let on = pick(th, ground.color(th));
            if set.stroke.is_none() {
                s.stroke = Some(on);
            }
            if set.text.is_none() {
                s.text = Some(on);
            }
        }
        if let Some(ground) = s.text_ground.filter(Paint::visible)
            && set.text.is_none()
        {
            s.text = Some(pick(th, ground.color(th)));
        }
        if s.stroke_width.is_none() {
            s.stroke_width = Some(th.stroke_width);
        }
        if s.radius.is_none() {
            s.radius = Some(match s.max_radius {
                Some(cap) => th.radius.min(cap),
                None => th.radius,
            });
        }
        if s.text.is_none() {
            s.text = Some(Paint::FG);
        }
        s
    }

    /// What a part painted [`Shade`](crate::Painting::Shade) is filled with, before the
    /// opacity a backend lays it at over a fill the author sets: the theme's background, or,
    /// on a fill the author sets that ink picks a color on, the other of the foreground and
    /// the background, the one that stands out less on the fill, as Pele 1.12 keeps an
    /// entity's stripes light on a light fill and dark on a dark one. It says too whether it
    /// was picked on a fill. A fill an override sets leaves the shade the theme's background,
    /// since a backend lays a shade at an opacity only over a fill the author sets.
    pub fn shade(&self, author: &Style) -> (Paint, bool) {
        match ink(&self.theme, author) {
            None => (Paint::BG, false),
            Some(on) if on == Paint::FG => (Paint::BG, true),
            Some(_) => (Paint::FG, true),
        }
    }

    /// Settles a part's style in the look, into what a backend draws with.
    pub fn resolve(&self, role: &str, own: &Style, author: &Style) -> Resolved {
        let s = self.cascade(role, own, author);
        let th = &self.theme;
        let ink = |paint: Option<Paint>, opacity: Option<f64>| {
            let paint = paint.unwrap_or(Paint::NONE);
            Ink {
                paint,
                opacity: opacity.unwrap_or(1.0),
                color: at(paint.color(th), opacity),
            }
        };
        let stroke = s.stroke.unwrap_or(Paint::NONE);
        let hollow = s.ground.filter(Paint::visible).unwrap_or(Paint::BG);
        let (shade, _) = self.shade(author);
        Resolved {
            fill: ink(s.fill, s.fill_opacity),
            stroke: ink(s.stroke, s.stroke_opacity),
            text: ink(s.text, None),
            marker: ink(Some(stroke), None),
            line: ink(Some(Paint::LINE), None),
            bg: ink(Some(Paint::BG), None),
            shade: ink(Some(shade), None),
            hollow: ink(Some(hollow), None),
            halo: ink(s.halo, None),
            stroke_width: s.stroke_width.unwrap_or(th.stroke_width),
            radius: s.radius.unwrap_or(th.radius),
            dash: s.dash.unwrap_or_default(),
            cap: s.cap.unwrap_or_default(),
            join: s.join.unwrap_or_default(),
            opacity: s.opacity.unwrap_or(1.0),
            halo_width: s.halo_width.unwrap_or(0.0),
        }
    }
}

/// The theme's foreground or background, whichever contrasts more with the fill a style
/// sets, as Pele's ink picks ON_LIGHT or ON_DARK. It is none where the style sets no fill, or
/// one drawn at less than half its opacity, since what is under the fill would then decide.
fn ink(th: &Theme, set: &Style) -> Option<Paint> {
    let fill = set.fill.filter(Paint::visible)?.color(th);
    if f64::from(fill.a) / 255.0 * set.fill_opacity.unwrap_or(1.0) * set.opacity.unwrap_or(1.0)
        < 0.5
    {
        return None;
    }
    Some(pick(th, fill))
}

/// The theme's foreground or background, whichever contrasts more with a color.
fn pick(th: &Theme, fill: Color) -> Paint {
    if contrast(fill, th.fg) > contrast(fill, th.bg) {
        Paint::FG
    } else {
        Paint::BG
    }
}

/// A color's relative luminance, as WCAG 2 defines it and Pele's ink computes it.
fn luminance(c: Color) -> f64 {
    let lin = |v: u8| {
        let x = f64::from(v) / 255.0;
        if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// The contrast ratio of two colors, as WCAG 2 defines it.
fn contrast(a: Color, b: Color) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// A color at an opacity, when one is set: its alpha times the opacity, kept from 0 to 1.
fn at(mut c: Color, opacity: Option<f64>) -> Color {
    if let Some(k) = opacity.filter(|&k| k < 1.0) {
        c.a = (f64::from(c.a) * k.max(0.0) + 0.5) as u8;
    }
    c
}

/// A color as a backend is given it: settled in the theme, for a backend that draws with
/// colors, and the paint and opacity it was settled from, for one that writes a theme's
/// colors as variables, as the SVG writer does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ink {
    /// The paint the color comes from.
    pub paint: Paint,
    /// The opacity it is drawn at, apart from the paint's own: a fill's or a stroke's.
    pub opacity: f64,
    /// The color, at its opacity. Its alpha is 0 when it draws nothing.
    pub color: Color,
}

impl Ink {
    /// Whether the ink draws anything.
    pub fn visible(&self) -> bool {
        self.color.a > 0
    }
}

/// A style with its colors and measures settled, ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    /// The part's fill, at the style's fill opacity.
    pub fill: Ink,
    /// The part's strokes, at the style's stroke opacity.
    pub stroke: Ink,
    /// The part's text.
    pub text: Ink,
    /// The stroke's color without the stroke's opacity, which markers are filled with, as
    /// Pele fills them in the color of the line alone.
    pub marker: Ink,
    /// The theme's color of lines, for parts painted solid.
    pub line: Ink,
    /// The theme's background.
    pub bg: Ink,
    /// What parts painted shade are filled with, as [`Look::shade`] picks it.
    pub shade: Ink,
    /// The theme's background, or the ground, for parts painted hollow.
    pub hollow: Ink,
    /// What is drawn about the letters of the part's text.
    pub halo: Ink,
    /// The width of the part's strokes.
    pub stroke_width: f64,
    /// The radius of the corners of a rounded box.
    pub radius: f64,
    /// The lengths of the dashes and gaps of strokes, none for a solid line.
    pub dash: Vec<f64>,
    /// The ends of lines, butt when nothing set them, as SVG's initial value.
    pub cap: Cap,
    /// The corners of lines, miter when nothing set them, as SVG's initial value.
    pub join: Join,
    /// The opacity of the whole part.
    pub opacity: f64,
    /// The width of the halo.
    pub halo_width: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn own(s: &str) -> Paint {
        Paint::own(Color::hex(s))
    }

    // What leaves the cap and join unset resolves them to SVG's initial values. What sets
    // them is cascaded as the rest of a style is.
    #[test]
    fn cap_join() {
        let look = Look::default();
        let r = look.resolve("x", &Style::default(), &Style::default());
        assert_eq!((r.cap, r.join), (Cap::Butt, Join::Miter));
        let mut over = Look::default();
        over.overrides.insert(
            "x.edge".into(),
            Style {
                join: Some(Join::Bevel),
                ..Style::default()
            },
        );
        let mine = Style {
            cap: Some(Cap::Round),
            join: Some(Join::Round),
            ..Style::default()
        };
        let r = over.resolve("x.edge", &mine, &Style::default());
        assert_eq!((r.cap, r.join), (Cap::Round, Join::Bevel));
        let author = Style {
            cap: Some(Cap::Square),
            ..Style::default()
        };
        let r = over.resolve("x.edge", &mine, &author);
        assert_eq!((r.cap, r.join), (Cap::Square, Join::Bevel));
        let s = look.cascade("x", &Style::default(), &Style::default());
        assert_eq!((s.cap, s.join), (None, None));
        assert_eq!(
            (Cap::Round.to_string(), Join::Miter.to_string()),
            ("round".into(), "miter".into())
        );
    }

    // A fill and its opacity cascade each on their own, as CSS's do: an author's fill over a
    // part the diagram draws at an opacity is drawn at that opacity, and an author's opacity
    // over the diagram's fill draws that fill at it.
    #[test]
    fn opacities_cascade() {
        let look = Look::default();
        let red = Paint::own(Color::new(255, 0, 0, 255));
        let mine = Style {
            fill: Some(Paint::SURFACE_ALT),
            fill_opacity: Some(0.5),
            stroke: Some(Paint::BORDER),
            ..Style::default()
        };
        let r = look.resolve(
            "x",
            &mine,
            &Style {
                fill: Some(red),
                ..Style::default()
            },
        );
        assert_eq!(
            r.fill.color,
            Color::new(255, 0, 0, 128),
            "author's fill at the diagram's opacity"
        );
        let author = Style {
            fill_opacity: Some(0.25),
            stroke_opacity: Some(0.5),
            ..Style::default()
        };
        let r = look.resolve(
            "x",
            &Style {
                fill: Some(Paint::SURFACE),
                stroke: Some(red),
                ..Style::default()
            },
            &author,
        );
        let surface = Theme::default().surface;
        assert_eq!(
            r.fill.color,
            Color::new(surface.r, surface.g, surface.b, 64)
        );
        assert_eq!(r.stroke.color, Color::new(255, 0, 0, 128));
        assert_eq!(
            r.marker.color,
            Color::new(255, 0, 0, 255),
            "a marker is filled without the opacity"
        );
        let s = look.cascade("x", &mine, &author);
        assert_eq!(
            (s.fill_opacity, s.stroke_opacity, s.fill),
            (Some(0.25), Some(0.5), Some(Paint::SURFACE_ALT))
        );
    }

    #[test]
    fn cascade() {
        let mut look = Look::default();
        look.overrides.insert(
            "flowchart.node".into(),
            Style {
                fill: Some(Paint::ACCENT),
                stroke_width: Some(2.0),
                ..Style::default()
            },
        );
        let mine = Style {
            fill: Some(Paint::SURFACE),
            stroke: Some(Paint::BORDER),
            text: Some(Paint::FG),
            ..Style::default()
        };
        let author = Style {
            stroke: Some(own("#123")),
            ..Style::default()
        };
        let s = look.cascade("flowchart.node", &mine, &author);
        assert_eq!(
            (s.fill, s.stroke, s.text),
            (Some(Paint::ACCENT), Some(own("#123")), Some(Paint::FG))
        );
        assert_eq!(s.stroke_width, Some(2.0));
        assert_eq!(s.radius, Some(4.0), "radius from the theme");
        let other = look.cascade("flowchart.edge", &mine, &Style::default());
        assert_eq!(
            other.fill,
            Some(Paint::SURFACE),
            "an override reached another role"
        );
        let r = Look::default().resolve("x", &mine, &Style::default());
        assert_eq!((r.fill.color, r.opacity), (Theme::default().surface, 1.0));
    }

    #[test]
    fn paint() {
        let th = Theme::default();
        assert!(!Paint::NONE.visible());
        assert_eq!(Paint::series(9).color(&th), th.series[1], "series wrap");
        assert_eq!(Paint::SURFACE.alpha(0.5).alpha(0.5).color(&th).a, 64);
        assert_eq!(Paint::SURFACE.alpha(0.0), Paint::NONE, "alpha 0 is none");
        assert_eq!(
            Paint::series(2).as_token().map(Token::name),
            Some("series-3")
        );
    }

    #[test]
    fn text_defaults_to_fg() {
        let r = Look::default().resolve("x", &Style::default(), &Style::default());
        assert_eq!(r.text.color, Theme::default().fg);
        assert_eq!(r.fill.color.a, 0);
    }

    #[test]
    fn over() {
        let mut th = Theme::default();
        (th.surface_alt, th.bg) = (Color::hex("#000"), Color::hex("#fff"));
        let c = Paint::SURFACE_ALT.alpha(0.75).over(Token::Bg).color(&th);
        assert_eq!(c, Color::new(64, 64, 64, 255));
        assert_eq!(
            Paint::NONE.over(Token::Bg),
            Paint::NONE,
            "none over anything is none"
        );
    }

    #[test]
    fn ink_on_fill() {
        let mut look = Look::default();
        look.overrides.insert(
            "x.lit".into(),
            Style {
                fill: Some(own("#ffd700")),
                ..Style::default()
            },
        );
        let th = Theme::default();
        let mine = Style {
            text: Some(Paint::MUTED),
            on_fill: true,
            ..Style::default()
        };
        let fill = |s: &str| Style {
            fill: Some(own(s)),
            ..Style::default()
        };
        for (name, role, mine, author, text) in [
            ("no fill set", "x", mine.clone(), Style::default(), th.muted),
            ("a dark fill", "x", mine.clone(), fill("#000080"), th.bg),
            ("a light fill", "x", mine.clone(), fill("#ffe4e1"), th.fg),
            // #7d7d7d is just darker than where #1f1f1f and #fff contrast with it equally,
            // #7f7f7f just lighter.
            ("just dark", "x", mine.clone(), fill("#7d7d7d"), th.bg),
            ("just light", "x", mine.clone(), fill("#7f7f7f"), th.fg),
            (
                "a text color set",
                "x",
                mine.clone(),
                Style {
                    text: Some(own("#f00")),
                    ..fill("#000")
                },
                Color::hex("#f00"),
            ),
            (
                "a faint fill",
                "x",
                mine.clone(),
                Style {
                    fill_opacity: Some(0.7),
                    opacity: Some(0.7),
                    ..fill("#000")
                },
                th.muted,
            ),
            (
                "a translucent color",
                "x",
                mine.clone(),
                Style {
                    fill: Some(Paint::own(Color::new(0, 0, 0, 100))),
                    ..Style::default()
                },
                th.muted,
            ),
            (
                "not on its fill",
                "x",
                Style {
                    text: Some(Paint::MUTED),
                    ..Style::default()
                },
                fill("#000"),
                th.muted,
            ),
            (
                "an override's fill",
                "x.lit",
                mine.clone(),
                Style::default(),
                th.fg,
            ),
        ] {
            assert_eq!(
                look.resolve(role, &mine, &author).text.color,
                text,
                "{name}"
            );
        }
        // In a dark theme, the light foreground is the one that stands out on a dark fill.
        let dark = Look::new(Theme::KOHU_DARK);
        assert_eq!(
            dark.resolve("x", &mine, &fill("#000080")).text.color,
            Theme::KOHU_DARK.fg
        );
    }

    #[test]
    fn ground() {
        let look = Look::default();
        let th = Theme::default();
        let navy = own("#000080");
        let mine = Style {
            stroke: Some(Paint::LINE),
            ground: Some(navy),
            ..Style::default()
        };
        let r = look.resolve("x", &mine, &Style::default());
        assert_eq!(
            (
                r.stroke.color,
                r.marker.color,
                r.text.color,
                r.hollow.color,
                r.bg.color
            ),
            (th.bg, th.bg, th.bg, Color::hex("#000080"), th.bg),
            "on navy"
        );
        let r = look.resolve(
            "x",
            &mine,
            &Style {
                stroke: Some(own("#f00")),
                ..Style::default()
            },
        );
        assert_eq!(r.stroke.color, Color::hex("#f00"), "an author's stroke");
        let r = look.resolve(
            "x",
            &Style {
                stroke: Some(Paint::LINE),
                ..Style::default()
            },
            &Style::default(),
        );
        assert_eq!(
            (r.stroke.color, r.hollow.color),
            (th.line, th.bg),
            "no ground"
        );
    }

    // A style's max radius caps the theme's radius and no radius a level sets.
    #[test]
    fn max_radius() {
        let mine = Style {
            max_radius: Some(2.0),
            ..Style::default()
        };
        for (theme, over, want) in [(4.0, None, 2.0), (0.0, None, 0.0), (4.0, Some(6.0), 6.0)] {
            let mut look = Look::new(Theme {
                radius: theme,
                ..Theme::default()
            });
            if let Some(r) = over {
                look.overrides.insert(
                    "x".into(),
                    Style {
                        radius: Some(r),
                        ..Style::default()
                    },
                );
            }
            assert_eq!(
                look.cascade("x", &mine, &Style::default()).radius,
                Some(want),
                "{theme} {over:?}"
            );
        }
    }

    // A shade is picked on a fill the author sets, and stays the background under one an
    // override sets, which a backend lays no shade at an opacity over.
    #[test]
    fn shade_on_author_fill() {
        let navy = own("#000080");
        let mut look = Look::default();
        look.overrides.insert(
            "er.entity".into(),
            Style {
                fill: Some(navy),
                ..Style::default()
            },
        );
        assert_eq!(look.shade(&Style::default()), (Paint::BG, false));
        assert_eq!(
            look.shade(&Style {
                fill: Some(navy),
                ..Style::default()
            }),
            (Paint::FG, true)
        );
        assert_eq!(
            look.shade(&Style {
                fill: Some(own("#ffff00")),
                ..Style::default()
            }),
            (Paint::BG, true)
        );
        let r = look.resolve("er.entity", &Style::default(), &Style::default());
        assert_eq!(
            r.shade.color, look.theme.bg,
            "resolved under an override's fill"
        );
    }
}
