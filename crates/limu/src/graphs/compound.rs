//! Layout of a graph whose nodes may be nested in groups, as Pele's src/layout/compound.ts
//! lays it out. Each group is laid out on its own, in its own direction, and then placed in its
//! parent as a single box. An edge that crosses a group's border passes through a port on the
//! side facing its other end.

use std::collections::HashMap;

use super::layered::{
    Groups, Kind, LEdge, LNode, LayeredOptions, LayeredResult, ease, layered, ledge, lnode, orient,
    rank,
};
use crate::js::math::{by, hypot, max, min, or, round};

/// The way a graph flows, as Pele's Dir names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dir {
    TB,
    BT,
    LR,
    RL,
}

/// The direction a diagram names, or top to bottom for anything else, as Pele's direction
/// reads it.
pub(crate) fn direction(dir: Option<&str>) -> Dir {
    match dir {
        Some("BT") => Dir::BT,
        Some("LR") => Dir::LR,
        Some("RL") => Dir::RL,
        _ => Dir::TB,
    }
}

/// A node of a compound layout, as Pele's CNode is.
#[derive(Clone, Debug)]
pub(crate) struct CNode {
    pub(crate) w: f64,
    pub(crate) h: f64,
    pub(crate) parent: i32,
    pub(crate) is_group: bool,
    pub(crate) dir: Option<Dir>,
    pub(crate) pad_x: f64,
    pub(crate) pad_top: f64,
    pub(crate) pad_bottom: f64,
    pub(crate) min_w: f64,
    /// Declaration order. A group takes the smallest value among its members.
    pub(crate) seq: f64,
    /// How much of a side its edges may spread along, measured across the flow. 0 keeps them
    /// at its middle.
    pub(crate) span: f64,
    pub(crate) x: f64,
    pub(crate) y: f64,
}

/// An edge of a compound layout, as Pele's CEdge is.
#[derive(Clone, Debug)]
pub(crate) struct CEdge {
    pub(crate) src: usize,
    pub(crate) dst: usize,
    pub(crate) minlen: f64,
    pub(crate) label_w: f64,
    pub(crate) label_h: f64,
    /// x, y, axis triples. The axis is 0 where the edge runs vertically into the point, 1
    /// horizontally.
    pub(crate) route: Vec<f64>,
    pub(crate) label_x: f64,
    pub(crate) label_y: f64,
}

/// The size of a compound layout, as Pele's CompoundResult is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct CompoundResult {
    pub(crate) width: f64,
    pub(crate) height: f64,
}

/// A side of a group, as Pele's Side numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

/// An edge where it crosses a group's border, as Pele's PortEdge is.
#[derive(Clone, Debug)]
struct PortEdge {
    e: usize,
    inner: usize,
    out: bool,
    side: Side,
    /// Cross-flow position outside the group from a previous layout, NaN if unknown.
    hint: f64,
    px: f64,
    py: f64,
    route: Vec<f64>,
}

/// An edge between two items of a level, as Pele's LevelEdge is.
#[derive(Clone, Copy, Debug)]
struct LevelEdge {
    e: usize,
    a: usize,
    b: usize,
}

/// The items a group holds, laid out together, as Pele's Level is. `port_of` finds a port by
/// its edge, twice the edge's index and one more for the end it arrives at, as Pele's portOf
/// Map does.
#[derive(Debug)]
struct Level {
    id: i32,
    dir: Dir,
    items: Vec<usize>,
    edges: Vec<LevelEdge>,
    ports: Vec<PortEdge>,
    port_of: HashMap<usize, usize>,
    routes: HashMap<usize, Vec<f64>>,
    x: f64,
    y: f64,
}

const MAX_PORTS: usize = 40000;

// Limit repeated layouts to small graphs.
const MAX_ORDERED_PORTS: usize = 400;
const MAX_ORDERED_EDGES: usize = 500;
// Crossing tolerance before trying independent group ordering.
const FLAT_SLACK: f64 = 1.5;
const FLAT_SPARE: f64 = 4.0;

/// Where a compound layout put everything, to go back to, as Pele's Snapshot is.
struct Snapshot {
    nodes: Vec<f64>,
    routes: Vec<Vec<f64>>,
    labels: Vec<f64>,
    levels: Vec<f64>,
    total: CompoundResult,
}

/// As Pele's snapshot.
fn snapshot(nodes: &[CNode], edges: &[CEdge], levels: &[Level], total: CompoundResult) -> Snapshot {
    let mut at = Vec::new();
    for c in nodes {
        at.extend([c.x, c.y, c.w, c.h]);
    }
    let mut labels = Vec::new();
    for e in edges {
        labels.extend([e.label_x, e.label_y]);
    }
    let mut places = Vec::new();
    for level in levels {
        places.extend([level.x, level.y]);
        for port in &level.ports {
            places.extend([port.px, port.py]);
        }
    }
    Snapshot {
        nodes: at,
        routes: edges.iter().map(|e| e.route.clone()).collect(),
        labels,
        levels: places,
        total,
    }
}

/// As Pele's restore.
fn restore(
    s: Snapshot,
    nodes: &mut [CNode],
    edges: &mut [CEdge],
    levels: &mut [Level],
) -> CompoundResult {
    for (i, c) in nodes.iter_mut().enumerate() {
        c.x = s.nodes[4 * i];
        c.y = s.nodes[4 * i + 1];
        c.w = s.nodes[4 * i + 2];
        c.h = s.nodes[4 * i + 3];
    }
    for ((i, e), route) in edges.iter_mut().enumerate().zip(s.routes) {
        e.route = route;
        e.label_x = s.labels[2 * i];
        e.label_y = s.labels[2 * i + 1];
    }
    let mut places = s.levels.into_iter();
    let mut next = || places.next().unwrap_or_default();
    for level in levels {
        level.x = next();
        level.y = next();
        for port in &mut level.ports {
            port.px = next();
            port.py = next();
        }
    }
    s.total
}

// Ignore crossings within this distance of a shared node.
const FAN: f64 = 24.0;
// Tolerance for collinear points.
const IN_LINE: f64 = 0.5;
const CURVE_PIECES: usize = 3;
const CORNER_PIECES: usize = 2;
// Allow for a step, an S-curve, or a straight segment plus a rounded corner.
const STEP_PIECES: usize = 3;
// Match the renderer's corner handles.
const HANDLE: f64 = 0.62;
const MAX_INSERT: usize = 128;

/// How edges are drawn, as Pele's Style says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Style {
    Curved,
    Stepped,
    Linear,
}

/// As Pele's styleOf.
pub(crate) fn style_of(opt: &LayeredOptions) -> Style {
    if opt.tracks {
        Style::Stepped
    } else if opt.straight {
        Style::Linear
    } else {
        Style::Curved
    }
}

