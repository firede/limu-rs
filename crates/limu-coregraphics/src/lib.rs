//! Draws limu's scenes with Core Graphics, into a `CGContext` the host gives or into an image,
//! and measures text with Core Text.
//!
//! The context's user space is the scene's, y down, as a flipped view's is. A host that draws
//! into a context with y up, as a bitmap context's is, flips it first, as `render` does.
//!
//! Text is shaped by Core Text in the fonts the host gives, a font for each face it has, and
//! drawn from the glyphs' outlines, so a halo is stroked as the text is filled. `Fonts`
//! measures text too, so a diagram laid out with it is drawn in the faces it was measured in.
//!
//! An item's opacity is drawn as a transparency layer, as SVG draws a group at an opacity.
//!
//! The crate is empty on platforms other than Apple's.
#![cfg(target_vendor = "apple")]

mod sys;
#[cfg(test)]
mod tests;

use limu_scene::style::{Cap, Ink, Join, Look, Resolved};
use limu_scene::text::{Face, Measurer};
use limu_scene::{
    Align, Canvas, Item, Outline, Painter, Painting, Part, Pen, Point, Rect, Run, Scene, Step,
    TextPaint,
};
use objc2_core_foundation::{CFRetained, CGAffineTransform, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGColor, CGContext, CGImage, CGLineCap, CGLineJoin, CGMutablePath};
use objc2_core_text::CTFont;

pub use {objc2_core_foundation, objc2_core_graphics, objc2_core_text};

/// A font the data of which could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontError;

impl std::fmt::Display for FontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the font could not be read")
    }
}

impl std::error::Error for FontError {}

/// A font from the data of a TrueType or OpenType file, for a host that carries its fonts
/// rather than taking the system's.
pub fn font_from_data(data: &[u8]) -> Result<CFRetained<CTFont>, FontError> {
    sys::font_from_data(data).ok_or(FontError)
}

/// The fonts text is set and drawn in: a regular one, and one for each face of
/// [`BOLD`](Face::BOLD), [`ITALIC`](Face::ITALIC) and [`MONO`](Face::MONO), or a mix of them,
/// that the host has. Text in a face it has no font for is set in the nearest it has,
/// without its italic, then without its bold, then in the regular font. The size a font was
/// made at does not matter. Text is set at the size its label asks for.
#[derive(Clone, Debug)]
pub struct Fonts {
    // The fonts, by the bits of the face they set.
    fonts: [Option<CFRetained<CTFont>>; 8],
}

impl Fonts {
    /// Fonts of one regular font.
    pub fn new(regular: CFRetained<CTFont>) -> Fonts {
        let mut f = Fonts {
            fonts: Default::default(),
        };
        f.set(Face::REGULAR, regular);
        f
    }

    /// Sets the font of a face of bold, italic and mono. The face's other bits are ignored.
    pub fn set(&mut self, face: Face, font: CFRetained<CTFont>) -> &mut Fonts {
        self.fonts[slot(face)] = Some(font);
        self
    }

    /// The font text in a face is set in, at a size.
    fn font(&self, face: Face, size: f64) -> CFRetained<CTFont> {
        let tries = [
            face,
            face.without(Face::ITALIC),
            face.without(Face::ITALIC | Face::BOLD),
            Face::REGULAR,
        ];
        let font = tries
            .iter()
            .find_map(|&f| self.fonts[slot(f)].as_deref())
            .expect("a regular font");
        sys::sized(font, size)
    }
}

/// Where the font of a face is kept.
fn slot(face: Face) -> usize {
    usize::from((face & (Face::BOLD | Face::ITALIC | Face::MONO)).bits())
}

impl Measurer for Fonts {
    fn width(&self, s: &str, size: f64, f: Face) -> f64 {
        sys::advance(&sys::line(s, &self.font(f, size)))
    }
}

/// Gives the parts of an icon by its name, on a box of 24 by 24, or none for an icon it does
/// not have. A part filled is filled in the color of the item's text, and one stroked is
/// stroked in it 2 wide with round ends and corners, as the SVG writer's icons are.
pub type Icons<'a> = Box<dyn Fn(&str) -> Option<Vec<Part>> + 'a>;

