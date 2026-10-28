//! The lines of a C4 diagram's relations, as Pele's route.ts routes them: straight where
//! nothing is in the way, around the shapes that are otherwise, and apart from the lines
//! routed before them.

use std::collections::HashMap;

use crate::js::math::{hypot, max, min, or1, sign};
use crate::scene::Path;

/// A laid out shape or boundary, Pele's Box: its top-left corner, its size, and the shared
/// shape it is drawn as. Pele tells boxes apart by what they are, and here each has an id of
/// its own.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Box {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
    pub(crate) form: &'static str,
    pub(crate) id: usize,
}

/// A relation's line, Pele's Route. Its path is in the frame the diagram is laid out in.
pub(crate) struct Route {
    pub(crate) path: Path,
    // Where the markers go: the tip, and the way it points.
    pub(crate) sx: f64,
    pub(crate) sy: f64,
    pub(crate) sdx: f64,
    pub(crate) sdy: f64,
    pub(crate) ex: f64,
    pub(crate) ey: f64,
    pub(crate) edx: f64,
    pub(crate) edy: f64,
    // The stretch the label may sit on.
    pub(crate) x0: f64,
    pub(crate) y0: f64,
    pub(crate) x1: f64,
    pub(crate) y1: f64,
    /// The side of that stretch the label is pushed to, as a unit vector. Zero centers it on
    /// the stretch.
    pub(crate) nx: f64,
    pub(crate) ny: f64,
    /// When set, the label stays over the line and moves that way by at most this much.
    pub(crate) lean: f64,
    /// The widest the label should be.
    pub(crate) room: f64,
    /// The corners of the line, as x, y pairs.
    pub(crate) points: Vec<f64>,
}

const LOOP: f64 = 28.0;
pub(crate) const LANE: f64 = 20.0;
const LANE_STEP: f64 = 7.0;
const CORNER: f64 = 8.0;
// A wide label on an upright lane leans this far out from it, to the middle of the gap
// between columns.
const LEAN: f64 = 30.0;
// Label widths that fit the gap between two columns, and where there is room to spare.
pub(crate) const NARROW: f64 = 88.0;
const WIDE: f64 = 160.0;
/// The space a label's background adds to the width of its text.
pub(crate) const LABEL_PAD: f64 = 8.0;
/// The shared person shape: a head of radius 13 centered 14 below the top, over a body that
/// starts 26 below it.
pub(crate) const HEAD: f64 = 26.0;

/// The cap radius drawShape gives a cylinder, as Pele's cylRy gives it.
pub(crate) fn cyl_ry(w: f64) -> f64 {
    min(12.0, max(6.0, w / 14.0))
}

/// The cap radius drawShape gives a queue, as Pele's queueRx gives it.
pub(crate) fn queue_rx(h: f64) -> f64 {
    min(12.0, h / 4.0)
}

/// How far along the segment from (px, py) by (dx, dy) it first meets the rectangle, as
/// Pele's hitRect finds it, and Infinity when it misses.
#[allow(clippy::too_many_arguments)]
fn hit_rect(x0: f64, y0: f64, x1: f64, y1: f64, px: f64, py: f64, dx: f64, dy: f64) -> f64 {
    let mut enter = 0.0;
    let mut leave = 1.0;
    if dx == 0.0 {
        if px < x0 || px > x1 {
            return f64::INFINITY;
        }
    } else {
        let a = (x0 - px) / dx;
        let b = (x1 - px) / dx;
        enter = max(enter, min(a, b));
        leave = min(leave, max(a, b));
    }
    if dy == 0.0 {
        if py < y0 || py > y1 {
            return f64::INFINITY;
        }
    } else {
        let a = (y0 - py) / dy;
        let b = (y1 - py) / dy;
        enter = max(enter, min(a, b));
        leave = min(leave, max(a, b));
    }
    if enter <= leave { enter } else { f64::INFINITY }
}