/// The straight segments that `route_crossings` cuts the edges into: each one's bounds, the
/// edge it is of, and whether it is the first (1) or last (2) segment of its edge. It keeps
/// the last segment added, to run on with the next one.
struct Segments {
    ax: Vec<f64>,
    ay: Vec<f64>,
    bx: Vec<f64>,
    by: Vec<f64>,
    low: Vec<f64>,
    high: Vec<f64>,
    owner: Vec<usize>,
    ends: Vec<u8>,
    count: usize,
    top: f64,
    bottom: f64,
    from_x: f64,
    from_y: f64,
    to_x: f64,
    to_y: f64,
}

impl Segments {
    /// Adds a segment, or lengthens the last one where this one runs on from it in line, as
    /// the add of Pele's routeCrossings does.
    fn add(&mut self, ei: usize, part: u8, mut x0: f64, mut y0: f64, x1: f64, y1: f64) {
        let on = x0 == self.to_x
            && y0 == self.to_y
            && (y1 == y0 && self.from_y == y0 || x1 == x0 && self.from_x == x0);
        if on && self.count > 0 {
            let p = self.count - 1;
            if self.owner[p] == ei
                && self.ends[p] == part
                && (x0 - self.from_x) * (x1 - x0) + (y0 - self.from_y) * (y1 - y0) > 0.0
            {
                self.count -= 1;
                x0 = self.from_x;
                y0 = self.from_y;
            }
        }
        self.from_x = x0;
        self.from_y = y0;
        self.to_x = x1;
        self.to_y = y1;
        let c = self.count;
        self.ends[c] = part;
        self.ax[c] = min(x0, x1);
        self.bx[c] = max(x0, x1);
        self.ay[c] = if x0 <= x1 { y0 } else { y1 };
        self.by[c] = if x0 <= x1 { y1 } else { y0 };
        self.low[c] = min(y0, y1);
        self.high[c] = max(y0, y1);
        if self.low[c] < self.top {
            self.top = self.low[c];
        }
        if self.high[c] > self.bottom {
            self.bottom = self.high[c];
        }
        self.owner[c] = ei;
        self.count += 1;
    }
}

