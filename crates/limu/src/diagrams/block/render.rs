//! A block diagram laid out as a scene, as Pele's renderBlock in render.ts draws it: the
//! blocks in the columns of their composites, arrow blocks, and the edges between blocks,
//! with the labels of edges between the same two blocks stacked where they would cover each
//! other.

use std::collections::HashMap;
use std::f64::consts::PI;

use super::arrows::{OVERHANG, arrow_axis, arrow_mask, arrow_size, draw_arrow};
use super::db::{BlockDb, ROOT};
use super::layout::{Cell, layout as lay_cells};
use crate::common::{class_names, setting, tighten, title_label, title_room};
use crate::css::{Resolved, declarations, resolve_style};
use crate::draw::{
    EdgePath, Fill, Tip, casing, draw_shape, label_patch, label_patch_box, marker_trim, on_fills,
    shape_inset, shape_size,
};
use crate::errors::Error;
use crate::js::chars::trim;
use crate::js::math::{asin, atan2, cos, hypot, max, min, or1, round, sign, sin};
use crate::layout::Options;
use crate::scene::style::{Cap, Join, Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{
    Edge, Item, Kind as ItemKind, MarkerType, Painting, Part, Path, Placed, Point, Rect, Scene,
    Shape,
};
use crate::text::layout_label;
use crate::value::{Map, Value};

/// The roles of the items of a block diagram. A casing lies under an edge that crosses the
/// border of a fill an author sets, drawn in the background so that the edge stands out on
/// the fill.
pub(crate) const ROLE_TITLE: &str = "block.title";
pub(crate) const ROLE_COMPOSITE: &str = "block.composite";
pub(crate) const ROLE_BLOCK: &str = "block.block";
pub(crate) const ROLE_EDGE: &str = "block.edge";
pub(crate) const ROLE_EDGE_LABEL: &str = "block.edge-label";
pub(crate) const ROLE_CASING: &str = "block.casing";

const GAP: f64 = 12.0;
const WRAP: f64 = 200.0;
const CORNER: f64 = 8.0;
/// How many checks of an edge against a block a diagram may take. Checking every edge
/// against every block is quadratic, so large diagrams route without them.
const OBSTACLE_BUDGET: usize = 2_000_000;
/// The room between labels stacked along a line, Pele's LABEL_GAP.
const LABEL_GAP: f64 = 4.0;
/// The places along a leg a label is tried at, in turn, Pele's SPOTS.
const SPOTS: [f64; 5] = [0.5, 0.3, 0.7, 0.15, 0.85];

/// The shape Pele draws a type of block with, Pele's SHAPES. Any other is a rect.
fn shape_of(kind: &str) -> &'static str {
    match kind {
        "square" => "rect",
        "round" => "rounded",
        "circle" => "circle",
        "doublecircle" => "dbl-circ",
        "diamond" => "diam",
        "hexagon" => "hex",
        "stadium" => "stadium",
        "subroutine" => "fr-rect",
        "cylinder" => "cyl",
        "rect_left_inv_arrow" => "odd",
        "lean_right" => "lean-r",
        "lean_left" => "lean-l",
        "trapezoid" => "trap-b",
        "inv_trapezoid" => "trap-t",
        _ => "rect",
    }
}

/// Whether a shape keeps its own size in a larger cell, where the others stretch to fill it,
/// Pele's FIXED.
fn fixed(shape: &str) -> bool {
    matches!(shape, "circle" | "dbl-circ" | "diam")
}

/// Whether a shape only widens, because its slants and curves are set by its height, Pele's
/// WIDE.
fn wide(shape: &str) -> bool {
    matches!(
        shape,
        "stadium" | "hex" | "odd" | "lean-r" | "lean-l" | "trap-b" | "trap-t"
    )
}

/// The kinds of view, Pele's Kind.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Leaf,
    Arrow,
    Composite,
    Space,
}

/// A block as it is drawn, Pele's View, beside the cell it is placed in, which has the same
/// index: what it is drawn with, and where it is once placed.
struct View {
    block: usize,
    kind: Kind,
    index: usize,
    shape: &'static str,
    label: Label,
    /// The declarations of the block's classes and styles.
    decls: Vec<String>,
    style: Resolved,
    mask: u8,
    dx: f64,
    dy: f64,
    // The center and the size of the cell, as the last layout placed it.
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    /// The size as drawn, which for a fixed shape is smaller than its cell.
    bw: f64,
    bh: f64,
}

/// The box of a container's title, which stacked labels keep clear of, Pele's Box.
struct TitleBox {
    x: f64,
    y: f64,
    bw: f64,
    bh: f64,
}

/// An edge as it is drawn, Pele's Route: two points for a straight line, three for a bend,
/// none for a self-loop.
struct Route {
    edge: usize,
    points: Vec<f64>,
    looped: usize,
    label: Label,
    lx: f64,
    ly: f64,
}

fn is_round(v: &View) -> bool {
    v.kind == Kind::Leaf && (v.shape == "circle" || v.shape == "dbl-circ")
}

/// Where a vertical line at x meets the outline, below the center (sign 1) or above it (sign
/// -1), as Pele's edgeY finds it.
fn edge_y(v: &View, x: f64, sign: f64) -> f64 {
    let hw = v.bw / 2.0;
    let hh = v.bh / 2.0;
    let dx = (x - v.x).abs();
    let mut reach = hh;
    if is_round(v) {
        reach = max(hw * hw - dx * dx, 0.0).sqrt();
    } else if v.kind == Kind::Leaf && v.shape == "diam" {
        reach = hh * max(1.0 - dx / hw, 0.0);
    } else if v.kind == Kind::Leaf {
        let side = if sign > 0.0 { 2 } else { 0 };
        reach = hh - shape_inset(v.shape, v.bw, v.bh, side, 0.0);
    } else if v.kind == Kind::Arrow && arrow_axis(v.mask) == Some('x') {
        reach = hh - OVERHANG;
    }
    v.y + sign * reach
}