/// Where the segment first meets an ellipse, as Pele's hitEllipse finds it.
#[allow(clippy::too_many_arguments)]
fn hit_ellipse(cx: f64, cy: f64, rx: f64, ry: f64, px: f64, py: f64, dx: f64, dy: f64) -> f64 {
    let ux = dx / rx;
    let uy = dy / ry;
    let vx = (px - cx) / rx;
    let vy = (py - cy) / ry;
    let a = ux * ux + uy * uy;
    let b = 2.0 * (ux * vx + uy * vy);
    let c = vx * vx + vy * vy - 1.0;
    let disc = b * b - 4.0 * a * c;
    if a == 0.0 || disc < 0.0 {
        return f64::INFINITY;
    }
    let t = (-b - disc.sqrt()) / (2.0 * a);
    if (0.0..=1.0).contains(&t) {
        t
    } else if c <= 0.0 {
        0.0
    } else {
        f64::INFINITY
    }
}

/// Where a segment from outside first meets the outline of a shape, as a fraction of the
/// segment, as Pele's hit finds it.
fn hit(b: &Box, px: f64, py: f64, qx: f64, qy: f64) -> f64 {
    let Box { x, y, w, h, .. } = *b;
    let dx = qx - px;
    let dy = qy - py;
    match b.form {
        "person" => min(
            hit_rect(x, y + HEAD, x + w, y + h, px, py, dx, dy),
            hit_ellipse(x + w / 2.0, y + 14.0, 13.0, 13.0, px, py, dx, dy),
        ),
        "cyl" => {
            let ry = cyl_ry(w);
            min(
                min(
                    hit_rect(x, y + ry, x + w, y + h - ry, px, py, dx, dy),
                    hit_ellipse(x + w / 2.0, y + ry, w / 2.0, ry, px, py, dx, dy),
                ),
                hit_ellipse(x + w / 2.0, y + h - ry, w / 2.0, ry, px, py, dx, dy),
            )
        }
        "h-cyl" => {
            let rx = queue_rx(h);
            min(
                min(
                    hit_rect(x + rx, y, x + w - rx, y + h, px, py, dx, dy),
                    hit_ellipse(x + rx, y + h / 2.0, rx, h / 2.0, px, py, dx, dy),
                ),
                hit_ellipse(x + w - rx, y + h / 2.0, rx, h / 2.0, px, py, dx, dy),
            )
        }
        _ => hit_rect(x, y, x + w, y + h, px, py, dx, dy),
    }
}

/// Whether a line with corners through points crosses a rectangle, as Pele's crosses says.
pub(crate) fn crosses(points: &[f64], x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
    let mut i = 2;
    while i < points.len() {
        let px = points[i - 2];
        let py = points[i - 1];
        if hit_rect(x0, y0, x1, y1, px, py, points[i] - px, points[i + 1] - py) <= 1.0 {
            return true;
        }
        i += 2;
    }
    false
}

/// Whether one box holds another, as Pele's contains says.
fn contains(outer: &Box, inner: &Box) -> bool {
    outer.x <= inner.x
        && outer.y <= inner.y
        && outer.x + outer.w >= inner.x + inner.w
        && outer.y + outer.h >= inner.y + inner.h
}

