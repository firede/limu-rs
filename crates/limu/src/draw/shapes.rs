//! The shapes of nodes, as Pele's src/svg/shapes.ts has them: their sizes about a label, how
//! far their outlines lie in from their boxes, how much of a side edges may spread along,
//! and their outlines.
//!
//! What layout depends on, the sizes, insets and spans, is Pele's to the last bit. The
//! outlines are Pele's in absolute coordinates. Where Pele draws with lines and curves they
//! are the numbers it writes, taken to hundredths as it writes them, and where it writes arcs,
//! circles and ellipses, which become cubic curves here, they are the same figures but not
//! the same numbers. A box with the theme's corners stays a box, as Pele's rect does, its
//! corners left to the backend.

use std::f64::consts::{FRAC_1_SQRT_2, PI};

use crate::js::math::{cos, max, min, round, sin};
use crate::scene::{Painting, Part, Path, Rect};

/// The padding about a label in the shapes that hold one, across and down.
pub(crate) const PAD_X: f64 = 18.0;
pub(crate) const PAD_Y: f64 = 12.0;

/// The size of a shape drawn about a label, and how far below the shape's center the label
/// is drawn, or above it where dy is negative, as Pele's ShapeSize is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShapeSize {
    pub(crate) w: f64,
    pub(crate) h: f64,
    pub(crate) dy: f64,
}

/// How far a shape w by h lies in from its box on a side at t, as Pele's inset takes them.
type Inset = fn(f64, f64, usize, f64) -> f64;

/// A shape, as Pele's ShapeDef is.
struct ShapeDef {
    size: fn(f64, f64) -> ShapeSize,
    draw: fn(&mut Drawing, f64, f64),
    /// The distance from the bounding box to the outline on a side, 0 top, 1 right, 2 bottom
    /// and 3 left, t along the side from its middle, to the right or down.
    inset: Option<Inset>,
    /// How much of a side edges may spread along, centered on its middle: the top and bottom,
    /// or the left and right when sideways. A shape without one keeps its edges at the middle.
    span: Option<fn(f64, f64, bool) -> f64>,
    no_label: bool,
    /// Whether the shape is drawn with no fill, so that a fill its author sets is not under
    /// its label.
    bare: bool,
}

const DEF: ShapeDef = ShapeDef {
    size: box_size,
    draw: |_, _, _| {},
    inset: None,
    span: None,
    no_label: false,
    bare: false,
};

/// Kept clear at each end of a straight side, for the corner.
const CORNER: f64 = 12.0;

fn flat(w: f64, h: f64, sideways: bool) -> f64 {
    (if sideways { h } else { w }) - 2.0 * CORNER
}

/// Straight along the top and bottom except for cut at each end, and not straight at the
/// sides, as Pele's flatAcross makes it.
fn flat_across(w: f64, cut: f64, sideways: bool) -> f64 {
    if sideways {
        0.0
    } else {
        w - 2.0 * cut - 2.0 * CORNER
    }
}

/// How far an ellipse's outline has fallen away from its bounding box, t from the middle of
/// a side half long, as Pele's bulge works it out.
fn bulge(t: f64, half: f64, depth: f64) -> f64 {
    let k = min(1.0, t.abs() / half);
    depth * (1.0 - (1.0 - k * k).sqrt())
}

fn round_inset(w: f64, h: f64, side: usize, t: f64) -> f64 {
    if side & 1 != 0 {
        bulge(t, h / 2.0, w / 2.0)
    } else {
        bulge(t, w / 2.0, h / 2.0)
    }
}

fn round_span(w: f64, h: f64, sideways: bool) -> f64 {
    (if sideways { h } else { w }) * 0.6
}

/// The height of the caps of a cylinder w wide.
fn cap_ry(w: f64) -> f64 {
    min(12.0, max(6.0, w / 14.0))
}

fn cap_inset(w: f64, _h: f64, side: usize, t: f64) -> f64 {
    if side & 1 != 0 {
        0.0
    } else {
        bulge(t, w / 2.0, cap_ry(w))
    }
}

fn tri_size(tw: f64, th: f64, flip: bool) -> ShapeSize {
    let w = max(1.6 * tw + 40.0, 80.0);
    let h = (th + 10.0) / (1.0 - (tw + 8.0) / w);
    let dy = h / 2.0 - 10.0 - th / 2.0;
    ShapeSize {
        w,
        h,
        dy: if flip { -dy } else { dy },
    }
}

fn lean_inset(_w: f64, h: f64, side: usize, _t: f64) -> f64 {
    if side & 1 != 0 { h / 8.0 } else { 0.0 }
}

fn box_size(tw: f64, th: f64) -> ShapeSize {
    ShapeSize {
        w: max(tw + 2.0 * PAD_X, 2.0 * PAD_X + 12.0),
        h: th + 2.0 * PAD_Y,
        dy: 0.0,
    }
}

/// The size of the many shapes that are the label and its padding, and more.
fn padded(tw: f64, th: f64, w: f64, h: f64, dy: f64) -> ShapeSize {
    ShapeSize {
        w: tw + 2.0 * PAD_X + w,
        h: th + 2.0 * PAD_Y + h,
        dy,
    }
}

/// A shape whose height is the label's and its padding, and whose width is more by a part of
/// that height.
fn by_height(tw: f64, th: f64, k: f64) -> ShapeSize {
    let h = th + 2.0 * PAD_Y;
    ShapeSize {
        w: tw + 2.0 * PAD_X + h * k,
        h,
        dy: 0.0,
    }
}

fn fixed(w: f64, h: f64) -> ShapeSize {
    ShapeSize { w, h, dy: 0.0 }
}

/// A shape with sides that lean in by a quarter of its height, at the top or the bottom.
const LEANING: ShapeDef = ShapeDef {
    inset: Some(lean_inset),
    span: Some(|w, h, sideways| flat_across(w, h / 4.0, sideways)),
    size: |tw, th| by_height(tw, th, 0.5),
    ..DEF
};