/// How often the routes of the edges cross, drawn in a style, as Pele's routeCrossings counts
/// it.
pub(crate) fn route_crossings(nodes: &[CNode], edges: &[CEdge], style: Style) -> usize {
    let mut size = 0;
    for e in edges {
        if e.src != e.dst && e.route.len() > 3 {
            size += (e.route.len() / 3 - 1) * STEP_PIECES;
        }
    }
    let mut s = Segments {
        ax: vec![0.0; size],
        ay: vec![0.0; size],
        bx: vec![0.0; size],
        by: vec![0.0; size],
        low: vec![0.0; size],
        high: vec![0.0; size],
        owner: vec![0; size],
        ends: vec![0; size],
        count: 0,
        top: f64::INFINITY,
        bottom: f64::NEG_INFINITY,
        from_x: f64::NAN,
        from_y: f64::NAN,
        to_x: f64::NAN,
        to_y: f64::NAN,
    };
    for (ei, e) in edges.iter().enumerate() {
        if e.src == e.dst {
            continue;
        }
        let r = &e.route;
        let aligned =
            |k: usize| (r[k] - r[k - 3]).abs() < 0.01 || (r[k + 1] - r[k - 2]).abs() < 0.01;
        let length = |k: usize| (r[k] - r[k - 3]).abs() + (r[k + 1] - r[k - 2]).abs();
        let turns = |k: usize| {
            style == Style::Curved
                && k > 0
                && k + 3 < r.len()
                && aligned(k)
                && aligned(k + 3)
                && length(k) >= 0.01
                && length(k + 3) >= 0.01
                && ((r[k] - r[k - 3]).abs() < 0.01) != ((r[k + 3] - r[k]).abs() < 0.01)
        };
        let mut at_x = r.first().copied().unwrap_or(f64::NAN);
        let mut at_y = r.get(1).copied().unwrap_or(f64::NAN);
        for k in (3..r.len()).step_by(3) {
            let part = u8::from(k == 3) | if k == r.len() - 3 { 2 } else { 0 };
            let x0 = at_x;
            let y0 = at_y;
            let x1 = r[k];
            let y1 = r[k + 1];
            at_x = x1;
            at_y = y1;
            if aligned(k) || style == Style::Linear {
                if !turns(k) {
                    s.add(ei, part, x0, y0, x1, y1);
                    continue;
                }
                // Adjacent corners share a leg, so each gets half.
                let before = if turns(k - 3) {
                    length(k) / 2.0
                } else {
                    length(k)
                };
                let after = if turns(k + 3) {
                    length(k + 3) / 2.0
                } else {
                    length(k + 3)
                };
                let ux = (x1 - r[k - 3]) / length(k);
                let uy = (y1 - r[k - 2]) / length(k);
                let vx = (r[k + 3] - x1) / length(k + 3);
                let vy = (r[k + 4] - y1) / length(k + 3);
                let sx = x1 - ux * before;
                let sy = y1 - uy * before;
                s.add(ei, part, x0, y0, sx, sy);
                let c1x = x1 - ux * before * (1.0 - HANDLE);
                let c1y = y1 - uy * before * (1.0 - HANDLE);
                let c2x = x1 + vx * after * (1.0 - HANDLE);
                let c2y = y1 + vy * after * (1.0 - HANDLE);
                at_x = x1 + vx * after;
                at_y = y1 + vy * after;
                let turn = part | if k + 3 == r.len() - 3 { 2 } else { 0 };
                let mut px = sx;
                let mut py = sy;
                for i in 1..=CORNER_PIECES {
                    let t = i as f64 / CORNER_PIECES as f64;
                    let u = 1.0 - t;
                    let x = u * u * u * sx
                        + 3.0 * u * u * t * c1x
                        + 3.0 * u * t * t * c2x
                        + t * t * t * at_x;
                    let y = u * u * u * sy
                        + 3.0 * u * u * t * c1y
                        + 3.0 * u * t * t * c2y
                        + t * t * t * at_y;
                    s.add(ei, turn, px, py, x, y);
                    px = x;
                    py = y;
                }
                continue;
            }
            if style == Style::Stepped {
                let down = r[k + 2] == 0.0;
                let mx = if down { x0 } else { (x0 + x1) / 2.0 };
                let my = if down { (y0 + y1) / 2.0 } else { y0 };
                let (nx, ny) = if down { (x1, my) } else { (mx, y1) };
                s.add(ei, part, x0, y0, mx, my);
                s.add(ei, part, mx, my, nx, ny);
                s.add(ei, part, nx, ny, x1, y1);
                continue;
            }
            // Approximate the rendered S-curve with straight segments.
            let down = r[k + 2] == 0.0;
            let c = if down {
                ease(x1 - x0, y1 - y0)
            } else {
                ease(y1 - y0, x1 - x0)
            };
            let mut px = x0;
            let mut py = y0;
            for i in 1..=CURVE_PIECES {
                let t = i as f64 / CURVE_PIECES as f64;
                let u = 1.0 - t;
                let across = t * t * (3.0 - 2.0 * t);
                let along = 3.0 * c * t * u * u + 3.0 * (1.0 - c) * t * t * u + t * t * t;
                let x = x0 + (x1 - x0) * if down { across } else { along };
                let y = y0 + (y1 - y0) * if down { along } else { across };
                s.add(ei, part, px, py, x, y);
                px = x;
                py = y;
            }
        }
    }
    let Segments {
        ref ax,
        ref ay,
        ref bx,
        by: ref sby,
        ref low,
        ref high,
        ref owner,
        ref ends,
        count,
        top,
        bottom,
        ..
    } = s;
    // Count each pair only in the first band shared by both segments.
    let bands = if bottom > top {
        round((count as f64).sqrt()) as usize
    } else {
        1
    };
    let scale = if bottom > top {
        bands as f64 / (bottom - top)
    } else {
        0.0
    };
    let band_of = |y: f64| min((bands - 1) as f64, ((y - top) * scale).floor()) as usize;
    let mut first = vec![0usize; count];
    let mut band_start = vec![0usize; bands + 1];
    for p in 0..count {
        first[p] = band_of(low[p]);
        for b in first[p]..=band_of(high[p]) {
            band_start[b + 1] += 1;
        }
    }
    for b in 0..bands {
        band_start[b + 1] += band_start[b];
    }
    let mut fill = band_start[..bands].to_vec();
    let mut in_band = vec![0usize; band_start[bands]];
    for p in 0..count {
        for b in first[p]..=band_of(high[p]) {
            in_band[fill[b]] = p;
            fill[b] += 1;
        }
    }
    // Edges merging into a shared endpoint run do not count as crossings.
    let mut runs = vec![0.0; edges.len() * 8];
    let in_line = |r: &[f64], a: usize, b: usize, c: usize| {
        (r[a] - r[b]).abs() < IN_LINE && (r[a] - r[c]).abs() < IN_LINE
            || (r[a + 1] - r[b + 1]).abs() < IN_LINE && (r[a + 1] - r[c + 1]).abs() < IN_LINE
    };
    for (ei, e) in edges.iter().enumerate() {
        let r = &e.route;
        if r.len() < 6 {
            continue;
        }
        let mut k = 3;
        while k + 3 < r.len() && in_line(r, 0, k, k + 3) {
            k += 3;
        }
        let last = r.len() - 3;
        let mut j = last - 3;
        while j >= 3 && in_line(r, last, j, j - 3) {
            j -= 3;
        }
        runs[ei * 8..ei * 8 + 8].copy_from_slice(&[
            r[0],
            r[1],
            r[k],
            r[k + 1],
            r[j],
            r[j + 1],
            r[last],
            r[last + 1],
        ]);
    }
    let on_run = |ei: usize, end: u8, x: f64, y: f64| {
        let o = ei * 8 + if end == 1 { 0 } else { 4 };
        let x0 = runs[o];
        let y0 = runs[o + 1];
        let dx = runs[o + 2] - x0;
        let dy = runs[o + 3] - y0;
        let length = dx * dx + dy * dy;
        let t = if length > 0.0 {
            max(0.0, min(1.0, ((x - x0) * dx + (y - y0) * dy) / length))
        } else {
            0.0
        };
        hypot(x - x0 - t * dx, y - y0 - t * dy) < IN_LINE
    };
    let mut total = 0;
    for band in 0..bands {
        let band_end = band_start[band + 1];
        let from = band_start[band];
        if band_end - from > MAX_INSERT {
            in_band[from..band_end].sort_by(|&p, &q| by(or(ax[p] - ax[q], p as f64 - q as f64)));
        } else {
            for i in from + 1..band_end {
                let p = in_band[i];
                let mut j = i;
                while j > from && ax[in_band[j - 1]] > ax[p] {
                    in_band[j] = in_band[j - 1];
                    j -= 1;
                }
                in_band[j] = p;
            }
        }
        for s in from..band_end {
            let p = in_band[s];
            let dx = bx[p] - ax[p];
            let dy = sby[p] - ay[p];
            let here = first[p] == band;
            for &q in &in_band[s + 1..band_end] {
                if ax[q] > bx[p] {
                    break;
                }
                if high[q] < low[p]
                    || low[q] > high[p]
                    || owner[p] == owner[q]
                    || !here && first[q] != band
                {
                    continue;
                }
                let d1 = dx * (ay[q] - ay[p]) - dy * (ax[q] - ax[p]);
                let d2 = dx * (sby[q] - ay[p]) - dy * (bx[q] - ax[p]);
                // Treat points on the line as one side to count segment-boundary crossings
                // once.
                if (d1 > 0.0) == (d2 > 0.0) {
                    continue;
                }
                let ex = bx[q] - ax[q];
                let ey = sby[q] - ay[q];
                let d3 = ex * (ay[p] - ay[q]) - ey * (ax[p] - ax[q]);
                let d4 = ex * (sby[p] - ay[q]) - ey * (bx[p] - ax[q]);
                if (d3 > 0.0) == (d4 > 0.0) {
                    continue;
                }
                let ep = &edges[owner[p]];
                let eq = &edges[owner[q]];
                let at = d3 / (d3 - d4);
                let x = ax[p] + at * dx;
                let y = ay[p] + at * dy;
                // Exclude fan-in and fan-out at shared nodes.
                let fan = |v: usize, pe: u8, qe: u8| {
                    ends[p] & pe != 0 && ends[q] & qe != 0
                        || (x - nodes[v].x).abs() < nodes[v].w / 2.0 + FAN
                            && (y - nodes[v].y).abs() < nodes[v].h / 2.0 + FAN
                        || on_run(owner[p], pe, x, y) && on_run(owner[q], qe, x, y)
                };
                if ep.src == eq.src && fan(ep.src, 1, 1) {
                    continue;
                }
                if ep.src == eq.dst && fan(ep.src, 1, 2) {
                    continue;
                }
                if ep.dst == eq.src && fan(ep.dst, 2, 1) {
                    continue;
                }
                if ep.dst == eq.dst && fan(ep.dst, 2, 2) {
                    continue;
                }
                total += 1;
            }
        }
    }
    total
}