/// The straight line from a to b, as Pele's connect draws it: x, y of its start, then of its
/// end.
fn connect(a: &Box, b: &Box, shift: f64) -> (f64, f64, f64, f64) {
    let ax = a.x + a.w / 2.0;
    let ay = a.y + a.h / 2.0;
    let bx = b.x + b.w / 2.0;
    let by = b.y + b.h / 2.0;
    let down = contains(a, b);
    if down || contains(b, a) {
        // One holds the other: a short line from the nearest side of the outer box.
        let (outer, inner) = if down { (a, b) } else { (b, a) };
        let ix = inner.x + inner.w / 2.0;
        let iy = inner.y + inner.h / 2.0;
        let left = inner.x - outer.x;
        let right = outer.x + outer.w - inner.x - inner.w;
        let top = inner.y - outer.y;
        let bottom = outer.y + outer.h - inner.y - inner.h;
        let least = min(min(left, right), min(top, bottom));
        let ox = if least == left {
            outer.x
        } else if least == right {
            outer.x + outer.w
        } else {
            ix
        };
        let oy = if least == left || least == right {
            iy
        } else if least == bottom {
            outer.y + outer.h
        } else {
            outer.y
        };
        let t = min(hit(inner, ox, oy, ix, iy), 1.0);
        let mx = ox + (ix - ox) * t;
        let my = oy + (iy - oy) * t;
        return if down {
            (ox, oy, mx, my)
        } else {
            (mx, my, ox, oy)
        };
    }

    let (mut px, mut py, mut qx, mut qy) = (ax, ay, bx, by);
    // Shapes that share a column or a row are joined by an upright or level line through the
    // shared span.
    let x0 = max(a.x, b.x);
    let x1 = min(a.x + a.w, b.x + b.w);
    let y0 = max(a.y, b.y);
    let y1 = min(a.y + a.h, b.y + b.h);
    if x0 < x1 {
        px = (x0 + x1) / 2.0;
        qx = px;
    } else if y0 < y1 {
        py = (y0 + y1) / 2.0;
        qy = py;
    }
    let len = or1(hypot(qx - px, qy - py));
    let nx = (-(qy - py) / len) * shift;
    let ny = ((qx - px) / len) * shift;
    let mut ta = hit(a, qx + nx, qy + ny, px + nx, py + ny);
    let mut tb = hit(b, px + nx, py + ny, qx + nx, qy + ny);
    if ta <= 1.0 && tb <= 1.0 {
        px += nx;
        py += ny;
        qx += nx;
        qy += ny;
    } else {
        (px, py, qx, qy) = (ax, ay, bx, by);
        ta = min(hit(a, qx, qy, px, py), 1.0);
        tb = min(hit(b, px, py, qx, qy), 1.0);
    }
    (
        qx + (px - qx) * ta,
        qy + (py - qy) * ta,
        px + (qx - px) * tb,
        py + (qy - py) * tb,
    )
}

// Parallel stretches closer than this read as one line, and a line this close to a
// boundary's frame reads as part of it.
const NEAR: f64 = 6.0;
const FRAME_NEAR: f64 = 3.0;
const PORT_STEP: f64 = 12.0;

/// What the routes drawn so far take up, so that the next keeps apart from them, Pele's
/// Lanes: upright stretches as x, top, bottom, level ones as y, left, right, and where lines
/// leave or reach each side of a shape.
#[derive(Default)]
pub(crate) struct Lanes {
    upright: Vec<f64>,
    level: Vec<f64>,
    frames: [Vec<f64>; 2],
    taken: HashMap<usize, Vec<f64>>,
    /// Routes that found no lane of their own: a line left straight behind a shape, or a
    /// lane that runs along another.
    pub(crate) squeezed: usize,
}

impl Lanes {
    fn free(&self, upright: bool, at: f64, a0: f64, a1: f64) -> bool {
        let lo = min(a0, a1);
        let hi = max(a0, a1);
        let near = |list: &[f64], gap: f64| {
            list.chunks(3)
                .any(|s| (s[0] - at).abs() < gap && s[1] < hi && s[2] > lo)
        };
        let list = if upright { &self.upright } else { &self.level };
        let frames = &self.frames[if upright { 0 } else { 1 }];
        !near(list, NEAR) && !near(frames, FRAME_NEAR)
    }

    fn crossings(&self, points: &[f64]) -> usize {
        let mut count = 0;
        let mut i = 2;
        while i < points.len() {
            let upright = points[i] == points[i - 2];
            let at = if upright { points[i] } else { points[i + 1] };
            let (a, b) = if upright {
                (points[i - 1], points[i + 1])
            } else {
                (points[i - 2], points[i])
            };
            let lo = min(a, b);
            let hi = max(a, b);
            let list = if upright { &self.level } else { &self.upright };
            for s in list.chunks(3) {
                if s[0] > lo && s[0] < hi && s[1] < at && s[2] > at {
                    count += 1;
                }
            }
            i += 2;
        }
        count
    }