/// Where a horizontal line at y meets the outline, right of the center (sign 1) or left of
/// it (sign -1), as Pele's edgeX finds it.
fn edge_x(v: &View, y: f64, sign: f64) -> f64 {
    let hw = v.bw / 2.0;
    let hh = v.bh / 2.0;
    let dy = (y - v.y).abs();
    let mut reach = hw;
    if is_round(v) {
        reach = max(hh * hh - dy * dy, 0.0).sqrt();
    } else if v.kind == Kind::Leaf && v.shape == "diam" {
        reach = hw * max(1.0 - dy / hh, 0.0);
    } else if v.kind == Kind::Leaf {
        let side = if sign > 0.0 { 1 } else { 3 };
        reach = hw - shape_inset(v.shape, v.bw, v.bh, side, 0.0);
    } else if v.kind == Kind::Arrow && arrow_axis(v.mask) == Some('y') {
        reach = hw - OVERHANG;
    }
    v.x + sign * reach
}

/// Where the line from the center toward tx, ty leaves the outline, as Pele's border finds
/// it, added to out.
fn border(v: &View, tx: f64, ty: f64, out: &mut Vec<f64>) {
    let dx = tx - v.x;
    let dy = ty - v.y;
    let hw = v.bw / 2.0;
    let hh = v.bh / 2.0;
    let mut k = if is_round(v) {
        hw / or1(hypot(dx, dy))
    } else if v.kind == Kind::Leaf && v.shape == "diam" {
        1.0 / or1(dx.abs() / hw + dy.abs() / hh)
    } else {
        min(
            if dx == 0.0 {
                f64::INFINITY
            } else {
                hw / dx.abs()
            },
            if dy == 0.0 {
                f64::INFINITY
            } else {
                hh / dy.abs()
            },
        )
    };
    if k == f64::INFINITY {
        k = 0.0;
    }
    out.extend([v.x + dx * k, v.y + dy * k]);
}

/// Whether the segment from x1, y1 to x2, y2 crosses the box of v, a little inside its
/// outline, as Pele's crossesBox says.
fn crosses_box(x1: f64, y1: f64, x2: f64, y2: f64, v: &View) -> bool {
    let l = v.x - v.bw / 2.0 + 1.0;
    let r = v.x + v.bw / 2.0 - 1.0;
    let t = v.y - v.bh / 2.0 + 1.0;
    let b = v.y + v.bh / 2.0 - 1.0;
    if (x1 < l && x2 < l) || (x1 > r && x2 > r) || (y1 < t && y2 < t) || (y1 > b && y2 > b) {
        return false;
    }
    if x1 == x2 || y1 == y2 {
        return true;
    }
    // The segment is diagonal and its bounding box overlaps: the box is hit unless all four
    // corners are on one side.
    let dx = x2 - x1;
    let dy = y2 - y1;
    let side = |x: f64, y: f64| sign(dx * (y - y1) - dy * (x - x1));
    let first = side(l, t);
    first == 0.0 || side(r, t) != first || side(r, b) != first || side(l, b) != first
}

/// How many of the obstacles other than s and t a path through points crosses, as Pele's
/// hits counts them.
fn hits(points: &[f64], s: usize, t: usize, views: &[View], obstacles: &[usize]) -> usize {
    let mut count = 0;
    for &v in obstacles {
        if v == s || v == t {
            continue;
        }
        let mut i = 0;
        while i + 3 < points.len() {
            if crosses_box(
                points[i],
                points[i + 1],
                points[i + 2],
                points[i + 3],
                &views[v],
            ) {
                count += 1;
                break;
            }
            i += 2;
        }
    }
    count
}

/// The path of an edge from s to t, as Pele's routeEdge finds it: a straight line where the
/// two blocks share a row or a column, otherwise one bend between facing sides, or a straight
/// line between their outlines where that crosses fewer blocks. obstacles is none where the
/// diagram is too large to check.
fn route_edge(
    views: &[View],
    si: usize,
    ti: usize,
    shift: f64,
    obstacles: Option<&[usize]>,
) -> Vec<f64> {
    let (s, t) = (&views[si], &views[ti]);
    let sl = s.x - s.bw / 2.0;
    let sr = s.x + s.bw / 2.0;
    let st = s.y - s.bh / 2.0;
    let sb = s.y + s.bh / 2.0;
    let tl = t.x - t.bw / 2.0;
    let tr = t.x + t.bw / 2.0;
    let tt = t.y - t.bh / 2.0;
    let tb = t.y + t.bh / 2.0;
    let x1 = max(sl, tl);
    let x2 = min(sr, tr);
    let y1 = max(st, tt);
    let y2 = min(sb, tb);
    let over_x = x2 - x1 > 6.0;
    let over_y = y2 - y1 > 6.0;

    if over_x && over_y {
        // One block is inside the other: join the inner one to the nearest side of the
        // outer one.
        let inner_is_s = s.bw * s.bh <= t.bw * t.bh;
        let (inner, outer) = if inner_is_s { (s, t) } else { (t, s) };
        let up = inner.y - inner.bh / 2.0 - (outer.y - outer.bh / 2.0);
        let down = outer.y + outer.bh / 2.0 - (inner.y + inner.bh / 2.0);
        let left = inner.x - inner.bw / 2.0 - (outer.x - outer.bw / 2.0);
        let right = outer.x + outer.bw / 2.0 - (inner.x + inner.bw / 2.0);
        let least = min(min(min(up, down), left), right);
        let points = if least == down {
            vec![
                inner.x,
                edge_y(inner, inner.x, 1.0),
                inner.x,
                outer.y + outer.bh / 2.0,
            ]
        } else if least == up {
            vec![
                inner.x,
                edge_y(inner, inner.x, -1.0),
                inner.x,
                outer.y - outer.bh / 2.0,
            ]
        } else if least == right {
            vec![
                edge_x(inner, inner.y, 1.0),
                inner.y,
                outer.x + outer.bw / 2.0,
                inner.y,
            ]
        } else {
            vec![
                edge_x(inner, inner.y, -1.0),
                inner.y,
                outer.x - outer.bw / 2.0,
                inner.y,
            ]
        };
        return if inner_is_s {
            points
        } else {
            vec![points[2], points[3], points[0], points[1]]
        };
    }
    if over_x {
        let margin = min(6.0, (x2 - x1) / 2.0);
        let x = min(max((x1 + x2) / 2.0 + shift, x1 + margin), x2 - margin);
        let down = if t.y > s.y { 1.0 } else { -1.0 };
        return vec![x, edge_y(s, x, down), x, edge_y(t, x, -down)];
    }
    if over_y {
        let margin = min(6.0, (y2 - y1) / 2.0);
        let y = min(max((y1 + y2) / 2.0 + shift, y1 + margin), y2 - margin);
        let right = if t.x > s.x { 1.0 } else { -1.0 };
        return vec![edge_x(s, y, right), y, edge_x(t, y, -right), y];
    }

    let right = if t.x > s.x { 1.0 } else { -1.0 };
    let down = if t.y > s.y { 1.0 } else { -1.0 };
    let across = vec![
        edge_x(s, s.y, right),
        s.y,
        t.x,
        s.y,
        t.x,
        edge_y(t, t.x, -down),
    ];
    let along = vec![
        s.x,
        edge_y(s, s.x, down),
        s.x,
        t.y,
        edge_x(t, t.y, -right),
        t.y,
    ];
    let wide = (t.x - s.x).abs() >= (t.y - s.y).abs();
    let (mut best, other) = if wide {
        (across, along)
    } else {
        (along, across)
    };
    let Some(obstacles) = obstacles else {
        return best;
    };
    let mut least = hits(&best, si, ti, views, obstacles);
    if least == 0 {
        return best;
    }
    let count = hits(&other, si, ti, views, obstacles);
    if count < least {
        best = other;
        least = count;
    }
    if least == 0 {
        return best;
    }
    let mut direct = Vec::new();
    border(s, t.x, t.y, &mut direct);
    border(t, s.x, s.y, &mut direct);
    if hits(&direct, si, ti, views, obstacles) < least {
        direct
    } else {
        best
    }
}

