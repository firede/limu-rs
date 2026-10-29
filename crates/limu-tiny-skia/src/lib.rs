//! Draws limu's scenes into [tiny-skia](https://docs.rs/tiny-skia) pixmaps, for PNGs and for
//! the golden images of the default drawing.
//!
//! Text is shaped with rustybuzz in the fonts the host gives, a font for each face it has,
//! and drawn from the glyphs' outlines. [`Fonts`] measures text too, so a diagram laid out
//! with it is drawn in the faces it was measured in.
//!
//! An item's opacity is drawn as a layer, a pixmap of its own laid over what is under it, as
//! SVG draws a group at an opacity. Boxes are drawn as paths, a radius too large for a box
//! held to half its smaller side both ways, where SVG takes half its width across and half
//! its height down.

use limu_scene::style::{Cap, Ink, Join, Look, Resolved};
use limu_scene::text::{Face, Measurer};
use limu_scene::{
    Align, Canvas, Icons, Item, Outline, Painter, Painting, Pen, Point, Rect, Run, Scene, TextPaint,
};
use rustybuzz::ttf_parser::{self, GlyphId, OutlineBuilder};
use tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, PathBuilder, Pattern, Pixmap, PixmapMut, PixmapPaint,
    SpreadMode, Stroke, StrokeDash, Transform,
};

pub use tiny_skia;

/// A font the data of which could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontError;

impl std::fmt::Display for FontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the font could not be read")
    }
}

impl std::error::Error for FontError {}

/// The fonts text is shaped and drawn in: a regular one, and one for each face of
/// [`BOLD`](Face::BOLD), [`ITALIC`](Face::ITALIC) and [`MONO`](Face::MONO), or a mix of them,
/// that the host has. Text in a face it has no font for is set in the nearest it has,
/// without its italic, then without its bold, then in the regular font.
#[derive(Clone, Debug)]
pub struct Fonts {
    // The fonts' data, by the bits of the face they set.
    fonts: [Option<Vec<u8>>; 8],
}

impl Fonts {
    /// Fonts of one regular font, from the data of a TrueType or OpenType file.
    pub fn new(regular: Vec<u8>) -> Result<Fonts, FontError> {
        let mut f = Fonts {
            fonts: Default::default(),
        };
        f.set(Face::REGULAR, regular)?;
        Ok(f)
    }

    /// Sets the font of a face of bold, italic and mono, from the data of a TrueType or
    /// OpenType file. The face's other bits are ignored.
    pub fn set(&mut self, face: Face, data: Vec<u8>) -> Result<&mut Fonts, FontError> {
        ttf_parser::Face::parse(&data, 0).map_err(|_| FontError)?;
        self.fonts[slot(face)] = Some(data);
        Ok(self)
    }

    /// The shaper of the font text in a face is set in.
    fn shaper(&self, face: Face) -> rustybuzz::Face<'_> {
        let tries = [
            face,
            face.without(Face::ITALIC),
            face.without(Face::ITALIC | Face::BOLD),
            Face::REGULAR,
        ];
        let data = tries.iter().find_map(|&f| self.fonts[slot(f)].as_deref());
        // The regular font is always there, and every font was read when it was set.
        rustybuzz::Face::from_slice(data.unwrap_or_default(), 0).expect("a font that was read")
    }
}

/// Where the font of a face is kept.
fn slot(face: Face) -> usize {
    usize::from((face & (Face::BOLD | Face::ITALIC | Face::MONO)).bits())
}

/// Text shaped in a font: its glyphs, where each is drawn from the start, and how far it
/// advances, in units of the font's em.
struct Shaped {
    glyphs: Vec<(GlyphId, f64, f64)>,
    advance: f64,
}

fn shape(font: &rustybuzz::Face<'_>, s: &str) -> Shaped {
    let mut buf = rustybuzz::UnicodeBuffer::new();
    buf.push_str(s);
    let out = rustybuzz::shape(font, &[], buf);
    let upem = f64::from(font.units_per_em());
    let mut x = 0.0;
    let mut glyphs = Vec::with_capacity(out.len());
    for (info, pos) in out.glyph_infos().iter().zip(out.glyph_positions()) {
        let id = GlyphId(info.glyph_id as u16);
        glyphs.push((
            id,
            (x + f64::from(pos.x_offset)) / upem,
            f64::from(pos.y_offset) / upem,
        ));
        x += f64::from(pos.x_advance);
    }
    Shaped {
        glyphs,
        advance: x / upem,
    }
}

