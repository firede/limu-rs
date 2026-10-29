//! A scene written as SVG.
//!
//! Colors of the theme are written as CSS variables, `--limu-surface` and the rest, with the
//! theme's colors as their fallbacks, so a page restyles a diagram without drawing it again.
//! With [`Options::literal`] they are written as colors.

use std::fmt::Write as _;

use crate::draw::{Canvas, Painter, Pen, Run, TextPaint, draw_with};
use crate::geom::{Point, Rect};
use crate::path::{Path, Step};
use crate::scene::{Align, Item, Scene};
use crate::style::{Cap, Ink, Join, Look, Paint, Resolved, Source, Theme, Token};
use crate::text::Face;

/// How a scene is written.
#[derive(Default)]
pub struct Options<'a> {
    /// The look the scene is drawn in.
    pub look: Look,
    /// Whether the theme's colors are written as colors rather than as variables.
    pub literal: bool,
    /// The font the scene was measured in, "sans-serif" when it is none. The page's
    /// `--limu-font` comes first when it sets one.
    pub font_family: Option<String>,
    /// The monospace font the scene was measured in, "monospace" when it is none. The page's
    /// `--limu-font-mono` comes first when it sets one.
    pub mono_family: Option<String>,
    /// The schemes a link may use besides a relative address. None is http, https, mailto
    /// and tel. A link of another is drawn as no link.
    pub link_schemes: Option<Vec<String>>,
    /// The schemes an image may be loaded with besides a relative address. None is http and
    /// https.
    pub image_schemes: Option<Vec<String>>,
    /// The markup inside an icon's `<svg>`, on a box of 24 by 24, by the icon's name, or none
    /// for an icon it does not have. The icon's `<svg>` sets its color to that of the item's
    /// text, for the markup to draw in as currentColor. Without it, icons are left out.
    pub icon_markup: Option<IconMarkup<'a>>,
}

/// Gives the markup of an icon by its name, or none for an icon it does not have.
pub type IconMarkup<'a> = Box<dyn Fn(&str) -> Option<String> + 'a>;

/// Writes a scene as an SVG document.
pub fn render(scene: &Scene, o: &Options<'_>) -> String {
    render_with(scene, o, |p, it, s| p.item(it, s))
}

/// Writes a scene as an SVG document, each item by a hook, as [`draw_with`] draws it. The
/// hook writes markup of its own with the painter's canvas, a [`Writer`].
pub fn render_with<F>(scene: &Scene, o: &Options<'_>, hook: F) -> String
where
    F: FnMut(&mut Painter<'_, Writer<'_>>, &Item, &Resolved),
{
    let mut w = Writer {
        b: String::with_capacity(256 + 160 * scene.items.len()),
        o,
        font_size: scene.font_size,
    };
    draw_with(scene, &o.look, &mut w, hook);
    w.b
}

/// The document being written, the canvas of the SVG writer.
pub struct Writer<'a> {
    b: String,
    o: &'a Options<'a>,
    font_size: f64,
}