/// Pele's SHAPES, in its order.
const SHAPES: &[(&str, ShapeDef)] = &[
    (
        "rect",
        ShapeDef {
            span: Some(flat),
            draw: |d, w, h| d.rounded(-w / 2.0, -h / 2.0, w, h),
            ..DEF
        },
    ),
    (
        "rounded",
        ShapeDef {
            span: Some(flat),
            draw: |d, w, h| {
                d.rect(
                    -w / 2.0,
                    -h / 2.0,
                    w,
                    h,
                    min(12.0, h / 2.0),
                    Painting::FillAndStroke,
                )
            },
            ..DEF
        },
    ),
    (
        "stadium",
        ShapeDef {
            inset: Some(|_, h, side, t| {
                if side & 1 != 0 {
                    bulge(t, h / 2.0, h / 2.0)
                } else {
                    0.0
                }
            }),
            span: Some(|w, h, sideways| if sideways { h * 0.6 } else { w - h }),
            size: |tw, th| {
                let h = th + 2.0 * PAD_Y;
                ShapeSize {
                    w: tw + h * 0.5 + 2.0 * PAD_X - 8.0,
                    h,
                    dy: 0.0,
                }
            },
            draw: |d, w, h| d.rect(-w / 2.0, -h / 2.0, w, h, h / 2.0, Painting::FillAndStroke),
            ..DEF
        },
    ),
    (
        "fr-rect",
        ShapeDef {
            span: Some(flat),
            size: |tw, th| padded(tw, th, 16.0, 0.0, 0.0),
            draw: |d, w, h| {
                let x = w / 2.0 - 8.0;
                d.rounded(-w / 2.0, -h / 2.0, w, h);
                d.pen()
                    .move_to(-x, -h / 2.0)
                    .v(h / 2.0)
                    .move_to(x, -h / 2.0)
                    .v(h / 2.0)
                    .done(Painting::StrokeOnly);
            },
            ..DEF
        },
    ),
    (
        "cyl",
        ShapeDef {
            inset: Some(cap_inset),
            span: Some(|w, h, sideways| {
                if sideways {
                    h - 2.0 * CORNER - 12.0
                } else {
                    w * 0.6
                }
            }),
            size: |tw, th| {
                let w = max(tw + 2.0 * PAD_X, 56.0);
                let ry = cap_ry(w);
                ShapeSize {
                    w,
                    h: th + 2.0 * PAD_Y + 2.0 * ry,
                    dy: ry / 2.0,
                }
            },
            draw: |d, w, h| {
                let (rx, ry) = (w / 2.0, cap_ry(w));
                let (top, bottom) = (-h / 2.0 + ry, h / 2.0 - ry);
                d.pen()
                    .move_to(-rx, top)
                    .arc(rx, ry, false, true, rx, top)
                    .v(bottom)
                    .arc(rx, ry, false, true, -rx, bottom)
                    .close()
                    .done(Painting::FillAndStroke);
                d.pen()
                    .move_to(-rx, top)
                    .arc(rx, ry, false, false, rx, top)
                    .done(Painting::StrokeOnly);
            },
            ..DEF
        },
    ),
    (
        "circle",
        ShapeDef {
            inset: Some(round_inset),
            span: Some(round_span),
            size: |tw, th| {
                let d = max((tw * tw + th * th).sqrt() + 16.0, 44.0);
                fixed(d, d)
            },
            draw: |d, w, _| d.circle(0.0, 0.0, w / 2.0, Painting::FillAndStroke),
            ..DEF
        },
    ),
    (
        "dbl-circ",
        ShapeDef {
            inset: Some(round_inset),
            span: Some(round_span),
            size: |tw, th| {
                let d = max((tw * tw + th * th).sqrt() + 26.0, 54.0);
                fixed(d, d)
            },
            draw: |d, w, _| {
                d.circle(0.0, 0.0, w / 2.0, Painting::FillAndStroke);
                d.circle(0.0, 0.0, w / 2.0 - 5.0, Painting::StrokeOnly);
            },
            ..DEF
        },
    ),
    (
        "ellipse",
        ShapeDef {
            inset: Some(round_inset),
            span: Some(round_span),
            size: |tw, th| ShapeSize {
                w: tw * 1.3 + 2.0 * PAD_X,
                h: th * 1.2 + 2.0 * PAD_Y,
                dy: 0.0,
            },
            draw: |d, w, h| d.ellipse(0.0, 0.0, w / 2.0, h / 2.0, Painting::FillAndStroke),
            ..DEF
        },
    ),
    (
        "diam",
        ShapeDef {
            inset: Some(|w, h, side, t| {
                if side & 1 != 0 {
                    (t.abs() * w) / h
                } else {
                    (t.abs() * h) / w
                }
            }),
            span: Some(round_span),
            size: |tw, th| {
                let (a, b) = (tw + 16.0, th + 8.0);
                ShapeSize {
                    w: a + 1.6 * b,
                    h: a / 1.6 + b,
                    dy: 0.0,
                }
            },
            draw: |d, w, h| {
                d.poly(
                    Painting::FillAndStroke,
                    &[0.0, -h / 2.0, w / 2.0, 0.0, 0.0, h / 2.0, -w / 2.0, 0.0],
                )
            },
            ..DEF
        },
    ),
    (
        "hex",
        ShapeDef {
            inset: Some(|_, _, side, t| if side & 1 != 0 { t.abs() / 2.0 } else { 0.0 }),
            span: Some(|w, h, sideways| {
                if sideways {
                    h * 0.5
                } else {
                    w - h / 2.0 - CORNER
                }
            }),
            size: |tw, th| by_height(tw, th, 0.5),
            draw: |d, w, h| {
                let m = h / 4.0;
                let (x, y) = (w / 2.0, h / 2.0);
                d.poly(
                    Painting::FillAndStroke,
                    &[-x + m, -y, x - m, -y, x, 0.0, x - m, y, -x + m, y, -x, 0.0],
                );
            },
            ..DEF
        },
    ),
    (
        "odd",
        ShapeDef {
            inset: Some(|_, h, side, _| if side == 3 { h / 4.0 } else { 0.0 }),
            span: Some(|w, _, sideways| flat_across(w, 0.0, sideways)),
            size: |tw, th| by_height(tw, th, 0.25),
            draw: |d, w, h| {
                let notch = h / 4.0;
                let (x, y) = (w / 2.0, h / 2.0);
                d.poly(
                    Painting::FillAndStroke,
                    &[-x, -y, x, -y, x, y, -x, y, -x + notch, 0.0],
                );
            },
            ..DEF
        },
    ),
    (
        "lean-r",
        ShapeDef {
            draw: |d, w, h| {
                let (s, x, y) = (h / 4.0, w / 2.0, h / 2.0);
                d.poly(
                    Painting::FillAndStroke,
                    &[-x + s, -y, x, -y, x - s, y, -x, y],
                );
            },
            ..LEANING
        },
    ),
    (
        "lean-l",
        ShapeDef {
            draw: |d, w, h| {
                let (s, x, y) = (h / 4.0, w / 2.0, h / 2.0);
                d.poly(
                    Painting::FillAndStroke,
                    &[-x, -y, x - s, -y, x, y, -x + s, y],
                );
            },
            ..LEANING
        },
    ),
    (
        "trap-b",
        ShapeDef {
            draw: |d, w, h| {
                let (s, x, y) = (h / 4.0, w / 2.0, h / 2.0);
                d.poly(
                    Painting::FillAndStroke,
                    &[-x + s, -y, x - s, -y, x, y, -x, y],
                );
            },
            ..LEANING
        },
    ),
    (
        "trap-t",
        ShapeDef {
            draw: |d, w, h| {
                let (s, x, y) = (h / 4.0, w / 2.0, h / 2.0);
                d.poly(
                    Painting::FillAndStroke,
                    &[-x, -y, x, -y, x - s, y, -x + s, y],
                );
            },
            ..LEANING
        },
    ),
    // Pele draws a text shape as a box with neither fill nor stroke, which shows nothing, and
    // here it has no parts.
    (
        "text",
        ShapeDef {
            bare: true,
            span: Some(flat),
            size: |tw, th| fixed(tw + 8.0, th + 8.0),
            ..DEF
        },
    ),
    (
        "sm-circ",
        ShapeDef {
            no_label: true,
            inset: Some(round_inset),
            span: Some(round_span),
            size: |_, _| fixed(14.0, 14.0),
            draw: |d, w, _| d.circle(0.0, 0.0, w / 2.0, Painting::FillAndStroke),
            ..DEF
        },
    ),
    (
        "f-circ",
        ShapeDef {
            no_label: true,
            inset: Some(round_inset),
            span: Some(round_span),
            size: |_, _| fixed(14.0, 14.0),
            draw: |d, w, _| d.circle(0.0, 0.0, w / 2.0, Painting::Solid),
            ..DEF
        },
    ),
    (
        "fr-circ",
        ShapeDef {
            no_label: true,
            inset: Some(round_inset),
            span: Some(round_span),
            size: |_, _| fixed(18.0, 18.0),
            draw: |d, w, _| {
                d.circle(0.0, 0.0, w / 2.0, Painting::FillAndStroke);
                d.circle(0.0, 0.0, w / 2.0 - 4.0, Painting::Solid);
            },
            ..DEF
        },
    ),
    (
        "fork",
        ShapeDef {
            no_label: true,
            span: Some(|w, h, sideways| (if sideways { h } else { w }) - 8.0),
            size: |_, _| fixed(72.0, 8.0),
            draw: |d, w, h| d.rect(-w / 2.0, -h / 2.0, w, h, 2.0, Painting::Solid),
            ..DEF
        },
    ),
    (
        "notch-rect",
        ShapeDef {
            span: Some(flat),
            draw: |d, w, h| {
                let (c, x, y) = (10.0, w / 2.0, h / 2.0);
                d.poly(
                    Painting::FillAndStroke,
                    &[-x + c, -y, x, -y, x, y, -x, y, -x, -y + c],
                );
            },
            ..DEF
        },
    ),
    (
        "lin-rect",
        ShapeDef {
            span: Some(flat),
            size: |tw, th| padded(tw, th, 8.0, 0.0, 0.0),
            draw: |d, w, h| {
                d.rounded(-w / 2.0, -h / 2.0, w, h);
                d.pen()
                    .move_to(-w / 2.0 + 8.0, -h / 2.0)
                    .v(h / 2.0)
                    .done(Painting::StrokeOnly);
            },
            ..DEF
        },
    ),
    (
        "div-rect",
        ShapeDef {
            span: Some(flat),
            size: |tw, th| padded(tw, th, 0.0, 8.0, 4.0),
            draw: |d, w, h| {
                d.rounded(-w / 2.0, -h / 2.0, w, h);
                d.pen()
                    .move_to(-w / 2.0, -h / 2.0 + 8.0)
                    .h(w / 2.0)
                    .done(Painting::StrokeOnly);
            },
            ..DEF
        },
    ),
    (
        "win-pane",
        ShapeDef {
            span: Some(flat),
            size: |tw, th| padded(tw, th, 8.0, 8.0, 4.0),
            draw: |d, w, h| {
                d.rounded(-w / 2.0, -h / 2.0, w, h);
                d.pen()
                    .move_to(-w / 2.0 + 8.0, -h / 2.0)
                    .v(h / 2.0)
                    .move_to(-w / 2.0, -h / 2.0 + 8.0)
                    .h(w / 2.0)
                    .done(Painting::StrokeOnly);
            },
            ..DEF
        },
    ),
    (
        "tri",
        ShapeDef {
            inset: Some(|w, _, side, _| if side & 1 != 0 { w / 4.0 } else { 0.0 }),
            size: |tw, th| tri_size(tw, th, false),
            draw: |d, w, h| {
                d.poly(
                    Painting::FillAndStroke,
                    &[0.0, -h / 2.0, w / 2.0, h / 2.0, -w / 2.0, h / 2.0],
                )
            },
            ..DEF
        },
    ),
    (
        "flip-tri",
        ShapeDef {
            inset: Some(|w, _, side, _| if side & 1 != 0 { w / 4.0 } else { 0.0 }),
            size: |tw, th| tri_size(tw, th, true),
            draw: |d, w, h| {
                d.poly(
                    Painting::FillAndStroke,
                    &[-w / 2.0, -h / 2.0, w / 2.0, -h / 2.0, 0.0, h / 2.0],
                )
            },
            ..DEF
        },
    ),
    (
        "sl-rect",
        ShapeDef {
            inset: Some(|_, _, side, _| if side == 0 { 5.0 } else { 0.0 }),
            size: |tw, th| padded(tw, th, 0.0, 10.0, 5.0),
            draw: |d, w, h| {
                let (x, y) = (w / 2.0, h / 2.0);
                d.poly(
                    Painting::FillAndStroke,
                    &[-x, -y + 10.0, x, -y, x, y, -x, y],
                );
            },
            ..DEF
        },
    ),
    (
        "notch-pent",
        ShapeDef {
            span: Some(flat),
            size: |tw, th| padded(tw, th, 0.0, 0.0, 0.0),
            draw: |d, w, h| {
                let (c, x, y) = (10.0, w / 2.0, h / 2.0);
                d.poly(
                    Painting::FillAndStroke,
                    &[-x + c, -y, x - c, -y, x, -y + c, x, y, -x, y, -x, -y + c],
                );
            },
            ..DEF
        },
    ),
    (
        "hourglass",
        ShapeDef {
            no_label: true,
            inset: Some(|w, _, side, _| if side & 1 != 0 { w / 2.0 } else { 0.0 }),
            size: |_, _| fixed(32.0, 40.0),
            draw: |d, w, h| {
                let (x, y) = (w / 2.0, h / 2.0);
                d.poly(Painting::FillAndStroke, &[-x, -y, x, -y, -x, y, x, y]);
            },
            ..DEF
        },
    ),
    (
        "delay",
        ShapeDef {
            size: |tw, th| by_height(tw, th, 0.25),
            draw: |d, w, h| {
                let r = h / 2.0;
                d.pen()
                    .move_to(-w / 2.0, -r)
                    .h(w / 2.0 - r)
                    .arc(r, r, false, true, w / 2.0 - r, r)
                    .h(-w / 2.0)
                    .close()
                    .done(Painting::FillAndStroke);
            },
            ..DEF
        },
    ),
    (
        "h-cyl",
        ShapeDef {
            size: |tw, th| {
                let h = th + 2.0 * PAD_Y;
                let rx = min(12.0, h / 4.0);
                ShapeSize {
                    w: tw + 2.0 * PAD_X + 2.0 * rx,
                    h,
                    dy: 0.0,
                }
            },
            draw: |d, w, h| {
                let (ry, rx) = (h / 2.0, min(12.0, h / 4.0));
                let (l, r) = (-w / 2.0 + rx, w / 2.0 - rx);
                d.pen()
                    .move_to(l, -ry)
                    .h(r)
                    .arc(rx, ry, false, true, r, ry)
                    .h(l)
                    .arc(rx, ry, false, true, l, -ry)
                    .close()
                    .done(Painting::FillAndStroke);
                d.pen()
                    .move_to(r, -ry)
                    .arc(rx, ry, false, false, r, ry)
                    .done(Painting::StrokeOnly);
            },
            ..DEF
        },
    ),
    (
        "cross-circ",
        ShapeDef {
            no_label: true,
            inset: Some(round_inset),
            span: Some(round_span),
            size: |_, _| fixed(44.0, 44.0),
            draw: |d, w, _| {
                let r = w / 2.0;
                let k = r * FRAC_1_SQRT_2;
                d.circle(0.0, 0.0, r, Painting::FillAndStroke);
                d.pen()
                    .move_to(-k, -k)
                    .line_to(k, k)
                    .move_to(k, -k)
                    .line_to(-k, k)
                    .done(Painting::StrokeOnly);
            },
            ..DEF
        },
    ),
    (
        "st-rect",
        ShapeDef {
            span: Some(|w, h, sideways| (if sideways { h } else { w }) - 2.0 * CORNER - 8.0),
            size: |tw, th| padded(tw, th, 8.0, 8.0, 4.0),
            draw: |d, w, h| {
                let (bw, bh) = (w - 8.0, h - 8.0);
                for i in 0..3 {
                    d.rounded(
                        -w / 2.0 + f64::from(2 - i) * 4.0,
                        -h / 2.0 + f64::from(i) * 4.0,
                        bw,
                        bh,
                    );
                }
            },
            ..DEF
        },
    ),
    (
        "doc",
        ShapeDef {
            inset: Some(|_, _, side, _| if side == 2 { 10.0 } else { 0.0 }),
            size: |tw, th| padded(tw, th, 0.0, 10.0, -5.0),
            draw: |d, w, h| d.wave(w, h),
            ..DEF
        },
    ),
    (
        "flag",
        ShapeDef {
            inset: Some(|_, _, side, _| if side & 1 != 0 { 0.0 } else { 8.0 }),
            size: |tw, th| padded(tw, th, 0.0, 16.0, 0.0),
            draw: |d, w, h| {
                let (x, y, q) = (w / 2.0, h / 2.0 - 8.0, w / 4.0);
                d.pen()
                    .move_to(-x, -y)
                    .rel_quad(q, -10.0, 2.0 * q, 0.0)
                    .rel_smooth(2.0 * q, 0.0)
                    .v(y)
                    .rel_quad(-q, 10.0, -2.0 * q, 0.0)
                    .rel_smooth(-2.0 * q, 0.0)
                    .close()
                    .done(Painting::FillAndStroke);
            },
            ..DEF
        },
    ),
    (
        "datastore",
        ShapeDef {
            span: Some(flat),
            draw: |d, w, h| {
                let r = Rect::new(d.cx - w / 2.0, d.cy - h / 2.0, w, h);
                d.parts.push(Part::rect(r, false, Painting::FillOnly));
                d.pen()
                    .move_to(-w / 2.0, -h / 2.0)
                    .h(w / 2.0)
                    .move_to(-w / 2.0, h / 2.0)
                    .h(w / 2.0)
                    .done(Painting::StrokeOnly);
            },
            ..DEF
        },
    ),
    (
        "folder",
        ShapeDef {
            size: |tw, th| ShapeSize {
                w: max(tw + 2.0 * PAD_X, 64.0),
                h: th + 2.0 * PAD_Y + 8.0,
                dy: 4.0,
            },
            draw: |d, w, h| {
                let (x, y) = (w / 2.0, h / 2.0);
                let tab = min(w * 0.4, 48.0);
                d.pen()
                    .move_to(-x, -y)
                    .h(-x + tab)
                    .line_to(-x + tab + 8.0, -y + 8.0)
                    .h(x)
                    .v(y)
                    .h(-x)
                    .close()
                    .done(Painting::FillAndStroke);
            },
            ..DEF
        },
    ),
    (
        "bucket",
        ShapeDef {
            size: |tw, th| padded(tw, th, 20.0, 10.0, 3.0),
            inset: Some(|_, h, side, _| if side & 1 != 0 { h / 12.0 } else { 0.0 }),
            draw: |d, w, h| {
                let (x, y, ry) = (w / 2.0, h / 2.0, 5.0);
                let taper = h / 6.0;
                d.pen()
                    .move_to(-x, -y + ry)
                    .arc(x, ry, false, true, x, -y + ry)
                    .line_to(x - taper, y - ry)
                    .arc(x - taper, ry, false, true, -x + taper, y - ry)
                    .close()
                    .done(Painting::FillAndStroke);
                d.pen()
                    .move_to(-x, -y + ry)
                    .arc(x, ry, false, false, x, -y + ry)
                    .done(Painting::StrokeOnly);
            },
            ..DEF
        },
    ),
    (
        "console",
        ShapeDef {
            span: Some(flat),
            size: |tw, th| ShapeSize {
                w: max(tw + 2.0 * PAD_X, 72.0),
                h: th + 2.0 * PAD_Y + 12.0,
                dy: 6.0,
            },
            draw: |d, w, h| {
                d.rounded(-w / 2.0, -h / 2.0, w, h);
                d.pen()
                    .move_to(-w / 2.0, -h / 2.0 + 12.0)
                    .h(w / 2.0)
                    .move_to(-w / 2.0 + 7.0, -h / 2.0 + 4.0)
                    .rel_line(3.0, 2.5)
                    .rel_line(-3.0, 2.5)
                    .move_to(-w / 2.0 + 13.0, -h / 2.0 + 9.0)
                    .rel_h(4.0)
                    .done(Painting::StrokeOnly);
            },
            ..DEF
        },
    ),
    (
        "browser",
        ShapeDef {
            span: Some(flat),
            size: |tw, th| ShapeSize {
                w: max(tw + 2.0 * PAD_X, 72.0),
                h: th + 2.0 * PAD_Y + 12.0,
                dy: 6.0,
            },
            draw: |d, w, h| {
                d.rounded(-w / 2.0, -h / 2.0, w, h);
                d.pen()
                    .move_to(-w / 2.0, -h / 2.0 + 12.0)
                    .h(w / 2.0)
                    .done(Painting::StrokeOnly);
                for i in 0..3 {
                    d.circle(
                        -w / 2.0 + 8.0 + f64::from(i) * 7.0,
                        -h / 2.0 + 6.0,
                        1.5,
                        Painting::StrokeOnly,
                    );
                }
            },
            ..DEF
        },
    ),
    (
        "person",
        ShapeDef {
            size: |tw, th| ShapeSize {
                w: max(tw + 2.0 * PAD_X, 56.0),
                h: th + 2.0 * PAD_Y + 26.0,
                dy: 13.0,
            },
            draw: |d, w, h| {
                let top = -h / 2.0;
                d.rect(
                    -w / 2.0,
                    top + 26.0,
                    w,
                    h - 26.0,
                    12.0,
                    Painting::FillAndStroke,
                );
                d.circle(0.0, top + 14.0, 13.0, Painting::FillAndStroke);
            },
            ..DEF
        },
    ),
    (
        "bang",
        ShapeDef {
            size: |tw, th| padded(tw, th, 28.0, 24.0, 0.0),
            inset: Some(|_, _, _, _| 3.0),
            draw: |d, w, h| {
                const SPIKES: f64 = 14.0;
                let mut pts = Vec::with_capacity(56);
                for i in 0..28 {
                    let angle = (PI * f64::from(i)) / SPIKES - PI / 2.0;
                    let k = if i % 2 == 0 { 1.0 } else { 0.82 };
                    pts.push(cos(angle) * (w / 2.0) * k);
                    pts.push(sin(angle) * (h / 2.0) * k);
                }
                d.poly(Painting::FillAndStroke, &pts);
            },
            ..DEF
        },
    ),
    (
        "cloud",
        ShapeDef {
            size: |tw, th| padded(tw, th, 32.0, 24.0, 0.0),
            inset: Some(|_, _, side, _| if side & 1 != 0 { 2.0 } else { 1.0 }),
            draw: |d, w, h| {
                let (x, y) = (w / 2.0, h / 2.0);
                let r = y * 0.55;
                d.pen()
                    .move_to(-x + r, y)
                    .arc(r, r, false, true, -x + r * 0.6, -y * 0.1)
                    .arc(x * 0.42, y * 0.7, false, true, -x * 0.1, -y * 0.72)
                    .arc(x * 0.4, y * 0.62, false, true, x * 0.62, -y * 0.42)
                    .arc(r * 1.05, r * 1.05, false, true, x - r, y)
                    .close()
                    .done(Painting::FillAndStroke);
            },
            ..DEF
        },
    ),
    // Pele draws the box of a brace with neither fill nor stroke, and here it has no part.
    (
        "brace",
        ShapeDef {
            bare: true,
            size: |tw, th| padded(tw, th, 12.0, 0.0, 0.0),
            draw: |d, w, h| d.brace(-w / 2.0 + 12.0, h, -1.0),
            ..DEF
        },
    ),
    (
        "brace-r",
        ShapeDef {
            bare: true,
            size: |tw, th| padded(tw, th, 12.0, 0.0, 0.0),
            draw: |d, w, h| d.brace(w / 2.0 - 12.0, h, 1.0),
            ..DEF
        },
    ),
    (
        "braces",
        ShapeDef {
            bare: true,
            size: |tw, th| padded(tw, th, 24.0, 0.0, 0.0),
            draw: |d, w, h| {
                d.brace(-w / 2.0 + 12.0, h, -1.0);
                d.brace(w / 2.0 - 12.0, h, 1.0);
            },
            ..DEF
        },
    ),
    (
        "bolt",
        ShapeDef {
            no_label: true,
            size: |_, _| fixed(36.0, 56.0),
            inset: Some(|w, _, side, _| if side & 1 != 0 { w / 4.0 } else { 0.0 }),
            draw: |d, w, h| {
                d.poly(
                    Painting::FillAndStroke,
                    &[
                        w * 0.15,
                        -h / 2.0,
                        -w / 2.0,
                        h * 0.08,
                        -w * 0.05,
                        h * 0.08,
                        -w * 0.15,
                        h / 2.0,
                        w / 2.0,
                        -h * 0.08,
                        w * 0.05,
                        -h * 0.08,
                    ],
                )
            },
            ..DEF
        },
    ),
    (
        "lin-cyl",
        ShapeDef {
            inset: Some(cap_inset),
            span: Some(|w, h, sideways| {
                if sideways {
                    h - 2.0 * CORNER - 20.0
                } else {
                    w * 0.6
                }
            }),
            size: |tw, th| {
                let w = max(tw + 2.0 * PAD_X, 56.0);
                let ry = cap_ry(w);
                ShapeSize {
                    w,
                    h: th + 2.0 * PAD_Y + 2.0 * ry + 8.0,
                    dy: ry / 2.0 + 4.0,
                }
            },
            draw: |d, w, h| {
                let (rx, ry) = (w / 2.0, cap_ry(w));
                let (top, bottom) = (-h / 2.0 + ry, h / 2.0 - ry);
                d.pen()
                    .move_to(-rx, top)
                    .arc(rx, ry, false, true, rx, top)
                    .v(bottom)
                    .arc(rx, ry, false, true, -rx, bottom)
                    .close()
                    .done(Painting::FillAndStroke);
                d.pen()
                    .move_to(-rx, top)
                    .arc(rx, ry, false, false, rx, top)
                    .move_to(-rx, top + 8.0)
                    .arc(rx, ry, false, false, rx, top + 8.0)
                    .done(Painting::StrokeOnly);
            },
            ..DEF
        },
    ),
    (
        "curv-trap",
        ShapeDef {
            size: |tw, th| by_height(tw, th, 0.6),
            draw: |d, w, h| {
                let (x, y, k) = (w / 2.0, h / 2.0, h * 0.3);
                d.pen()
                    .move_to(-x + k, -y)
                    .h(x - k)
                    .arc(k, y, false, true, x - k, y)
                    .h(-x + k)
                    .line_to(-x, 0.0)
                    .close()
                    .done(Painting::FillAndStroke);
            },
            ..DEF
        },
    ),
    (
        "docs",
        ShapeDef {
            size: |tw, th| padded(tw, th, 8.0, 18.0, -1.0),
            inset: Some(|_, _, side, _| if side == 2 { 10.0 } else { 0.0 }),
            draw: |d, w, h| {
                for i in 0..3 {
                    let mut moved = Drawing {
                        cx: d.cx + f64::from(1 - i) * 4.0,
                        cy: d.cy + f64::from(i - 1) * 4.0,
                        parts: Vec::new(),
                    };
                    moved.wave(w - 8.0, h - 8.0);
                    d.parts.append(&mut moved.parts);
                }
            },
            ..DEF
        },
    ),
    (
        "bow-rect",
        ShapeDef {
            size: |tw, th| by_height(tw, th, 0.4),
            inset: Some(|_, h, side, _| if side == 1 { h * 0.2 } else { 0.0 }),
            draw: |d, w, h| {
                let (x, y, k) = (w / 2.0, h / 2.0, h * 0.2);
                d.pen()
                    .move_to(-x + k, -y)
                    .h(x)
                    .arc(k, y, false, false, x, y)
                    .h(-x + k)
                    .arc(k, y, false, true, -x + k, -y)
                    .close()
                    .done(Painting::FillAndStroke);
            },
            ..DEF
        },
    ),
    (
        "tag-rect",
        ShapeDef {
            span: Some(flat),
            draw: |d, w, h| {
                d.rounded(-w / 2.0, -h / 2.0, w, h);
                d.pen()
                    .move_to(w / 2.0 - 12.0, h / 2.0)
                    .line_to(w / 2.0, h / 2.0 - 12.0)
                    .done(Painting::StrokeOnly);
            },
            ..DEF
        },
    ),
    (
        "tag-doc",
        ShapeDef {
            size: |tw, th| padded(tw, th, 0.0, 10.0, -5.0),
            inset: Some(|_, _, side, _| if side == 2 { 10.0 } else { 0.0 }),
            draw: |d, w, h| {
                d.wave(w, h);
                d.pen()
                    .move_to(w / 2.0 - 12.0, h / 2.0 - 14.0)
                    .line_to(w / 2.0, h / 2.0 - 26.0)
                    .done(Painting::StrokeOnly);
            },
            ..DEF
        },
    ),
    (
        "lin-doc",
        ShapeDef {
            size: |tw, th| padded(tw, th, 8.0, 10.0, -5.0),
            inset: Some(|_, _, side, _| if side == 2 { 10.0 } else { 0.0 }),
            draw: |d, w, h| {
                d.wave(w, h);
                d.pen()
                    .move_to(-w / 2.0 + 8.0, -h / 2.0)
                    .v(h / 2.0 - 6.0)
                    .done(Painting::StrokeOnly);
            },
            ..DEF
        },
    ),
    (
        "collapsedGroup",
        ShapeDef {
            size: |tw, th| padded(tw, th, 8.0, 8.0, -2.0),
            draw: |d, w, h| {
                d.rounded(-w / 2.0 + 6.0, -h / 2.0 + 8.0, w - 12.0, h - 8.0);
                d.rounded(-w / 2.0, -h / 2.0, w, h - 4.0);
            },
            ..DEF
        },
    ),
];