/// Gives an image by its address, or none for one it can't load.
pub type Images<'a> = Box<dyn Fn(&str) -> Option<CFRetained<CGImage>> + 'a>;

/// How a scene is drawn.
#[derive(Default)]
pub struct Options<'a> {
    /// The look the scene is drawn in.
    pub look: Look,
    /// The fonts text is drawn in. Without them, text is left out.
    pub fonts: Option<&'a Fonts>,
    /// How many pixels a unit of the scene takes in [`render`]. Zero or less is 1.
    pub scale: f64,
    /// The icons. Without them, icons are left out.
    pub icons: Option<Icons<'a>>,
    /// The images. Without them, images are left out.
    pub images: Option<Images<'a>>,
}

/// Draws a scene into an image of its size at the options' scale, on the theme's background,
/// in sRGB. It is none for a scene too small or too large for an image.
pub fn render(scene: &Scene, o: &Options<'_>) -> Option<CFRetained<CGImage>> {
    let cx = bitmap(scene, o)?;
    objc2_core_graphics::CGBitmapContextCreateImage(Some(&cx))
}

/// Draws a scene into a bitmap context of its size at the options' scale, on the theme's
/// background.
fn bitmap(scene: &Scene, o: &Options<'_>) -> Option<CFRetained<CGContext>> {
    let k = if o.scale > 0.0 { o.scale } else { 1.0 };
    let w = (scene.width * k).ceil();
    let h = (scene.height * k).ceil();
    if !(w >= 1.0 && h >= 1.0 && w <= f64::from(u16::MAX) && h <= f64::from(u16::MAX)) {
        return None;
    }
    let cx = sys::bitmap(w as usize, h as usize)?;
    CGContext::set_fill_color_with_color(Some(&cx), Some(&color(o.look.theme.bg)));
    CGContext::fill_rect(
        Some(&cx),
        CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(w, h)),
    );
    CGContext::translate_ctm(Some(&cx), 0.0, h);
    CGContext::scale_ctm(Some(&cx), k, -k);
    draw(scene, o, &cx);
    Some(cx)
}

/// Draws a scene into a context, its user space the scene's, y down.
pub fn draw(scene: &Scene, o: &Options<'_>, cx: &CGContext) {
    draw_with(scene, o, cx, |p, it, s| p.item(it, s));
}

/// Draws a scene into a context, each item by a hook, as
/// [`draw_with`](limu_scene::draw_with) draws it. The hook draws on its own with the
/// painter's canvas, a [`ContextCanvas`].
pub fn draw_with<F>(scene: &Scene, o: &Options<'_>, cx: &CGContext, hook: F)
where
    F: FnMut(&mut Painter<'_, ContextCanvas<'_>>, &Item, &Resolved),
{
    let mut c = ContextCanvas { cx, o };
    CGContext::save_g_state(Some(cx));
    limu_scene::draw_with(scene, &o.look, &mut c, hook);
    CGContext::restore_g_state(Some(cx));
}

/// A context drawn on, the canvas of [`draw_with`]. Each layer and turn saves the context's
/// state when it begins and restores it when it ends.
pub struct ContextCanvas<'a> {
    cx: &'a CGContext,
    o: &'a Options<'a>,
}