impl Writer<'_> {
    /// Writes markup as it is.
    pub fn raw(&mut self, s: &str) {
        self.b.push_str(s);
    }

    /// Writes text, escaped for an attribute or an element.
    pub fn escaped(&mut self, s: &str) {
        esc(&mut self.b, s);
    }

    /// Writes a number to two decimals, with no more digits than it needs.
    pub fn num(&mut self, f: f64) {
        num(&mut self.b, f);
    }

    fn theme(&self) -> &Theme {
        &self.o.look.theme
    }

    /// Writes a paint as an opaque color: a variable of the theme's, a color, or none. The
    /// rest of its opacity is what [`opacity`](Writer::opacity) gives.
    pub fn paint(&mut self, p: Paint) {
        let token = p.as_token();
        if !p.visible() {
            self.b.push_str("none");
        } else if token.is_none() || self.o.literal {
            // A color of its own, or one mixed with the theme's, is settled here, as are all
            // of them when they are written as colors.
            let mut c = match (p.base(), p.source()) {
                (Some(_), _) => p.color(self.theme()),
                (None, Source::Own(c)) => c,
                (None, Source::Theme(t)) => self.theme().color(t),
                (None, Source::None) => unreachable!(),
            };
            c.a = 255;
            let _ = write!(self.b, "{c}");
        } else if let (Some(t), Some(base)) = (token, p.base()) {
            // The paint's opacity is its share of the mix.
            self.b.push_str("color-mix(in srgb,");
            self.variable(t);
            self.b.push(' ');
            self.num(100.0 * f64::from(p.opacity()) * f64::from(self.theme().color(t).a) / 255.0);
            self.b.push_str("%,");
            self.variable(base);
            self.b.push(')');
        } else if let Some(t) = token {
            self.variable(t);
        }
    }

    /// The part of a paint's opacity that [`paint`](Writer::paint) does not write: the part
    /// of its opacity it is drawn at times the alpha of its color, its own or the theme's. A
    /// paint laid over a color of the theme is opaque, as the mix is written. One that draws
    /// nothing is 0.
    pub fn opacity(&self, p: Paint) -> f64 {
        let c = match p.source() {
            _ if p.base().is_some() => return 1.0,
            Source::None => return 0.0,
            Source::Own(c) => c,
            Source::Theme(t) => self.theme().color(t),
        };
        f64::from(p.opacity()) * f64::from(c.a) / 255.0
    }

    /// Writes a token of the theme as a CSS variable, its color in the theme the fallback.
    fn variable(&mut self, t: Token) {
        let mut c = self.theme().color(t);
        c.a = 255;
        let _ = write!(self.b, "var(--limu-{},{c})", t.name());
    }

    /// Writes an attribute of an ink, name="…" as [`paint`](Writer::paint) writes its paint,
    /// and name-opacity with its opacity when that is under 1: for "fill", fill="…"
    /// fill-opacity="…".
    pub fn paint_attr(&mut self, name: &str, ink: &Ink) {
        let _ = write!(self.b, " {name}=\"");
        self.paint(ink.paint);
        self.b.push('"');
        let a = self.opacity(ink.paint) * ink.opacity.clamp(0.0, 1.0);
        if ink.paint.visible() && a < 1.0 {
            let _ = write!(self.b, " {name}-opacity=\"");
            self.num(a);
            self.b.push('"');
        }
    }

    /// Writes a path's steps, as the d of a `<path>`.
    pub fn path_data(&mut self, p: &Path) {
        for step in p.steps() {
            let (op, points): (char, &[Point]) = match &step {
                Step::MoveTo(a) => ('M', std::slice::from_ref(a)),
                Step::LineTo(a) => ('L', std::slice::from_ref(a)),
                Step::QuadTo(a, b) => ('Q', &[*a, *b]),
                Step::CubeTo(a, b, c) => ('C', &[*a, *b, *c]),
                Step::Close => ('Z', &[]),
            };
            self.b.push(op);
            for (i, pt) in points.iter().enumerate() {
                if i > 0 {
                    self.b.push(',');
                }
                self.num(pt.x);
                self.b.push(',');
                self.num(pt.y);
            }
        }
    }

    fn family(&self, mono: bool) -> String {
        if mono {
            format!(
                "var(--limu-font-mono,{})",
                self.o.mono_family.as_deref().unwrap_or("monospace")
            )
        } else {
            format!(
                "var(--limu-font,{})",
                self.o.font_family.as_deref().unwrap_or("sans-serif")
            )
        }
    }

    /// Writes the attributes x, y, width and height of a box.
    fn bounds(&mut self, r: Rect) {
        for (name, v) in [("x", r.x), ("y", r.y), ("width", r.w), ("height", r.h)] {
            let _ = write!(self.b, " {name}=\"");
            self.num(v);
            self.b.push('"');
        }
    }

    /// Writes how a shape is filled and stroked, and closes its element.
    fn paints(&mut self, fill: Option<&Ink>, pen: Option<&Pen<'_>>) {
        self.paint_attr("fill", fill.unwrap_or(&Ink::NONE));
        if let Some(pen) = pen {
            self.paint_attr("stroke", &pen.ink);
            if pen.width != 1.0 {
                self.b.push_str(" stroke-width=\"");
                self.num(pen.width);
                self.b.push('"');
            }
            if !pen.dash.is_empty() {
                self.b.push_str(" stroke-dasharray=\"");
                for (i, &f) in pen.dash.iter().enumerate() {
                    if i > 0 {
                        self.b.push(' ');
                    }
                    self.num(f);
                }
                self.b.push('"');
            }
            if pen.cap != Cap::Butt {
                let _ = write!(self.b, " stroke-linecap=\"{}\"", pen.cap);
            }
            if pen.join != Join::Miter {
                let _ = write!(self.b, " stroke-linejoin=\"{}\"", pen.join);
            }
        }
        self.b.push_str("/>");
    }
}

