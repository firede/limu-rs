//! Draws limu's scenes in [GPUI](https://www.gpui.rs) windows, and measures text with GPUI's
//! text system.
//!
//! A scene is painted in an element's paint phase, its origin at a point of the window and each
//! of its units a number of pixels. Paths are tessellated by GPUI, filled by the nonzero rule
//! and stroked with SVG's caps, joins and mitre limit. Text is shaped by GPUI's text system in
//! the families the host names, and painted glyph by glyph on the run's baseline, so a diagram
//! measured with [`TextMeasurer`] is drawn in the faces it was measured in.
//!
//! GPUI's public API paints a glyph only upright and only filled, as Zed's GPUI does, and
//! GPUI Kit's charts keep their text upright. A label limu turns, such as an
//! axis title or a swimlane's name, and a halo are written as SVG text instead, which GPUI's
//! SVG renderer draws as a mask and turns by a matrix, as GPUI turns its icons. That text is
//! shaped by usvg, in the system's fonts and the fonts the app bundles as assets, so a family
//! the host only adds to GPUI's text system is not found there. Should GPUI come to paint
//! glyphs turned, this text can be painted as the rest is.
//!
//! GPUI has no layers drawn apart. An item at an opacity has the opacity laid on each of its
//! colors, as the default drawing does for a canvas without layers, and an image is drawn
//! opaque, as GPUI draws images.
//!
//! On macOS, GPUI sets text only with the `font-kit` feature of gpui-fast-platform. Without
//! it every glyph is missing.

mod svg;
#[cfg(test)]
mod tests;

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use gpui::{
    App, Bounds, Corners, Font, FontStyle, FontWeight, Hsla, PathBuilder, PathStyle, Pixels,
    RenderImage, Rgba, SharedString, TextRun, TransformationMatrix, Window, WindowTextSystem,
    point, px, radians, size,
};
use limu_scene::style::{Cap, Ink, Join, Look, Resolved};
use limu_scene::text::{Face, Measurer};
use limu_scene::{
    Align, Canvas, Item, Outline, Painter, Painting, Part, Pen, Point, Rect, Run, Scene, Step,
    TextPaint,
};

/// The families text is set in: one for text, and one for monospaced text. Bold and italic
/// text is set in the family's bold and italic faces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fonts {
    /// The family of text.
    pub sans: SharedString,
    /// The family of monospaced text.
    pub mono: SharedString,
}

impl Default for Fonts {
    /// Arial and Courier New, the faces limu measures in by default.
    fn default() -> Fonts {
        Fonts {
            sans: "Arial".into(),
            mono: "Courier New".into(),
        }
    }
}

impl Fonts {
    /// The font of a face.
    fn font(&self, face: Face) -> Font {
        let family = if face.contains(Face::MONO) {
            &self.mono
        } else {
            &self.sans
        };
        let mut f = gpui::font(family.clone());
        if face.contains(Face::BOLD) {
            f.weight = FontWeight::BOLD;
        }
        if face.contains(Face::ITALIC) {
            f.style = FontStyle::Italic;
        }
        f
    }
}

/// Measures text as a window's text system sets it in the fonts, so that a scene laid out
/// with it is drawn in the faces it was measured in.
#[derive(Clone)]
pub struct TextMeasurer {
    system: Arc<WindowTextSystem>,
    fonts: Fonts,
}

impl TextMeasurer {
    /// A measurer of a window's text system and fonts.
    pub fn new(window: &Window, fonts: Fonts) -> TextMeasurer {
        TextMeasurer {
            system: window.text_system().clone(),
            fonts,
        }
    }
}

impl Measurer for TextMeasurer {
    fn width(&self, s: &str, size: f64, f: Face) -> f64 {
        let run = TextRun {
            len: s.len(),
            font: self.fonts.font(f),
            ..TextRun::default()
        };
        f64::from(
            self.system
                .layout_line(s, px(size as f32), &[run], None)
                .width,
        )
    }
}

/// Gives the parts of an icon by its name, on a box of 24 by 24, or none for an icon it does
/// not have. A part filled is filled in the color of the item's text, and one stroked is
/// stroked in it 2 wide with round ends and corners, as the SVG writer's icons are.
pub type Icons<'a> = Box<dyn Fn(&str) -> Option<Vec<Part>> + 'a>;

/// Gives an image by its address, or none for one it can't load.
pub type Images<'a> = Box<dyn Fn(&str) -> Option<Arc<RenderImage>> + 'a>;