    pub(crate) fn frame(&mut self, b: &Box) {
        self.frames[0].extend([b.x, b.y, b.y + b.h, b.x + b.w, b.y, b.y + b.h]);
        self.frames[1].extend([b.y, b.x, b.x + b.w, b.y + b.h, b.x, b.x + b.w]);
    }

    fn take(&mut self, points: &[f64]) {
        let mut i = 2;
        while i < points.len() {
            if points[i] == points[i - 2] {
                self.upright.extend([
                    points[i],
                    min(points[i - 1], points[i + 1]),
                    max(points[i - 1], points[i + 1]),
                ]);
            } else if points[i + 1] == points[i - 1] {
                self.level.extend([
                    points[i + 1],
                    min(points[i - 2], points[i]),
                    max(points[i - 2], points[i]),
                ]);
            }
            i += 2;
        }
    }

    /// Where on one side of a shape (0 left, 1 right, 2 top, 3 bottom) a line can leave or
    /// arrive, between lo and hi: the places clear of the lines already there, nearest want
    /// first, as Pele's ports finds them.
    fn ports(&self, b: &Box, side: usize, want: f64, lo: f64, hi: f64) -> Vec<f64> {
        let used = self.taken.get(&b.id).map_or(&[][..], Vec::as_slice);
        let mut out = Vec::new();
        let mut k = 0;
        while k < 9 && out.len() < 4 {
            let sign = if k & 1 == 1 { 1.0 } else { -1.0 };
            let at = want + sign * (f64::from(k) / 2.0).ceil() * PORT_STEP;
            k += 1;
            if at < lo || at > hi {
                continue;
            }
            let clear = !used
                .chunks(2)
                .any(|u| u[0] == side as f64 && (u[1] - at).abs() < PORT_STEP);
            if clear {
                out.push(at);
            }
        }
        if out.is_empty() { vec![want] } else { out }
    }

    fn claim(&mut self, b: &Box, side: usize, at: f64) {
        self.taken
            .entry(b.id)
            .or_default()
            .extend([side as f64, at]);
    }
}

/// Whether JavaScript takes a number as true: neither zero nor NaN.
pub(crate) fn truthy(v: f64) -> bool {
    v != 0.0 && !v.is_nan()
}

/// The line with corners through points, cut short at its ends for its markers and with its
/// corners rounded, as Pele's path writes it: each straight stretch is level or upright,
/// drawn as SVG's H or V would draw it, from where the one before ended.
fn path_of(points: &[f64], start_trim: f64, end_trim: f64) -> Path {
    let n = points.len();
    let ux = sign(points[2] - points[0]);
    let uy = sign(points[3] - points[1]);
    let (mut cx, mut cy) = (points[0] + ux * start_trim, points[1] + uy * start_trim);
    let mut p = Path::new();
    p.move_to(cx, cy);
    let mut i = 2;
    while i < n - 2 {
        let x = points[i];
        let y = points[i + 1];
        let ix = sign(x - points[i - 2]);
        let iy = sign(y - points[i - 1]);
        let ox = sign(points[i + 2] - x);
        let oy = sign(points[i + 3] - y);
        let r = min(
            min(CORNER, (x - points[i - 2] + y - points[i - 1]).abs() / 2.0),
            (points[i + 2] - x + points[i + 3] - y).abs() / 2.0,
        );
        if truthy(ix) {
            cx = x - ix * r;
        } else {
            cy = y - iy * r;
        }
        p.line_to(cx, cy);
        (cx, cy) = (x + ox * r, y + oy * r);
        p.quad_to(x, y, cx, cy);
        i += 2;
    }
    let lx = sign(points[n - 2] - points[n - 4]);
    let ly = sign(points[n - 1] - points[n - 3]);
    if truthy(lx) {
        cx = points[n - 2] - lx * end_trim;
    } else {
        cy = points[n - 1] - ly * end_trim;
    }
    p.line_to(cx, cy);
    p
}