impl Measurer for Fonts {
    fn width(&self, s: &str, size: f64, f: Face) -> f64 {
        shape(&self.shaper(f), s).advance * size
    }
}

/// Gives an image by its address, or none for one it can't load.
pub type Images<'a> = Box<dyn Fn(&str) -> Option<Pixmap> + 'a>;

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

/// Draws a scene into a pixmap of its size at the options' scale, on the theme's background.
/// It is none for a scene too small or too large for a pixmap.
pub fn render(scene: &Scene, o: &Options<'_>) -> Option<Pixmap> {
    let k = if o.scale > 0.0 { o.scale } else { 1.0 };
    let w = (scene.width * k).ceil();
    let h = (scene.height * k).ceil();
    if !(w >= 1.0 && h >= 1.0) {
        return None;
    }
    let mut pixmap = Pixmap::new(w as u32, h as u32)?;
    pixmap.fill(color(o.look.theme.bg));
    draw(
        scene,
        o,
        &mut pixmap.as_mut(),
        Transform::from_scale(k as f32, k as f32),
    );
    Some(pixmap)
}

/// Draws a scene into a pixmap, its units placed in the pixmap's pixels by a transform.
pub fn draw(scene: &Scene, o: &Options<'_>, pixmap: &mut PixmapMut<'_>, transform: Transform) {
    draw_with(scene, o, pixmap, transform, |p, it, s| p.item(it, s));
}

/// Draws a scene into a pixmap, each item by a hook, as
/// [`draw_with`](limu_scene::draw_with) draws it. The hook draws on its own with the
/// painter's canvas, a [`PixmapCanvas`].
pub fn draw_with<F>(
    scene: &Scene,
    o: &Options<'_>,
    pixmap: &mut PixmapMut<'_>,
    transform: Transform,
    hook: F,
) where
    F: FnMut(&mut Painter<'_, PixmapCanvas<'_, '_>>, &Item, &Resolved),
{
    let (width, height) = (pixmap.width(), pixmap.height());
    let mut c = PixmapCanvas {
        base: pixmap.data_mut(),
        width,
        height,
        layers: Vec::new(),
        transforms: vec![transform],
        o,
    };
    limu_scene::draw_with(scene, &o.look, &mut c, hook);
}

/// A pixmap drawn on, the canvas of [`draw_with`].
pub struct PixmapCanvas<'p, 'o> {
    // The pixels of the pixmap the scene is drawn into.
    base: &'p mut [u8],
    width: u32,
    height: u32,
    // The layers begun, each with the opacity it is laid down at.
    layers: Vec<(Pixmap, f64)>,
    transforms: Vec<Transform>,
    o: &'o Options<'o>,
}

impl PixmapCanvas<'_, '_> {
    /// The pixmap being drawn on: the layer begun last, or the one the scene is drawn into.
    pub fn pixmap(&mut self) -> PixmapMut<'_> {
        match self.layers.last_mut() {
            Some((l, _)) => l.as_mut(),
            None => PixmapMut::from_bytes(self.base, self.width, self.height)
                .expect("the pixels of a pixmap"),
        }
    }

    /// The transform the scene's units are placed in the pixmap's pixels by.
    pub fn transform(&self) -> Transform {
        *self.transforms.last().expect("a transform")
    }

    fn fill(&mut self, path: &tiny_skia::Path, ink: &Ink) {
        let t = self.transform();
        self.pixmap()
            .fill_path(path, &paint(ink), FillRule::Winding, t, None);
    }

    fn stroke(&mut self, path: &tiny_skia::Path, ink: &Ink, stroke: &Stroke) {
        let t = self.transform();
        self.pixmap()
            .stroke_path(path, &paint(ink), stroke, t, None);
    }

    /// Adds the outlines of a run of text to a path.
    fn outline_run(&self, fonts: &Fonts, run: &Run<'_>, pb: &mut PathBuilder) {
        let spans: Vec<_> = run
            .spans
            .iter()
            .filter(|s| !s.text.is_empty())
            .map(|s| {
                let font = fonts.shaper(s.face);
                let shaped = shape(&font, &s.text);
                (s, font, shaped)
            })
            .collect();
        let width: f64 = spans
            .iter()
            .map(|(s, _, t)| t.advance * s.face.size_at(run.size))
            .sum();
        let mut x = match run.anchor {
            Align::Start => run.at.x,
            Align::Middle => run.at.x - width / 2.0,
            Align::End => run.at.x - width,
        };
        for (s, font, shaped) in &spans {
            let size = s.face.size_at(run.size);
            // Lowered a fifth or raised a third of the label's size, about where a browser
            // puts SVG's sub and super baseline shifts.
            let y = run.at.y
                + if s.face.contains(Face::SUB) {
                    run.size / 5.0
                } else if s.face.contains(Face::SUP) {
                    -run.size / 3.0
                } else {
                    0.0
                };
            for &(id, gx, gy) in &shaped.glyphs {
                let mut o = Outliner {
                    pb,
                    x: x + gx * size,
                    y: y - gy * size,
                    k: size / f64::from(font.units_per_em()),
                };
                font.outline_glyph(id, &mut o);
            }
            let w = shaped.advance * size;
            if s.face.contains(Face::UNDERLINE) {
                let k = size / f64::from(font.units_per_em());
                let (top, thick) = match font.underline_metrics() {
                    Some(m) => (-f64::from(m.position) * k, f64::from(m.thickness) * k),
                    None => (size / 10.0, size / 15.0),
                };
                if let Some(r) = tiny_skia::Rect::from_xywh(
                    x as f32,
                    (y + top) as f32,
                    w as f32,
                    thick.max(0.0) as f32,
                ) {
                    pb.push_rect(r);
                }
            }
            x += w;
        }
    }
}