/// Gives the groups that edges cross no direction of their own, so that they take their
/// parent's, as Mermaid does and as Pele's inheritCrossedDirections does.
pub(crate) fn inherit_crossed_directions(
    nodes: &mut [CNode],
    ends: impl IntoIterator<Item = (i32, i32)>,
) {
    let ancestors = |nodes: &[CNode], i: usize| {
        let mut set: Vec<usize> = Vec::new();
        let mut p = nodes[i].parent;
        while p >= 0 && !set.contains(&(p as usize)) {
            set.push(p as usize);
            p = nodes[p as usize].parent;
        }
        set
    };
    for (src, dst) in ends {
        if src < 0 || dst < 0 || src == dst {
            continue;
        }
        let a = ancestors(nodes, src as usize);
        let b = ancestors(nodes, dst as usize);
        for &g in &a {
            if !b.contains(&g) {
                nodes[g].dir = None;
            }
        }
        for &g in &b {
            if !a.contains(&g) {
                nodes[g].dir = None;
            }
        }
    }
}

/// Moves a whole layout by ox, oy, as Pele's shiftLayout does.
pub(crate) fn shift_layout(nodes: &mut [CNode], edges: &mut [CEdge], ox: f64, oy: f64) {
    for c in nodes.iter_mut() {
        c.x += ox;
        c.y += oy;
    }
    for e in edges.iter_mut() {
        for point in e.route.chunks_mut(3) {
            point[0] += ox;
            point[1] += oy;
        }
        e.label_x += ox;
        e.label_y += oy;
    }
}

/// A node, as Pele's cnode makes one.
pub(crate) fn cnode(w: f64, h: f64, parent: i32) -> CNode {
    CNode {
        w,
        h,
        parent,
        is_group: false,
        dir: None,
        pad_x: 0.0,
        pad_top: 0.0,
        pad_bottom: 0.0,
        min_w: 0.0,
        seq: 0.0,
        span: 0.0,
        x: 0.0,
        y: 0.0,
    }
}

/// As Pele's startSide.
fn start_side(dir: Dir) -> Side {
    match dir {
        Dir::TB => Side::Top,
        Dir::BT => Side::Bottom,
        Dir::LR => Side::Left,
        Dir::RL => Side::Right,
    }
}

/// As Pele's opposite.
fn opposite(side: Side) -> Side {
    match side {
        Side::Top => Side::Bottom,
        Side::Right => Side::Left,
        Side::Bottom => Side::Top,
        Side::Left => Side::Right,
    }
}

fn vertical(dir: Dir) -> bool {
    dir == Dir::TB || dir == Dir::BT
}

/// As Pele's endpointBelow: whether the end of an edge that leaves (`out`) or arrives at an
/// item is inside it, not the item itself.
fn endpoint_below(item: usize, e: &CEdge, out: bool) -> bool {
    (if out { e.src } else { e.dst }) != item
}

/// The key a port is found by in its level's `port_of`.
fn port_key(e: usize, out: bool) -> usize {
    e * 2 + usize::from(!out)
}

/// What the flat layout of the whole graph tells the levels, for Pele's farHint: which of its
/// edges each edge is, its edges' points, and the last rank each group takes.
struct Far {
    ledge_of: Vec<i32>,
    ledges: Vec<LEdge>,
    last: Vec<i32>,
}

impl Far {
    /// Orders outgoing ports at the group's last rank, and breaks ties using the next rank, as
    /// Pele's farHint does.
    fn hint(&self, key: &[f64], edges: &[CEdge], level: &Level, port: &PortEdge) -> f64 {
        let le = self.ledge_of[port.e];
        if le < 0 || level.id < 0 || !port.out || port.side != opposite(start_side(level.dir)) {
            return f64::NAN;
        }
        let points = &self.ledges[le as usize].points;
        let last = self.last[level.id as usize];
        let mut inside = key[edges[port.e].src];
        let mut outside = 0.0;
        for point in points.chunks(2) {
            if point[0] == f64::from(last) {
                inside = point[1];
            }
            if point[0] == f64::from(last + 1) {
                outside = point[1];
            }
        }
        inside + outside * 1e-6
    }
}

/// A level laid out, with what it was laid out from, for Pele's layOut to reuse.
struct Laid {
    given: Vec<f64>,
    lnodes: Vec<LNode>,
    ledges: Vec<LEdge>,
    size: LayeredResult,
}

/// What a level is laid out from, as the build of Pele's compoundLayout makes it: its items
/// and the ports that take a rank of their own as nodes, its edges and theirs, and the node of
/// each port, or -1 for a port that is pinned to its inner item's side instead.
struct Built {
    lnodes: Vec<LNode>,
    ledges: Vec<LEdge>,
    dummies: Vec<i32>,
}

/// The state that compoundLayout's closures share, which are its methods here.
struct Compound<'a> {
    nodes: &'a mut [CNode],
    edges: &'a mut [CEdge],
    opt: &'a LayeredOptions,
    order: Vec<Level>,
    /// Each level's place in `order`, by its id plus one.
    level_of: Vec<usize>,
    local: Vec<usize>,
    key: Vec<f64>,
    keyed: bool,
    far: Option<Far>,
    ordered: bool,
    laid: Vec<Vec<Laid>>,
    total: CompoundResult,
}