/// How a scene is drawn.
#[derive(Default)]
pub struct Options<'a> {
    /// The look the scene is drawn in.
    pub look: Look,
    /// The families text is drawn in.
    pub fonts: Fonts,
    /// The icons. Without them, icons are left out.
    pub icons: Option<Icons<'a>>,
    /// The images. Without them, images are left out.
    pub images: Option<Images<'a>>,
}

/// Paints a scene in a window, its origin at a point and each of its units scale pixels. The
/// scene's background is left to the host.
pub fn paint(
    scene: &Scene,
    o: &Options<'_>,
    origin: gpui::Point<Pixels>,
    scale: f32,
    window: &mut Window,
    cx: &mut App,
) {
    paint_with(scene, o, origin, scale, window, cx, |p, it, s| {
        p.item(it, s)
    });
}

/// Paints a scene in a window, each item by a hook, as [`draw_with`](limu_scene::draw_with)
/// draws it. The hook draws on its own with the window the painter's canvas gives.
pub fn paint_with<F>(
    scene: &Scene,
    o: &Options<'_>,
    origin: gpui::Point<Pixels>,
    scale: f32,
    window: &mut Window,
    cx: &mut App,
    hook: F,
) where
    F: FnMut(&mut Painter<'_, GpuiCanvas<'_>>, &Item, &Resolved),
{
    let base = Affine::place(f64::from(origin.x), f64::from(origin.y), f64::from(scale));
    let mut c = GpuiCanvas {
        out: Out::Window { window, cx },
        o,
        transforms: vec![base],
    };
    limu_scene::draw_with(scene, &o.look, &mut c, hook);
}

/// Where a canvas paints: a window, or, in the tests, a record of what it would paint.
enum Out<'a> {
    Window {
        window: &'a mut Window,
        cx: &'a mut App,
    },
    #[cfg(test)]
    Record(&'a mut Vec<String>),
}

/// A window painted in, the canvas of [`paint_with`].
pub struct GpuiCanvas<'a> {
    out: Out<'a>,
    o: &'a Options<'a>,
    // From the scene's units to the window's pixels, the last for what is drawn now.
    transforms: Vec<Affine>,
}