fn tip(x: f64, y: f64, dx: f64, dy: f64) -> Tip {
    Tip {
        at: Point::new(x, y),
        dx,
        dy,
    }
}

/// The path through two or three points, its bend rounded and room left for markers, as
/// Pele's trace builds it.
fn trace(p: &[f64], mut start_trim: f64, mut end_trim: f64) -> EdgePath {
    let n = p.len();
    let (sx, sy) = (p[0], p[1]);
    let (ex, ey) = (p[n - 2], p[n - 1]);
    let mut ax = p[2] - sx;
    let mut ay = p[3] - sy;
    let first = hypot(ax, ay);
    ax /= or1(first);
    ay /= or1(first);
    let mut bx = ex - p[n - 4];
    let mut by = ey - p[n - 3];
    let last = hypot(bx, by);
    bx /= or1(last);
    by /= or1(last);
    if n == 4 && start_trim + end_trim > first {
        let scale = first / (start_trim + end_trim);
        start_trim *= scale;
        end_trim *= scale;
    }
    start_trim = min(start_trim, first);
    end_trim = min(end_trim, last);
    let mut path = Path::new();
    path.move_to(sx + ax * start_trim, sy + ay * start_trim);
    if n == 6 {
        let r = max(0.0, min(min(CORNER, first - start_trim), last - end_trim));
        path.line_to(p[2] - ax * r, p[3] - ay * r);
        path.quad_to(p[2], p[3], p[2] + bx * r, p[3] + by * r);
    }
    path.line_to(ex - bx * end_trim, ey - by * end_trim);
    EdgePath {
        path,
        start: tip(sx, sy, -ax, -ay),
        end: tip(ex, ey, bx, by),
    }
}

/// The arc of the kth self-loop on a block, as Pele's loopArc gives it: around the top
/// right corner of the block, or around the matching point on a round or diamond outline. It
/// gives the arc's center and radius and the angles where it leaves and returns.
fn loop_arc(v: &View, k: usize) -> (f64, f64, f64, f64, f64) {
    let hw = v.bw / 2.0;
    let hh = v.bh / 2.0;
    let r = max(1.0, min(min(10.0 + 5.0 * (k as f64 - 1.0), hw), hh));
    if is_round(v) {
        let px = v.x + hw * std::f64::consts::FRAC_1_SQRT_2;
        let py = v.y - hw * std::f64::consts::FRAC_1_SQRT_2;
        let half = 2.0 * asin(min(1.0, r / (2.0 * hw)));
        let angle = |at: f64| atan2(v.y - hw * sin(at) - py, v.x + hw * cos(at) - px);
        return (px, py, r, angle(PI / 4.0 + half), angle(PI / 4.0 - half));
    }
    if v.kind == Kind::Leaf && v.shape == "diam" {
        let slope = atan2(hh, hw);
        return (v.x + hw / 2.0, v.y - hh / 2.0, r, slope + PI, slope);
    }
    (v.x + hw, v.y - hh, r, PI, PI / 2.0)
}

/// The path of the kth self-loop on a block, as Pele's loopPath draws it: an arc, with each
/// marker along the chord it covers, so that it follows the curve instead of the tangent at
/// its tip.
fn loop_path(v: &View, k: usize, start_trim: f64, end_trim: f64) -> EdgePath {
    let (px, py, r, a1, a2) = loop_arc(v, k);
    let from = a1 + start_trim / r;
    let to = a2 - end_trim / r;
    let mut sweep = (to - from) % (2.0 * PI);
    if sweep <= 0.0 {
        sweep += 2.0 * PI;
    }
    let x1 = px + r * cos(from);
    let y1 = py + r * sin(from);
    let x2 = px + r * cos(to);
    let y2 = py + r * sin(to);
    let sx = px + r * cos(a1);
    let sy = py + r * sin(a1);
    let ex = px + r * cos(a2);
    let ey = py + r * sin(a2);
    let sl = hypot(sx - x1, sy - y1);
    let el = hypot(ex - x2, ey - y2);
    let mut path = Path::new();
    path.move_to(x1, y1)
        .arc_to(r, r, 0.0, sweep > PI, true, x2, y2);
    EdgePath {
        path,
        start: if sl > 0.0 {
            tip(sx, sy, (sx - x1) / sl, (sy - y1) / sl)
        } else {
            tip(sx, sy, sin(a1), -cos(a1))
        },
        end: if el > 0.0 {
            tip(ex, ey, (ex - x2) / el, (ey - y2) / el)
        } else {
            tip(ex, ey, -sin(a2), cos(a2))
        },
    }
}

