//! The drawing every backend shares: a [`Canvas`] of the few things a backend does itself,
//! and the default drawing of a scene's items with them.
//!
//! The default drawing turns each part into a path or a box filled and stroked in the
//! colors of the item's resolved style, draws markers from their outlines in the color of
//! the edge's line, and places a label's lines from its point, alignment and rotation. A
//! host that wants its own look for some items gives [`draw_with`] a hook, which is handed a
//! [`Painter`] whose methods draw the default's pieces.

use crate::geom::{Point, Rect};
use crate::path::Path;
use crate::scene::{Align, Item, Kind, Outline, Painting, PictureSource, Scene};
use crate::style::{Cap, Ink, Join, Look, Resolved};
use crate::text::Span;

/// How far a stroke's corners may reach out, in widths of the stroke, before they are
/// bevelled: SVG's initial stroke-miterlimit.
pub const MITER_LIMIT: f64 = 4.0;

/// How a path or a box is stroked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pen<'a> {
    /// The color.
    pub ink: Ink,
    /// The width of the line, which is above zero.
    pub width: f64,
    /// The lengths of the dashes and gaps in turn. An empty slice is a solid line.
    pub dash: &'a [f64],
    /// The ends of the line and of its dashes.
    pub cap: Cap,
    /// The corners of the line, mitred as far as [`MITER_LIMIT`].
    pub join: Join,
}

/// How a run of text is painted: filled, or stroked as a halo about its letters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextPaint {
    /// The letters filled.
    Fill(Ink),
    /// The letters stroked, a line `width` wide with round corners, under the text.
    Halo {
        /// The color.
        ink: Ink,
        /// The width of the stroke.
        width: f64,
    },
}

/// Spans of a line of text set one after another from where they are anchored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Run<'a> {
    /// The spans. A span with no text draws nothing, and there is no icon among them.
    pub spans: &'a [Span],
    /// Where the run is anchored, on its baseline.
    pub at: Point,
    /// Whether the run starts there, ends there or is centered on it.
    pub anchor: Align,
    /// The size its label is set at. A span set lower or higher is smaller
    /// ([`Face::size_at`](crate::text::Face::size_at)).
    pub size: f64,
    /// How it is painted.
    pub paint: TextPaint,
}

/// What a backend draws on. The default drawing calls it for the pieces of every item, in
/// the order they are drawn, with colors settled from the look.
///
/// The spans of a run are set as they are shaped in the canvas's fonts, one after another,
/// so that where a line starts, ends or is centered does not depend on how layout measured
/// its text. A canvas sets them in the faces the scene was measured in: layout leaves room
/// for text as its measurer found it.
pub trait Canvas {
    /// Begins the scene, before its first item.
    fn begin_scene(&mut self, scene: &Scene) {
        let _ = scene;
    }

    /// Ends the scene, after its last item.
    fn end_scene(&mut self, scene: &Scene) {
        let _ = scene;
    }

    /// Begins an item, before any of its pieces, as the SVG writer begins a group with its
    /// classes, its tooltip and its link.
    fn begin_item(&mut self, item: &Item) {
        let _ = item;
    }

    /// Ends an item, after its pieces.
    fn end_item(&mut self, item: &Item) {
        let _ = item;
    }

    /// Whether the canvas draws layers at an opacity. Where it does not, the default drawing
    /// lays an item's opacity on each color it draws the item in, and parts of the item that
    /// overlap show through one another.
    fn has_layers(&self) -> bool {
        false
    }

    /// Begins a layer drawn at an opacity, which is from 0 to 1. It is called only when
    /// [`has_layers`](Canvas::has_layers) says the canvas has them.
    fn begin_layer(&mut self, opacity: f64) {
        let _ = opacity;
    }

    /// Ends the layer begun last.
    fn end_layer(&mut self) {}

    /// Turns what is drawn next by a number of degrees, clockwise, about a point, until
    /// [`end_rotate`](Canvas::end_rotate).
    fn begin_rotate(&mut self, degrees: f64, about: Point);

    /// Ends the turn begun last.
    fn end_rotate(&mut self);