/// Whether a line with corners through points crosses none of the other shapes but a and b,
/// as Pele's clearOf says.
fn clear_of(points: &[f64], others: &[Box], a: &Box, b: &Box) -> bool {
    !others
        .iter()
        .any(|o| o.id != a.id && o.id != b.id && crosses(points, o.x, o.y, o.x + o.w, o.y + o.h))
}

/// The route for a line with corners through points, from a's outline to b's, as Pele's bent
/// makes it. along is the stretch its label sits on, and nx, lean and room are what the
/// route sets of where its label goes.
#[allow(clippy::too_many_arguments)]
fn bent(
    points: Vec<f64>,
    along: usize,
    start_trim: f64,
    end_trim: f64,
    nx: f64,
    lean: f64,
    room: f64,
) -> Route {
    let n = points.len();
    Route {
        path: path_of(&points, start_trim, end_trim),
        sx: points[0],
        sy: points[1],
        sdx: -sign(points[2] - points[0]),
        sdy: -sign(points[3] - points[1]),
        ex: points[n - 2],
        ey: points[n - 1],
        edx: sign(points[n - 2] - points[n - 4]),
        edy: sign(points[n - 1] - points[n - 3]),
        x0: points[along],
        y0: points[along + 1],
        x1: points[along + 2],
        y1: points[along + 3],
        nx,
        ny: 0.0,
        lean,
        room,
        points,
    }
}

/// Where lanes may run across the gap from lo to hi, nearest its middle first, as Pele's
/// across finds them.
fn across(lo: f64, hi: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let mid = (lo + hi) / 2.0;
    let step = max(LANE_STEP, (hi - lo) / 4.0);
    for k in 0..9u8 {
        let sign = if k & 1 == 1 { 1.0 } else { -1.0 };
        let at = mid + sign * (f64::from(k) / 2.0).ceil() * step;
        if at >= lo + NEAR && at <= hi - NEAR {
            out.push(at);
        }
    }
    out
}

/// The best route with two corners found so far, in Pele's elbow.
struct Best {
    points: Vec<f64>,
    cost: f64,
    upright: bool,
    sides: [usize; 2],
}