impl Canvas for Writer<'_> {
    fn begin_scene(&mut self, sc: &Scene) {
        self.b
            .push_str(r#"<svg xmlns="http://www.w3.org/2000/svg" class="limu limu-"#);
        self.escaped(&sc.diagram);
        self.b.push_str("\" viewBox=\"0 0 ");
        self.num(sc.width);
        self.b.push(' ');
        self.num(sc.height);
        self.b.push_str("\" width=\"");
        self.num(sc.width);
        self.b.push_str("\" height=\"");
        self.num(sc.height);
        self.b.push_str("\" font-size=\"");
        self.num(sc.font_size);
        self.b.push_str("\" style=\"font-family:");
        let family = self.family(false);
        self.escaped(&family);
        self.b.push_str("\" fill=\"");
        self.paint(Paint::FG);
        self.b
            .push_str(r#"" role="graphics-document document" aria-roledescription=""#);
        self.escaped(&sc.diagram);
        self.b.push_str("\">");
        for (tag, s) in [("title", &sc.title), ("desc", &sc.desc)] {
            if let Some(s) = s.as_deref().filter(|s| !s.is_empty()) {
                let _ = write!(self.b, "<{tag}>");
                self.escaped(s);
                let _ = write!(self.b, "</{tag}>");
            }
        }
    }

    fn end_scene(&mut self, _: &Scene) {
        self.b.push_str("</svg>");
    }

    /// Begins a group of the item's classes, as Pele writes one: its kind and its role, as
    /// "limu-node limu-flowchart-node", its shape, its tags and then its classes, with its
    /// id, its tooltip and its link.
    fn begin_item(&mut self, it: &Item) {
        let _ = write!(self.b, "<g class=\"limu-{}", it.kind.name());
        if !it.role.as_str().is_empty() {
            self.b.push_str(" limu-");
            self.escaped(&it.role.as_str().replace('.', "-"));
        }
        if let Some(shape) = &it.shape {
            self.b.push_str(" limu-shape-");
            self.escaped(shape.as_str());
        }
        for c in it.tags.iter().chain(&it.class) {
            self.b.push(' ');
            self.escaped(c);
        }
        self.b.push('"');
        if let Some(id) = &it.id {
            self.b.push_str(" data-id=\"");
            self.escaped(id);
            self.b.push('"');
        }
        self.b.push('>');
        if let Some(t) = it.tooltip.as_deref().filter(|t| !t.is_empty()) {
            self.b.push_str("<title>");
            self.escaped(t);
            self.b.push_str("</title>");
        }
        if let Some(link) = self.link(it) {
            self.b.push_str("<a href=\"");
            self.escaped(&link);
            self.b.push_str("\">");
        }
    }

    fn end_item(&mut self, it: &Item) {
        if self.link(it).is_some() {
            self.b.push_str("</a>");
        }
        self.b.push_str("</g>");
    }

    fn has_layers(&self) -> bool {
        true
    }

    fn begin_layer(&mut self, opacity: f64) {
        self.b.push_str("<g opacity=\"");
        self.num(opacity);
        self.b.push_str("\">");
    }

    fn end_layer(&mut self) {
        self.b.push_str("</g>");
    }

    fn begin_rotate(&mut self, degrees: f64, about: Point) {
        self.b.push_str("<g transform=\"rotate(");
        self.num(degrees);
        self.b.push(' ');
        self.num(about.x);
        self.b.push(' ');
        self.num(about.y);
        self.b.push_str(")\">");
    }

    fn end_rotate(&mut self) {
        self.b.push_str("</g>");
    }

    fn path(&mut self, path: &Path, fill: Option<&Ink>, pen: Option<&Pen<'_>>) {
        self.b.push_str("<path d=\"");
        self.path_data(path);
        self.b.push('"');
        self.paints(fill, pen);
    }

    /// Writes a `<rect>`. SVG takes a radius too large for a box as half its width across
    /// and half its height down, so that a narrow box's corners are elliptical, where the
    /// default draws them round.
    fn rect(&mut self, rect: Rect, radius: f64, fill: Option<&Ink>, pen: Option<&Pen<'_>>) {
        self.b.push_str("<rect");
        self.bounds(rect);
        if radius > 0.0 {
            self.b.push_str(" rx=\"");
            self.num(radius);
            self.b.push('"');
        }
        self.paints(fill, pen);
    }

    fn marker(&mut self, path: &Path, fill: Option<&Ink>, pen: Option<&Pen<'_>>) {
        self.b.push_str("<path class=\"limu-marker\" d=\"");
        self.path_data(path);
        self.b.push('"');
        self.paints(fill, pen);
    }

    /// Writes a `<text>` of the run. Its spans after the first follow it as the browser sets
    /// them, each in its face: underlined, and lowered or raised and smaller, as Pele's
    /// spanStyle writes them.
    fn text(&mut self, run: &Run<'_>) {
        let anchor = match run.anchor {
            Align::Middle => "middle",
            Align::Start => "start",
            Align::End => "end",
        };
        let _ = write!(
            self.b,
            "<text text-anchor=\"{anchor}\" xml:space=\"preserve\""
        );
        match &run.paint {
            TextPaint::Halo { ink, width } => {
                self.b.push_str(" fill=\"none\"");
                self.paint_attr("stroke", ink);
                self.b.push_str(" stroke-width=\"");
                self.num(*width);
                self.b.push_str("\" stroke-linejoin=\"round\"");
            }
            // The document's fill is the theme's text.
            TextPaint::Fill(ink) if ink.paint == Paint::FG && ink.opacity >= 1.0 => {}
            TextPaint::Fill(ink) => self.paint_attr("fill", ink),
        }
        if run.size != self.font_size {
            self.b.push_str(" font-size=\"");
            self.num(run.size);
            self.b.push('"');
        }
        self.b.push('>');
        let mut first = true;
        for s in run.spans.iter().filter(|s| !s.text.is_empty()) {
            self.b.push_str("<tspan");
            if first {
                self.b.push_str(" x=\"");
                self.num(run.at.x);
                self.b.push_str("\" y=\"");
                self.num(run.at.y);
                self.b.push('"');
                first = false;
            }
            let f = s.face;
            if f.contains(Face::BOLD) {
                self.b.push_str(" font-weight=\"bold\"");
            }
            if f.contains(Face::ITALIC) {
                self.b.push_str(" font-style=\"italic\"");
            }
            if f.contains(Face::MONO) {
                self.b.push_str(" font-family=\"");
                let family = self.family(true);
                self.escaped(&family);
                self.b.push('"');
            }
            if f.contains(Face::UNDERLINE) {
                self.b.push_str(" text-decoration=\"underline\"");
            }
            if f.contains(Face::SUB) {
                self.b
                    .push_str(" baseline-shift=\"sub\" font-size=\"smaller\"");
            } else if f.contains(Face::SUP) {
                self.b
                    .push_str(" baseline-shift=\"super\" font-size=\"smaller\"");
            }
            self.b.push('>');
            self.escaped(&s.text);
            self.b.push_str("</tspan>");
        }
        self.b.push_str("</text>");
    }

    /// Writes an icon through [`Options::icon_markup`] as an `<svg>` in its box, its color
    /// the ink for its markup's currentColor, at the ink's opacity.
    fn icon(&mut self, name: &str, bounds: Rect, ink: &Ink) {
        let Some(markup) = self.o.icon_markup.as_ref().and_then(|f| f(name)) else {
            return;
        };
        self.b.push_str("<svg class=\"limu-icon\" data-icon=\"");
        self.escaped(name);
        self.b.push('"');
        self.bounds(bounds);
        self.b.push_str(" color=\"");
        self.paint(ink.paint);
        self.b.push('"');
        let a = self.opacity(ink.paint) * ink.opacity.clamp(0.0, 1.0);
        if a < 1.0 {
            self.b.push_str(" opacity=\"");
            self.num(a);
            self.b.push('"');
        }
        self.b.push_str(r#" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">"#);
        self.b.push_str(&markup);
        self.b.push_str("</svg>");
    }

    /// Writes an `<image>`, if the options allow its address. Its opacity is the layer's.
    fn image(&mut self, src: &str, bounds: Rect, fit: bool, _: f64) {
        let Some(href) = safe_url(src, self.o.image_schemes.as_deref(), &["http", "https"]) else {
            return;
        };
        self.b.push_str("<image href=\"");
        self.escaped(href);
        self.b.push('"');
        self.bounds(bounds);
        let aspect = if fit { "xMidYMid meet" } else { "none" };
        let _ = write!(self.b, " preserveAspectRatio=\"{aspect}\"/>");
    }
}

impl Writer<'_> {
    /// The item's link, if it is relative or of a scheme the options allow. The diagrams
    /// sanitize what their authors write as Mermaid does, and this is the last check.
    fn link(&self, it: &Item) -> Option<String> {
        let link = it.link.as_deref()?;
        safe_url(
            link,
            self.o.link_schemes.as_deref(),
            &["http", "https", "mailto", "tel"],
        )
        .map(str::to_string)
    }
}

/// An address, trimmed, if it is relative or of one of the schemes, or of the defaults when
/// there are none.
fn safe_url<'a>(link: &'a str, schemes: Option<&[String]>, defaults: &[&str]) -> Option<&'a str> {
    let link = link.trim();
    if link
        .chars()
        .any(|c| c < '\u{20}' || ('\u{7f}'..='\u{9f}').contains(&c))
        || link.is_empty()
    {
        return None;
    }
    match link.find([':', '/', '?', '#']) {
        Some(end) if end > 0 && link.as_bytes()[end] == b':' => {
            let scheme = link[..end].to_ascii_lowercase();
            let ok = match schemes {
                Some(s) => s.contains(&scheme),
                None => defaults.contains(&scheme.as_str()),
            };
            ok.then_some(link)
        }
        _ if link.starts_with("//") || link.starts_with("\\\\") => None,
        _ => Some(link),
    }
}

