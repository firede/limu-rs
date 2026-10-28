//! The paths of edges, as Pele's src/svg/edges.ts draws them: along a route, around a
//! self-loop, and on a patch of the background under an edge's label.

use std::collections::HashMap;

use crate::graphs::layered::ease;
use crate::js::math::{hypot, max, min, or1, sign, sign_or1};
use crate::scene::{Marker, MarkerType, Painting, Part, Path, Point, Rect};

/// The most a corner is rounded by in a stepped route.
const CORNER: f64 = 12.0;
/// How far the handles of a wide turn reach along its two legs. A little more than a
/// circle's, which makes the turn fuller.
const HANDLE: f64 = 0.62;
/// How far an edge runs straight before a marker at its end, where it has the room.
pub(crate) const LEAD: f64 = 6.0;

/// Where a marker's tip is at an end of an edge, and the way it points, as a unit vector.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Tip {
    pub(crate) at: Point,
    pub(crate) dx: f64,
    pub(crate) dy: f64,
}

impl Tip {
    fn new(x: f64, y: f64, dx: f64, dy: f64) -> Tip {
        Tip {
            at: Point::new(x, y),
            dx,
            dy,
        }
    }

    /// A marker of a type here, or none where there is no type.
    pub(crate) fn marker(self, kind: Option<MarkerType>) -> Option<Marker> {
        kind.map(|kind| Marker {
            kind,
            at: self.at,
            dx: self.dx,
            dy: self.dy,
        })
    }
}

/// The path an edge is drawn along, and the tips of the markers at its start and its end, as
/// Pele's EdgePath has them.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EdgePath {
    pub(crate) path: Path,
    pub(crate) start: Tip,
    pub(crate) end: Tip,
}

impl EdgePath {
    /// The markers of types at the start and the end.
    pub(crate) fn ends(
        &self,
        start: Option<MarkerType>,
        end: Option<MarkerType>,
    ) -> [Option<Marker>; 2] {
        [self.start.marker(start), self.end.marker(end)]
    }
}

/// How far an edge stops short of its end to leave room for a marker of a type, or for none,
/// as Pele's markerTrim gives it for the markers it knows.
pub(crate) fn marker_trim(kind: Option<MarkerType>) -> f64 {
    kind.map_or(0.0, MarkerType::trim)
}

/// How far from each side of each node the edges that meet it start to turn: as far as the
/// largest marker there needs, for every edge on the side, so that none turns sooner than its
/// neighbour and crosses it, as Pele's sideReach works it out. Sides are 0 top, 1 right, 2
/// bottom and 3 left, and none, -1, is left out.
#[derive(Clone, Debug, Default)]
pub(crate) struct SideReach {
    reach: HashMap<(usize, i32), f64>,
}

impl SideReach {
    /// Takes the marker of an edge's end on a side of a node.
    pub(crate) fn add(&mut self, node: usize, side: i32, kind: Option<MarkerType>) {
        let trim = marker_trim(kind);
        if side < 0 || trim <= 0.0 {
            return;
        }
        let at = self.reach.entry((node, side)).or_insert(0.0);
        if trim + LEAD > *at {
            *at = trim + LEAD;
        }
    }

    /// How far from a side of a node its edges start to turn.
    pub(crate) fn of(&self, node: usize, side: i32) -> f64 {
        if side < 0 {
            0.0
        } else {
            self.reach.get(&(node, side)).copied().unwrap_or(0.0)
        }
    }
}

/// How far from its node an edge starts to turn when nothing beside it needs it to turn
/// further out: far enough to clear its own marker, trimmed by trim, as Pele's routePath
/// takes it when it is given no reach.
pub(crate) fn own_reach(trim: f64) -> f64 {
    if trim > 0.0 { trim + LEAD } else { 0.0 }
}

/// How a route is drawn between its points, of Mermaid's curves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Curve {
    /// An S between points not in line, and wide turns at corners, as Pele draws every curve
    /// but these.
    #[default]
    Smooth,
    /// "linear": straight from point to point.
    Linear,
    /// "step": at right angles, turning halfway.
    Step,
    /// "stepBefore": at right angles, turning first.
    StepBefore,
    /// "stepAfter": at right angles, turning last.
    StepAfter,
}