impl GpuiCanvas<'_> {
    /// The window painted in, and the app, for a hook that paints on its own.
    pub fn window(&mut self) -> Option<(&mut Window, &mut App)> {
        match &mut self.out {
            Out::Window { window, cx } => Some((&mut **window, &mut **cx)),
            #[cfg(test)]
            Out::Record(_) => None,
        }
    }

    /// The transform from the scene's units to the window's pixels.
    fn at(&self) -> Affine {
        *self.transforms.last().expect("a transform")
    }

    fn fill(&mut self, path: &limu_scene::Path, ink: &Ink) {
        let t = self.at();
        match &mut self.out {
            Out::Window { window, .. } => {
                let mut b =
                    PathBuilder::fill().with_style(PathStyle::Fill(gpui::FillOptions::non_zero()));
                trace(&mut b, path, t);
                if let Ok(p) = b.build() {
                    window.paint_path(p, hsla(ink));
                }
            }
            #[cfg(test)]
            Out::Record(r) => r.push(format!("fill {} {}", hex(ink), data(path, t))),
        }
    }

    fn stroke(
        &mut self,
        path: &limu_scene::Path,
        ink: &Ink,
        width: f64,
        cap: Cap,
        join: Join,
        dash: &[f64],
    ) {
        let t = self.at();
        let k = t.scale();
        let w = width * k;
        let dash: Vec<f64> = dash_of(dash).iter().map(|d| d * k).collect();
        match &mut self.out {
            Out::Window { window, .. } => {
                let mut b = PathBuilder::stroke(px(w as f32))
                    .with_style(PathStyle::Stroke(stroke_options(w, cap, join)));
                if !dash.is_empty() {
                    let d: Vec<Pixels> = dash.iter().map(|&d| px(d as f32)).collect();
                    b = b.dash_array(&d);
                }
                trace(&mut b, path, t);
                if let Ok(p) = b.build() {
                    window.paint_path(p, hsla(ink));
                }
            }
            #[cfg(test)]
            Out::Record(r) => {
                let dash: Vec<String> = dash.iter().map(|&d| svg::num(d)).collect();
                r.push(format!(
                    "stroke {} {} {cap:?} {join:?} [{}] {}",
                    hex(ink),
                    svg::num(w),
                    dash.join(" "),
                    data(path, t)
                ));
            }
        }
    }

    /// Paints a run upright and filled, glyph by glyph on its baseline.
    fn glyphs(&mut self, run: &Run<'_>, ink: &Ink) {
        let t = self.at();
        let k = t.scale();
        let (x, y) = t.apply(run.at);
        let fonts = &self.o.fonts;
        match &mut self.out {
            Out::Window { window, .. } => {
                let system = window.text_system().clone();
                let lines: Vec<_> = run
                    .spans
                    .iter()
                    .filter(|s| !s.text.is_empty())
                    .map(|s| {
                        let size = s.face.size_at(run.size) * k;
                        let r = TextRun {
                            len: s.text.len(),
                            font: fonts.font(s.face),
                            ..TextRun::default()
                        };
                        (
                            s,
                            size,
                            system.layout_line(&s.text, px(size as f32), &[r], None),
                        )
                    })
                    .collect();
                let width: f64 = lines.iter().map(|(_, _, l)| f64::from(l.width)).sum();
                let mut at = match run.anchor {
                    Align::Start => x,
                    Align::Middle => x - width / 2.0,
                    Align::End => x - width,
                };
                let color = hsla(ink);
                for (s, size, line) in &lines {
                    let base = y + shift(s.face, run.size) * k;
                    for r in &line.runs {
                        for g in &r.glyphs {
                            let o =
                                point(px(at as f32) + g.position.x, px(base as f32) + g.position.y);
                            let font_size = px(*size as f32);
                            let _ = if g.is_emoji {
                                window.paint_emoji(o, r.font_id, g.id, font_size)
                            } else {
                                window.paint_glyph(o, r.font_id, g.id, font_size, color)
                            };
                        }
                    }
                    let w = f64::from(line.width);
                    if s.face.contains(Face::UNDERLINE) {
                        // GPUI keeps a font's metrics to itself, so the line is a tenth of the
                        // size under the baseline and a fifteenth of it thick, as
                        // limu-tiny-skia draws it for a font that gives none.
                        let mut p = limu_scene::Path::new();
                        let (top, thick) = (*size / 10.0, *size / 15.0);
                        p.polygon(&[
                            (at, base + top),
                            (at + w, base + top),
                            (at + w, base + top + thick),
                            (at, base + top + thick),
                        ]);
                        let mut b = PathBuilder::fill()
                            .with_style(PathStyle::Fill(gpui::FillOptions::non_zero()));
                        trace(&mut b, &p, Affine::IDENTITY);
                        if let Ok(p) = b.build() {
                            window.paint_path(p, color);
                        }
                    }
                    at += w;
                }
            }
            #[cfg(test)]
            Out::Record(r) => {
                let spans: Vec<String> = run
                    .spans
                    .iter()
                    .filter(|s| !s.text.is_empty())
                    .map(|s| {
                        format!(
                            "{:?}{:?}{}",
                            s.face,
                            s.text,
                            svg::num(shift(s.face, run.size) * k)
                        )
                    })
                    .collect();
                r.push(format!(
                    "text {} {} {:?} {} {} {}",
                    hex(ink),
                    svg::num(run.size * k),
                    run.anchor,
                    svg::num(x),
                    svg::num(y),
                    spans.join(" ")
                ));
            }
        }
    }

    /// Paints a run turned, or as a halo, as SVG text.
    fn svg_text(&mut self, run: &Run<'_>, ink: &Ink, halo: Option<f64>) {
        let t = self.at();
        let (x, y) = t.apply(run.at);
        let turn = t.angle();
        let text = svg::text_svg(&self.o.fonts, run, t.scale(), halo);
        match &mut self.out {
            Out::Window { window, cx } => {
                let h = text.half as f32;
                let bounds = Bounds::new(
                    point(px(x as f32 - h), px(y as f32 - h)),
                    size(px(2.0 * h), px(2.0 * h)),
                );
                // GPUI turns a sprite about the window's origin, in device pixels.
                let k = window.scale_factor();
                let c = point(
                    gpui::ScaledPixels(x as f32 * k),
                    gpui::ScaledPixels(y as f32 * k),
                );
                let m = TransformationMatrix::unit()
                    .translate(c)
                    .rotate(radians(turn as f32))
                    .translate(point(
                        gpui::ScaledPixels(-c.x.0),
                        gpui::ScaledPixels(-c.y.0),
                    ));
                let mut hasher = std::hash::DefaultHasher::new();
                text.svg.hash(&mut hasher);
                let key = SharedString::from(format!("limu-text-{:016x}", hasher.finish()));
                let _ = window.paint_svg(bounds, key, Some(text.svg.as_bytes()), m, hsla(ink), cx);
            }
            #[cfg(test)]
            Out::Record(r) => r.push(format!(
                "svg {} turn {} at {} {} {}",
                hex(ink),
                svg::num(turn.to_degrees()),
                svg::num(x),
                svg::num(y),
                text.svg
            )),
        }
    }
}