/// A route with two corners from a to b, across the gap between them, clear of the other
/// shapes and of the lanes taken, as Pele's elbow finds it, or none.
fn elbow(
    a: &Box,
    b: &Box,
    others: &[Box],
    lanes: &mut Lanes,
    start_trim: f64,
    end_trim: f64,
) -> Option<Route> {
    let ax = a.x + a.w / 2.0;
    let ay = a.y + a.h / 2.0;
    let bx = b.x + b.w / 2.0;
    let by = b.y + b.h / 2.0;
    let mut best: Option<Best> = None;
    // Takes points as the best route where they are, and says whether it did.
    let consider = |best: &mut Option<Best>,
                    lanes: &Lanes,
                    points: Vec<f64>,
                    upright: bool,
                    sides: [usize; 2]|
     -> bool {
        if !clear_of(&points, others, a, b) {
            return false;
        }
        let mut i = 2;
        while i < points.len() {
            let up = points[i] == points[i - 2];
            let free = if up {
                lanes.free(up, points[i], points[i - 1], points[i + 1])
            } else {
                lanes.free(up, points[i + 1], points[i - 2], points[i])
            };
            if !free {
                return false;
            }
            i += 2;
        }
        let mut cost = lanes.crossings(&points) as f64 * 1e6;
        let mut i = 2;
        while i < points.len() {
            cost += (points[i] - points[i - 2]).abs() + (points[i + 1] - points[i - 1]).abs();
            i += 2;
        }
        if best.as_ref().is_none_or(|b| cost < b.cost) {
            *best = Some(Best {
                points,
                cost,
                upright,
                sides,
            });
            return true;
        }
        false
    };
    if a.x + a.w <= b.x || b.x + b.w <= a.x {
        let right = bx > ax;
        let down = sign(by - ay);
        let sides = if right { [1, 0] } else { [0, 1] };
        let lanes_across = if right {
            across(a.x + a.w, b.x)
        } else {
            across(b.x + b.w, a.x)
        };
        'search: for y0 in lanes.ports(
            a,
            sides[0],
            ay + (down * a.h) / 4.0,
            a.y + 8.0,
            a.y + a.h - 8.0,
        ) {
            for y1 in lanes.ports(
                b,
                sides[1],
                by - (down * b.h) / 4.0,
                b.y + 8.0,
                b.y + b.h - 8.0,
            ) {
                for &lane in &lanes_across {
                    let x0 = lane + (ax - lane) * min(hit(a, lane, y0, ax, y0), 1.0);
                    let x1 = lane + (bx - lane) * min(hit(b, lane, y1, bx, y1), 1.0);
                    let points = vec![x0, y0, lane, y0, lane, y1, x1, y1];
                    if consider(&mut best, lanes, points, true, sides) {
                        break 'search;
                    }
                }
            }
        }
    }
    if a.y + a.h <= b.y || b.y + b.h <= a.y {
        let below = by > ay;
        let sideways = sign(bx - ax);
        let sides = if below { [3, 2] } else { [2, 3] };
        let lanes_across = if below {
            across(a.y + a.h, b.y)
        } else {
            across(b.y + b.h, a.y)
        };
        'search: for x0 in lanes.ports(
            a,
            sides[0],
            ax + (sideways * a.w) / 4.0,
            a.x + 8.0,
            a.x + a.w - 8.0,
        ) {
            for x1 in lanes.ports(
                b,
                sides[1],
                bx - (sideways * b.w) / 4.0,
                b.x + 8.0,
                b.x + b.w - 8.0,
            ) {
                for &lane in &lanes_across {
                    let y0 = lane + (ay - lane) * min(hit(a, x0, lane, x0, ay), 1.0);
                    let y1 = lane + (by - lane) * min(hit(b, x1, lane, x1, by), 1.0);
                    let points = vec![x0, y0, x0, lane, x1, lane, x1, y1];
                    if consider(&mut best, lanes, points, false, sides) {
                        break 'search;
                    }
                }
            }
        }
    }
    let Best {
        points,
        upright,
        sides,
        ..
    } = best?;
    lanes.take(&points);
    lanes.claim(a, sides[0], if upright { points[1] } else { points[0] });
    lanes.claim(b, sides[1], if upright { points[7] } else { points[6] });
    // The label sits on the middle stretch, across the gap.
    let room = if upright { NARROW } else { WIDE };
    Some(bent(points, 2, start_trim, end_trim, 0.0, 0.0, room))
}

/// A detour along one side, in Pele's route.
struct Way {
    points: Vec<f64>,
    side: usize,
    p0: f64,
    p1: f64,
    step: usize,
}