/// The shape of a name, as Pele's BY_NAME finds it.
fn by_name(shape: &str) -> Option<&'static ShapeDef> {
    SHAPES
        .iter()
        .find(|(name, _)| *name == shape)
        .map(|(_, def)| def)
}

/// The shape of a name, or rect, Pele's FALLBACK, where there is none.
fn or_fallback(shape: &str) -> &'static ShapeDef {
    by_name(shape).unwrap_or(&SHAPES[0].1)
}

/// The names of the shapes, in Pele's order.
pub(crate) fn shape_names() -> impl Iterator<Item = &'static str> {
    SHAPES.iter().map(|(name, _)| *name)
}

/// The size of a shape drawn about a label tw by th, as Pele's shapeSize gives it.
pub(crate) fn shape_size(shape: &str, tw: f64, th: f64) -> ShapeSize {
    (or_fallback(shape).size)(tw, th)
}

/// The outline of a shape w by h centered on cx, cy, as Pele's drawShape draws it: a part for
/// each element Pele writes, in its order, but none for those it writes with neither fill
/// nor stroke, which show nothing. A text shape has no parts.
pub(crate) fn draw_shape(shape: &str, cx: f64, cy: f64, w: f64, h: f64) -> Vec<Part> {
    let mut d = Drawing {
        cx,
        cy,
        parts: Vec::new(),
    };
    (or_fallback(shape).draw)(&mut d, w, h);
    d.parts
}