impl ContextCanvas<'_> {
    /// The context being drawn on.
    pub fn context(&self) -> &CGContext {
        self.cx
    }

    fn fill(&self, path: &CGMutablePath, ink: &Ink) {
        let cx = Some(self.cx);
        CGContext::set_fill_color_with_color(cx, Some(&color(ink.color)));
        CGContext::add_path(cx, Some(path));
        CGContext::fill_path(cx);
    }

    fn stroke(
        &self,
        path: &CGMutablePath,
        ink: &Ink,
        width: f64,
        cap: Cap,
        join: Join,
        dash: &[f64],
    ) {
        let cx = Some(self.cx);
        CGContext::set_stroke_color_with_color(cx, Some(&color(ink.color)));
        CGContext::set_line_width(cx, width);
        CGContext::set_line_cap(
            cx,
            match cap {
                Cap::Butt => CGLineCap::Butt,
                Cap::Round => CGLineCap::Round,
                Cap::Square => CGLineCap::Square,
            },
        );
        CGContext::set_line_join(
            cx,
            match join {
                Join::Miter => CGLineJoin::Miter,
                Join::Round => CGLineJoin::Round,
                Join::Bevel => CGLineJoin::Bevel,
            },
        );
        CGContext::set_miter_limit(cx, limu_scene::MITER_LIMIT);
        sys::set_dash(self.cx, &dash_of(dash));
        CGContext::add_path(cx, Some(path));
        CGContext::stroke_path(cx);
    }

    /// Adds the outlines of a run of text to a path.
    fn outline_run(&self, fonts: &Fonts, run: &Run<'_>, path: &CGMutablePath) {
        let lines: Vec<_> = run
            .spans
            .iter()
            .filter(|s| !s.text.is_empty())
            .map(|s| {
                let font = fonts.font(s.face, s.face.size_at(run.size));
                let line = sys::line(&s.text, &font);
                (s, font, line)
            })
            .collect();
        let width: f64 = lines.iter().map(|(_, _, l)| sys::advance(l)).sum();
        let mut x = match run.anchor {
            Align::Start => run.at.x,
            Align::Middle => run.at.x - width / 2.0,
            Align::End => run.at.x - width,
        };
        for (s, font, line) in &lines {
            // Lowered a fifth or raised a third of the label's size, about where a browser
            // puts SVG's sub and super baseline shifts, as limu-tiny-skia does.
            let y = run.at.y
                + if s.face.contains(Face::SUB) {
                    run.size / 5.0
                } else if s.face.contains(Face::SUP) {
                    -run.size / 3.0
                } else {
                    0.0
                };
            for (f, glyph, at) in sys::glyphs(line) {
                // The glyph's outline is y up. It is turned over onto the baseline.
                let t = CGAffineTransform {
                    a: 1.0,
                    b: 0.0,
                    c: 0.0,
                    d: -1.0,
                    tx: x + at.x,
                    ty: y - at.y,
                };
                sys::add_glyph(path, &f, glyph, &t);
            }
            let w = sys::advance(line);
            if s.face.contains(Face::UNDERLINE) {
                let (top, thick) = sys::underline(font);
                add_rect(path, Rect::new(x, y + top, w, thick.max(0.0)));
            }
            x += w;
        }
    }
}