/// Routes a relation from a to b, as Pele's route does. shift moves the line sideways when
/// another runs the opposite way. others are the shapes to steer clear of, and none skips
/// that check. lanes holds what the routes before it take up, so that this one keeps apart
/// from them.
pub(crate) fn route(
    a: &Box,
    b: &Box,
    shift: f64,
    others: Option<&[Box]>,
    lanes: &mut Lanes,
    start_trim: f64,
    end_trim: f64,
) -> Route {
    if a.id == b.id {
        let spread = min(a.h / 2.0 - 4.0, 10.0);
        let len = hypot(LOOP, spread);
        let x = a.x + a.w;
        let cy = a.y + a.h / 2.0;
        let ux = LOOP / len;
        let uy = spread / len;
        let mut path = Path::new();
        path.move_to(x + ux * start_trim, cy - spread - uy * start_trim)
            .cube_to(
                x + LOOP,
                cy - spread * 2.0,
                x + LOOP,
                cy + spread * 2.0,
                x + ux * end_trim,
                cy + spread + uy * end_trim,
            );
        return Route {
            path,
            sx: x,
            sy: cy - spread,
            sdx: -ux,
            sdy: uy,
            ex: x,
            ey: cy + spread,
            edx: -ux,
            edy: -uy,
            x0: x + LOOP + 4.0,
            y0: cy,
            x1: x + LOOP + 4.0,
            y1: cy,
            nx: 1.0,
            ny: 0.0,
            lean: 0.0,
            room: 64.0,
            points: Vec::new(),
        };
    }

    let (sx, sy, ex, ey) = connect(a, b, shift);
    let upright = sx == ex;
    let in_way = |o: &Box| {
        o.id != a.id
            && o.id != b.id
            && hit_rect(o.x, o.y, o.x + o.w, o.y + o.h, sx, sy, ex - sx, ey - sy) <= 1.0
    };
    let blocked = others
        .is_some_and(|others| !contains(a, b) && !contains(b, a) && others.iter().any(in_way));
    if let Some(others) = others.filter(|_| blocked) {
        if !upright && sy != ey {
            if let Some(around) = elbow(a, b, others, lanes, start_trim, end_trim) {
                return around;
            }
            lanes.squeezed += 1;
        } else {
            return detour(a, b, others, lanes, start_trim, end_trim, (sx, sy, ex, ey));
        }
    }

    let len = or1(hypot(ex - sx, ey - sy));
    let dx = (ex - sx) / len;
    let dy = (ey - sy) / len;
    if upright || sy == ey {
        lanes.take(&[sx, sy, ex, ey]);
    }
    let mut path = Path::new();
    path.move_to(sx + dx * start_trim, sy + dy * start_trim)
        .line_to(ex - dx * end_trim, ey - dy * end_trim);
    Route {
        path,
        sx,
        sy,
        sdx: -dx,
        sdy: -dy,
        ex,
        ey,
        edx: dx,
        edy: dy,
        x0: sx,
        y0: sy,
        x1: ex,
        y1: ey,
        nx: if truthy(shift) { -dy } else { 0.0 },
        ny: if truthy(shift) { dx } else { 0.0 },
        lean: 0.0,
        room: if dx.abs() > dy.abs() { NARROW } else { WIDE },
        points: vec![sx, sy, ex, ey],
    }
}