/// How far the outline of a shape w by h lies in from its bounding box on a side, 0 top, 1
/// right, 2 bottom and 3 left, t along the side from its middle, as Pele's shapeInset gives
/// it.
pub(crate) fn shape_inset(shape: &str, w: f64, h: f64, side: usize, t: f64) -> f64 {
    by_name(shape)
        .and_then(|s| s.inset)
        .map_or(0.0, |inset| inset(w, h, side, t))
}

/// How much of a side of a shape w by h edges may spread along, when the flow runs down or,
/// if sideways, across, as Pele's shapeSpan gives it.
pub(crate) fn shape_span(shape: &str, w: f64, h: f64, sideways: bool) -> f64 {
    max(
        0.0,
        by_name(shape)
            .and_then(|s| s.span)
            .map_or(0.0, |span| span(w, h, sideways)),
    )
}

/// Whether a shape is drawn with a label, as Pele's shapeHasLabel says.
pub(crate) fn shape_has_label(shape: &str) -> bool {
    by_name(shape).is_none_or(|s| !s.no_label)
}

/// Whether a shape is drawn with a fill its label lies on, as Pele's shapeFilled says.
pub(crate) fn shape_filled(shape: &str) -> bool {
    !or_fallback(shape).bare
}

/// A box laid out, as much of Pele's CNode as the drawing needs: its center, its size, and
/// whether it is a group of others.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Node {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
    pub(crate) group: bool,
}