impl Compound<'_> {
    /// The level of a group, or of the root for -1.
    fn level(&self, id: i32) -> usize {
        self.level_of[(id + 1) as usize]
    }

    /// The port of an edge's end that leaves (`out`) or arrives at group, in the group's own
    /// level.
    fn port(&self, group: usize, e: usize, out: bool) -> &PortEdge {
        let level = &self.order[self.level(group as i32)];
        &level.ports[level.port_of[&port_key(e, out)]]
    }

    fn port_mut(&mut self, group: usize, e: usize, out: bool) -> &mut PortEdge {
        let q = self.level(group as i32);
        let level = &mut self.order[q];
        let i = level.port_of[&port_key(e, out)];
        &mut level.ports[i]
    }

    /// As the build of Pele's compoundLayout.
    fn build(&mut self, q: usize, sized: bool) -> Built {
        let level = &self.order[q];
        let nodes = &*self.nodes;
        let edges = &*self.edges;
        let vertical = vertical(level.dir);
        let start = start_side(level.dir);
        let mut lnodes = Vec::new();
        let mut ledges = Vec::new();
        for (i, &item) in level.items.iter().enumerate() {
            self.local[item] = i;
            let node = &nodes[item];
            let mut ln = if sized {
                if vertical {
                    lnode(node.w, node.h, Kind::Node)
                } else {
                    lnode(node.h, node.w, Kind::Node)
                }
            } else {
                lnode(0.0, 0.0, Kind::Node)
            };
            ln.span = if node.is_group {
                min(node.span, ln.w - 2.0 * node.pad_x)
            } else {
                node.span
            };
            ln.key = if self.keyed { self.key[item] } else { f64::NAN };
            lnodes.push(ln);
        }
        let local = &self.local;
        let held = |item: usize, ei: usize, out: bool| {
            nodes[item].is_group && endpoint_below(item, &edges[ei], out)
        };
        let offset = |item: usize, ei: usize, out: bool| {
            if !sized || !nodes[item].is_group || !endpoint_below(item, &edges[ei], out) {
                return 0.0;
            }
            let port = self.port(item, ei, out);
            if vertical {
                port.px - nodes[item].w / 2.0
            } else {
                port.py - nodes[item].h / 2.0
            }
        };
        for le in &level.edges {
            let e = &edges[le.e];
            let mut edge = ledge(local[le.a], local[le.b], e.minlen, 0.0, 0.0);
            if sized {
                edge.label_w = if vertical { e.label_w } else { e.label_h };
                edge.label_h = if vertical { e.label_h } else { e.label_w };
                edge.tail_dx = offset(le.a, le.e, true);
                edge.head_dx = offset(le.b, le.e, false);
                edge.tail_fixed = held(le.a, le.e, true);
                edge.head_fixed = held(le.b, le.e, false);
            }
            ledges.push(edge);
        }
        let mut dummies = vec![-1; level.ports.len()];
        for (i, port) in level.ports.iter().enumerate() {
            if port.side == start || port.side == opposite(start) {
                let d = lnodes.len();
                let kind = if port.side == start {
                    Kind::StartPort
                } else {
                    Kind::EndPort
                };
                let mut ln = lnode(0.0, 0.0, kind);
                let far_end = if port.out {
                    edges[port.e].dst
                } else {
                    edges[port.e].src
                };
                ln.key = if self.keyed {
                    self.key[far_end]
                } else {
                    f64::NAN
                };
                ln.hint = match &self.far {
                    Some(far) if port.hint.is_nan() && self.keyed => {
                        far.hint(&self.key, edges, level, port)
                    }
                    _ => port.hint,
                };
                lnodes.push(ln);
                dummies[i] = d as i32;
                let mut edge = if port.out {
                    ledge(local[port.inner], d, 1.0, 0.0, 0.0)
                } else {
                    ledge(d, local[port.inner], 1.0, 0.0, 0.0)
                };
                if port.out {
                    edge.tail_dx = offset(port.inner, port.e, true);
                    edge.tail_fixed = held(port.inner, port.e, true);
                } else {
                    edge.head_dx = offset(port.inner, port.e, false);
                    edge.head_fixed = held(port.inner, port.e, false);
                }
                ledges.push(edge);
            } else {
                lnodes[local[port.inner]].pin = if port.side == Side::Top || port.side == Side::Left
                {
                    -1
                } else {
                    1
                };
            }
        }
        Built {
            lnodes,
            ledges,
            dummies,
        }
    }

    /// Lays out a level, or gives back how it was laid out before from the same nodes and
    /// edges, as the layOut of Pele's compoundLayout does.
    fn lay_out(
        &mut self,
        q: usize,
        mut lnodes: Vec<LNode>,
        mut ledges: Vec<LEdge>,
    ) -> (Vec<LNode>, Vec<LEdge>, LayeredResult) {
        if !self.ordered {
            let size = layered(&mut lnodes, &mut ledges, self.opt);
            return (lnodes, ledges, size);
        }
        let mut given = Vec::new();
        for ln in &lnodes {
            given.extend([
                ln.w,
                ln.h,
                f64::from(ln.kind as u8),
                f64::from(ln.pin),
                ln.hint,
                ln.key,
                ln.span,
            ]);
        }
        for le in &ledges {
            given.extend([
                le.tail as f64,
                le.head as f64,
                le.minlen,
                le.label_w,
                le.label_h,
                le.tail_dx,
                le.head_dx,
                f64::from(u8::from(le.tail_fixed)),
                f64::from(u8::from(le.head_fixed)),
            ]);
        }
        // Object.is: NaN is itself, and 0 is not -0.
        let is = |a: f64, b: f64| a.to_bits() == b.to_bits() || a.is_nan() && b.is_nan();
        for p in &self.laid[q] {
            if p.given.len() == given.len() && p.given.iter().zip(&given).all(|(&a, &b)| is(a, b)) {
                return (p.lnodes.clone(), p.ledges.clone(), p.size);
            }
        }
        let size = layered(&mut lnodes, &mut ledges, self.opt);
        self.laid[q].push(Laid {
            given,
            lnodes: lnodes.clone(),
            ledges: ledges.clone(),
            size,
        });
        (lnodes, ledges, size)
    }

    /// Lays out every level and routes the edges through the ports, as the place of Pele's
    /// compoundLayout does.
    fn place(&mut self) {
        // Rank parents first to determine which group sides edges cross.
        for q in 0..self.order.len() {
            let mut sides = Vec::new();
            for port in &self.order[q].ports {
                if self.nodes[port.inner].is_group
                    && endpoint_below(port.inner, &self.edges[port.e], port.out)
                {
                    sides.push((port.inner, port.e, port.out, port.side));
                }
            }
            for (inner, e, out, side) in sides {
                self.port_mut(inner, e, out).side = side;
            }
            let level = &self.order[q];
            if level.edges.is_empty() {
                continue;
            }
            let needed = level.edges.iter().any(|le| {
                let e = &self.edges[le.e];
                self.nodes[le.a].is_group && e.src != le.a
                    || self.nodes[le.b].is_group && e.dst != le.b
            });
            if !needed {
                continue;
            }
            let Built {
                mut lnodes,
                mut ledges,
                ..
            } = self.build(q, false);
            orient(&lnodes, &mut ledges);
            rank(&mut lnodes, &ledges, 1);
            let level = &self.order[q];
            let start = start_side(level.dir);
            let mut sides = Vec::new();
            for (i, le) in level.edges.iter().enumerate() {
                let e = &self.edges[le.e];
                let a_above = !ledges[i].reversed;
                if self.nodes[le.a].is_group && e.src != le.a {
                    let side = if a_above { opposite(start) } else { start };
                    sides.push((le.a, le.e, true, side));
                }
                if self.nodes[le.b].is_group && e.dst != le.b {
                    let side = if a_above { start } else { opposite(start) };
                    sides.push((le.b, le.e, false, side));
                }
            }
            for (group, e, out, side) in sides {
                self.port_mut(group, e, out).side = side;
            }
        }

        // Lay out children first so their sizes are known.
        for q in (0..self.order.len()).rev() {
            let (id, dir) = (self.order[q].id, self.order[q].dir);
            let group = usize::try_from(id).ok();
            let vertical = vertical(dir);
            let flip = dir == Dir::BT || dir == Dir::RL;
            let built = self.build(q, true);
            let (lnodes, ledges, size) = self.lay_out(q, built.lnodes, built.ledges);
            let dummies = built.dummies;
            let content_w = if vertical { size.width } else { size.height };
            let content_h = if vertical { size.height } else { size.width };
            let group_node = group.map(|g| self.nodes[g].clone());
            let pad_x = group_node.as_ref().map_or(0.0, |g| g.pad_x);
            let pad_top = group_node.as_ref().map_or(0.0, |g| g.pad_top);
            let box_w = max(
                content_w + 2.0 * pad_x,
                group_node.as_ref().map_or(0.0, |g| g.min_w),
            );
            let box_h = content_h + pad_top + group_node.as_ref().map_or(0.0, |g| g.pad_bottom);
            let off_x = (box_w - content_w) / 2.0;
            let off_y = pad_top;
            let tx = |lx: f64, ly: f64| {
                off_x
                    + if vertical {
                        lx
                    } else if flip {
                        size.height - ly
                    } else {
                        ly
                    }
            };
            let ty = |lx: f64, ly: f64| {
                off_y
                    + if vertical {
                        if flip { size.height - ly } else { ly }
                    } else {
                        lx
                    }
            };
            let axis = if vertical { 0.0 } else { 1.0 };
            let convert = |points: &[f64]| {
                let mut out = Vec::new();
                for p in points.chunks(2) {
                    out.extend([tx(p[0], p[1]), ty(p[0], p[1]), axis]);
                }
                out
            };

            let items = std::mem::take(&mut self.order[q].items);
            for (i, &item) in items.iter().enumerate() {
                self.nodes[item].x = tx(lnodes[i].x, lnodes[i].y);
                self.nodes[item].y = ty(lnodes[i].x, lnodes[i].y);
            }
            self.order[q].items = items;
            for i in 0..self.order[q].edges.len() {
                let le = self.order[q].edges[i];
                let edge = &ledges[i];
                self.order[q].routes.insert(le.e, convert(&edge.points));
                let e = &mut self.edges[le.e];
                if e.label_w > 0.0 {
                    e.label_x = tx(edge.label_x, edge.label_y);
                    e.label_y = ty(edge.label_x, edge.label_y);
                }
            }
            let mut k = self.order[q].edges.len();
            for i in 0..self.order[q].ports.len() {
                let port = &self.order[q].ports[i];
                let (inner, e, out, side) = (port.inner, port.e, port.out, port.side);
                let item = &self.nodes[inner];
                let (bx, by, route);
                if dummies[i] >= 0 {
                    let mut r = convert(&ledges[k].points);
                    k += 1;
                    let at = if out { r.len() - 3 } else { 0 };
                    let (mut x, mut y) = (r[at], r[at + 1]);
                    match side {
                        Side::Top => y = 0.0,
                        Side::Bottom => y = box_h,
                        Side::Left => x = 0.0,
                        Side::Right => x = box_w,
                    }
                    if out {
                        r.extend([x, y, axis]);
                    } else {
                        r.splice(0..0, [x, y, axis]);
                    }
                    (bx, by, route) = (x, y, r);
                } else {
                    let mut ix = item.x;
                    let mut iy = item.y;
                    if item.is_group && endpoint_below(inner, &self.edges[e], out) {
                        let inner_port = self.port(inner, e, out);
                        ix += inner_port.px - item.w / 2.0;
                        iy += inner_port.py - item.h / 2.0;
                    } else {
                        match side {
                            Side::Top => iy -= item.h / 2.0,
                            Side::Bottom => iy += item.h / 2.0,
                            Side::Left => ix -= item.w / 2.0,
                            Side::Right => ix += item.w / 2.0,
                        }
                    }
                    let (mut x, mut y) = (ix, iy);
                    match side {
                        Side::Top => y = 0.0,
                        Side::Bottom => y = box_h,
                        Side::Left => x = 0.0,
                        Side::Right => x = box_w,
                    }
                    let a = if side == Side::Top || side == Side::Bottom {
                        0.0
                    } else {
                        1.0
                    };
                    let r = if out {
                        vec![ix, iy, a, x, y, a]
                    } else {
                        vec![x, y, a, ix, iy, a]
                    };
                    (bx, by, route) = (x, y, r);
                }
                let port = &mut self.order[q].ports[i];
                port.px = bx;
                port.py = by;
                port.route = route;
            }

            match group {
                Some(g) => {
                    self.nodes[g].w = box_w;
                    self.nodes[g].h = box_h;
                }
                None => {
                    self.total = CompoundResult {
                        width: box_w,
                        height: box_h,
                    }
                }
            }
        }

        for q in 0..self.order.len() {
            let ox = self.order[q].x;
            let oy = self.order[q].y;
            for i in 0..self.order[q].items.len() {
                let item = self.order[q].items[i];
                let node = &mut self.nodes[item];
                node.x += ox;
                node.y += oy;
                if node.is_group {
                    let (x, y) = (node.x - node.w / 2.0, node.y - node.h / 2.0);
                    let child = self.level(item as i32);
                    self.order[child].x = x;
                    self.order[child].y = y;
                }
            }
        }

        let append = |route: &mut Vec<f64>, part: &[f64], ox: f64, oy: f64| {
            for p in part.chunks(3) {
                let x = p[0] + ox;
                let y = p[1] + oy;
                let len = route.len();
                if len >= 3
                    && (route[len - 3] - x).abs() < 0.01
                    && (route[len - 2] - y).abs() < 0.01
                {
                    continue;
                }
                route.extend([x, y, p[2]]);
            }
        };

        for q in 0..self.order.len() {
            let level = &self.order[q];
            for le in &level.edges {
                let e = &self.edges[le.e];
                let mut route = Vec::new();
                let mut outward = Vec::new();
                let mut item = e.src;
                while self.nodes[item].parent != level.id {
                    outward.push(self.nodes[item].parent);
                    item = self.nodes[item].parent as usize;
                }
                for &id in &outward {
                    let lv = &self.order[self.level(id)];
                    append(
                        &mut route,
                        &self.port(id as usize, le.e, true).route,
                        lv.x,
                        lv.y,
                    );
                }
                append(&mut route, &level.routes[&le.e], level.x, level.y);
                let mut inward = Vec::new();
                let mut item = e.dst;
                while self.nodes[item].parent != level.id {
                    inward.push(self.nodes[item].parent);
                    item = self.nodes[item].parent as usize;
                }
                for &id in inward.iter().rev() {
                    let lv = &self.order[self.level(id)];
                    append(
                        &mut route,
                        &self.port(id as usize, le.e, false).route,
                        lv.x,
                        lv.y,
                    );
                }
                let (lx, ly) = (level.x, level.y);
                let e = &mut self.edges[le.e];
                e.route = route;
                if e.label_w > 0.0 {
                    e.label_x += lx;
                    e.label_y += ly;
                }
            }
        }
    }
}