/// Math.round, which rounds halves up.
fn js_round(x: f64) -> f64 {
    let f = x.floor();
    if x - f >= 0.5 { f + 1.0 } else { f }
}

/// Writes a number rounded to hundredths, as Pele's num does: 12, 12.5, 12.05. NaN and the
/// infinities, which Pele writes as words SVG does not read, are written as 0.
fn num(b: &mut String, f: f64) {
    if !f.is_finite() {
        b.push('0');
        return;
    }
    let n = js_round(f * 100.0);
    if n == 0.0 {
        b.push('0');
    } else if n.abs() >= 1e11 {
        // Too large to count in hundredths as an integer, so written as it is.
        let _ = if n.is_finite() {
            write!(b, "{}", n / 100.0)
        } else {
            write!(b, "{f}")
        };
    } else {
        let i = n as i64;
        if i < 0 {
            b.push('-');
        }
        let (whole, frac) = (i.unsigned_abs() / 100, i.unsigned_abs() % 100);
        let _ = match frac {
            0 => write!(b, "{whole}"),
            _ if frac % 10 == 0 => write!(b, "{whole}.{}", frac / 10),
            _ => write!(b, "{whole}.{frac:02}"),
        };
    }
}

/// Writes text escaped for XML, with what XML 1.0 does not allow left out: the control
/// characters but tab, line feed and carriage return, DEL, and U+FFFE and U+FFFF.
fn esc(b: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => b.push_str("&amp;"),
            '<' => b.push_str("&lt;"),
            '>' => b.push_str("&gt;"),
            '"' => b.push_str("&quot;"),
            '\t' | '\n' | '\r' => b.push(c),
            '\u{0}'..='\u{1f}' | '\u{7f}' | '\u{fffe}' | '\u{ffff}' => {}
            _ => b.push(c),
        }
    }
}

#[cfg(test)]
mod tests;