/// The side of a node's box that the end of a route at route[at..at + 2] lies on, 0 top, 1
/// right, 2 bottom and 3 left, or none, as Pele's endSide gives it.
pub(crate) fn end_side(route: &[f64], at: usize, c: &Node) -> Option<usize> {
    let dx = route[at] - c.x;
    let dy = route[at + 1] - c.y;
    if (dy.abs() - c.h / 2.0).abs() < 0.5 {
        return Some(if dy < 0.0 { 0 } else { 2 });
    }
    if (dx.abs() - c.w / 2.0).abs() < 0.5 {
        return Some(if dx < 0.0 { 3 } else { 1 });
    }
    None
}

/// Moves the end of a route at route[at..at + 2] from a node's box onto the outline of the
/// shape drawn in it, w by h, as Pele's insetRoute does. A group's ends stay.
pub(crate) fn inset_route(route: &mut [f64], at: usize, shape: &str, w: f64, h: f64, c: &Node) {
    if c.group {
        return;
    }
    match end_side(route, at, c) {
        Some(side @ (0 | 2)) => {
            let amount = shape_inset(shape, w, h, side, route[at] - c.x) + (c.h - h) / 2.0;
            route[at + 1] += if side == 0 { amount } else { -amount };
        }
        Some(side) => {
            let amount = shape_inset(shape, w, h, side, route[at + 1] - c.y) + (c.w - w) / 2.0;
            route[at] += if side == 3 { amount } else { -amount };
        }
        None => {}
    }
}