/// Adds a glyph's outline to a path, placed at x, y and scaled by k, its y up.
struct Outliner<'a> {
    pb: &'a mut PathBuilder,
    x: f64,
    y: f64,
    k: f64,
}

impl Outliner<'_> {
    fn at(&self, x: f32, y: f32) -> (f32, f32) {
        (
            (self.x + f64::from(x) * self.k) as f32,
            (self.y - f64::from(y) * self.k) as f32,
        )
    }
}

impl OutlineBuilder for Outliner<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.at(x, y);
        self.pb.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.at(x, y);
        self.pb.line_to(x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let ((x1, y1), (x, y)) = (self.at(x1, y1), self.at(x, y));
        self.pb.quad_to(x1, y1, x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let ((x1, y1), (x2, y2), (x, y)) = (self.at(x1, y1), self.at(x2, y2), self.at(x, y));
        self.pb.cubic_to(x1, y1, x2, y2, x, y);
    }

    fn close(&mut self) {
        self.pb.close();
    }
}

impl Canvas for PixmapCanvas<'_, '_> {
    fn has_layers(&self) -> bool {
        true
    }

    fn begin_layer(&mut self, opacity: f64) {
        let layer =
            Pixmap::new(self.width, self.height).expect("a pixmap of the size of one there is");
        self.layers.push((layer, opacity));
    }

    fn end_layer(&mut self) {
        self.lay();
    }

    fn begin_rotate(&mut self, degrees: f64, about: Point) {
        let t = self.transform().pre_concat(Transform::from_rotate_at(
            degrees as f32,
            about.x as f32,
            about.y as f32,
        ));
        self.transforms.push(t);
    }

    fn end_rotate(&mut self) {
        if self.transforms.len() > 1 {
            self.transforms.pop();
        }
    }

    fn path(&mut self, path: &limu_scene::Path, fill: Option<&Ink>, pen: Option<&Pen<'_>>) {
        let Some(p) = convert(path) else { return };
        if let Some(ink) = fill {
            self.fill(&p, ink);
        }
        if let Some(pen) = pen {
            self.stroke(&p, &pen.ink, &stroke_of(pen));
        }
    }

    fn text(&mut self, run: &Run<'_>) {
        let Some(fonts) = self.o.fonts else { return };
        let mut pb = PathBuilder::new();
        self.outline_run(fonts, run, &mut pb);
        let Some(p) = pb.finish() else { return };
        match &run.paint {
            TextPaint::Fill(ink) => self.fill(&p, ink),
            TextPaint::Halo { ink, width } => {
                let s = Stroke {
                    width: *width as f32,
                    line_join: LineJoin::Round,
                    ..Stroke::default()
                };
                self.stroke(&p, ink, &s);
            }
        }
    }