impl Curve {
    /// The curve of Mermaid's name.
    pub(crate) fn of(name: &str) -> Curve {
        match name {
            "linear" => Curve::Linear,
            "step" => Curve::Step,
            "stepBefore" => Curve::StepBefore,
            "stepAfter" => Curve::StepAfter,
            _ => Curve::Smooth,
        }
    }
}

/// The path of a route of x, y, axis triples, trimmed at each end to leave room for markers,
/// as Pele's routePath draws it. Between two points that are not in line, a route is one S
/// that leaves and arrives along the flow, or a step in a stepped route. Where the route
/// itself turns a corner, a stepped route rounds it a little, and any other but a linear one
/// turns as widely as it can: the turn takes the whole of a leg that ends at a node and half
/// of a leg it shares with the next turn. start_reach and end_reach say how far from its
/// nodes the edge starts to turn, where the other edges on the same side of a node need it
/// to, and own_reach gives what Pele takes where it is given none. The route has at least two
/// points.
pub(crate) fn route_path(
    route: &[f64],
    curve: Curve,
    start_trim: f64,
    end_trim: f64,
    start_reach: f64,
    end_reach: f64,
) -> EdgePath {
    let linear = curve == Curve::Linear;
    let stepped = matches!(curve, Curve::Step | Curve::StepBefore | Curve::StepAfter);
    let (mut xs, mut ys, mut axes): (Vec<f64>, Vec<f64>, Vec<f64>) =
        (Vec::new(), Vec::new(), Vec::new());
    let near =
        |ax: f64, ay: f64, bx: f64, by: f64| (ax - bx).abs() < 0.01 && (ay - by).abs() < 0.01;
    for &[x, y, axis] in route.as_chunks::<3>().0 {
        // A stepped route turns at right angles where a smooth one would curve.
        if let (true, Some(&px), Some(&py)) = (stepped, xs.last(), ys.last())
            && (x - px).abs() >= 0.01
            && (y - py).abs() >= 0.01
        {
            let my = match curve {
                Curve::StepBefore => py,
                Curve::StepAfter => y,
                _ => (py + y) / 2.0,
            };
            let mx = match curve {
                Curve::StepBefore => px,
                Curve::StepAfter => x,
                _ => (px + x) / 2.0,
            };
            let turns = if axis == 0.0 {
                [px, my, x, my]
            } else {
                [mx, py, mx, y]
            };
            for &[tx, ty] in turns.as_chunks::<2>().0 {
                if near(tx, ty, px, py) || near(tx, ty, x, y) {
                    continue;
                }
                xs.push(tx);
                ys.push(ty);
                axes.push(axis);
            }
        }
        xs.push(x);
        ys.push(y);
        axes.push(axis);
    }
    let mut last = xs.len() - 1;
    let aligned = |xs: &[f64], ys: &[f64], i: usize| {
        (xs[i] - xs[i - 1]).abs() < 0.01 || (ys[i] - ys[i - 1]).abs() < 0.01
    };
    let straight = |xs: &[f64], ys: &[f64], i: usize| linear || aligned(xs, ys, i);
    // The way the route heads from point i to point j: along the flow where an S joins them.
    let unit = |xs: &[f64], ys: &[f64], i: usize, j: usize, axis: f64| -> (f64, f64) {
        if !straight(xs, ys, i.max(j)) {
            return if axis == 0.0 {
                (0.0, sign_or1(ys[j] - ys[i]))
            } else {
                (sign_or1(xs[j] - xs[i]), 0.0)
            };
        }
        let (dx, dy) = (xs[j] - xs[i], ys[j] - ys[i]);
        let len = or1(hypot(dx, dy));
        (dx / len, dy / len)
    };
    let (sdx, sdy) = unit(&xs, &ys, 0, 1, axes[1]);
    let (edx, edy) = unit(&xs, &ys, last - 1, last, axes[last]);
    let (sx, sy, ex, ey) = (xs[0], ys[0], xs[last], ys[last]);
    xs[0] += sdx * start_trim;
    ys[0] += sdy * start_trim;
    xs[last] -= edx * end_trim;
    ys[last] -= edy * end_trim;
    let length =
        |xs: &[f64], ys: &[f64], i: usize| (xs[i] - xs[i - 1]).abs() + (ys[i] - ys[i - 1]).abs();
    // Whether the route turns a corner at point i, from running one way to running the other.
    let turns = |xs: &[f64], ys: &[f64], last: usize, i: usize| {
        !linear
            && i > 0
            && i < last
            && aligned(xs, ys, i)
            && aligned(xs, ys, i + 1)
            && length(xs, ys, i) >= 0.01
            && length(xs, ys, i + 1) >= 0.01
            && ((xs[i] - xs[i - 1]).abs() < 0.01) != ((xs[i + 1] - xs[i]).abs() < 0.01)
    };
    // An edge that curves into a marker runs straight for a little way first, where it has
    // the room, so that the marker does not sit on the bend.
    let along = |xs: &[f64], ys: &[f64], i: usize, ux: f64, uy: f64| {
        (xs[i] - xs[i - 1]).abs() * ux.abs() + (ys[i] - ys[i - 1]).abs() * uy.abs()
    };
    // Of the way from the end of a run to where it turns, the part that makes up for a larger
    // marker beside this one is kept where it can be, and the rest is at most a quarter of
    // what is left, so that ends on one side of a node turn equally far from it.
    let lead = |reach: f64, trim: f64, run: f64| {
        let room = min(max(0.0, reach - LEAD - trim), run / 4.0);
        room + min(reach - trim - room, (run - room) / 4.0)
    };
    let arrives =
        if end_reach > end_trim && (!straight(&xs, &ys, last) || turns(&xs, &ys, last, last - 1)) {
            lead(end_reach, end_trim, along(&xs, &ys, last, edx, edy))
        } else {
            0.0
        };
    let leaves = if start_reach > start_trim && (!straight(&xs, &ys, 1) || turns(&xs, &ys, last, 1))
    {
        lead(start_reach, start_trim, along(&xs, &ys, 1, sdx, sdy))
    } else {
        0.0
    };
    if arrives >= 1.0 {
        xs.insert(last, xs[last] - edx * arrives);
        ys.insert(last, ys[last] - edy * arrives);
        axes.insert(last, axes[last]);
        last += 1;
    }
    if leaves >= 1.0 {
        xs.insert(1, xs[0] + sdx * leaves);
        ys.insert(1, ys[0] + sdy * leaves);
        axes.insert(1, axes[1]);
        last += 1;
    }

    let mut p = Path::new();
    p.move_to(xs[0], ys[0]);
    let (mut at_x, mut at_y) = (xs[0], ys[0]);
    let line_to = |p: &mut Path, at_x: &mut f64, at_y: &mut f64, x: f64, y: f64| {
        if near(x, y, *at_x, *at_y) {
            return;
        }
        p.line_to(x, y);
        (*at_x, *at_y) = (x, y);
    };
    for i in 1..=last {
        let (x, y) = (xs[i], ys[i]);
        if !straight(&xs, &ys, i) {
            let down = axes[i] == 0.0;
            let run = if down { y - at_y } else { x - at_x };
            let k = run.abs() * ease(if down { x - at_x } else { y - at_y }, run);
            let ux = if down { 0.0 } else { sign(run) * k };
            let uy = if down { sign(run) * k } else { 0.0 };
            p.cube_to(at_x + ux, at_y + uy, x - ux, y - uy, x, y);
            (at_x, at_y) = (x, y);
            continue;
        }
        if !turns(&xs, &ys, last, i) {
            // A point part of the way along one straight run adds nothing to the path.
            if i < last
                && aligned(&xs, &ys, i)
                && aligned(&xs, &ys, i + 1)
                && (x - xs[i - 1]) * (xs[i + 1] - x) + (y - ys[i - 1]) * (ys[i + 1] - y) > 0.0
            {
                continue;
            }
            line_to(&mut p, &mut at_x, &mut at_y, x, y);
            continue;
        }
        // How much of the leg before the turn and of the leg after it the turn takes: all of
        // one that ends at a node, short of the straight run into a marker, and half of one
        // it shares.
        let mut before = length(&xs, &ys, i);
        if turns(&xs, &ys, last, i - 1) {
            before /= 2.0;
        }
        let mut after = length(&xs, &ys, i + 1);
        if turns(&xs, &ys, last, i + 1) {
            after /= 2.0;
        }
        if stepped {
            before = min(CORNER, min(before, after));
            after = before;
        }
        let (ux, uy) = unit(&xs, &ys, i - 1, i, axes[i]);
        let (vx, vy) = unit(&xs, &ys, i, i + 1, axes[i + 1]);
        line_to(
            &mut p,
            &mut at_x,
            &mut at_y,
            x - ux * before,
            y - uy * before,
        );
        (at_x, at_y) = (x + vx * after, y + vy * after);
        if stepped {
            p.quad_to(x, y, at_x, at_y);
        } else {
            p.cube_to(
                x - ux * before * (1.0 - HANDLE),
                y - uy * before * (1.0 - HANDLE),
                x + vx * after * (1.0 - HANDLE),
                y + vy * after * (1.0 - HANDLE),
                at_x,
                at_y,
            );
        }
    }
    EdgePath {
        path: p,
        start: Tip::new(sx, sy, -sdx, -sdy),
        end: Tip::new(ex, ey, edx, edy),
    }
}