/// The parts of a shape centered on cx, cy. The shapes are written in their own frame,
/// centered on the origin, as Pele writes them.
struct Drawing {
    cx: f64,
    cy: f64,
    parts: Vec<Part>,
}

impl Drawing {
    /// A box with the corners of the theme's radius.
    fn rounded(&mut self, x: f64, y: f64, w: f64, h: f64) {
        let r = Rect::new(self.cx + x, self.cy + y, w, h);
        self.parts
            .push(Part::rect(r, true, Painting::FillAndStroke));
    }

    /// A box with corners rounded by rx, as SVG draws a rect with rx and no ry: the corners
    /// are rx across and as much down, each made no larger than half the side.
    #[allow(clippy::too_many_arguments)]
    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, rx: f64, paint: Painting) {
        let (rx, ry) = (min(rx, w / 2.0), min(rx, h / 2.0));
        if rx <= 0.0 {
            self.poly(paint, &[x, y, x + w, y, x + w, y + h, x, y + h]);
            return;
        }
        self.pen()
            .move_to(x + rx, y)
            .h(x + w - rx)
            .arc(rx, ry, false, true, x + w, y + ry)
            .v(y + h - ry)
            .arc(rx, ry, false, true, x + w - rx, y + h)
            .h(x + rx)
            .arc(rx, ry, false, true, x, y + h - ry)
            .v(y + ry)
            .arc(rx, ry, false, true, x + rx, y)
            .close()
            .done(paint);
    }

    fn circle(&mut self, x: f64, y: f64, r: f64, paint: Painting) {
        self.ellipse(x, y, r, r, paint);
    }

    fn ellipse(&mut self, x: f64, y: f64, rx: f64, ry: f64, paint: Painting) {
        let mut p = Path::new();
        p.ellipse(self.cx + x, self.cy + y, rx, ry);
        self.parts.push(Part::path(p, paint));
    }

    /// A polygon of the points xy holds, x and y by turns.
    fn poly(&mut self, paint: Painting, xy: &[f64]) {
        let at: Vec<(f64, f64)> = xy
            .chunks(2)
            .map(|p| (self.cx + p[0], self.cy + p[1]))
            .collect();
        let mut p = Path::new();
        p.polygon(&at);
        self.parts.push(Part::path(p, paint));
    }

    /// A document: a box whose bottom runs in a wave, as Pele's wave draws it.
    fn wave(&mut self, w: f64, h: f64) {
        let (x, top, base, q) = (w / 2.0, -h / 2.0, h / 2.0 - 10.0, w / 4.0);
        self.pen()
            .move_to(-x, top)
            .h(x)
            .v(base)
            .rel_quad(-q, -12.0, -2.0 * q, 0.0)
            .rel_smooth(-2.0 * q, 0.0)
            .close()
            .done(Painting::FillAndStroke);
    }

    /// A curly brace at x, h high, opening to the right when dir is -1, as Pele's brace draws
    /// it.
    fn brace(&mut self, x: f64, h: f64, dir: f64) {
        let (y, k) = (h / 2.0, 6.0 * dir);
        self.pen()
            .move_to(x, -y)
            .rel_quad(k, 0.0, k, 6.0)
            .v(-6.0)
            .rel_quad(0.0, 6.0, k, 6.0)
            .rel_quad(-k, 0.0, -k, 6.0)
            .v(y - 6.0)
            .rel_quad(0.0, 6.0, -k, 6.0)
            .done(Painting::StrokeOnly);
    }

    fn pen(&mut self) -> Pen<'_> {
        Pen {
            d: self,
            p: Path::new(),
            x: 0.0,
            y: 0.0,
            x0: 0.0,
            y0: 0.0,
            qx: 0.0,
            qy: 0.0,
            quad: false,
        }
    }
}