impl Canvas for ContextCanvas<'_> {
    fn has_layers(&self) -> bool {
        true
    }

    fn begin_layer(&mut self, opacity: f64) {
        CGContext::save_g_state(Some(self.cx));
        CGContext::set_alpha(Some(self.cx), opacity.clamp(0.0, 1.0));
        sys::begin_layer(self.cx);
    }

    fn end_layer(&mut self) {
        CGContext::end_transparency_layer(Some(self.cx));
        CGContext::restore_g_state(Some(self.cx));
    }

    fn begin_rotate(&mut self, degrees: f64, about: Point) {
        let cx = Some(self.cx);
        CGContext::save_g_state(cx);
        CGContext::translate_ctm(cx, about.x, about.y);
        CGContext::rotate_ctm(cx, degrees.to_radians());
        CGContext::translate_ctm(cx, -about.x, -about.y);
    }

    fn end_rotate(&mut self) {
        CGContext::restore_g_state(Some(self.cx));
    }

    fn path(&mut self, path: &limu_scene::Path, fill: Option<&Ink>, pen: Option<&Pen<'_>>) {
        let p = convert(path);
        if let Some(ink) = fill {
            self.fill(&p, ink);
        }
        if let Some(pen) = pen {
            self.stroke(&p, &pen.ink, pen.width, pen.cap, pen.join, pen.dash);
        }
    }

    fn text(&mut self, run: &Run<'_>) {
        let Some(fonts) = self.o.fonts else { return };
        let p = CGMutablePath::new();
        self.outline_run(fonts, run, &p);
        match &run.paint {
            TextPaint::Fill(ink) => self.fill(&p, ink),
            TextPaint::Halo { ink, width } => {
                self.stroke(&p, ink, *width, Cap::Butt, Join::Round, &[]);
            }
        }
    }

    fn icon(&mut self, name: &str, bounds: Rect, ink: &Ink) {
        let Some(parts) = self.o.icons.as_ref().and_then(|f| f(name)) else {
            return;
        };
        let cx = Some(self.cx);
        CGContext::save_g_state(cx);
        CGContext::translate_ctm(cx, bounds.x, bounds.y);
        CGContext::scale_ctm(cx, bounds.w / 24.0, bounds.h / 24.0);
        for part in &parts {
            let p = match &part.outline {
                Outline::Path(p) => convert(p),
                Outline::Box { rect, .. } => {
                    let p = CGMutablePath::new();
                    add_rect(&p, *rect);
                    p
                }
            };
            if part.paint != Painting::StrokeOnly {
                self.fill(&p, ink);
            }
            if matches!(part.paint, Painting::StrokeOnly | Painting::FillAndStroke) {
                self.stroke(&p, ink, 2.0, Cap::Round, Join::Round, &[]);
            }
        }
        CGContext::restore_g_state(cx);
    }

    fn image(&mut self, src: &str, bounds: Rect, fit: bool, opacity: f64) {
        let Some(img) = self.o.images.as_ref().and_then(|f| f(src)) else {
            return;
        };
        let iw = CGImage::width(Some(&img)) as f64;
        let ih = CGImage::height(Some(&img)) as f64;
        if !(iw > 0.0 && ih > 0.0) {
            return;
        }
        let (mut w, mut h) = (bounds.w, bounds.h);
        let (mut x, mut y) = (bounds.x, bounds.y);
        if fit {
            let s = (bounds.w / iw).min(bounds.h / ih);
            (w, h) = (iw * s, ih * s);
            x += (bounds.w - w) / 2.0;
            y += (bounds.h - h) / 2.0;
        }
        let cx = Some(self.cx);
        CGContext::save_g_state(cx);
        CGContext::set_alpha(cx, opacity.clamp(0.0, 1.0));
        // An image is drawn y up, so it is turned over in its box.
        CGContext::translate_ctm(cx, x, y + h);
        CGContext::scale_ctm(cx, 1.0, -1.0);
        CGContext::draw_image(
            cx,
            CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(w, h)),
            Some(&img),
        );
        CGContext::restore_g_state(cx);
    }
}

/// A scene's path as Core Graphics's.
fn convert(path: &limu_scene::Path) -> CFRetained<CGMutablePath> {
    let p = CGMutablePath::new();
    for step in path.steps() {
        match step {
            Step::MoveTo(a) => sys::move_to(&p, a.x, a.y),
            Step::LineTo(a) => sys::line_to(&p, a.x, a.y),
            Step::QuadTo(c, a) => sys::quad_to(&p, c.x, c.y, a.x, a.y),
            Step::CubeTo(c1, c2, a) => sys::cube_to(&p, c1.x, c1.y, c2.x, c2.y, a.x, a.y),
            Step::Close => CGMutablePath::close_subpath(Some(&p)),
        }
    }
    p
}

fn add_rect(p: &CGMutablePath, r: Rect) {
    sys::move_to(p, r.x, r.y);
    sys::line_to(p, r.x + r.w, r.y);
    sys::line_to(p, r.x + r.w, r.y + r.h);
    sys::line_to(p, r.x, r.y + r.h);
    CGMutablePath::close_subpath(Some(p));
}

fn color(c: limu_scene::style::Color) -> CFRetained<CGColor> {
    let k = |v: u8| f64::from(v) / 255.0;
    CGColor::new_srgb(k(c.r), k(c.g), k(c.b), k(c.a))
}

/// A dash as Core Graphics strokes it. A dash of an odd number of lengths is repeated, as
/// SVG's is, and one SVG draws solid, of a negative length or of none but zeros, is solid.
fn dash_of(dash: &[f64]) -> Vec<f64> {
    if dash.iter().any(|&d| !(d >= 0.0)) || dash.iter().all(|&d| d == 0.0) {
        return Vec::new();
    }
    let mut d = dash.to_vec();
    if d.len() % 2 == 1 {
        d.extend_from_within(..);
    }
    d
}