impl Canvas for GpuiCanvas<'_> {
    fn has_layers(&self) -> bool {
        false
    }

    fn begin_rotate(&mut self, degrees: f64, about: Point) {
        let t = self.at().then(Affine::rotate(degrees, about));
        self.transforms.push(t);
    }

    fn end_rotate(&mut self) {
        if self.transforms.len() > 1 {
            self.transforms.pop();
        }
    }

    fn path(&mut self, path: &limu_scene::Path, fill: Option<&Ink>, pen: Option<&Pen<'_>>) {
        if let Some(ink) = fill {
            self.fill(path, ink);
        }
        if let Some(pen) = pen {
            self.stroke(path, &pen.ink, pen.width, pen.cap, pen.join, pen.dash);
        }
    }

    fn text(&mut self, run: &Run<'_>) {
        match &run.paint {
            TextPaint::Halo { ink, width } => self.svg_text(run, ink, Some(*width)),
            TextPaint::Fill(ink) if self.at().angle() != 0.0 => self.svg_text(run, ink, None),
            TextPaint::Fill(ink) => self.glyphs(run, ink),
        }
    }

    fn icon(&mut self, name: &str, bounds: Rect, ink: &Ink) {
        let Some(parts) = self.o.icons.as_ref().and_then(|f| f(name)) else {
            return;
        };
        let on = Affine {
            a: bounds.w / 24.0,
            d: bounds.h / 24.0,
            e: bounds.x,
            f: bounds.y,
            ..Affine::IDENTITY
        };
        self.transforms.push(self.at().then(on));
        for part in &parts {
            let p = match &part.outline {
                Outline::Path(p) => p.clone(),
                Outline::Box { rect, .. } => {
                    let mut p = limu_scene::Path::new();
                    p.polygon(&[
                        (rect.x, rect.y),
                        (rect.x + rect.w, rect.y),
                        (rect.x + rect.w, rect.y + rect.h),
                        (rect.x, rect.y + rect.h),
                    ]);
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
        self.transforms.pop();
    }

    fn image(&mut self, src: &str, bounds: Rect, fit: bool, _opacity: f64) {
        let Some(img) = self.o.images.as_ref().and_then(|f| f(src)) else {
            return;
        };
        let s = img.size(0);
        let (iw, ih) = (f64::from(s.width.0), f64::from(s.height.0));
        if !(iw > 0.0 && ih > 0.0) {
            return;
        }
        let t = self.at();
        let (x, y) = t.apply(Point::new(bounds.x, bounds.y));
        let k = t.scale();
        let (w, h) = (bounds.w * k, bounds.h * k);
        let (mut fw, mut fh, mut fx, mut fy) = (w, h, x, y);
        if fit {
            let r = (w / iw).min(h / ih);
            (fw, fh) = (iw * r, ih * r);
            fx += (w - fw) / 2.0;
            fy += (h - fh) / 2.0;
        }
        match &mut self.out {
            Out::Window { window, .. } => {
                let rect = |x: f64, y: f64, w: f64, h: f64| {
                    Bounds::new(
                        point(px(x as f32), px(y as f32)),
                        size(px(w as f32), px(h as f32)),
                    )
                };
                let _ = window.paint_image(
                    rect(x, y, w, h),
                    rect(fx, fy, fw, fh),
                    Corners::default(),
                    img,
                    0,
                    false,
                );
            }
            #[cfg(test)]
            Out::Record(r) => r.push(format!(
                "image {src:?} {} {} {} {}",
                svg::num(fx),
                svg::num(fy),
                svg::num(fw),
                svg::num(fh)
            )),
        }
    }
}

/// An affine transform, x' = a x + c y + e and y' = b x + d y + f.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Affine {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Affine {
    const IDENTITY: Affine = Affine {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// A scene's units placed at a point, each k pixels.
    fn place(x: f64, y: f64, k: f64) -> Affine {
        Affine {
            a: k,
            d: k,
            e: x,
            f: y,
            ..Affine::IDENTITY
        }
    }

    /// A turn by degrees about a point, clockwise on the screen.
    fn rotate(degrees: f64, about: Point) -> Affine {
        let (sin, cos) = degrees.to_radians().sin_cos();
        Affine {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: about.x - cos * about.x + sin * about.y,
            f: about.y - sin * about.x - cos * about.y,
        }
    }

    /// This transform after another: the other is applied first.
    fn then(self, m: Affine) -> Affine {
        Affine {
            a: self.a * m.a + self.c * m.b,
            b: self.b * m.a + self.d * m.b,
            c: self.a * m.c + self.c * m.d,
            d: self.b * m.c + self.d * m.d,
            e: self.a * m.e + self.c * m.f + self.e,
            f: self.b * m.e + self.d * m.f + self.f,
        }
    }

    fn apply(self, p: Point) -> (f64, f64) {
        (
            self.a * p.x + self.c * p.y + self.e,
            self.b * p.x + self.d * p.y + self.f,
        )
    }

    /// How much the transform scales, the same both ways.
    fn scale(self) -> f64 {
        self.a.hypot(self.b)
    }

    /// How far the transform turns, in radians, clockwise on the screen.
    fn angle(self) -> f64 {
        self.b.atan2(self.a)
    }
}

/// How far a span's baseline is lowered from the run's, in the run's units: a fifth of the
/// label's size for a sub span and a third of it raised for a sup span, as the other backends
/// set them.
fn shift(face: Face, size: f64) -> f64 {
    if face.contains(Face::SUB) {
        size / 5.0
    } else if face.contains(Face::SUP) {
        -size / 3.0
    } else {
        0.0
    }
}

/// How lyon, which GPUI tessellates with, strokes a line as SVG would.
fn stroke_options(width: f64, cap: Cap, join: Join) -> gpui::StrokeOptions {
    gpui::StrokeOptions::default()
        .with_line_width(width as f32)
        .with_line_cap(match cap {
            Cap::Butt => lyon::tessellation::LineCap::Butt,
            Cap::Round => lyon::tessellation::LineCap::Round,
            Cap::Square => lyon::tessellation::LineCap::Square,
        })
        .with_line_join(match join {
            Join::Miter => lyon::tessellation::LineJoin::Miter,
            Join::Round => lyon::tessellation::LineJoin::Round,
            Join::Bevel => lyon::tessellation::LineJoin::Bevel,
        })
        // lyon bevels a corner where the miter is longer than twice its limit times the
        // line's width, where SVG takes once, so it is given half.
        .with_miter_limit((limu_scene::MITER_LIMIT / 2.0) as f32)
}

/// Adds a scene's path, placed by a transform, to a path builder.
fn trace(b: &mut PathBuilder, path: &limu_scene::Path, t: Affine) {
    let at = |p: Point| {
        let (x, y) = t.apply(p);
        point(px(x as f32), px(y as f32))
    };
    for step in path.steps() {
        match step {
            Step::MoveTo(p) => b.move_to(at(p)),
            Step::LineTo(p) => b.line_to(at(p)),
            Step::QuadTo(c, p) => b.curve_to(at(p), at(c)),
            Step::CubeTo(c1, c2, p) => b.cubic_bezier_to(at(p), at(c1), at(c2)),
            Step::Close => b.close(),
        }
    }
}

fn hsla(ink: &Ink) -> Hsla {
    let c = ink.color;
    let k = |v: u8| f32::from(v) / 255.0;
    Rgba {
        r: k(c.r),
        g: k(c.g),
        b: k(c.b),
        a: k(c.a),
    }
    .into()
}

/// A dash as GPUI strokes it, none for solid: one SVG draws solid, of a negative length or
/// of none but zeros, is solid. GPUI repeats a dash of an odd number of lengths, as SVG does.
fn dash_of(dash: &[f64]) -> &[f64] {
    if dash.iter().any(|&d| !(d >= 0.0)) || dash.iter().all(|&d| d == 0.0) {
        return &[];
    }
    dash
}

/// A color as the tests record it.
#[cfg(test)]
fn hex(ink: &Ink) -> String {
    let c = ink.color;
    format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a)
}

/// A path, placed by a transform, as the tests record it.
#[cfg(test)]
fn data(path: &limu_scene::Path, t: Affine) -> String {
    let n = |p: Point| {
        let (x, y) = t.apply(p);
        format!("{} {}", svg::num(x), svg::num(y))
    };
    let mut out = Vec::new();
    for step in path.steps() {
        out.push(match step {
            Step::MoveTo(p) => format!("M{}", n(p)),
            Step::LineTo(p) => format!("L{}", n(p)),
            Step::QuadTo(c, p) => format!("Q{} {}", n(c), n(p)),
            Step::CubeTo(c1, c2, p) => format!("C{} {} {}", n(c1), n(c2), n(p)),
            Step::Close => "Z".into(),
        });
    }
    out.join(" ")
}