/// Follows SVG's path commands in the shape's frame: where it is, where the figure began, and
/// the control point of the last quadratic curve, which a smooth one reflects. It takes each
/// number it is given to hundredths, as Pele writes it, so that an arc whose radii barely
/// reach across it is the arc Pele draws, as the turn of such an arc follows the last
/// decimal. Where a command is relative, or reflects a control point, the point is worked out
/// from those as SVG reads them, and is not taken to hundredths again.
struct Pen<'a> {
    d: &'a mut Drawing,
    p: Path,
    x: f64,
    y: f64,
    x0: f64,
    y0: f64,
    qx: f64,
    qy: f64,
    quad: bool,
}

impl Pen<'_> {
    fn at(&mut self, x: f64, y: f64) -> &mut Self {
        (self.x, self.y, self.quad) = (x, y, false);
        self
    }

    /// SVG's M.
    fn move_to(&mut self, x: f64, y: f64) -> &mut Self {
        let (x, y) = (r2(x), r2(y));
        self.p.move_to(self.d.cx + x, self.d.cy + y);
        (self.x0, self.y0) = (x, y);
        self.at(x, y)
    }

    /// SVG's L.
    fn line_to(&mut self, x: f64, y: f64) -> &mut Self {
        self.line(r2(x), r2(y))
    }

    /// SVG's H.
    fn h(&mut self, x: f64) -> &mut Self {
        self.line(r2(x), self.y)
    }

    /// SVG's V.
    fn v(&mut self, y: f64) -> &mut Self {
        self.line(self.x, r2(y))
    }

    /// SVG's l.
    fn rel_line(&mut self, dx: f64, dy: f64) -> &mut Self {
        self.line(self.x + r2(dx), self.y + r2(dy))
    }

    /// SVG's h.
    fn rel_h(&mut self, dx: f64) -> &mut Self {
        self.line(self.x + r2(dx), self.y)
    }

    /// A line to x, y, which are as SVG reads them.
    fn line(&mut self, x: f64, y: f64) -> &mut Self {
        self.p.line_to(self.d.cx + x, self.d.cy + y);
        self.at(x, y)
    }

    /// SVG's A, with no rotation.
    fn arc(&mut self, rx: f64, ry: f64, large: bool, sweep: bool, x: f64, y: f64) -> &mut Self {
        let (x, y) = (r2(x), r2(y));
        self.p.arc_to(
            r2(rx),
            r2(ry),
            0.0,
            large,
            sweep,
            self.d.cx + x,
            self.d.cy + y,
        );
        self.at(x, y)
    }

    /// SVG's q.
    fn rel_quad(&mut self, dcx: f64, dcy: f64, dx: f64, dy: f64) -> &mut Self {
        let (x, y) = (self.x, self.y);
        self.quad_to(x + r2(dcx), y + r2(dcy), x + r2(dx), y + r2(dy))
    }

    /// SVG's t: a quadratic curve by dx, dy whose control point is the last one's reflected
    /// about where the pen is.
    fn rel_smooth(&mut self, dx: f64, dy: f64) -> &mut Self {
        let (cx, cy) = if self.quad {
            (2.0 * self.x - self.qx, 2.0 * self.y - self.qy)
        } else {
            (self.x, self.y)
        };
        let (x, y) = (self.x, self.y);
        self.quad_to(cx, cy, x + r2(dx), y + r2(dy))
    }

    /// A quadratic curve to x, y about cx, cy, which are as SVG reads them.
    fn quad_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) -> &mut Self {
        let (ox, oy) = (self.d.cx, self.d.cy);
        self.p.quad_to(ox + cx, oy + cy, ox + x, oy + y);
        self.at(x, y);
        (self.qx, self.qy, self.quad) = (cx, cy, true);
        self
    }

    /// SVG's Z.
    fn close(&mut self) -> &mut Self {
        self.p.close();
        let (x, y) = (self.x0, self.y0);
        self.at(x, y)
    }

    fn done(&mut self, paint: Painting) {
        let p = std::mem::take(&mut self.p);
        self.d.parts.push(Part::path(p, paint));
    }
}

/// A number to hundredths, as Pele's num writes it.
fn r2(v: f64) -> f64 {
    round(v * 100.0) / 100.0
}