    fn icon(&mut self, name: &str, bounds: Rect, ink: &Ink) {
        let Some(parts) = self.o.icons.as_ref().and_then(|f| f(name)) else {
            return;
        };
        let t = self.transform();
        let on = Transform::from_row(
            (bounds.w / 24.0) as f32,
            0.0,
            0.0,
            (bounds.h / 24.0) as f32,
            bounds.x as f32,
            bounds.y as f32,
        );
        self.transforms.push(t.pre_concat(on));
        let pen = Stroke {
            width: 2.0,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        for part in &parts {
            let p = match &part.outline {
                Outline::Path(p) => convert(p),
                Outline::Box { rect, .. } => tiny_skia::Rect::from_xywh(
                    rect.x as f32,
                    rect.y as f32,
                    rect.w as f32,
                    rect.h as f32,
                )
                .map(PathBuilder::from_rect),
            };
            let Some(p) = p else { continue };
            if part.paint != Painting::StrokeOnly {
                self.fill(&p, ink);
            }
            if matches!(part.paint, Painting::StrokeOnly | Painting::FillAndStroke) {
                self.stroke(&p, ink, &pen);
            }
        }
        self.transforms.pop();
    }

    fn image(&mut self, src: &str, bounds: Rect, fit: bool, opacity: f64) {
        let Some(img) = self.o.images.as_ref().and_then(|f| f(src)) else {
            return;
        };
        let (iw, ih) = (f64::from(img.width()), f64::from(img.height()));
        let (mut sx, mut sy) = (bounds.w / iw, bounds.h / ih);
        let (mut x, mut y) = (bounds.x, bounds.y);
        if fit {
            let s = sx.min(sy);
            (sx, sy) = (s, s);
            x += (bounds.w - iw * s) / 2.0;
            y += (bounds.h - ih * s) / 2.0;
        }
        let Some(r) =
            tiny_skia::Rect::from_xywh(x as f32, y as f32, (iw * sx) as f32, (ih * sy) as f32)
        else {
            return;
        };
        let shader = Pattern::new(
            img.as_ref(),
            SpreadMode::Pad,
            tiny_skia::FilterQuality::Bicubic,
            opacity.clamp(0.0, 1.0) as f32,
            Transform::from_row(sx as f32, 0.0, 0.0, sy as f32, x as f32, y as f32),
        );
        let paint = Paint {
            shader,
            ..Paint::default()
        };
        let t = self.transform();
        self.pixmap().fill_rect(r, &paint, t, None);
    }
}

impl Drop for PixmapCanvas<'_, '_> {
    fn drop(&mut self) {
        // Layers a hook left open are laid down, so that nothing drawn is lost.
        while !self.layers.is_empty() {
            self.lay();
        }
    }
}

impl PixmapCanvas<'_, '_> {
    /// Lays the layer begun last over what is under it, at its opacity.
    fn lay(&mut self) {
        let Some((top, opacity)) = self.layers.pop() else {
            return;
        };
        let paint = PixmapPaint {
            opacity: opacity.clamp(0.0, 1.0) as f32,
            ..PixmapPaint::default()
        };
        self.pixmap()
            .draw_pixmap(0, 0, top.as_ref(), &paint, Transform::identity(), None);
    }
}

/// A scene's path as tiny-skia's, or none for one that draws nothing.
fn convert(path: &limu_scene::Path) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    for step in path.steps() {
        match step {
            limu_scene::Step::MoveTo(p) => pb.move_to(p.x as f32, p.y as f32),
            limu_scene::Step::LineTo(p) => pb.line_to(p.x as f32, p.y as f32),
            limu_scene::Step::QuadTo(c, p) => {
                pb.quad_to(c.x as f32, c.y as f32, p.x as f32, p.y as f32)
            }
            limu_scene::Step::CubeTo(c1, c2, p) => pb.cubic_to(
                c1.x as f32,
                c1.y as f32,
                c2.x as f32,
                c2.y as f32,
                p.x as f32,
                p.y as f32,
            ),
            limu_scene::Step::Close => pb.close(),
        }
    }
    pb.finish()
}

fn color(c: limu_scene::style::Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)
}

fn paint(ink: &Ink) -> Paint<'static> {
    let mut p = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    p.set_color(color(ink.color));
    p
}

/// A pen as tiny-skia strokes it. A dash of an odd number of lengths is repeated, as SVG's
/// is, and one SVG draws solid, of a negative length or of none but zeros, is solid.
fn stroke_of(pen: &Pen<'_>) -> Stroke {
    let mut dash: Vec<f32> = pen.dash.iter().map(|&d| d as f32).collect();
    if dash.len() % 2 == 1 {
        dash.extend_from_within(..);
    }
    Stroke {
        width: pen.width as f32,
        miter_limit: limu_scene::MITER_LIMIT as f32,
        line_cap: match pen.cap {
            Cap::Butt => LineCap::Butt,
            Cap::Round => LineCap::Round,
            Cap::Square => LineCap::Square,
        },
        line_join: match pen.join {
            Join::Miter => LineJoin::Miter,
            Join::Round => LineJoin::Round,
            Join::Bevel => LineJoin::Bevel,
        },
        dash: StrokeDash::new(dash, 0.0),
    }
}