/// What a label is kept clear of: the blocks other than the two an edge joins, and the
/// titles of containers, as Pele's obstacles are. It is none where the diagram is too large
/// to check, and then a label covers nothing.
#[derive(Clone, Copy)]
struct Blocked<'a> {
    views: &'a [View],
    obstacles: &'a [usize],
    titles: &'a [TitleBox],
}

/// Whether a label w by h centered on x, y covers a block other than s and t, or a title, as
/// Pele's covers says.
fn covers(x: f64, y: f64, w: f64, h: f64, s: usize, t: usize, blocked: Option<Blocked>) -> bool {
    let Some(b) = blocked else {
        return false;
    };
    for &i in b.obstacles {
        let v = &b.views[i];
        if i != s
            && i != t
            && (x - v.x).abs() < (w + v.bw) / 2.0
            && (y - v.y).abs() < (h + v.bh) / 2.0
        {
            return true;
        }
    }
    b.titles
        .iter()
        .any(|v| (x - v.x).abs() < (w + v.bw) / 2.0 && (y - v.y).abs() < (h + v.bh) / 2.0)
}

/// The point along the leg of p from at where a label w by h covers no block other than the
/// two it joins, as Pele's labelSpot picks it.
fn label_spot(
    p: &[f64],
    at: usize,
    w: f64,
    h: f64,
    s: usize,
    t: usize,
    blocked: Option<Blocked>,
) -> f64 {
    if blocked.is_none() {
        return 0.5;
    }
    for spot in SPOTS {
        let x = p[at] + (p[at + 2] - p[at]) * spot;
        let y = p[at + 1] + (p[at + 3] - p[at + 1]) * spot;
        if !covers(x, y, w, h, s, t, blocked) {
            return spot;
        }
    }
    0.5
}

/// Stacks the labels of the edges of stack, which join the same two blocks, along the line
/// they share where they cover each other, if they fit between its markers and cover no
/// other block or title, as Pele's stackLabels does. It gives how much longer the line would
/// have to be for them to fit, or 0.
fn stack_labels(
    stack: &[usize],
    routes: &mut [Route],
    legs: &[Vec<f64>],
    s: usize,
    t: usize,
    blocked: Option<Blocked>,
) -> f64 {
    let vertical = legs[stack[0]][0] == legs[stack[0]][2];
    let mut lo = f64::NEG_INFINITY;
    let mut hi = f64::INFINITY;
    let mut need = -LABEL_GAP;
    let mut crowded = false;
    let along = |r: &Route| {
        if vertical {
            r.label.height
        } else {
            r.label.width + 8.0
        }
    };
    for (a, &i) in stack.iter().enumerate() {
        let leg = &legs[i];
        if if vertical {
            leg[0] != leg[2]
        } else {
            leg[1] != leg[3]
        } {
            return 0.0;
        }
        if vertical {
            lo = max(lo, min(leg[1], leg[3]));
            hi = min(hi, max(leg[1], leg[3]));
        } else {
            lo = max(lo, min(leg[0], leg[2]));
            hi = min(hi, max(leg[0], leg[2]));
        }
        let p = &routes[i];
        need += along(p) + LABEL_GAP;
        for &j in &stack[..a] {
            let q = &routes[j];
            if (p.lx - q.lx).abs() < (p.label.width + q.label.width) / 2.0 + 8.0
                && (p.ly - q.ly).abs() < (p.label.height + q.label.height) / 2.0
            {
                crowded = true;
            }
        }
    }
    if !crowded {
        return 0.0;
    }
    let room = hi - lo - 2.0 * crate::scene::MARKER_SIZE;
    if need > room {
        return need - room;
    }
    let mut places = Vec::with_capacity(stack.len());
    let mut next = (lo + hi - need) / 2.0;
    for &i in stack {
        let r = &routes[i];
        let size = along(r);
        let at = next + size / 2.0;
        let (x, y) = if vertical { (r.lx, at) } else { (at, r.ly) };
        if covers(x, y, r.label.width + 8.0, r.label.height, s, t, blocked) {
            return 0.0;
        }
        places.push(at);
        next += size + LABEL_GAP;
    }
    for (k, &i) in stack.iter().enumerate() {
        if vertical {
            routes[i].ly = places[k];
        } else {
            routes[i].lx = places[k];
        }
    }
    0.0
}

/// Widens the gap between s and t in the nearest container they share by short, between rows
/// where vertical is set and between columns otherwise, as Pele's widen does, and says
/// whether there was a gap to widen. base keeps the gaps as they were when the labels were
/// measured, so that widening the same gap again does not add to what it added.
fn widen(
    cells: &mut [Cell],
    parents: &[Option<usize>],
    s: usize,
    t: usize,
    vertical: bool,
    short: f64,
    base: &mut HashMap<(usize, bool), Vec<f64>>,
) -> bool {
    let mut chain = std::collections::HashSet::new();
    let mut v = Some(s);
    while let Some(i) = v {
        chain.insert(i);
        v = parents[i];
    }
    let mut tb = None;
    let mut container = Some(t);
    while let Some(c) = container.filter(|c| !chain.contains(c)) {
        tb = Some(c);
        container = parents[c];
    }
    let (Some(container), Some(tb)) = (container, tb) else {
        return false;
    };
    if container == s {
        return false;
    }
    let mut sb = s;
    while parents[sb] != Some(container) {
        sb = parents[sb].expect("a parent");
    }
    let (sbc, tbc) = (&cells[sb], &cells[tb]);
    let at = if vertical {
        if sbc.row == tbc.row {
            return false;
        }
        max(sbc.row, tbc.row) - 1.0
    } else {
        let (l, r) = if sbc.col < tbc.col {
            (sbc, tbc)
        } else {
            (tbc, sbc)
        };
        if r.col < l.col + l.span {
            return false;
        }
        r.col - 1.0
    } as usize;
    let gaps = if vertical {
        &mut cells[container].row_gaps
    } else {
        &mut cells[container].col_gaps
    };
    let was = base
        .entry((container, vertical))
        .or_insert_with(|| gaps.clone());
    let before = was.get(at).copied().unwrap_or(0.0);
    if gaps.len() <= at {
        gaps.resize(at + 1, 0.0);
    }
    gaps[at] = max(gaps[at], before + short);
    true
}