/// The route around what blocks an upright or level line from sx, sy to ex, ey, along one
/// side of the shapes in its way, as the part of Pele's route for it finds it.
fn detour(
    a: &Box,
    b: &Box,
    others: &[Box],
    lanes: &mut Lanes,
    start_trim: f64,
    end_trim: f64,
    (sx, sy, ex, ey): (f64, f64, f64, f64),
) -> Route {
    let upright = sx == ex;
    let in_way = |o: &Box| {
        o.id != a.id
            && o.id != b.id
            && hit_rect(o.x, o.y, o.x + o.w, o.y + o.h, sx, sy, ex - sx, ey - sy) <= 1.0
    };
    let lane_on = |forward: bool| {
        let mut lane = if forward {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
        let mut widen = |b: &Box| {
            let edge = if upright { b.x } else { b.y };
            lane = if forward {
                max(lane, edge + if upright { b.w } else { b.h })
            } else {
                min(lane, edge)
            };
        };
        widen(a);
        widen(b);
        for o in others {
            if in_way(o) {
                widen(o);
            }
        }
        lane
    };
    let lo = if upright {
        min(a.y, b.y)
    } else {
        min(a.x, b.x)
    };
    let hi = if upright {
        max(a.y + a.h, b.y + b.h)
    } else {
        max(a.x + a.w, b.x + b.w)
    };
    let outside = |forward: bool, lane: f64| {
        for o in others {
            if o.id == a.id || o.id == b.id {
                continue;
            }
            let near = if upright {
                o.y < hi && o.y + o.h > lo
            } else {
                o.x < hi && o.x + o.w > lo
            };
            let start = if upright { o.x } else { o.y };
            let end = start + if upright { o.w } else { o.h };
            if near && (if forward { end > lane } else { start < lane }) {
                return false;
            }
        }
        true
    };
    let ax = a.x + a.w / 2.0;
    let ay = a.y + a.h / 2.0;
    let bx = b.x + b.w / 2.0;
    let by = b.y + b.h / 2.0;
    let towards = if upright {
        sign(by - ay)
    } else {
        sign(bx - ax)
    };
    // The route on one side: each end leaves its shape a quarter of the way from the middle
    // towards the other, clear of the straight lines that meet the middle of that side and
    // of other routes there. The lane runs a little further out for each route already
    // along that gap, and never behind a shape. step is how many lanes out it went, and 5
    // when none was free.
    let along = |lanes: &Lanes, forward: bool| -> Way {
        let lane = lane_on(forward);
        let sign = if forward { 1.0 } else { -1.0 };
        let side = match (upright, forward) {
            (true, true) => 1,
            (true, false) => 0,
            (false, true) => 3,
            (false, false) => 2,
        };
        let (p0, p1) = if upright {
            (
                lanes.ports(
                    a,
                    side,
                    ay + (towards * a.h) / 4.0,
                    a.y + 8.0,
                    a.y + a.h - 8.0,
                )[0],
                lanes.ports(
                    b,
                    side,
                    by - (towards * b.h) / 4.0,
                    b.y + 8.0,
                    b.y + b.h - 8.0,
                )[0],
            )
        } else {
            (
                lanes.ports(
                    a,
                    side,
                    ax + (towards * a.w) / 4.0,
                    a.x + 8.0,
                    a.x + a.w - 8.0,
                )[0],
                lanes.ports(
                    b,
                    side,
                    bx - (towards * b.w) / 4.0,
                    b.x + 8.0,
                    b.x + b.w - 8.0,
                )[0],
            )
        };
        let points_at = |at: f64| {
            if upright {
                let x0 = at + (ax - at) * min(hit(a, at, p0, ax, p0), 1.0);
                let x1 = at + (bx - at) * min(hit(b, at, p1, bx, p1), 1.0);
                return vec![x0, p0, at, p0, at, p1, x1, p1];
            }
            let y0 = at + (ay - at) * min(hit(a, p0, at, p0, ay), 1.0);
            let y1 = at + (by - at) * min(hit(b, p1, at, p1, by), 1.0);
            vec![p0, y0, p0, at, p1, at, p1, y1]
        };
        let mut points = points_at(lane + sign * LANE);
        for k in 0..=4 {
            let at = lane + sign * (LANE + k as f64 * LANE_STEP);
            let tried = points_at(at);
            if !clear_of(&tried, others, a, b) {
                continue;
            }
            points = tried;
            if lanes.free(upright, at, p0, p1) {
                return Way {
                    points,
                    side,
                    p0,
                    p1,
                    step: k,
                };
            }
        }
        Way {
            points,
            side,
            p0,
            p1,
            step: 5,
        }
    };
    // A detour takes the outside of the drawing when only one side is outside, leaving the
    // gaps between columns and rows to the relations that cross them. Otherwise it takes the
    // side where it crosses fewer lines, then the one where it runs nearer its shapes, so
    // that detours spread over both sides.
    let natural = if upright { ey > sy } else { ex < sx };
    let out = outside(natural, lane_on(natural));
    let back = outside(!natural, lane_on(!natural));
    let first = along(lanes, natural);
    let other = (out == back).then(|| along(lanes, !natural));
    let cost = |way: &Way| lanes.crossings(&way.points) * 8 + way.step;
    let cheaper = other.as_ref().is_some_and(|o| cost(o) < cost(&first));
    let forward = if (back && !out) || cheaper {
        !natural
    } else {
        natural
    };
    let way = if forward == natural {
        first
    } else {
        other.unwrap_or_else(|| along(lanes, !natural))
    };
    let sign = if forward { 1.0 } else { -1.0 };
    if way.step > 4 {
        lanes.squeezed += 1;
    }
    lanes.take(&way.points);
    lanes.claim(a, way.side, way.p0);
    lanes.claim(b, way.side, way.p1);
    if upright {
        bent(way.points, 2, start_trim, end_trim, sign, LEAN, NARROW)
    } else {
        bent(way.points, 2, start_trim, end_trim, 0.0, 0.0, WIDE)
    }
}