    /// Fills a path, then strokes it, with what is given of the two.
    fn path(&mut self, path: &Path, fill: Option<&Ink>, pen: Option<&Pen<'_>>);

    /// Fills a box, then strokes it, with its corners rounded by a radius held to half its
    /// smaller side. The default draws it as a path.
    fn rect(&mut self, rect: Rect, radius: f64, fill: Option<&Ink>, pen: Option<&Pen<'_>>) {
        let mut p = Path::new();
        p.round_rect(rect, radius);
        self.path(&p, fill, pen);
    }

    /// Fills a part of the marker at an end of an edge, then strokes it. The default draws
    /// it as a path.
    fn marker(&mut self, path: &Path, fill: Option<&Ink>, pen: Option<&Pen<'_>>) {
        self.path(path, fill, pen);
    }

    /// Draws a run of text.
    fn text(&mut self, run: &Run<'_>);

    /// Draws an icon by its name, as "fa:fa-car", in a box, in a color. The default draws
    /// nothing. Labels leave room only for the icons the measurer was told of.
    fn icon(&mut self, name: &str, bounds: Rect, ink: &Ink) {
        let _ = (name, bounds, ink);
    }

    /// Draws an image from its address in a box, at an opacity: fitted in it and centered,
    /// keeping its proportions, when fit is set, and otherwise stretched to fill it. Loading
    /// the image is the canvas's. The default draws nothing.
    fn image(&mut self, src: &str, bounds: Rect, fit: bool, opacity: f64) {
        let _ = (src, bounds, fit, opacity);
    }
}

/// Draws a scene on a canvas in a look, each item as [`Painter::item`] does.
pub fn draw<C: Canvas + ?Sized>(scene: &Scene, look: &Look, canvas: &mut C) {
    draw_with(scene, look, canvas, |p, it, s| p.item(it, s));
}

/// Draws a scene on a canvas in a look, each item by the hook, which is given the item and
/// its style resolved in the look. The hook draws on the painter's canvas, and draws what it
/// leaves to the default with the painter's methods. It draws inside the item, between
/// [`Canvas::begin_item`] and [`Canvas::end_item`], and inside the layer of the item's
/// opacity, or, on a canvas with no layers, with the opacity laid on the colors it is given.
/// Either way the style it is given is opaque.
pub fn draw_with<C, F>(scene: &Scene, look: &Look, canvas: &mut C, mut hook: F)
where
    C: Canvas + ?Sized,
    F: FnMut(&mut Painter<'_, C>, &Item, &Resolved),
{
    canvas.begin_scene(scene);
    for it in &scene.items {
        let mut s = look.resolve(it.role.as_str(), &it.style, &it.author);
        canvas.begin_item(it);
        let k = s.opacity.clamp(0.0, 1.0);
        let layer = k < 1.0 && canvas.has_layers();
        let mut opacity = 1.0;
        if layer {
            canvas.begin_layer(k);
            s.opacity = 1.0;
        } else if k < 1.0 {
            s.fade();
            opacity = k;
        }
        hook(
            &mut Painter {
                canvas: &mut *canvas,
                opacity,
            },
            it,
            &s,
        );
        if layer {
            canvas.end_layer();
        }
        canvas.end_item(it);
    }
    canvas.end_scene(scene);
}

/// Draws the default's pieces of an item on a canvas, for a draw hook.
#[derive(Debug)]
pub struct Painter<'a, C: ?Sized> {
    canvas: &'a mut C,
    opacity: f64,
}

impl<'a, C: Canvas + ?Sized> Painter<'a, C> {
    /// The canvas, to draw on in one's own way.
    pub fn canvas(&mut self) -> &mut C {
        self.canvas
    }

    /// The opacity an image of the item is drawn at. It is the item's on a canvas with no
    /// layers, where its colors have it already, and 1 inside the layer of its opacity.
    pub fn opacity(&self) -> f64 {
        self.opacity
    }

    /// Draws an item as the default does: its parts, its markers, its picture and its
    /// label.
    pub fn item(&mut self, it: &Item, s: &Resolved) {
        self.parts(it, s);
        self.ends(it, s);
        self.picture(it, s);
        self.label(it, s);
    }

    /// Draws the parts of an item's outline, paths and boxes, each filled and stroked as its
    /// painting says, a box's corners rounded by the style's radius when it is rounded.
    pub fn parts(&mut self, it: &Item, s: &Resolved) {
        for part in &it.parts {
            let (fill, stroke) = match part.paint {
                Painting::FillAndStroke => (s.fill, Some(s.stroke)),
                Painting::StrokeOnly => (Ink::NONE, Some(s.stroke)),
                Painting::FillOnly => (s.fill, None),
                Painting::Solid => (s.line, Some(s.stroke)),
                Painting::Hollow => (s.hollow, Some(s.stroke)),
                Painting::Shade => (s.shade, None),
                Painting::Dot => (s.marker, None),
            };
            let fill = Some(&fill).filter(|f| f.visible());
            let pen = stroke
                .filter(|i| i.visible() && s.stroke_width > 0.0)
                .map(|ink| Pen {
                    ink,
                    width: s.stroke_width,
                    dash: &s.dash,
                    cap: s.cap,
                    join: s.join,
                });
            if fill.is_none() && pen.is_none() {
                continue;
            }
            match &part.outline {
                Outline::Path(p) => self.canvas.path(p, fill, pen.as_ref()),
                Outline::Box { rect, rounded } => {
                    if rect.w < 0.0 || rect.h < 0.0 {
                        continue;
                    }
                    let radius = if *rounded { s.radius.max(0.0) } else { 0.0 };
                    self.canvas.rect(*rect, radius, fill, pen.as_ref());
                }
            }
        }
    }

    /// Draws the markers at the ends of an edge, each part as
    /// [`Marker::outline`](crate::Marker::outline) says: filled in the color of the edge's
    /// stroke without the stroke's opacity, and stroked at it, as Pele draws them. A marker
    /// is stroked solid, with the edge's caps and with its corners turned as the marker's
    /// type says, or else as the edge's are.
    pub fn ends(&mut self, it: &Item, s: &Resolved) {
        let Kind::Edge(e) = &it.kind else { return };
        for m in e.ends.iter().flatten() {
            let pen = Pen {
                ink: s.stroke,
                width: s.stroke_width,
                dash: &[],
                cap: s.cap,
                join: m.kind.join().unwrap_or(s.join),
            };
            let pen = Some(&pen).filter(|p| p.ink.visible() && p.width > 0.0);
            for part in m.outline() {
                let path = match part.outline {
                    Outline::Path(p) => p,
                    Outline::Box { rect, .. } => {
                        let mut p = Path::new();
                        p.round_rect(rect, 0.0);
                        p
                    }
                };
                let (fill, pen) = match part.paint {
                    Painting::StrokeOnly => (None, pen),
                    Painting::Hollow => (Some(&s.hollow), pen),
                    Painting::FillAndStroke => (Some(&s.marker), pen),
                    _ => (Some(&s.marker), None),
                };
                let fill = fill.filter(|f| f.visible());
                if fill.is_some() || pen.is_some() {
                    self.canvas.marker(&path, fill, pen);
                }
            }
        }
    }

    /// Draws an item's picture: an image, or an icon in the color of the item's text.
    pub fn picture(&mut self, it: &Item, s: &Resolved) {
        let Some(pic) = &it.picture else { return };
        match &pic.source {
            PictureSource::Image { src, fit } => {
                self.canvas.image(src, pic.bounds, *fit, self.opacity)
            }
            PictureSource::Icon(name) => self.canvas.icon(name, pic.bounds, &s.text),
        }
    }

    /// Draws an item's label about its point, as Pele's labelSvg does. A line's baseline is
    /// a little below the middle of its line's height. A line is anchored at the point as
    /// the label's alignment says, and its spans follow one another. A line with an icon in
    /// it is placed span by span from where layout measured them, centered on its middle,
    /// which a line starting or ending at the point has half its width away, as Pele's
    /// leftLabelSvg, textLeft and textBlock place one. A halo is drawn under the whole label,
    /// and the label is turned about its point by its rotation, icons and all.
    pub fn label(&mut self, it: &Item, s: &Resolved) {
        let Some(placed) = &it.label else { return };
        if placed.label.is_empty() || !s.text.visible() {
            return;
        }
        let turned = placed.rotate != 0.0;
        if turned {
            self.canvas.begin_rotate(placed.rotate, placed.at);
        }
        if s.halo.visible() && s.halo_width > 0.0 {
            self.lines(
                it,
                TextPaint::Halo {
                    ink: s.halo,
                    width: s.halo_width,
                },
            );
        }
        self.lines(it, TextPaint::Fill(s.text));
        if turned {
            self.canvas.end_rotate();
        }
    }

    /// Draws the lines of a label in a paint, and its icons when it is filled.
    fn lines(&mut self, it: &Item, paint: TextPaint) {
        let Some(placed) = &it.label else { return };
        let l = &placed.label;
        let (cx, cy) = (placed.at.x, placed.at.y);
        let shift = l.size * 0.35;
        // Each line is placed from the top of the label down, as Pele's labelSvg places them.
        let mut top = cy - l.height / 2.0;
        for (i, line) in l.lines.iter().enumerate() {
            let mid = top + l.line_height / 2.0;
            let y = mid + shift;
            top += l.line_height;
            let run = |spans, x, anchor| Run {
                spans,
                at: Point::new(x, y),
                anchor,
                size: l.size,
                paint,
            };
            if line.iter().all(|sp| sp.icon.is_none()) {
                if line.iter().any(|sp| !sp.text.is_empty()) {
                    self.canvas.text(&run(line, cx, placed.align));
                }
                continue;
            }
            let w = l.widths.get(i).copied().unwrap_or(0.0);
            let middle = match placed.align {
                Align::Middle => cx,
                Align::Start => cx + w / 2.0,
                Align::End => cx - w / 2.0,
            };
            let mut x = middle - w / 2.0;
            for sp in line {
                if let Some(name) = &sp.icon {
                    if let TextPaint::Fill(ink) = paint {
                        let side = l.size * 1.1;
                        self.canvas
                            .icon(name, Rect::new(x, mid - side / 2.0, side, side), &ink);
                    }
                } else if !sp.text.is_empty() {
                    self.canvas
                        .text(&run(std::slice::from_ref(sp), x, Align::Start));
                }
                x += sp.width;
            }
        }
    }
}