/// Makes a level and puts it at the end of the order, as Pele's makeLevel and its callers do.
fn make_level(
    order: &mut Vec<Level>,
    level_of: &mut [usize],
    id: i32,
    dir: Dir,
    items: Vec<usize>,
) {
    level_of[(id + 1) as usize] = order.len();
    order.push(Level {
        id,
        dir,
        items,
        edges: Vec::new(),
        ports: Vec::new(),
        port_of: HashMap::new(),
        routes: HashMap::new(),
        x: 0.0,
        y: 0.0,
    });
}

/// Lays out a graph whose nodes may be nested in groups, as Pele's compoundLayout does. Each
/// group is laid out on its own, in its own direction, and then placed in its parent as a
/// single box. An edge that crosses a group's border passes through a port on the side facing
/// its other end.
pub(crate) fn compound_layout(
    nodes: &mut [CNode],
    edges: &mut [CEdge],
    root_dir: Dir,
    opt: &LayeredOptions,
) -> CompoundResult {
    let n = nodes.len();
    let mut order: Vec<Level> = Vec::new();
    let mut level_of = vec![usize::MAX; n + 1];
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut root_items = Vec::new();
    for i in 0..n {
        let p = nodes[i].parent;
        if p >= 0 && (p as usize) < n && p as usize != i && nodes[p as usize].is_group {
            children[p as usize].push(i);
        } else {
            nodes[i].parent = -1;
            root_items.push(i);
        }
    }
    make_level(&mut order, &mut level_of, -1, root_dir, root_items);
    let mut depth = vec![0usize; n];
    let mut placed = vec![false; n];
    {
        let descend = |order: &mut Vec<Level>,
                       level_of: &mut [usize],
                       nodes: &[CNode],
                       children: &[Vec<usize>],
                       depth: &mut [usize],
                       placed: &mut [bool],
                       from: usize| {
            let mut q = from;
            while q < order.len() {
                let (id, dir) = (order[q].id, order[q].dir);
                for k in 0..order[q].items.len() {
                    let item = order[q].items[k];
                    placed[item] = true;
                    depth[item] = if id < 0 { 0 } else { depth[id as usize] + 1 };
                    if nodes[item].is_group {
                        let child_dir = nodes[item].dir.unwrap_or(dir);
                        make_level(
                            order,
                            level_of,
                            item as i32,
                            child_dir,
                            children[item].clone(),
                        );
                    }
                }
                q += 1;
            }
        };
        descend(
            &mut order,
            &mut level_of,
            nodes,
            &children,
            &mut depth,
            &mut placed,
            0,
        );
        // Groups that contain each other are unreachable from the root. Lift them out.
        for i in 0..n {
            if placed[i] {
                continue;
            }
            let former = nodes[i].parent as usize;
            children[former].retain(|&c| c != i);
            nodes[i].parent = -1;
            order[0].items.push(i);
            placed[i] = true;
            depth[i] = 0;
            if nodes[i].is_group {
                let from = order.len();
                let dir = nodes[i].dir.unwrap_or(root_dir);
                make_level(
                    &mut order,
                    &mut level_of,
                    i as i32,
                    dir,
                    children[i].clone(),
                );
                descend(
                    &mut order,
                    &mut level_of,
                    nodes,
                    &children,
                    &mut depth,
                    &mut placed,
                    from,
                );
            }
        }
    }

    for q in (1..order.len()).rev() {
        let level = &order[q];
        let mut seq = f64::INFINITY;
        for &item in &level.items {
            if nodes[item].seq < seq {
                seq = nodes[item].seq;
            }
        }
        if seq != f64::INFINITY {
            nodes[level.id as usize].seq = seq;
        }
    }
    for level in &mut order {
        level
            .items
            .sort_by(|&a, &b| by(or(nodes[a].seq - nodes[b].seq, a as f64 - b as f64)));
    }

    // Every group border an edge crosses costs a port. Past the budget, an edge is drawn
    // between the groups that hold its ends, at the level they share.
    let parent = |nodes: &[CNode], i: usize| nodes[i].parent as usize;
    let mut ports = 0;
    for e in edges.iter_mut() {
        let mut a = e.src;
        let mut b = e.dst;
        let mut crossed = 0;
        while depth[a] > depth[b] {
            a = parent(nodes, a);
            crossed += 1;
        }
        while depth[b] > depth[a] {
            b = parent(nodes, b);
            crossed += 1;
        }
        while a != b && nodes[a].parent != nodes[b].parent {
            a = parent(nodes, a);
            b = parent(nodes, b);
            crossed += 2;
        }
        if a == b {
            continue;
        }
        if ports + crossed > MAX_PORTS {
            e.src = a;
            e.dst = b;
        } else {
            ports += crossed;
        }
    }

    let mut loops = Vec::new();
    let mut stray = Vec::new();
    for ei in 0..edges.len() {
        let e = &mut edges[ei];
        e.route = Vec::new();
        if e.src == e.dst {
            loops.push(ei);
            continue;
        }
        let mut a = e.src;
        let mut b = e.dst;
        let mut a_path = Vec::new();
        let mut b_path = Vec::new();
        while depth[a] > depth[b] {
            a_path.push(a);
            a = parent(nodes, a);
        }
        while depth[b] > depth[a] {
            b_path.push(b);
            b = parent(nodes, b);
        }
        if a == b {
            stray.push(ei);
            continue;
        }
        while nodes[a].parent != nodes[b].parent {
            a_path.push(a);
            b_path.push(b);
            a = parent(nodes, a);
            b = parent(nodes, b);
        }
        let level = level_of[(nodes[a].parent + 1) as usize];
        order[level].edges.push(LevelEdge { e: ei, a, b });
        for (path, out) in [(a_path, true), (b_path, false)] {
            for inner in path {
                let level = &mut order[level_of[(nodes[inner].parent + 1) as usize]];
                level.port_of.insert(port_key(ei, out), level.ports.len());
                level.ports.push(PortEdge {
                    e: ei,
                    inner,
                    out,
                    side: Side::Top,
                    hint: f64::NAN,
                    px: 0.0,
                    py: 0.0,
                    route: Vec::new(),
                });
            }
        }
    }

    let ordered = ports > 1 && ports <= MAX_ORDERED_PORTS && edges.len() <= MAX_ORDERED_EDGES;

    // Use a flat layout to coordinate node and port order across groups.
    let mut key = vec![f64::NAN; n];
    let mut flat_crossings = 0.0;
    let mut far = None;
    let flat = order.len() > 1 && ordered && order.iter().all(|level| level.dir == root_dir);
    if flat {
        let vertical = vertical(root_dir);
        let mut leaf = vec![-1i32; n];
        let mut group = vec![-1i32; n];
        let mut lnodes = Vec::new();
        let mut of = Vec::new();
        let mut parent = Vec::new();
        for i in 0..n {
            if !nodes[i].is_group {
                continue;
            }
            group[i] = parent.len() as i32;
            parent.push(-1);
        }
        for i in 0..n {
            let p = nodes[i].parent;
            let of_p = if p >= 0 { group[p as usize] } else { -1 };
            if nodes[i].is_group {
                parent[group[i] as usize] = of_p;
            } else {
                leaf[i] = lnodes.len() as i32;
                let (w, h) = if vertical {
                    (nodes[i].w, nodes[i].h)
                } else {
                    (nodes[i].h, nodes[i].w)
                };
                lnodes.push(lnode(w, h, Kind::Node));
                of.push(of_p);
            }
        }
        let mut ledges = Vec::new();
        let mut ledge_of = vec![-1i32; edges.len()];
        for (ei, e) in edges.iter().enumerate() {
            if e.src == e.dst || leaf[e.src] < 0 || leaf[e.dst] < 0 {
                continue;
            }
            ledge_of[ei] = ledges.len() as i32;
            let (label_w, label_h) = if vertical {
                (e.label_w, e.label_h)
            } else {
                (e.label_h, e.label_w)
            };
            ledges.push(ledge(
                leaf[e.src] as usize,
                leaf[e.dst] as usize,
                e.minlen,
                label_w,
                label_h,
            ));
        }
        let flat_opt = LayeredOptions {
            groups: Some(Groups { of, parent }),
            order_only: true,
            ..opt.clone()
        };
        let flat_layout = layered(&mut lnodes, &mut ledges, &flat_opt);
        flat_crossings = min(
            flat_layout.crossings.unwrap_or(0) as f64,
            flat_layout.declared.unwrap_or(0) as f64,
        );
        let mut sum = vec![0.0; n];
        let mut count = vec![0usize; n];
        // Last rank occupied by each group.
        let mut last = vec![-1i32; n];
        for i in 0..n {
            let Ok(l) = usize::try_from(leaf[i]) else {
                continue;
            };
            key[i] = lnodes[l].x;
            let mut p = nodes[i].parent;
            while p >= 0 {
                let g = p as usize;
                sum[g] += key[i];
                count[g] += 1;
                last[g] = last[g].max(lnodes[l].rank as i32);
                p = nodes[g].parent;
            }
        }
        for i in 0..n {
            if count[i] > 0 {
                key[i] = sum[i] / count[i] as f64;
            }
        }
        far = Some(Far {
            ledge_of,
            ledges,
            last,
        });
    }

    let levels = order.len();
    let mut c = Compound {
        nodes,
        edges,
        opt,
        order,
        level_of,
        local: vec![0; n],
        key,
        keyed: false,
        far,
        ordered,
        laid: (0..levels).map(|_| Vec::new()).collect(),
        total: CompoundResult::default(),
    };
    // The flat layout ignores group boxes, so independent ordering may yield fewer crossings.
    let style = style_of(opt);
    c.keyed = flat;
    c.place();
    let mut crossings = None;
    if flat {
        let mut found = route_crossings(c.nodes, c.edges, style);
        if found as f64 > flat_crossings * FLAT_SLACK + FLAT_SPARE {
            let first = snapshot(c.nodes, c.edges, &c.order, c.total);
            c.keyed = false;
            c.place();
            let own = route_crossings(c.nodes, c.edges, style);
            if own >= found {
                c.keyed = true;
                c.total = restore(first, c.nodes, c.edges, &mut c.order);
            } else {
                found = own;
            }
        }
        crossings = Some(found);
    }

    // Refine port order using routes outside each group. Skip this if independent ordering
    // won, since it rarely helps enough to justify another layout.
    if c.keyed == flat && ordered {
        let mut again = false;
        for level in &mut c.order {
            let Ok(id) = usize::try_from(level.id) else {
                continue;
            };
            let bx0 = &c.nodes[id];
            let vertical = vertical(level.dir);
            let end = opposite(start_side(level.dir));
            let mut seen: Vec<(f64, f64)> = Vec::new();
            for port in &mut level.ports {
                if port.side != end {
                    continue;
                }
                // Skip the straight run out of the port to find where the edge turns.
                let route = &c.edges[port.e].route;
                let bx = level.x + port.px;
                let by = level.y + port.py;
                let mut at = None;
                let mut k = 0;
                while k < route.len() && (at.is_none() || port.out) {
                    if (route[k] - bx).abs() < 0.01 && (route[k + 1] - by).abs() < 0.01 {
                        at = Some(k);
                    }
                    k += 3;
                }
                let Some(at) = at else {
                    continue;
                };
                let a = if vertical { 0 } else { 1 };
                let step: isize = if port.out { 3 } else { -3 };
                let inside = |k: isize| k >= 0 && (k as usize) < route.len();
                let mut next = at as isize + step;
                while inside(next) && (route[next as usize + a] - route[at + a]).abs() < IN_LINE {
                    next += step;
                }
                if !inside(next) {
                    continue;
                }
                port.hint = route[next as usize + a]
                    - if vertical {
                        bx0.x - bx0.w / 2.0
                    } else {
                        bx0.y - bx0.h / 2.0
                    };
                seen.push((if vertical { port.px } else { port.py }, port.hint));
            }
            seen.sort_by(|p, q| by(p.0 - q.0));
            for i in 1..seen.len() {
                if seen[i - 1].1 > seen[i].1 {
                    again = true;
                }
            }
        }
        if again {
            let before = crossings.unwrap_or_else(|| route_crossings(c.nodes, c.edges, style));
            if before > 0 {
                let first = snapshot(c.nodes, c.edges, &c.order, c.total);
                c.place();
                if route_crossings(c.nodes, c.edges, style) >= before {
                    c.total = restore(first, c.nodes, c.edges, &mut c.order);
                }
            }
        }
    }

    let Compound {
        nodes,
        edges,
        total,
        ..
    } = c;
    for ei in stray {
        let (a, b) = (&nodes[edges[ei].src], &nodes[edges[ei].dst]);
        let route = vec![a.x, a.y, 0.0, b.x, b.y, 0.0];
        let (label_x, label_y) = ((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        let e = &mut edges[ei];
        e.route = route;
        e.label_x = label_x;
        e.label_y = label_y;
    }
    for ei in loops {
        let node = &nodes[edges[ei].src];
        let e = &mut edges[ei];
        e.label_x = node.x + node.w / 2.0 + 28.0 + e.label_w / 2.0;
        e.label_y = node.y;
    }

    total
}