/// Lays out a parsed block diagram as a scene, as Pele's renderBlock draws it: with room to
/// spare, and tightly instead when that does not fit the host's width and is narrower. Its
/// error is one of a composite of no columns.
pub(crate) fn layout(db: &BlockDb, config: &Map, o: &Options) -> Result<Scene, Error> {
    let mut err = None;
    let sc = tighten(o, |tight| match draw(db, config, o, tight) {
        Ok(sc) => sc,
        Err(e) => {
            err = Some(e);
            Scene::default()
        }
    });
    match err {
        Some(e) => Err(e),
        None => Ok(sc),
    }
}

/// Lays the diagram out once, as Pele's draw does.
fn draw(db: &BlockDb, config: &Map, o: &Options, tight: bool) -> Result<Scene, Error> {
    let size = o.font_size;
    let edge_size = round(size * 0.875);
    let set = setting(o);
    let pad = o.pad();
    let configured = match config
        .get("block")
        .and_then(Value::as_object)
        .and_then(|s| s.get("padding"))
    {
        Some(&Value::Number(v)) if v >= 0.0 => min(v, 200.0),
        _ => GAP,
    };
    let gap = min(configured, if tight { 6.0 } else { f64::INFINITY });
    let wrap = if tight { 80.0 } else { WRAP };

    // The declarations of a block's classes and styles, as Pele's styleOf reads them. A
    // style list is split on commas only, so one entry can still hold several declarations.
    let decls_of = |b: usize| {
        let block = &db.arena[b];
        let mut decls: Vec<String> = Vec::new();
        let mut add = |styles: &[String]| {
            for style in styles {
                decls.extend(style.split(';').map(str::to_string));
            }
        };
        let default = ["default".to_string()];
        let names = match &block.classes {
            Some(c) if !c.is_empty() => c.as_slice(),
            _ => &default,
        };
        for name in names {
            if let Some((_, def)) = db.classes.get(name) {
                add(&def.styles);
            }
        }
        if let Some(styles) = &block.styles {
            add(styles);
        }
        decls
    };

    let view = |b: usize, index: usize| -> (View, Cell) {
        let block = &db.arena[b];
        let kind = block.r#type.as_str();
        let decls = if kind == "space" {
            Vec::new()
        } else {
            decls_of(b)
        };
        let style = resolve_style(&decls);
        let mut face = Face::REGULAR;
        if style.bold {
            face |= Face::BOLD;
        }
        if style.italic {
            face |= Face::ITALIC;
        }
        let font_size = style.font_size.unwrap_or(size);
        let raw = block.label.as_deref().unwrap_or("");
        let mut v = View {
            block: b,
            kind: Kind::Leaf,
            index,
            shape: "",
            label: Label::default(),
            decls: Vec::new(),
            style: Resolved::default(),
            mask: 0,
            dx: 0.0,
            dy: 0.0,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            bw: 0.0,
            bh: 0.0,
        };
        let mut c = Cell::default();
        match kind {
            "space" => v.kind = Kind::Space,
            "composite" => {
                v.kind = Kind::Composite;
                let room = if tight { title_room(o) } else { 4000.0 };
                v.label = layout_label(raw, false, &set, font_size, room, face);
                c.head = if v.label.height > 0.0 {
                    v.label.height + 4.0
                } else {
                    0.0
                };
                c.nw = v.label.width + 2.0 * gap;
                c.nh = c.head + 2.0 * gap;
            }
            "block_arrow" => {
                v.kind = Kind::Arrow;
                v.label = layout_label(raw, false, &set, font_size, wrap, face);
                let blank = v
                    .label
                    .lines
                    .iter()
                    .flatten()
                    .all(|part| part.icon.is_none() && trim(&part.text).is_empty());
                v.mask = arrow_mask(block.directions.as_deref());
                let s = if blank {
                    arrow_size(v.mask, 0.0, 0.0)
                } else {
                    arrow_size(v.mask, v.label.width, v.label.height)
                };
                (c.nw, c.nh, v.dx, v.dy) = (s.w, s.h, s.dx, s.dy);
            }
            _ => {
                v.shape = shape_of(kind);
                v.label = layout_label(raw, false, &set, font_size, wrap, face);
                let s = shape_size(v.shape, v.label.width, v.label.height);
                c.nw = s.w;
                // A cylinder's caps deepen as it widens, so it is sized for the deepest ones.
                c.nh = if v.shape == "cyl" {
                    max(s.h, v.label.height + 40.0)
                } else {
                    s.h
                };
                v.dy = s.dy;
            }
        }
        c.columns = if v.kind == Kind::Composite {
            block.columns.unwrap_or(-1.0)
        } else {
            -1.0
        };
        c.span = if v.kind == Kind::Space {
            1.0
        } else {
            block.width_in_columns.unwrap_or(1.0)
        };
        c.space = v.kind == Kind::Space;
        v.decls = decls;
        v.style = style;
        (v, c)
    };

    let (mut root, mut root_cell) = view(ROOT, 0);
    root_cell.nw = 0.0;
    root_cell.nh = 0.0;
    root.kind = Kind::Composite;
    let mut views = vec![root];
    let mut cells = vec![root_cell];
    let mut parents: Vec<Option<usize>> = vec![None];
    let mut by_id: HashMap<&str, usize> = HashMap::new();
    let mut i = 0;
    while i < views.len() {
        if views[i].kind == Kind::Composite {
            let children = db.arena[views[i].block]
                .children
                .as_deref()
                .unwrap_or_default();
            for &child in children {
                let index = views.len();
                let (v, c) = view(child, index);
                cells[i].children.push(index);
                parents.push(Some(i));
                if v.kind != Kind::Space {
                    by_id.insert(db.arena[child].id.as_deref().unwrap_or(""), index);
                }
                views.push(v);
                cells.push(c);
            }
        }
        i += 1;
    }

    // The layout keeps the widest a container has been, so each run starts from the widths
    // it needs.
    let widths: Vec<f64> = cells.iter().map(|c| c.nw).collect();
    let mut obstacles: Vec<usize> = Vec::new();
    let mut titles: Vec<TitleBox> = Vec::new();
    // Lays the blocks out in their gaps, as Pele's place does.
    let place = |views: &mut [View],
                 cells: &mut [Cell],
                 obstacles: &mut Vec<usize>,
                 titles: &mut Vec<TitleBox>|
     -> Result<(f64, f64), Error> {
        for (c, &w) in cells.iter_mut().zip(&widths) {
            c.nw = w;
        }
        let content = lay_cells(cells, gap)?;
        obstacles.clear();
        titles.clear();
        for (v, c) in views.iter_mut().zip(cells.iter()) {
            (v.x, v.y, v.w, v.h) = (c.x, c.y, c.w, c.h);
            if v.kind == Kind::Leaf && fixed(v.shape) {
                v.bw = c.nw;
                v.bh = c.nh;
            } else if v.kind == Kind::Arrow {
                v.bw = if c.span > 1.0 { c.w } else { c.nw };
                v.bh = c.nh;
            } else {
                v.bw = c.w;
                v.bh = if v.kind == Kind::Leaf && wide(v.shape) {
                    c.nh
                } else {
                    c.h
                };
            }
            if v.kind == Kind::Leaf && v.shape == "cyl" {
                v.dy = min(12.0, max(6.0, v.bw / 14.0)) / 2.0;
            }
            if v.kind == Kind::Leaf || v.kind == Kind::Arrow {
                obstacles.push(v.index);
            } else if c.head > 0.0 {
                titles.push(TitleBox {
                    x: c.x,
                    y: c.y - c.h / 2.0 + (c.head + gap) / 2.0 + 2.0,
                    bw: v.label.width,
                    bh: v.label.height,
                });
            }
        }
        Ok(content)
    };
    let mut content = place(&mut views, &mut cells, &mut obstacles, &mut titles)?;

    // Edges between the same two blocks are spread apart instead of drawn on top of each
    // other.
    let order_len = views.len();
    let key_of = |s: usize, t: usize| s.min(t) * order_len + s.max(t);
    let mut pairs: HashMap<usize, usize> = HashMap::new();
    let mut labelled = std::collections::HashSet::new();
    let mut ends: Vec<(usize, usize)> = Vec::new();
    let mut drawn: Vec<usize> = Vec::new();
    for &edge in &db.edges {
        let e = &db.arena[edge];
        let s = by_id.get(e.start.as_deref().unwrap_or(""));
        let t = by_id.get(e.end.as_deref().unwrap_or(""));
        let (Some(&s), Some(&t)) = (s, t) else {
            continue;
        };
        let key = key_of(s, t);
        *pairs.entry(key).or_insert(0) += 1;
        if e.label.as_deref().is_some_and(|l| !l.is_empty()) {
            labelled.insert(key);
        }
        ends.push((s, t));
        drawn.push(edge);
    }
    let checking = drawn.len() * obstacles.len() <= OBSTACLE_BUDGET;
    let mut routes: Vec<Route> = Vec::new();
    // The labelled edges between each pair of blocks joined by more than one, in the order of
    // the first.
    let mut stacks: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut stack_keys: Vec<usize> = Vec::new();
    let mut legs: Vec<Vec<f64>> = vec![Vec::new(); drawn.len()];

    // Routes every edge, as Pele's route does.
    let route = |views: &[View],
                 obstacles: &[usize],
                 routes: &mut Vec<Route>,
                 stacks: &mut HashMap<usize, Vec<usize>>,
                 stack_keys: &mut Vec<usize>,
                 legs: &mut Vec<Vec<f64>>| {
        let mut seen: HashMap<usize, usize> = HashMap::new();
        routes.clear();
        stacks.clear();
        stack_keys.clear();
        let check = checking.then_some(obstacles);
        let blocked = check.map(|obstacles| Blocked {
            views,
            obstacles,
            titles: &[],
        });
        for i in 0..drawn.len() {
            let (s, t) = ends[i];
            let key = key_of(s, t);
            let total = pairs.get(&key).copied().unwrap_or(1);
            let k = seen.get(&key).copied().unwrap_or(0);
            seen.insert(key, k + 1);
            let raw = db.arena[drawn[i]].label.as_deref().unwrap_or("");
            let label = layout_label(raw, false, &set, edge_size, WRAP, Face::REGULAR);
            let mut points = Vec::new();
            let (lx, ly);
            if s == t {
                let (px, py, r, _, _) = loop_arc(&views[s], k + 1);
                lx = px + r + 6.0 + label.width / 2.0;
                ly = py - r / 2.0;
            } else {
                let step = if labelled.contains(&key) {
                    round(edge_size * 1.5) + 2.0
                } else {
                    14.0
                };
                let shift = (k as f64 - (total as f64 - 1.0) / 2.0) * step;
                points = route_edge(views, s, t, shift, check);
                let n = points.len();
                // The label goes on the longer leg.
                let first_leg = (points[2] - points[0]).abs() + (points[3] - points[1]).abs();
                let last_leg =
                    (points[n - 2] - points[n - 4]).abs() + (points[n - 1] - points[n - 3]).abs();
                let at = if n == 6 && last_leg > first_leg { 2 } else { 0 };
                let spot = if label.width > 0.0 {
                    label_spot(&points, at, label.width + 8.0, label.height, s, t, blocked)
                } else {
                    0.5
                };
                lx = points[at] + (points[at + 2] - points[at]) * spot;
                ly = points[at + 1] + (points[at + 3] - points[at + 1]) * spot;
                if label.width > 0.0 && total > 1 {
                    legs[i] = points[at..at + 4].to_vec();
                    stacks
                        .entry(key)
                        .or_insert_with(|| {
                            stack_keys.push(key);
                            Vec::new()
                        })
                        .push(i);
                }
            }
            routes.push(Route {
                edge: drawn[i],
                points,
                looped: if s == t { k + 1 } else { 0 },
                label,
                lx,
                ly,
            });
        }
    };

    // Routes the edges and stacks the labels of each pair of blocks that cover each other,
    // as Pele's settle does. Where they do not fit and grow is set, it widens the gap between
    // the blocks, across the rows always and across the columns unless the layout is tight,
    // which keeps columns narrow to fit the host, and says whether it did.
    let settle = |grow: bool,
                  views: &[View],
                  cells: &mut [Cell],
                  obstacles: &[usize],
                  titles: &[TitleBox],
                  routes: &mut Vec<Route>,
                  stacks: &mut HashMap<usize, Vec<usize>>,
                  stack_keys: &mut Vec<usize>,
                  legs: &mut Vec<Vec<f64>>| {
        route(views, obstacles, routes, stacks, stack_keys, legs);
        let blocked = checking.then_some(Blocked {
            views,
            obstacles,
            titles,
        });
        let mut base = HashMap::new();
        let mut grown = false;
        for key in stack_keys.iter() {
            let stack = &stacks[key];
            if stack.len() < 2 {
                continue;
            }
            let (s, t) = ends[stack[0]];
            let short = stack_labels(stack, routes, legs, s, t, blocked);
            let vertical = legs[stack[0]][0] == legs[stack[0]][2];
            if grow
                && short > 0.0
                && (vertical || !tight)
                && widen(cells, &parents, s, t, vertical, short.ceil(), &mut base)
            {
                grown = true;
            }
        }
        grown
    };

    // The area of the edge labels that cover each other, as Pele's overlap sums it.
    let overlap = |routes: &[Route]| {
        let mut area = 0.0;
        for (i, p) in routes.iter().enumerate() {
            for q in &routes[i + 1..] {
                let pw = (p.label.width + 8.0) / 2.0;
                let qw = (q.label.width + 8.0) / 2.0;
                let w = min(p.lx + pw, q.lx + qw) - max(p.lx - pw, q.lx - qw);
                let h = min(p.ly + p.label.height / 2.0, q.ly + q.label.height / 2.0)
                    - max(p.ly - p.label.height / 2.0, q.ly - q.label.height / 2.0);
                if p.label.width > 0.0 && q.label.width > 0.0 && w > 0.0 && h > 0.0 {
                    area += w * h;
                }
            }
        }
        area
    };
    // The extent of the drawing, as Pele's bound finds it: its left, top, right and bottom.
    let bound = |content: (f64, f64), views: &[View], routes: &[Route]| {
        let mut b = [0.0, 0.0, content.0, content.1];
        let mut grow = |x: f64, y: f64, w: f64, h: f64| {
            if x - w / 2.0 < b[0] {
                b[0] = x - w / 2.0;
            }
            if x + w / 2.0 > b[2] {
                b[2] = x + w / 2.0;
            }
            if y - h / 2.0 < b[1] {
                b[1] = y - h / 2.0;
            }
            if y + h / 2.0 > b[3] {
                b[3] = y + h / 2.0;
            }
        };
        for (i, r) in routes.iter().enumerate() {
            if r.looped > 0 {
                let (px, py, rr, _, _) = loop_arc(&views[ends[i].0], r.looped);
                grow(px, py, 2.0 * rr + 2.0, 2.0 * rr + 2.0);
            }
            if r.label.width > 0.0 {
                grow(r.lx, r.ly, r.label.width + 8.0, r.label.height);
            }
        }
        b
    };

    if settle(
        drawn.len() * drawn.len() <= OBSTACLE_BUDGET,
        &views,
        &mut cells,
        &obstacles,
        &titles,
        &mut routes,
        &mut stacks,
        &mut stack_keys,
        &mut legs,
    ) {
        let before = overlap(&routes);
        let b = bound(content, &views, &routes);
        let narrow = b[2] - b[0];
        // A container stretched by its row gives some of a new gap back, so the gap may take
        // a few rounds.
        let mut grown = true;
        let mut round = 0;
        while round < 3 && grown {
            content = place(&mut views, &mut cells, &mut obstacles, &mut titles)?;
            grown = settle(
                round < 2,
                &views,
                &mut cells,
                &obstacles,
                &titles,
                &mut routes,
                &mut stacks,
                &mut stack_keys,
                &mut legs,
            );
            round += 1;
        }
        // The drawing only grows if that leaves less of the labels covering each other, and
        // within the host's width.
        let b = bound(content, &views, &routes);
        let wide = b[2] - b[0];
        let room = if o.max_width > 0.0 {
            o.max_width
        } else {
            f64::INFINITY
        };
        if overlap(&routes) > before - 1.0 || (wide > narrow && (wide + 2.0 * pad).ceil() > room) {
            for c in &mut cells {
                c.row_gaps = Vec::new();
                c.col_gaps = Vec::new();
            }
            content = place(&mut views, &mut cells, &mut obstacles, &mut titles)?;
            settle(
                false,
                &views,
                &mut cells,
                &obstacles,
                &titles,
                &mut routes,
                &mut stacks,
                &mut stack_keys,
                &mut legs,
            );
        }
    }
    let [min_x, min_y, max_x, max_y] = bound(content, &views, &routes);

    let room = if tight { title_room(o) } else { 4000.0 };
    let title = title_label(db.title.as_deref(), &set, size, room);
    let title_height = if title.height > 0.0 {
        title.height + 12.0
    } else {
        0.0
    };
    let inner = max(max_x - min_x, title.width);
    let ox = pad - min_x + (inner - (max_x - min_x)) / 2.0;
    let oy = pad + title_height - min_y;
    for v in &mut views {
        v.x += ox;
        v.y += oy;
    }
    let width = (inner + 2.0 * pad).ceil();
    let height = (max_y - min_y + title_height + 2.0 * pad).ceil();

    let mut heading = Vec::new();
    let mut clusters = Vec::new();
    let mut casings = Vec::new();
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut labels = Vec::new();
    // The composites whose authors paint them a fill, which edges inside them are drawn on,
    // as Pele's fills.
    let mut fills: Vec<Fill> = Vec::new();
    if !title.is_empty() {
        let mut it = Item::new(ItemKind::Text, ROLE_TITLE);
        let at = Point::new(width / 2.0, pad + title.height / 2.0);
        it.label = Some(Placed::new(title, at));
        heading.push(it);
    }
    for (v, c) in views.iter().zip(&cells).skip(1) {
        if v.kind == Kind::Space {
            continue;
        }
        let b = &db.arena[v.block];
        let classes = class_names(&b.classes.as_deref().unwrap_or_default().join(" "));
        let author = declarations(&v.decls).style;
        let inks = v.style.inks_text(&v.style);
        if v.kind == Kind::Composite {
            let top = v.y - v.h / 2.0;
            let bounds = Rect::new(v.x - v.w / 2.0, top, v.w, v.h);
            if v.style.paints() {
                fills.push(Fill {
                    bounds: [v.x - v.w / 2.0, top, v.x + v.w / 2.0, top + v.h],
                    decls: v.decls.clone(),
                });
            }
            let mut it = Item::new(ItemKind::Group, ROLE_COMPOSITE);
            it.id = b.id.clone();
            it.class = classes;
            it.bounds = bounds;
            it.parts = vec![Part::rect(bounds, true, Painting::FillAndStroke)];
            it.style = Style {
                fill: Some(Paint::SURFACE_ALT),
                // A composite is drawn at half its opacity, unless its author sets a fill.
                fill_opacity: if v.style.fill_declared() {
                    None
                } else {
                    Some(0.5)
                },
                stroke: Some(Paint::BORDER),
                text: Some(Paint::MUTED),
                on_fill: inks,
                ..Style::default()
            };
            it.author = author;
            if !v.label.is_empty() {
                let at = Point::new(v.x, top + (c.head + gap) / 2.0 + 2.0);
                it.label = Some(Placed::new(v.label.clone(), at));
            }
            clusters.push(it);
            continue;
        }
        let mut it = Item::new(ItemKind::Node, ROLE_BLOCK);
        it.shape = Some(Shape::new(v.shape));
        it.id = b.id.clone();
        it.class = classes;
        it.bounds = Rect::new(v.x - v.bw / 2.0, v.y - v.bh / 2.0, v.bw, v.bh);
        it.style = Style {
            fill: Some(Paint::SURFACE),
            stroke: Some(Paint::BORDER),
            on_fill: inks,
            ..Style::default()
        };
        it.author = author;
        if v.kind == Kind::Arrow {
            it.shape = Some(Shape::new("block-arrow"));
            let p = draw_arrow(v.mask, v.x, v.y, v.bw, v.bh);
            it.parts = vec![Part::path(p, Painting::FillAndStroke)];
            // Pele turns the corners of an arrow round.
            it.style.join = Some(Join::Round);
        } else {
            it.parts = draw_shape(v.shape, v.x, v.y, v.bw, v.bh);
        }
        if !v.label.is_empty() {
            let at = Point::new(v.x + v.dx, v.y + v.dy);
            it.label = Some(Placed::new(v.label.clone(), at));
        }
        nodes.push(it);
    }

    for (i, r) in routes.iter_mut().enumerate() {
        let edge = &db.arena[r.edge];
        let start = edge
            .arrow_type_start
            .as_deref()
            .and_then(MarkerType::from_name);
        let end = edge
            .arrow_type_end
            .as_deref()
            .and_then(MarkerType::from_name);
        let path = if r.looped > 0 {
            loop_path(
                &views[ends[i].0],
                r.looped,
                marker_trim(start),
                marker_trim(end),
            )
        } else {
            for k in (0..r.points.len()).step_by(2) {
                r.points[k] += ox;
                r.points[k + 1] += oy;
            }
            trace(&r.points, marker_trim(start), marker_trim(end))
        };
        let thick = edge.thickness.as_deref() == Some("thick");
        // Pele draws the edges in a group with round caps.
        let mut own = Style {
            stroke: Some(Paint::LINE),
            fill: Some(Paint::NONE),
            cap: Some(Cap::Round),
            ..Style::default()
        };
        if thick {
            own.stroke_width = Some(2.5);
        }
        if edge.pattern.as_deref() == Some("dotted") {
            own.dash = Some(vec![3.0, 4.0]);
        }
        // An edge inside one fill is drawn in the color of its labels, and one that crosses
        // the border of a fill is laid on a casing, as Pele's overFills finds.
        let marks = path.ends(start, end);
        let tips = [
            path.start.at.x,
            path.start.at.y,
            path.end.at.x,
            path.end.at.y,
        ];
        let (ground, cased) = on_fills(&path.path, &tips, &fills, "");
        let id = edge.id.clone();
        if cased && let Some(mut c) = casing(ROLE_CASING, &path.path, &marks, &[], thick) {
            c.id = id.clone();
            casings.push(c);
        }
        own.ground = ground;
        let mut route: Vec<Point> = r.points.chunks(2).map(|p| Point::new(p[0], p[1])).collect();
        if route.is_empty() {
            // A self-loop, which the layout gives no route: the corners its curve is drawn
            // about.
            route = crate::items::corners(&path.path);
        }
        let mut it = Item::new(ItemKind::Edge(Edge { route, ends: marks }), ROLE_EDGE);
        it.id = id.clone();
        it.bounds = path.path.bounds();
        it.parts = vec![Part::path(path.path, Painting::StrokeOnly)];
        it.style = own;
        edges.push(it);
        if r.label.width > 0.0 {
            let (x, y) = (r.lx + ox, r.ly + oy);
            let l = &r.label;
            let mut it = Item::new(ItemKind::Text, ROLE_EDGE_LABEL);
            it.id = id;
            it.bounds = label_patch_box(l.width, l.height, x, y);
            it.parts = vec![label_patch(l.width, l.height, x, y)];
            it.label = Some(Placed::new(l.clone(), Point::new(x, y)));
            it.style = Style {
                fill: Some(Paint::BG),
                stroke: Some(Paint::NONE),
                ..Style::default()
            };
            labels.push(it);
        }
    }

    let mut sc = Scene {
        diagram: "block".into(),
        width,
        height,
        font_size: size,
        title: None,
        desc: None,
        items: Vec::new(),
    };
    for layer in [heading, clusters, casings, edges, nodes, labels] {
        sc.items.extend(layer);
    }
    Ok(sc)
}