/// A self-loop beside a node centered on c and drawn w by h, as Pele's loopPath draws it:
/// out of the side that lies across the flow and back in. k counts the loops already on the
/// node, so each one reaches a little further. base is how far the first reaches, and trim
/// is the marker's at its end. It also gives where the edge's label, label_w by label_h, is
/// centered: beyond the loop.
#[allow(clippy::too_many_arguments)]
pub(crate) fn loop_path(
    c: Point,
    w: f64,
    h: f64,
    k: usize,
    sideways: bool,
    base: f64,
    trim: f64,
    label_w: f64,
    label_h: f64,
) -> (EdgePath, Point) {
    let k = k as f64;
    let half = (if sideways { w } else { h }) / 2.0;
    let spread = min(half - 4.0, 8.0 + k * 6.0);
    let reach = base + k * 8.0;
    let len = or1(hypot(reach, spread));
    // In the loop's own frame, out points away from the node and along runs along its side.
    let at = |out: f64, along: f64| {
        if sideways {
            (c.x + along, c.y + h / 2.0 + out)
        } else {
            (c.x + w / 2.0 + out, c.y + along)
        }
    };
    let tx = (reach / len) * trim;
    let ty = (spread / len) * trim;
    let label = if sideways {
        Point::new(c.x, c.y + h / 2.0 + reach + 4.0 + label_h / 2.0)
    } else {
        Point::new(c.x + w / 2.0 + reach + 4.0 + label_w / 2.0, c.y)
    };
    let mut p = Path::new();
    let (x0, y0) = at(0.0, -spread);
    let (ax, ay) = at(reach, -spread * 2.0);
    let (bx, by) = at(reach, spread * 2.0);
    let (ex, ey) = at(tx, spread + ty);
    p.move_to(x0, y0).cube_to(ax, ay, bx, by, ex, ey);
    let (start, end) = if sideways {
        (
            Tip::new(c.x - spread, c.y + h / 2.0, 0.0, -1.0),
            Tip::new(c.x + spread, c.y + h / 2.0, -spread / len, -reach / len),
        )
    } else {
        (
            Tip::new(c.x + w / 2.0, c.y - spread, -1.0, 0.0),
            Tip::new(c.x + w / 2.0, c.y + spread, -reach / len, -spread / len),
        )
    };
    (
        EdgePath {
            path: p,
            start,
            end,
        },
        label,
    )
}

/// How much the corners of an edge label's patch are rounded.
const PATCH_RADIUS: f64 = 3.0;

/// The box of the patch of the background an edge's label w by h, centered on x, y, is drawn
/// on: the label's, and 4 more on the left and right, as Pele's edgeLabelSvg draws it.
pub(crate) fn label_patch_box(w: f64, h: f64, x: f64, y: f64) -> Rect {
    let w = w + 8.0;
    Rect::new(x - w / 2.0, y - h / 2.0, w, h)
}

/// The patch of the background an edge's label w by h, centered on x, y, is drawn on, so the
/// line does not run through the text, as Pele's edgeLabelSvg draws it. Its corners are
/// rounded by 3, which is not the theme's radius, so it is a path. SVG would round the
/// corners of a patch less than 6 high less across than down, and no label is.
pub(crate) fn label_patch(w: f64, h: f64, x: f64, y: f64) -> Part {
    let mut p = Path::new();
    p.round_rect(label_patch_box(w, h, x, y), PATCH_RADIUS);
    Part::path(p, Painting::FillOnly)
}
