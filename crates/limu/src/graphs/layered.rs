//! Layered (Sugiyama-style) layout of one flat graph, as Pele's src/layout/layered.ts lays it
//! out. Ranks run down the y axis, and callers rotate the result for other directions.
//!
//! What the layout of edges aims for, most important first. Where two aims pull apart, the one
//! higher up decides, and each names where it is carried out.
//!
//! 1. Lines can be told apart. No two touch or run along each other, and none runs through a
//!    node or a label. Ends on one side of a node have places of their own (`spread`), and
//!    curves that would pass within a few pixels turn at levels of their own (`assign_tracks`).
//! 2. Lines cross as little as the order of the nodes allows (`reduce_crossings`, and the order
//!    of the levels in `assign_tracks`).
//! 3. An edge takes the shortest and simplest way: straight if it can, else one curve, else a
//!    curve with a level run. One that passes several ranks runs in line with one of its ends
//!    and turns once, at the other (`align_labels`, `straighten`). Ends slide along their
//!    sides, and nodes move a little, where that makes an edge straight (`settle`, `snap`).
//! 4. An edge meets the middle of a side. An end with a side to itself stays there, and ends
//!    that share a side sit around the middle in the order of where they lead. An end moves off
//!    the middle only for the aims above (`settle`).
//! 5. The drawing is balanced, and what is alike is drawn alike. A label sits midway along the
//!    line that shows, the curves either side of it have much the same room to turn in, and a
//!    gap whose curves mostly cross has them all as the same plain curve rather than each on a
//!    level of its own (`head_room` in `layered`, the crossed gaps in `assign_tracks`).
//! 6. An edge leaves and reaches a node square to it, and an arrowhead points straight at the
//!    node (Pele's routePath in svg/edges.ts, which draws the routes).
//! 7. Nodes keep the order they were written in, where nothing above says otherwise.
//! 8. The drawing stays compact. A gap between ranks grows only to hold the levels in it, or
//!    to let curves that cross do so at a clear angle.

use std::collections::HashSet;

use crate::js::math::{by, hypot, max, min, or};

/// What a node of the layout is, as Pele's Kind says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Kind {
    Node,
    Dummy,
    Label,
    StartPort,
    EndPort,
}

/// A node of the layout, as Pele's LNode is.
#[derive(Clone, Debug)]
pub(crate) struct LNode {
    pub(crate) w: f64,
    pub(crate) h: f64,
    pub(crate) kind: Kind,
    pub(crate) pin: i32,
    /// Port's cross-flow destination, NaN if unknown. Fully hinted port ranks keep this order.
    pub(crate) hint: f64,
    /// Initial cross-flow position, NaN if unknown. Used only when every node in a rank has one.
    pub(crate) key: f64,
    /// How much of the side that edges meet may be shared out between them. 0 keeps them all
    /// at its middle.
    pub(crate) span: f64,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) rank: usize,
}

/// An edge of the layout, as Pele's LEdge is.
#[derive(Clone, Debug)]
pub(crate) struct LEdge {
    pub(crate) tail: usize,
    pub(crate) head: usize,
    pub(crate) minlen: f64,
    pub(crate) label_w: f64,
    pub(crate) label_h: f64,
    pub(crate) tail_dx: f64,
    pub(crate) head_dx: f64,
    /// An end that is held where its offset puts it, and takes no part in sharing out a node's
    /// side.
    pub(crate) tail_fixed: bool,
    pub(crate) head_fixed: bool,
    pub(crate) reversed: bool,
    pub(crate) points: Vec<f64>,
    pub(crate) label_x: f64,
    pub(crate) label_y: f64,
}

/// The groups of a layout that keeps each group contiguous within ranks. `of` maps nodes to
/// groups, and `parent` maps groups to parents. Both use -1 for none.
#[derive(Clone, Debug)]
pub(crate) struct Groups {
    pub(crate) of: Vec<i32>,
    pub(crate) parent: Vec<i32>,
}

/// The options of a layered layout, as Pele's LayeredOptions are.
#[derive(Clone, Debug, Default)]
pub(crate) struct LayeredOptions {
    pub(crate) node_sep: f64,
    pub(crate) edge_sep: f64,
    pub(crate) rank_sep: f64,
    pub(crate) port_sep: f64,
    /// Route edges at right angles, each on a track of its own across the gap between two
    /// ranks.
    pub(crate) tracks: bool,
    /// Draw every edge straight from point to point: none is given a level to turn onto.
    pub(crate) straight: bool,
    /// The caller sets the ends of the edges on one side of a node apart itself, so two edges
    /// between the same two nodes need no bend to keep them from being drawn as one.
    pub(crate) apart: bool,
    /// The room that the marker at the head of an edge takes. A rank of labels whose edges all
    /// head the same way sits half of this nearer their tails, so that each label is midway
    /// along the line that shows, and the curves either side of it have the same room to turn
    /// in.
    pub(crate) head_room: Option<f64>,
    /// Keep each group contiguous within ranks.
    pub(crate) groups: Option<Groups>,
    /// Compute cross-flow positions in `x` without full routing.
    pub(crate) order_only: bool,
}

/// The size of a layered layout, as Pele's LayeredResult is. With `order_only`, it also has the
/// final crossings and the crossings before grouping or reordering.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LayeredResult {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) crossings: Option<usize>,
    pub(crate) declared: Option<usize>,
}

impl LayeredResult {
    fn sized(width: f64, height: f64) -> LayeredResult {
        LayeredResult {
            width,
            height,
            crossings: None,
            declared: None,
        }
    }
}

/// A node, as Pele's lnode makes one.
pub(crate) fn lnode(w: f64, h: f64, kind: Kind) -> LNode {
    LNode {
        w,
        h,
        kind,
        pin: 0,
        hint: f64::NAN,
        key: f64::NAN,
        span: 0.0,
        x: 0.0,
        y: 0.0,
        rank: 0,
    }
}

/// An edge, as Pele's ledge makes one.
pub(crate) fn ledge(tail: usize, head: usize, minlen: f64, label_w: f64, label_h: f64) -> LEdge {
    LEdge {
        tail,
        head,
        minlen,
        label_w,
        label_h,
        tail_dx: 0.0,
        head_dx: 0.0,
        tail_fixed: false,
        head_fixed: false,
        reversed: false,
        points: Vec::new(),
        label_x: 0.0,
        label_y: 0.0,
    }
}

const MAX_SWEEPS: usize = 24;

// An edge gets a bend point in every rank it skips. Past this many, the longest edges are
// drawn as one direct curve instead, which keeps huge graphs from exhausting time and memory.
const MAX_DUMMIES: usize = 20000;

// The room between two edges that meet one side of a node, and the least that keeps their
// arrowheads apart.
const PORT_PITCH: f64 = 16.0;
const MIN_PITCH: f64 = 8.0;

// Where edges are not given levels, a gap between ranks grows to keep the edges that cross it
// at least this steep, up to this many times its usual height. Shallower than that, the edges
// of a wide rank are hard to tell apart.
const MIN_SLOPE: f64 = 0.125;
const MAX_GAP: f64 = 3.0;

// Tracks: the room kept clear between a rank and the nearest track, the room between two
// tracks when the gap has to grow to hold them, and the most they spread to when it has room
// to spare.
const TRACK_MARGIN: f64 = 18.0;
const TRACK_PITCH: f64 = 10.0;
const TRACK_SPREAD: f64 = 14.0;
// A curve is one plain S unless it runs more than this many times as far across as the gap is
// high, and so does another that it would run along. Then each turns onto a level of its own
// to run across on. The levels of a gap keep this far from its ranks, to leave room to turn
// in, and from each other.
const LEVEL_WIDE: f64 = 2.0;
const LEVEL_MARGIN: f64 = 28.0;
const LEVEL_PITCH: f64 = 12.0;
// Two plain curves that would pass too near each other turn at levels of their own as well:
// nearer than LEVEL_CLEAR where one of them runs far across, since it stays near the other for
// a long way, and otherwise nearer than LEVEL_TOUCH. LEVEL_LEAD is the room a plain curve is
// taken to keep straight at each end of its gap.
const LEVEL_LEAD: f64 = 12.0;
const LEVEL_CLEAR: f64 = 8.0;
const LEVEL_TOUCH: f64 = 4.0;
// Levels tell apart curves that run alongside each other. Where the curves of a gap cross each
// other more often than that, runs along levels only make the crossings long and shallow, so
// each curve is one plain S instead, and the gap is made this many times as tall as its widest
// curve runs across (and no taller than MAX_GAP allows), for them to cross at a clear angle.
const CROSSED_SLOPE: f64 = 0.5;
// How far out of line two points may be and still count as in line.
const IN_LINE: f64 = 0.5;
// A node, with whatever it is already in line with, moves this far at most to bring one more
// of its edges into line.
const SNAP: f64 = 12.0;
// An end that cannot be brought into line leans towards where its edge goes, but keeps this
// far from the ends of the part of its side that edges spread along. On a narrow node that
// leaves it in the middle.
const LEAN_CLEAR: f64 = 40.0;
// Past this many edges across one gap, tracks are given out in a plain order: weighing every
// pair of edges against each other would take too long.
const MAX_WEIGHED: usize = 400;
// How many pieces a curve is sampled in. Pele keeps the buffers it samples into from one
// layout to the next, and here each layout has its own.
const SAMPLES: usize = 16;

/// What the end of a segment is to the node it meets, as Pele's End says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum End {
    Held,
    Leaves,
    Arrives,
}

/// As Pele's isDummy.
fn is_dummy(kind: Kind) -> bool {
    kind == Kind::Dummy || kind == Kind::Label
}

/// Sorts list[from..to] by key, then by item, as Pele's sortRange does, by insertion.
fn sort_range<K: Copy + PartialOrd>(list: &mut [usize], from: usize, to: usize, key: &[K]) {
    for i in from + 1..to {
        let item = list[i];
        let k = key[item];
        let mut j = i;
        while j > from && (key[list[j - 1]] > k || key[list[j - 1]] == k && list[j - 1] > item) {
            list[j] = list[j - 1];
            j -= 1;
        }
        list[j] = item;
    }
}

/// Sorts the first count of list by bary, then by pos, as Pele's sortByBary does: by insertion
/// in runs of eight, which are then merged with spare to hold each pass.
fn sort_by_bary(
    list: &mut [usize],
    count: usize,
    bary: &[f64],
    pos: &[usize],
    right: bool,
    spare: &mut [usize],
) {
    let before = |a: usize, b: usize| {
        bary[a] < bary[b]
            || bary[a] == bary[b]
                && (if right {
                    pos[a] > pos[b]
                } else {
                    pos[a] < pos[b]
                })
    };
    const RUN: usize = 8;
    for s in (0..count).step_by(RUN) {
        for i in s + 1..count.min(s + RUN) {
            let item = list[i];
            let mut j = i;
            while j > s && before(item, list[j - 1]) {
                list[j] = list[j - 1];
                j -= 1;
            }
            list[j] = item;
        }
    }
    let mut in_list = true;
    let mut width = RUN;
    while width < count {
        let (from, to): (&[usize], &mut [usize]) = if in_list {
            (&*list, &mut *spare)
        } else {
            (&*spare, &mut *list)
        };
        merge_pass(from, to, count, width, &before);
        in_list = !in_list;
        width *= 2;
    }
    if !in_list {
        list[..count].copy_from_slice(&spare[..count]);
    }
}

/// One pass of sortByBary's merge: each two neighbouring runs of width from `from`, merged
/// into `to`.
fn merge_pass(
    from: &[usize],
    to: &mut [usize],
    count: usize,
    width: usize,
    before: &impl Fn(usize, usize) -> bool,
) {
    for s in (0..count).step_by(2 * width) {
        let mid = count.min(s + width);
        let end = count.min(s + 2 * width);
        let (mut i, mut j, mut k) = (s, mid, s);
        while i < mid && j < end {
            if before(from[j], from[i]) {
                to[k] = from[j];
                j += 1;
            } else {
                to[k] = from[i];
                i += 1;
            }
            k += 1;
        }
        while i < mid {
            to[k] = from[i];
            i += 1;
            k += 1;
        }
        while j < end {
            to[k] = from[j];
            j += 1;
            k += 1;
        }
    }
}

/// Lays out a flat graph in ranks, as Pele's layered does.
pub(crate) fn layered(
    nodes: &mut [LNode],
    edges: &mut [LEdge],
    opt: &LayeredOptions,
) -> LayeredResult {
    let real = nodes.len();
    if real == 0 {
        return LayeredResult::sized(0.0, 0.0);
    }
    let m = edges.len();
    if real == 1 && m == 0 && nodes[0].kind == Kind::Node {
        let node = &mut nodes[0];
        node.rank = 0;
        node.x = node.w / 2.0;
        node.y = node.h / 2.0;
        return LayeredResult::sized(node.w, node.h);
    }

    // Every edge gets a middle rank of its own when labels or parallel edges need the room.
    let mut step = 1;
    if m > 0 {
        let mut seen = HashSet::new();
        for e in edges.iter() {
            let a = e.tail.min(e.head);
            let b = e.tail.max(e.head);
            let key = a * real + b;
            if e.label_w > 0.0 || !seen.insert(key) {
                step = 2;
                break;
            }
        }
    }

    orient(nodes, edges);
    rank(nodes, edges, step);

    let mut span = vec![0usize; m];
    let mut dummies = 0;
    for ei in 0..m {
        let e = &edges[ei];
        span[ei] = nodes[e.head].rank.abs_diff(nodes[e.tail].rank);
        if span[ei] > 1 {
            dummies += span[ei] - 1;
        }
    }
    let mut direct = vec![false; m];
    if dummies > MAX_DUMMIES {
        let mut longest: Vec<usize> = (0..m).filter(|&ei| span[ei] > 1).collect();
        longest.sort_by(|&a, &b| span[b].cmp(&span[a]).then(a.cmp(&b)));
        for ei in longest {
            if dummies <= MAX_DUMMIES {
                break;
            }
            direct[ei] = true;
            dummies -= span[ei] - 1;
        }
    }

    let n = real + dummies;
    let mut w = vec![0.0; n];
    let mut h = vec![0.0; n];
    let mut kind = vec![Kind::Node; n];
    let mut rank = vec![0usize; n];
    let mut max_rank = 0;
    for i in 0..real {
        let node = &nodes[i];
        w[i] = node.w;
        h[i] = node.h;
        kind[i] = node.kind;
        rank[i] = node.rank;
        if node.rank > max_rank {
            max_rank = node.rank;
        }
    }

    // The graph that gets ordered and positioned: every edge cut into rank-to-rank segments.
    let mut segments = 0;
    for ei in 0..m {
        if !direct[ei] {
            segments += span[ei];
        }
    }
    let mut ef = vec![0usize; segments];
    let mut et = vec![0usize; segments];
    let mut efd = vec![0.0; segments];
    let mut etd = vec![0.0; segments];
    let mut first_dummy = vec![-1i32; m];
    // Which way the edges with a label in each rank head: 1 down, 2 up, 3 both.
    let mut heads = vec![0u8; max_rank + 1];
    let mut label_node = vec![-1i32; m];
    let mut spans = false;
    for node in nodes.iter() {
        if node.span > 0.0 && node.kind == Kind::Node {
            spans = true;
        }
    }
    // What each segment's end is to the node it meets, for the ends that may move along the
    // node's side.
    let mut efk = spans.then(|| vec![End::Held; segments]);
    let mut etk = spans.then(|| vec![End::Held; segments]);
    let mut first_seg = vec![-1i32; m];
    let mut last_seg = vec![-1i32; m];
    let mut key = vec![f64::NAN; n];
    for i in 0..real {
        key[i] = nodes[i].key;
    }
    let mut offsets = false;
    {
        let mut next = real;
        let mut ce = 0;
        for ei in 0..m {
            if direct[ei] {
                continue;
            }
            let e = &edges[ei];
            let (a, b) = if e.reversed {
                (e.head, e.tail)
            } else {
                (e.tail, e.head)
            };
            let (a_dx, b_dx) = if e.reversed {
                (e.head_dx, e.tail_dx)
            } else {
                (e.tail_dx, e.head_dx)
            };
            if a_dx != 0.0 || b_dx != 0.0 {
                offsets = true;
            }
            let ra = rank[a];
            let rb = rank[b];
            let mut label_rank: isize = -1;
            if e.label_w > 0.0 {
                // The label goes halfway along. An edge that passes ranks of nodes on its way has
                // its label in one of those, beside the nodes, where it takes no height of its
                // own: the one halfway, or of the two either side of halfway, the one nearer the
                // node the edge starts from, which is the lower when the edge runs against the
                // flow. An edge between two neighbouring ranks has its label in the rank between
                // them.
                let toward = if e.reversed { 1 } else { -1 };
                label_rank = (ra + ((rb - ra) >> 1)) as isize;
                if step == 2 && rb - ra >= 4 {
                    if label_rank & 1 == 1 {
                        label_rank += toward;
                    }
                } else if step == 2 && label_rank & 1 == 0 {
                    label_rank += toward;
                }
            }
            let mut prev = a;
            if rb - ra > 1 {
                first_dummy[ei] = next as i32;
            }
            first_seg[ei] = ce as i32;
            if let Some(efk) = &mut efk
                && !(if e.reversed {
                    e.head_fixed
                } else {
                    e.tail_fixed
                })
            {
                efk[ce] = if e.reversed {
                    End::Arrives
                } else {
                    End::Leaves
                };
            }
            for r in ra + 1..rb {
                let d = next;
                next += 1;
                rank[d] = r;
                key[d] = key[a] + (key[b] - key[a]) * (r - ra) as f64 / (rb - ra) as f64;
                if r as isize == label_rank {
                    w[d] = e.label_w;
                    h[d] = e.label_h;
                    kind[d] = Kind::Label;
                    label_node[ei] = d as i32;
                    heads[r] |= if e.reversed { 2 } else { 1 };
                } else {
                    kind[d] = Kind::Dummy;
                }
                ef[ce] = prev;
                et[ce] = d;
                efd[ce] = if prev == a { a_dx } else { 0.0 };
                ce += 1;
                prev = d;
            }
            ef[ce] = prev;
            et[ce] = b;
            efd[ce] = if prev == a { a_dx } else { 0.0 };
            etd[ce] = b_dx;
            if let Some(etk) = &mut etk
                && !(if e.reversed {
                    e.tail_fixed
                } else {
                    e.head_fixed
                })
            {
                etk[ce] = if e.reversed {
                    End::Leaves
                } else {
                    End::Arrives
                };
            }
            last_seg[ei] = ce as i32;
            ce += 1;
        }
    }

    // Bends belong to the innermost group containing both edge endpoints.
    let mut grp = None;
    if let Some(Groups { of, parent }) = &opt.groups {
        let mut gr_of = vec![0i32; n];
        gr_of[..real].copy_from_slice(&of[..real]);
        let depth = |mut gr: i32| {
            let mut d = 0;
            while gr >= 0 {
                d += 1;
                gr = parent[gr as usize];
            }
            d
        };
        let lca = |mut x: i32, mut y: i32| {
            let mut dx = depth(x);
            let mut dy = depth(y);
            while dx > dy {
                x = parent[x as usize];
                dx -= 1;
            }
            while dy > dx {
                y = parent[y as usize];
                dy -= 1;
            }
            while x != y {
                x = parent[x as usize];
                y = parent[y as usize];
            }
            x
        };
        for ei in 0..m {
            if first_dummy[ei] < 0 {
                continue;
            }
            let e = &edges[ei];
            let gr = lca(of[e.tail], of[e.head]);
            let d0 = first_dummy[ei] as usize;
            for d in d0..d0 + span[ei] - 1 {
                gr_of[d] = gr;
            }
        }
        grp = Some(gr_of);
    }

    let mut succ_start = vec![0usize; n + 1];
    let mut pred_start = vec![0usize; n + 1];
    for e in 0..segments {
        succ_start[ef[e] + 1] += 1;
        pred_start[et[e] + 1] += 1;
    }
    for i in 0..n {
        succ_start[i + 1] += succ_start[i];
        pred_start[i + 1] += pred_start[i];
    }
    let mut succ_e = vec![0usize; segments];
    let mut pred_e = vec![0usize; segments];
    {
        let mut sf = succ_start[..n].to_vec();
        let mut pf = pred_start[..n].to_vec();
        for e in 0..segments {
            succ_e[sf[ef[e]]] = e;
            sf[ef[e]] += 1;
            pred_e[pf[et[e]]] = e;
            pf[et[e]] += 1;
        }
    }

    let mut layer_start = vec![0usize; max_rank + 2];
    for &r in &rank {
        layer_start[r + 1] += 1;
    }
    for r in 0..=max_rank {
        layer_start[r + 1] += layer_start[r];
    }
    let mut order = vec![0usize; n];
    let pos = vec![0usize; n];

    // Initial order: depth-first from the top so that subtrees start out contiguous.
    {
        let mut by_rank = vec![0usize; n];
        let mut fill = layer_start[..=max_rank].to_vec();
        for i in 0..n {
            by_rank[fill[rank[i]]] = i;
            fill[rank[i]] += 1;
        }
        fill.copy_from_slice(&layer_start[..=max_rank]);
        let mut visited = vec![false; n];
        let mut stack = Vec::new();
        for k in 0..n {
            if visited[by_rank[k]] {
                continue;
            }
            stack.push(by_rank[k]);
            while let Some(v) = stack.pop() {
                if visited[v] {
                    continue;
                }
                visited[v] = true;
                order[fill[rank[v]]] = v;
                fill[rank[v]] += 1;
                for j in (succ_start[v]..succ_start[v + 1]).rev() {
                    let w = et[succ_e[j]];
                    if !visited[w] {
                        stack.push(w);
                    }
                }
            }
        }
    }

    // Pele sorts by the key, then by the place in the rank, which is the stable sort by the
    // key.
    let sort_rank = |order: &mut [usize], r: usize, key: &dyn Fn(usize) -> f64| -> bool {
        let s = layer_start[r];
        let e = layer_start[r + 1];
        if e - s < 2 {
            return false;
        }
        if order[s..e].iter().any(|&v| key(v).is_nan()) {
            return false;
        }
        order[s..e].sort_by(|&a, &b| by(key(a) - key(b)));
        true
    };
    for r in 0..=max_rank {
        sort_rank(&mut order, r, &|v| key[v]);
    }

    let pinned = nodes.iter().any(|node| node.pin != 0);
    if pinned {
        let pin_of = |v: usize| if v < real { nodes[v].pin } else { 0 };
        for r in 0..=max_rank {
            order[layer_start[r]..layer_start[r + 1]].sort_by_key(|&v| pin_of(v));
        }
    }
    // Preserve port order to avoid crossings outside the group.
    let mut fixed = vec![false; max_rank + 1];
    let hint = |v: usize| {
        if v < real && (nodes[v].kind == Kind::StartPort || nodes[v].kind == Kind::EndPort) {
            nodes[v].hint
        } else {
            f64::NAN
        }
    };
    for r in [0, max_rank] {
        if sort_rank(&mut order, r, &hint) {
            fixed[r] = true;
        }
    }

    let mut g = Graph {
        n,
        real,
        max_rank,
        w,
        kind,
        rank,
        ef,
        et,
        efd,
        etd,
        succ_start,
        succ_e,
        pred_start,
        pred_e,
        layer_start,
        order,
        pos,
        offsets,
        fixed,
        grp,
        group_parent: opt.groups.as_ref().map(|groups| groups.parent.clone()),
    };
    g.set_positions();

    let mut crossings = 0;
    let mut declared = 0;
    if segments > 0 && max_rank > 0 {
        (crossings, declared) = reduce_crossings(&mut g, nodes);
        g.set_positions();
    }
    if opt.order_only {
        // Points are (rank, cross-flow position) pairs.
        let at = position(&mut g, opt);
        for i in 0..real {
            nodes[i].x = at[i];
        }
        for ei in 0..m {
            let e = &mut edges[ei];
            let (a, b) = if e.reversed {
                (e.head, e.tail)
            } else {
                (e.tail, e.head)
            };
            let mut pts = vec![g.rank[a] as f64, at[a]];
            if !direct[ei] && first_dummy[ei] >= 0 {
                let d0 = first_dummy[ei] as usize;
                for d in d0..d0 + span[ei] - 1 {
                    pts.extend([g.rank[d] as f64, at[d]]);
                }
            }
            pts.extend([g.rank[b] as f64, at[b]]);
            e.points = pts;
        }
        return LayeredResult {
            width: 0.0,
            height: 0.0,
            crossings: Some(crossings),
            declared: Some(declared),
        };
    }

    if let (Some(efk), Some(etk)) = (&efk, &etk) {
        spread(&mut g, nodes, efk, etk);
    }

    let mut xs = position(&mut g, opt);

    let empty = empty_ranks(&g);
    if let (Some(efk), Some(etk)) = (&efk, &etk) {
        let free = free_ends(&g, efk, etk);
        align_labels(&g, nodes, &mut xs, &free, opt, empty.as_deref());
        settle(&mut g, nodes, &xs, efk, etk, &free, empty.as_deref());
        snap(&g, &mut xs, empty.as_deref(), opt);
        for ei in 0..m {
            if direct[ei] {
                continue;
            }
            let e = &mut edges[ei];
            let a_dx = g.efd[first_seg[ei] as usize];
            let b_dx = g.etd[last_seg[ei] as usize];
            e.tail_dx = if e.reversed { b_dx } else { a_dx };
            e.head_dx = if e.reversed { a_dx } else { b_dx };
        }
    }

    // A rank that holds nothing but the bends of edges is only there because some other edge
    // has a label or a twin. Its bends move onto the straight line between the ends of their
    // edges, and one that gets there is left out of the route, so that the edge is drawn as a
    // single curve.
    let mut on_line = vec![false; n];
    if let Some(empty) = &empty
        && !opt.tracks
    {
        let sep = if opt.apart { 0.0 } else { opt.edge_sep };
        straighten(&g, &mut xs, empty, &first_dummy, &span, sep, &mut on_line);
    }
    if !opt.tracks {
        past_labels(&g, &mut xs, &first_dummy, &span, opt.edge_sep, &mut on_line);
    }
    let Graph {
        ref kind,
        ref rank,
        ref ef,
        ref et,
        ref efd,
        ref etd,
        ref succ_start,
        ref pred_start,
        ref layer_start,
        ref order,
        ..
    } = g;
    let is_empty = |r: usize| empty.as_ref().is_some_and(|empty| empty[r]);
    let bends = |ei: usize| match usize::try_from(first_dummy[ei]) {
        Ok(d0) => d0..d0 + span[ei] - 1,
        Err(_) => 0..0,
    };
    // An edge is cut where it passes a rank that holds something, and each piece crosses one
    // gap. With `tracks`, every piece that does not run straight gets a track to run across on
    // at right angles. Otherwise, where edges spread along their nodes, a piece gets one only if
    // it needs a level to turn onto. `piece_start` gives every edge's run of pieces.
    let mut piece_start = vec![0usize; m + 1];
    let mut pieces = (opt.tracks || efk.is_some() && !opt.straight).then(Pieces::default);
    let mut track_count = vec![0usize; max_rank + 1];
    if let Some(pieces) = &mut pieces {
        for ei in 0..m {
            piece_start[ei] = pieces.from.len();
            if direct[ei] {
                continue;
            }
            // An edge that keeps a bend to stay clear of its twin is drawn through the bend, as
            // it is.
            let mut twin = false;
            for d in bends(ei) {
                // So is one that has a bend left out in a rank of labels: its curve crosses
                // that rank.
                let bare = is_empty(rank[d]);
                if !opt.tracks && bare != on_line[d] {
                    twin = true;
                }
            }
            if twin {
                continue;
            }
            let first = first_seg[ei] as usize;
            let mut x = xs[ef[first]] + efd[first];
            let mut at = rank[ef[first]];
            for d in bends(ei) {
                if is_empty(rank[d]) {
                    continue;
                }
                pieces.from.push(x);
                pieces.to.push(xs[d]);
                pieces.rank.push(at);
                pieces.lower.push(d as i32);
                x = xs[d];
                at = rank[d];
            }
            let last = last_seg[ei] as usize;
            pieces.from.push(x);
            pieces.to.push(xs[et[last]] + etd[last]);
            pieces.rank.push(at);
            pieces.lower.push(-1);
        }
        piece_start[m] = pieces.from.len();
    }

    // Ranks that hold only border ports or their pass-through dummies need little room of
    // their own.
    let port_only = |r: usize, of: Kind| {
        let mut found = false;
        for &v in &order[layer_start[r]..layer_start[r + 1]] {
            let k = kind[v];
            if k == of {
                found = true;
            } else if k != Kind::Dummy {
                return false;
            }
        }
        found
    };
    let start_ranks = if port_only(0, Kind::StartPort) {
        step
    } else {
        0
    };
    let end_ranks = if port_only(max_rank, Kind::EndPort) {
        step
    } else {
        0
    };
    // The longest way across that any edge travels in the gap below each rank.
    let mut run = vec![0.0; max_rank + 1];
    for e in 0..segments {
        let across = (xs[ef[e]] + efd[e] - xs[et[e]] - etd[e]).abs();
        if across > run[rank[ef[e]]] {
            run[rank[ef[e]]] = across;
        }
    }
    let mut band_top = vec![0.0; max_rank + 1];
    let mut band_bottom = vec![0.0; max_rank + 1];
    let step_f = step as f64;
    let usual = |r: usize| {
        (if r < start_ranks || r + end_ranks >= max_rank {
            opt.port_sep
        } else {
            opt.rank_sep
        }) / step_f
    };
    // The room under each rank. Edges that are not cut into pieces need more where they run far
    // across.
    let mut gap_under = vec![0.0; max_rank + 1];
    for r in 0..max_rank {
        gap_under[r] = if pieces.is_some() {
            usual(r)
        } else {
            max(
                usual(r),
                min(MAX_GAP * opt.rank_sep / step_f, run[r] * MIN_SLOPE),
            )
        };
    }
    // A rank between two ranks of nodes, which holds labels and bends only.
    let mut between = vec![true; max_rank + 1];
    for &r in &rank[..real] {
        between[r] = false;
    }
    for r in 1..max_rank {
        if opt.tracks || !between[r] || heads[r] != 1 && heads[r] != 2 {
            continue;
        }
        let down = heads[r] == 1;
        let shift = min(
            opt.head_room.unwrap_or(0.0) / 2.0,
            gap_under[if down { r - 1 } else { r }] / 2.0,
        );
        gap_under[r - 1] += if down { -shift } else { shift };
        gap_under[r] += if down { shift } else { -shift };
    }
    // The rank that the pieces leaving each rank arrive at, past any that hold only bends, and
    // the room between the two, which grows to hold the gap's tracks or levels: these keep
    // `margin` from the ranks, and between `least_pitch` and `most_pitch` from each other.
    let margin = if opt.tracks {
        TRACK_MARGIN
    } else {
        LEVEL_MARGIN
    };
    let least_pitch = if opt.tracks { TRACK_PITCH } else { LEVEL_PITCH };
    let most_pitch = if opt.tracks {
        TRACK_SPREAD
    } else {
        LEVEL_PITCH
    };
    let mut below = vec![0usize; max_rank + 1];
    if let Some(pieces) = &mut pieces {
        let mut room = vec![0.0; max_rank + 1];
        for r in 0..max_rank {
            if is_empty(r) {
                continue;
            }
            let mut next = r + 1;
            room[r] = gap_under[r];
            while next < max_rank && is_empty(next) {
                room[r] += gap_under[next];
                next += 1;
            }
            below[r] = next;
        }
        for k in 0..pieces.from.len() {
            let wide = (pieces.from[k] - pieces.to[k]).abs() > LEVEL_WIDE * room[pieces.rank[k]];
            pieces.wide.push(wide);
        }
        let mut tall = vec![0.0; max_rank + 1];
        assign_tracks(pieces, &mut track_count, !opt.tracks, &room, &mut tall);
        for r in 0..max_rank {
            if track_count[r] > 0 {
                gap_under[r] += max(
                    0.0,
                    2.0 * margin + (track_count[r] - 1) as f64 * least_pitch - room[r],
                );
            } else if tall[r] > 0.0 {
                gap_under[r] += max(0.0, min(MAX_GAP * opt.rank_sep, tall[r]) - room[r]);
            }
        }
    }
    let mut y = 0.0;
    for r in 0..=max_rank {
        let mut tallest = 0.0;
        for &v in &order[layer_start[r]..layer_start[r + 1]] {
            if h[v] > tallest {
                tallest = h[v];
            }
        }
        band_top[r] = y;
        band_bottom[r] = y + tallest;
        y += tallest + gap_under[r];
    }
    let height = y;
    // The part of its rank that an edge runs straight through at a bend or label. A bare bend
    // beside labels keeps to the middle half, which leaves the curves either side of it more
    // room and still keeps them clear of the labels' corners.
    let inset = |d: usize| {
        if kind[d] == Kind::Dummy && between[rank[d]] && !opt.tracks {
            (band_bottom[rank[d]] - band_top[rank[d]]) / 4.0
        } else {
            0.0
        }
    };
    // Where a piece runs across its gap, or would on another track of its group: its tracks
    // sit in the middle of the gap, the first at the top.
    let track_y = |pieces: &Pieces, piece: usize, on_track: f64| {
        let r = pieces.rank[piece];
        let top = band_bottom[r];
        let bottom = band_top[below[r]];
        let count = if opt.tracks {
            track_count[r]
        } else {
            pieces.levels[piece]
        } as f64;
        let pitch = if count > 1.0 {
            min(most_pitch, (bottom - top - 2.0 * margin) / (count - 1.0))
        } else {
            0.0
        };
        (top + bottom) / 2.0 + (on_track - (count - 1.0) / 2.0) * pitch
    };

    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    for i in 0..n {
        let half = g.w[i] / 2.0;
        if xs[i] - half < min_x {
            min_x = xs[i] - half;
        }
        if xs[i] + half > max_x {
            max_x = xs[i] + half;
        }
    }
    // Where edges spread along their nodes, a rank whose nodes only send edges on stands on one
    // line, and a rank whose nodes only receive them hangs from one, so that the edges of nodes
    // of different heights set out from, or arrive at, the same level. In any other rank the
    // nodes are centered.
    let mut arrives = vec![false; max_rank + 1];
    let mut leaves = vec![false; max_rank + 1];
    for i in 0..real {
        if kind[i] != Kind::Node || efk.is_none() {
            continue;
        }
        if pred_start[i + 1] > pred_start[i] {
            arrives[rank[i]] = true;
        }
        if succ_start[i + 1] > succ_start[i] {
            leaves[rank[i]] = true;
        }
    }
    for i in 0..real {
        let node = &mut nodes[i];
        let r = node.rank;
        node.x = xs[i] - min_x;
        node.y = if kind[i] == Kind::Node && leaves[r] && !arrives[r] {
            band_bottom[r] - node.h / 2.0
        } else if kind[i] == Kind::Node && arrives[r] && !leaves[r] {
            band_top[r] + node.h / 2.0
        } else {
            (band_top[r] + band_bottom[r]) / 2.0
        };
    }

    for ei in 0..m {
        let e = &edges[ei];
        let first = &nodes[if e.reversed { e.head } else { e.tail }];
        let last = &nodes[if e.reversed { e.tail } else { e.head }];
        let mut pts = vec![
            first.x + if e.reversed { e.head_dx } else { e.tail_dx },
            first.y + first.h / 2.0,
        ];
        match &pieces {
            Some(pieces) if piece_start[ei + 1] > piece_start[ei] => {
                for k in piece_start[ei]..piece_start[ei + 1] {
                    let track = pieces.track[k];
                    if track >= 0 && (opt.tracks || pieces.wide[k]) {
                        let across = track_y(pieces, k, track as f64);
                        pts.extend([pieces.from[k] - min_x, across, pieces.to[k] - min_x, across]);
                    } else if track > 0 {
                        // A short piece among levels runs straight until it is past the level
                        // before its own, and turns in one S from there, so that it crosses
                        // none of them.
                        pts.extend([
                            pieces.from[k] - min_x,
                            track_y(pieces, k, (track - 1) as f64),
                        ]);
                    }
                    let Ok(d) = usize::try_from(pieces.lower[k]) else {
                        continue;
                    };
                    pts.extend([xs[d] - min_x, band_top[rank[d]] + inset(d)]);
                    if band_bottom[rank[d]] > band_top[rank[d]] {
                        pts.extend([xs[d] - min_x, band_bottom[rank[d]] - inset(d)]);
                    }
                }
            }
            _ => {
                for d in bends(ei) {
                    if on_line[d] {
                        continue;
                    }
                    let x = xs[d] - min_x;
                    let top = band_top[rank[d]];
                    let bottom = band_bottom[rank[d]];
                    pts.extend([x, top + inset(d)]);
                    if bottom > top {
                        pts.extend([x, bottom - inset(d)]);
                    }
                }
            }
        }
        pts.extend([
            last.x + if e.reversed { e.tail_dx } else { e.head_dx },
            last.y - last.h / 2.0,
        ]);
        let e = &mut edges[ei];
        if e.reversed {
            let points = pts.len() / 2;
            for i in 0..points / 2 {
                let j = points - 1 - i;
                pts.swap(2 * i, 2 * j);
                pts.swap(2 * i + 1, 2 * j + 1);
            }
        }
        if let Ok(ln) = usize::try_from(label_node[ei]) {
            e.label_x = xs[ln] - min_x;
            e.label_y = (band_top[rank[ln]] + band_bottom[rank[ln]]) / 2.0;
        } else if e.label_w > 0.0 {
            e.label_x = (pts[0] + pts[pts.len() - 2]) / 2.0;
            e.label_y = (pts[1] + pts[pts.len() - 1]) / 2.0;
        }
        e.points = pts;
    }

    LayeredResult::sized(max_x - min_x, height)
}

/// The pieces of edges that each cross one gap, as Pele's Pieces are: where they leave the rank
/// above and reach the one below, the rank above, the bend they end at (or -1 at the edge's
/// last node), and the track they run across on (or -1 when they have none). Where only some
/// pieces are given a level to turn onto, `wide` marks those that run far across, `levels` is
/// how many tracks the group a piece belongs to takes, or 0 for a piece that needs none, and
/// each piece's track counts from its group's first. A piece that is not `wide` curves from the
/// level above its own.
#[derive(Default)]
struct Pieces {
    from: Vec<f64>,
    to: Vec<f64>,
    rank: Vec<usize>,
    lower: Vec<i32>,
    track: Vec<i32>,
    levels: Vec<usize>,
    wide: Vec<bool>,
}

/// The graph that gets ordered and positioned, as Pele's Graph is.
struct Graph {
    n: usize,
    real: usize,
    max_rank: usize,
    w: Vec<f64>,
    kind: Vec<Kind>,
    rank: Vec<usize>,
    // Segment endpoints, and where each segment attaches on them.
    ef: Vec<usize>,
    et: Vec<usize>,
    efd: Vec<f64>,
    etd: Vec<f64>,
    succ_start: Vec<usize>,
    succ_e: Vec<usize>,
    pred_start: Vec<usize>,
    pred_e: Vec<usize>,
    layer_start: Vec<usize>,
    order: Vec<usize>,
    pos: Vec<usize>,
    offsets: bool,
    // Ranks whose order must remain fixed.
    fixed: Vec<bool>,
    grp: Option<Vec<i32>>,
    group_parent: Option<Vec<i32>>,
}

impl Graph {
    /// Each node's place in its rank, from the order, as Pele's setPositions does.
    fn set_positions(&mut self) {
        for r in 0..=self.max_rank {
            let s = self.layer_start[r];
            for i in s..self.layer_start[r + 1] {
                self.pos[self.order[i]] = i - s;
            }
        }
    }
}

/// Reverses the edges that close a cycle, found by depth-first search in declaration order, as
/// Pele's orient does.
pub(crate) fn orient(nodes: &[LNode], edges: &mut [LEdge]) {
    let n = nodes.len();
    let m = edges.len();
    let mut start = vec![0usize; n + 1];
    for e in edges.iter_mut() {
        e.reversed = nodes[e.head].kind == Kind::StartPort || nodes[e.tail].kind == Kind::EndPort;
        if !e.reversed {
            start[e.tail + 1] += 1;
        }
    }
    for i in 0..n {
        start[i + 1] += start[i];
    }
    let mut out = vec![0usize; start[n]];
    {
        let mut fill = start[..n].to_vec();
        for i in 0..m {
            if !edges[i].reversed {
                out[fill[edges[i].tail]] = i;
                fill[edges[i].tail] += 1;
            }
        }
    }
    let mut state = vec![0u8; n];
    let mut cursor = start[..n].to_vec();
    let mut stack = Vec::new();
    for root in 0..n {
        if state[root] != 0 {
            continue;
        }
        stack.push(root);
        state[root] = 1;
        while let Some(&v) = stack.last() {
            if cursor[v] < start[v + 1] {
                let e = &mut edges[out[cursor[v]]];
                cursor[v] += 1;
                let w = e.head;
                if state[w] == 1 {
                    e.reversed = true;
                } else if state[w] == 0 {
                    state[w] = 1;
                    stack.push(w);
                }
            } else {
                state[v] = 2;
                stack.pop();
            }
        }
    }
}

/// Gives each node its rank, as Pele's rank does: by the longest path, and then towards the
/// side that has more edges.
pub(crate) fn rank(nodes: &mut [LNode], edges: &[LEdge], step: usize) {
    let n = nodes.len();
    let m = edges.len();
    let mut from = vec![0usize; m];
    let mut to = vec![0usize; m];
    let mut len = vec![0i32; m];
    let mut out_start = vec![0usize; n + 1];
    let mut in_start = vec![0usize; n + 1];
    for (i, e) in edges.iter().enumerate() {
        let (a, b) = if e.reversed {
            (e.head, e.tail)
        } else {
            (e.tail, e.head)
        };
        from[i] = a;
        to[i] = b;
        // Kept in an Int32Array, as Pele keeps it.
        len[i] = (max(1.0, e.minlen) * step as f64) as i32;
        out_start[a + 1] += 1;
        in_start[b + 1] += 1;
    }
    for i in 0..n {
        out_start[i + 1] += out_start[i];
        in_start[i + 1] += in_start[i];
    }
    let mut out_e = vec![0usize; m];
    let mut in_e = vec![0usize; m];
    {
        let mut of = out_start[..n].to_vec();
        let mut inf = in_start[..n].to_vec();
        for i in 0..m {
            out_e[of[from[i]]] = i;
            of[from[i]] += 1;
            in_e[inf[to[i]]] = i;
            inf[to[i]] += 1;
        }
    }

    let mut r = vec![0i32; n];
    let mut indeg: Vec<usize> = (0..n).map(|i| in_start[i + 1] - in_start[i]).collect();
    let mut topo = vec![0usize; n];
    let mut count = 0;
    for i in 0..n {
        if indeg[i] == 0 {
            topo[count] = i;
            count += 1;
        }
    }
    let mut q = 0;
    while q < count {
        let v = topo[q];
        for &ei in &out_e[out_start[v]..out_start[v + 1]] {
            let w = to[ei];
            if r[v] + len[ei] > r[w] {
                r[w] = r[v] + len[ei];
            }
            indeg[w] -= 1;
            if indeg[w] == 0 {
                topo[count] = w;
                count += 1;
            }
        }
        q += 1;
    }

    // Longest-path ranking leaves loosely attached nodes far from their neighbours. Move each
    // node toward the side that has more edges.
    for _ in 0..8 {
        let mut changed = false;
        for &v in topo[..count].iter().rev() {
            let ins = in_start[v + 1] - in_start[v];
            let outs = out_start[v + 1] - out_start[v];
            if ins == 0 && outs == 0 {
                continue;
            }
            let lo = in_e[in_start[v]..in_start[v + 1]]
                .iter()
                .map(|&ei| r[from[ei]] + len[ei])
                .max();
            let hi = out_e[out_start[v]..out_start[v + 1]]
                .iter()
                .map(|&ei| r[to[ei]] - len[ei])
                .min();
            let mut next = r[v];
            if let (true, Some(hi)) = (outs > ins, hi) {
                next = hi;
            } else if let (true, Some(lo)) = (ins > outs, lo) {
                next = lo;
            }
            if next != r[v] {
                r[v] = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut lowest = i32::MAX;
    let mut highest = i32::MIN;
    let mut start_ports = false;
    for i in 0..n {
        let kind = nodes[i].kind;
        if kind == Kind::StartPort {
            start_ports = true;
        }
        if kind == Kind::StartPort || kind == Kind::EndPort {
            continue;
        }
        lowest = lowest.min(r[i]);
        highest = highest.max(r[i]);
    }
    if lowest == i32::MAX {
        lowest = 0;
        highest = 0;
    }
    let step = step as i32;
    for i in 0..n {
        if nodes[i].kind == Kind::StartPort {
            r[i] = lowest - step;
        } else if nodes[i].kind == Kind::EndPort {
            r[i] = highest + step;
        }
    }
    if start_ports {
        lowest -= step;
    }
    for i in 0..n {
        nodes[i].rank = (r[i] - lowest) as usize;
    }
}

/// The ends of edges that may move along one side of a node (0 the side its edges arrive at, 1
/// the other), ordered by their opposite nodes to avoid crossings, as Pele's sideEnds finds
/// them. `ids` and `room` must start empty. `ids` receives the ends, and `room` their minimum
/// gaps. Returns the sum of the gaps. When there is too little room for a full pitch between
/// each pair, the edges that leave side by side move closer together, since they have no
/// arrowheads to keep apart. With less room still, neighbours that run the same way share a
/// place, which keeps an arrowhead apart from an edge that leaves beside it.
fn side_ends(
    g: &Graph,
    v: usize,
    side: usize,
    span: f64,
    kinds: &[End],
    ids: &mut Vec<usize>,
    room: &mut Vec<f64>,
) -> f64 {
    let (start, list, other) = if side == 0 {
        (&g.pred_start, &g.pred_e, &g.ef)
    } else {
        (&g.succ_start, &g.succ_e, &g.et)
    };
    let pos = &g.pos;
    for &e in &list[start[v]..start[v + 1]] {
        if kinds[e] != End::Held {
            ids.push(e);
        }
    }
    if ids.len() < 2 {
        return 0.0;
    }
    ids.sort_by(|&a, &b| pos[other[a]].cmp(&pos[other[b]]).then(a.cmp(&b)));
    let ids = &*ids;
    let fill = |room: &mut Vec<f64>, share: &dyn Fn(bool, bool) -> f64| {
        let mut total = 0.0;
        room.clear();
        room.push(0.0);
        for k in 1..ids.len() {
            let same = kinds[ids[k]] == kinds[ids[k - 1]];
            let gap = share(same && kinds[ids[k]] == End::Leaves, same);
            room.push(gap);
            total += gap;
        }
        total
    };
    let mut total = fill(room, &|_, _| 1.0);
    if span / total < MIN_PITCH {
        total = fill(room, &|leaves, _| if leaves { 0.5 } else { 1.0 });
    }
    if span / total < MIN_PITCH {
        total = fill(room, &|_, same| if same { 0.0 } else { 1.0 });
    }
    if total == 0.0 {
        return 0.0;
    }
    let pitch = min(PORT_PITCH, span / total);
    for gap in &mut room[1..] {
        *gap *= pitch;
    }
    total * pitch
}

/// Gives the edges that meet one side of a node places of their own along it, around its
/// middle, as Pele's spread does.
fn spread(g: &mut Graph, nodes: &[LNode], efk: &[End], etk: &[End]) {
    for v in 0..g.real {
        let span = min(nodes[v].span, g.w[v]);
        if span <= 0.0 || nodes[v].kind != Kind::Node {
            continue;
        }
        for side in 0..2 {
            let mut ids = Vec::new();
            let mut room = Vec::new();
            let kinds = if side == 0 { etk } else { efk };
            let total = side_ends(g, v, side, span, kinds, &mut ids, &mut room);
            if total == 0.0 {
                continue;
            }
            let dx = if side == 0 { &mut g.etd } else { &mut g.efd };
            let mut at = -total / 2.0;
            for k in 0..ids.len() {
                at += room[k];
                dx[ids[k]] = at;
            }
        }
    }
}

/// The segment of an edge that reaches the next point held in place, on the way up from
/// segment `e` (side 0) or down: past the bends in ranks that hold nothing else, as Pele's
/// follow finds it.
fn follow(g: &Graph, empty: Option<&[bool]>, mut e: usize, side: usize) -> usize {
    let Some(empty) = empty else {
        return e;
    };
    for _ in 0..100000 {
        let u = if side == 0 { g.ef[e] } else { g.et[e] };
        if g.kind[u] != Kind::Dummy || !empty[g.rank[u]] {
            break;
        }
        e = if side == 0 {
            g.pred_e[g.pred_start[u]]
        } else {
            g.succ_e[g.succ_start[u]]
        };
    }
    e
}

/// How many ends that may move along it each node has on the side its edges arrive at, and on
/// the other, as Pele's freeEnds counts them. An end that has a side to itself keeps to the
/// middle. Ends that share one cannot all be there, so those may move.
fn free_ends(g: &Graph, efk: &[End], etk: &[End]) -> [Vec<usize>; 2] {
    let mut free = [vec![0; g.n], vec![0; g.n]];
    for e in 0..g.ef.len() {
        if etk[e] != End::Held {
            free[0][g.et[e]] += 1;
        }
        if efk[e] != End::Held {
            free[1][g.ef[e]] += 1;
        }
    }
    free
}

/// How far along the flow the handles of a plain S reach, as a share of the way it travels
/// along it, as Pele's ease says. Half gives an even S. One that runs more than eight times as
/// far across as along turns sooner and crosses in a straighter line, down to a quarter at
/// sixteen times. The layout uses this to tell how near two curves will pass, and the drawing
/// to draw them.
pub(crate) fn ease(across: f64, along: f64) -> f64 {
    max(0.25, min(0.5, 4.0 * along.abs() / or(across.abs(), 1.0)))
}

/// Brings the labels and bends of each edge into line with an end of the edge, so that it runs
/// straight from that end and turns once, at the other, as Pele's alignLabels does: with the
/// end that has fewer edges beside it on its node, which is the one nearer the middle of its
/// side, or else with the other, and as near to that as what is beside them in their ranks
/// leaves room. The other end may then move along its node to meet the line. Bends in ranks
/// that hold nothing else are left to `straighten`. An edge with no one line that all of its
/// labels and bends have room on has each brought into line with the nearest point above or
/// below that is held in place.
fn align_labels(
    g: &Graph,
    nodes: &[LNode],
    xs: &mut [f64],
    free: &[Vec<usize>; 2],
    opt: &LayeredOptions,
    empty: Option<&[bool]>,
) {
    let sep = opt.edge_sep;
    let Graph {
        n,
        real,
        max_rank,
        ref kind,
        ref rank,
        ref w,
        ref ef,
        ref et,
        ref efd,
        ref etd,
        ref succ_start,
        ref succ_e,
        ref pred_start,
        ref pred_e,
        ref layer_start,
        ref order,
        ref pos,
        ..
    } = *g;
    let placed = |v: usize| {
        kind[v] == Kind::Label || kind[v] == Kind::Dummy && !empty.is_some_and(|e| e[rank[v]])
    };
    // Where segment `e` ends on the way up (side 0) or down, and how many ends share that side
    // of the node there: none for a point that is not on a node.
    let end = |xs: &[f64], e: usize, side: usize| {
        let u = if side == 0 { ef[e] } else { et[e] };
        let x = if side == 0 {
            xs[u] + efd[e]
        } else {
            xs[u] + etd[e]
        };
        let sharing = if u < real && nodes[u].kind == Kind::Node {
            free[1 - side][u]
        } else {
            0
        };
        (x, sharing)
    };
    // The room the neighbours in its rank leave a label or bend: a full gap from a node or
    // between two labels, half of one between a label and a line that passes it.
    let room = |xs: &[f64], v: usize| {
        let s = layer_start[rank[v]];
        let i = s + pos[v];
        let clear = |other: usize| {
            w[other] / 2.0
                + w[v] / 2.0
                + if kind[other] == Kind::Node {
                    opt.node_sep / 2.0 + sep / 2.0
                } else if kind[other] == kind[v] {
                    sep
                } else {
                    sep / 2.0
                }
        };
        let least = if i > s {
            xs[order[i - 1]] + clear(order[i - 1])
        } else {
            f64::NEG_INFINITY
        };
        let most = if i + 1 < layer_start[rank[v] + 1] {
            xs[order[i + 1]] - clear(order[i + 1])
        } else {
            f64::INFINITY
        };
        (least, most)
    };
    // The place between `lo` and `hi` that is in line with one of two ends, or else nearest
    // the first choice.
    let choose = |x: f64, up: (f64, usize), down: (f64, usize), lo: f64, hi: f64| {
        let up_first = if up.1 != down.1 {
            up.1 < down.1
        } else {
            (up.0 - x).abs() <= (down.0 - x).abs()
        };
        let choices = if up_first {
            [up.0, down.0]
        } else {
            [down.0, up.0]
        };
        choices
            .into_iter()
            .find(|&choice| choice >= lo && choice <= hi)
            .unwrap_or_else(|| max(lo, min(hi, choices[0])))
    };
    let mut done = vec![false; n];
    let mut chain = Vec::new();
    for r in 1..max_rank {
        for i in layer_start[r]..layer_start[r + 1] {
            if !placed(order[i]) || done[order[i]] {
                continue;
            }
            // The edge's labels and bends from its first node to its last, and the room they
            // share.
            let mut top = order[i];
            while is_dummy(kind[ef[pred_e[pred_start[top]]]]) {
                top = ef[pred_e[pred_start[top]]];
            }
            let mut bottom = top;
            let mut lo = f64::NEG_INFINITY;
            let mut hi = f64::INFINITY;
            chain.clear();
            let mut d = top;
            while is_dummy(kind[d]) {
                bottom = d;
                if placed(d) {
                    done[d] = true;
                    chain.push(d);
                    let (least, most) = room(xs, d);
                    if least > lo {
                        lo = least;
                    }
                    if most < hi {
                        hi = most;
                    }
                }
                d = et[succ_e[succ_start[d]]];
            }
            if lo <= hi {
                let up = end(xs, pred_e[pred_start[top]], 0);
                let down = end(xs, succ_e[succ_start[bottom]], 1);
                let x = choose(xs[chain[0]], up, down, lo, hi);
                for &d in &chain {
                    xs[d] = x;
                }
                continue;
            }
            for &d in &chain {
                let (least, most) = room(xs, d);
                if least > most {
                    continue;
                }
                let up = end(xs, follow(g, empty, pred_e[pred_start[d]], 0), 0);
                let down = end(xs, follow(g, empty, succ_e[succ_start[d]], 1), 1);
                xs[d] = choose(xs[d], up, down, least, most);
            }
        }
    }
}

/// The ends of one side of a node, which `settle` goes over: as `side_ends` orders them, with
/// their gaps, and all of the side's ends, held ones among them, in the same order.
struct SideOf {
    v: usize,
    side: usize,
    span: f64,
    ids: Vec<usize>,
    room: Vec<f64>,
    all: Vec<usize>,
}

/// Once the nodes have their places, moves an end along its side when that lets its edge run
/// straight, as Pele's settle does: when the point the edge runs to, or some part of the side
/// it runs to, is in line with the end's own side. Any other end leans towards the point its
/// edge runs to, where its side is long enough, and an end with a side to itself stays in the
/// middle. On a group, the ends keep clear of the points where other edges pass through its
/// border.
fn settle(
    g: &mut Graph,
    nodes: &[LNode],
    xs: &[f64],
    efk: &[End],
    etk: &[End],
    free: &[Vec<usize>; 2],
    empty: Option<&[bool]>,
) {
    let real = g.real;
    // Where the end of segment `e` on node `v` should be, from the middle of its side. The edge
    // is followed up from the segment (side 0) or down, past the bends in ranks that hold
    // nothing else, to where it next comes to a point that is held in place or to another
    // node's side.
    let wanted = |g: &Graph, e: usize, side: usize, v: usize, span: f64, again: bool| {
        if free[side][v] < 2 {
            return 0.0;
        }
        let e = follow(g, empty, e, side);
        let u = if side == 0 { g.ef[e] } else { g.et[e] };
        let point = if side == 0 {
            xs[u] + g.efd[e]
        } else {
            xs[u] + g.etd[e]
        };
        // Once the other end has its place, this one comes into line with it if it can.
        if again && (point - xs[v]).abs() <= span / 2.0 + IN_LINE {
            return max(-span / 2.0, min(span / 2.0, point - xs[v]));
        }
        // The part of the other side that its end may move along, if it may move.
        let mut middle = point;
        let mut width = 0.0;
        let kinds = if side == 0 { efk } else { etk };
        if u < real
            && nodes[u].kind == Kind::Node
            && kinds[e] != End::Held
            && free[1 - side][u] >= 2
        {
            width = max(0.0, min(nodes[u].span, g.w[u]));
            if width > 0.0 {
                middle = xs[u];
            }
        }
        let lo = max(xs[v] - span / 2.0, middle - width / 2.0);
        let hi = min(xs[v] + span / 2.0, middle + width / 2.0);
        // Two sides with no part in line: the end leans towards the other, if its side is long
        // enough.
        if lo > hi + IN_LINE {
            let lean = max(0.0, span / 2.0 - LEAN_CLEAR);
            return max(-lean, min(lean, point - xs[v]));
        }
        // Both ends settle on one place in the part the two sides share: nearer the middle of
        // the narrower.
        let place = (xs[v] * width + middle * span) / (width + span);
        max(lo, min(hi, place)) - xs[v]
    };
    // End order and spacing stay fixed across passes. Held ends divide free ends into runs.
    let mut sides = Vec::new();
    for v in 0..real {
        let span = min(nodes[v].span, g.w[v]);
        if span <= 0.0 || nodes[v].kind != Kind::Node {
            continue;
        }
        for side in 0..2 {
            let (other, start, list) = if side == 0 {
                (&g.ef, &g.pred_start, &g.pred_e)
            } else {
                (&g.et, &g.succ_start, &g.succ_e)
            };
            let mut ids = Vec::new();
            let mut room = Vec::new();
            let kinds = if side == 0 { etk } else { efk };
            side_ends(g, v, side, span, kinds, &mut ids, &mut room);
            if ids.is_empty() {
                continue;
            }
            if ids.len() == 1 {
                room = vec![0.0];
            }
            let mut all = list[start[v]..start[v + 1]].to_vec();
            all.sort_by(|&a, &b| g.pos[other[a]].cmp(&g.pos[other[b]]).then(a.cmp(&b)));
            sides.push(SideOf {
                v,
                side,
                span,
                ids,
                room,
                all,
            });
        }
    }
    // The ends of an edge follow each other, so the nodes are gone over until none moves: a
    // few times at most, in practice.
    let mut pass = 0;
    let mut moved = true;
    while moved && pass < 6 {
        moved = false;
        for s in &sides {
            let kinds = if s.side == 0 { etk } else { efk };
            let mut k = 0;
            let mut lo = -s.span / 2.0;
            for i in 0..=s.all.len() {
                let held = i < s.all.len() && kinds[s.all[i]] == End::Held;
                if i < s.all.len() && !held {
                    continue;
                }
                // Free ends since the last held end occupy ids[k..k + size].
                let size = s.all[..i]
                    .iter()
                    .rev()
                    .take_while(|&&e| kinds[e] != End::Held)
                    .count();
                let dx = |g: &Graph, e: usize| if s.side == 0 { g.etd[e] } else { g.efd[e] };
                let hi = if held {
                    dx(g, s.all[i]) - PORT_PITCH
                } else {
                    s.span / 2.0
                };
                if size > 0 {
                    // Isotonic fitting preserves order and spacing by pooling neighbours that
                    // are too close.
                    let mut sum: Vec<f64> = Vec::new();
                    let mut count: Vec<usize> = Vec::new();
                    let mut before = 0.0;
                    for j in 0..size {
                        if j > 0 {
                            before += s.room[k + j];
                        }
                        sum.push(wanted(g, s.ids[k + j], s.side, s.v, s.span, pass > 0) - before);
                        count.push(1);
                        // Keep ends together when spacing forced them to share a place.
                        let mut together = j > 0 && s.room[k + j] == 0.0;
                        while sum.len() > 1 && {
                            let l = sum.len();
                            together
                                || sum[l - 2] / count[l - 2] as f64
                                    > sum[l - 1] / count[l - 1] as f64
                        } {
                            together = false;
                            let top = sum.pop().unwrap_or_default();
                            let pooled = count.pop().unwrap_or_default();
                            *sum.last_mut().unwrap() += top;
                            *count.last_mut().unwrap() += pooled;
                        }
                    }
                    let total = before;
                    let mut j = 0;
                    before = 0.0;
                    for b in 0..sum.len() {
                        // Center the run if it cannot fit between held ends.
                        let level = if hi - total < lo {
                            (lo + hi - total) / 2.0
                        } else {
                            max(lo, min(hi - total, sum[b] / count[b] as f64))
                        };
                        for _ in 0..count[b] {
                            if j > 0 {
                                before += s.room[k + j];
                            }
                            let dx = if s.side == 0 { &mut g.etd } else { &mut g.efd };
                            let at = &mut dx[s.ids[k + j]];
                            if (*at - level - before).abs() > 0.01 {
                                moved = true;
                            }
                            *at = level + before;
                            j += 1;
                        }
                    }
                    k += size;
                }
                if held {
                    lo = dx(g, s.all[i]) + PORT_PITCH;
                }
            }
        }
        pass += 1;
    }
}

/// Joins the blocks of x and y into one, the smaller into the larger, as Pele's snap does.
fn join(block: &mut [usize], members: &mut [Vec<usize>], x: usize, y: usize) {
    let mut keep = block[x];
    let mut drop = block[y];
    if keep == drop {
        return;
    }
    if members[keep].len() < members[drop].len() {
        (keep, drop) = (drop, keep);
    }
    let moved = std::mem::take(&mut members[drop]);
    for &v in &moved {
        block[v] = keep;
    }
    members[keep].extend(moved);
}

/// Brings edges that are nearly straight into line, by moving a node a little across the flow
/// along with everything it is in line with already, where the nodes beside them leave room,
/// as Pele's snap does. What is in line stays in line, so each move leaves one more edge
/// straight.
fn snap(g: &Graph, xs: &mut [f64], empty: Option<&[bool]>, opt: &LayeredOptions) {
    let Graph {
        n,
        ref kind,
        ref rank,
        ref ef,
        ref et,
        ref efd,
        ref etd,
        ref layer_start,
        ref order,
        ref pos,
        ..
    } = *g;
    let passed = |u: usize| empty.is_some_and(|empty| kind[u] == Kind::Dummy && empty[rank[u]]);
    // Each edge from one point that is held in place to the next, past the bends in ranks that
    // hold nothing else.
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut da = Vec::new();
    let mut db = Vec::new();
    for e in 0..ef.len() {
        if passed(ef[e]) {
            continue;
        }
        let f = follow(g, empty, e, 1);
        a.push(ef[e]);
        b.push(et[f]);
        da.push(efd[e]);
        db.push(etd[f]);
    }
    let off = |xs: &[f64], k: usize| xs[b[k]] + db[k] - xs[a[k]] - da[k];
    // The nodes in line with each other move as one.
    let mut block: Vec<usize> = (0..n).collect();
    let mut members: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
    let mut near = Vec::new();
    for k in 0..a.len() {
        let d = off(xs, k).abs();
        if d < 0.01 {
            join(&mut block, &mut members, a[k], b[k]);
        } else if d <= SNAP {
            near.push(k);
        }
    }
    if near.is_empty() {
        return;
    }
    near.sort_by(|&x, &y| by(or(off(xs, x).abs() - off(xs, y).abs(), x as f64 - y as f64)));
    let gap = |v: usize| half_room(g, opt, v);
    // Whether every node of a block can move by `shift` and keep its room from those beside it.
    let free = |xs: &[f64], block: &[usize], members: &[Vec<usize>], id: usize, shift: f64| {
        for &v in &members[id] {
            let s = layer_start[rank[v]];
            let end = layer_start[rank[v] + 1];
            let i = s + pos[v];
            if i > s
                && block[order[i - 1]] != id
                && xs[v] + shift - xs[order[i - 1]] < gap(v) + gap(order[i - 1]) - 0.01
            {
                return false;
            }
            if i + 1 < end
                && block[order[i + 1]] != id
                && xs[order[i + 1]] - xs[v] - shift < gap(v) + gap(order[i + 1]) - 0.01
            {
                return false;
            }
        }
        true
    };
    for &k in &near {
        let d = off(xs, k);
        if d.abs() < 0.01 {
            join(&mut block, &mut members, a[k], b[k]);
            continue;
        }
        if d.abs() > SNAP || block[a[k]] == block[b[k]] {
            continue;
        }
        // The smaller of the two blocks moves, if it can, and else the other.
        let upper = block[a[k]];
        let lower = block[b[k]];
        let first = if members[upper].len() <= members[lower].len() {
            upper
        } else {
            lower
        };
        for id in [first, if first == upper { lower } else { upper }] {
            let shift = if id == upper { d } else { -d };
            if !free(xs, &block, &members, id, shift) {
                continue;
            }
            for &v in &members[id] {
                xs[v] += shift;
            }
            join(&mut block, &mut members, a[k], b[k]);
            break;
        }
    }
}

/// Marks the ranks that hold nothing but the bends of edges, if there are any, as Pele's
/// emptyRanks does.
fn empty_ranks(g: &Graph) -> Option<Vec<bool>> {
    let Graph {
        max_rank,
        ref kind,
        ref layer_start,
        ref order,
        ..
    } = *g;
    let mut empty = vec![false; max_rank + 1];
    let mut any = false;
    for r in 1..max_rank {
        let row = &order[layer_start[r]..layer_start[r + 1]];
        if !row.is_empty() && row.iter().all(|&v| kind[v] == Kind::Dummy) {
            empty[r] = true;
            any = true;
        }
    }
    any.then_some(empty)
}

/// Gives every piece that does not run straight down a track across its gap, and counts the
/// tracks of each gap, as Pele's assignTracks does. Two pieces in one gap share a track only
/// where they cannot be mistaken for each other: where they do not overlap, or where they leave
/// from or arrive at the same point, as the branches of a tree do. Which of two goes above the
/// other is whichever makes them cross less. With `curved`, the tracks are levels for curves to
/// turn onto, and only the pieces that would run along or too near another get one: each group
/// of those counts its tracks from its own first. `room` is the height of the gap under each
/// rank, and `tall` receives the height that a gap whose curves mostly cross each other should
/// have instead of levels.
fn assign_tracks(
    pieces: &mut Pieces,
    track_count: &mut [usize],
    curved: bool,
    room: &[f64],
    tall: &mut [f64],
) {
    let Pieces {
        ref from,
        ref to,
        ref rank,
        ref mut track,
        ref mut levels,
        ref wide,
        ..
    } = *pieces;
    let total = from.len();
    // Pele keeps the pieces of each rank in a Map, which goes over its ranks in the order they
    // were first put in.
    let mut by_rank: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut slot = vec![usize::MAX; track_count.len()];
    for k in 0..total {
        track.push(-1);
        levels.push(0);
        if (from[k] - to[k]).abs() < IN_LINE {
            continue;
        }
        if slot[rank[k]] == usize::MAX {
            slot[rank[k]] = by_rank.len();
            by_rank.push((rank[k], Vec::new()));
        }
        by_rank[slot[rank[k]]].1.push(k);
    }
    if by_rank.is_empty() {
        return;
    }
    // The pieces are compared pair by pair, so where each begins and ends across the flow is
    // worked out once.
    let lo: Vec<f64> = (0..total).map(|k| min(from[k], to[k])).collect();
    let hi: Vec<f64> = (0..total).map(|k| max(from[k], to[k])).collect();
    let within = |x: f64, k: usize| usize::from(x >= lo[k] - IN_LINE && x <= hi[k] + IN_LINE);
    let mates =
        |a: usize, b: usize| (from[a] - from[b]).abs() < IN_LINE || (to[a] - to[b]).abs() < IN_LINE;
    let overlap = |a: usize, b: usize| lo[a] < hi[b] - IN_LINE && lo[b] < hi[a] - IN_LINE;
    // Whether a and b need tracks of their own.
    let clash = |a: usize, b: usize| overlap(a, b) && !mates(a, b);
    // Whether one of a and b could cross the other, depending on which is above.
    let meet = |a: usize, b: usize| {
        overlap(a, b)
            || within(to[a], b) + within(from[b], a) + within(to[b], a) + within(from[a], b) > 0
    };
    // How often a and b cross with a on the higher track: a's way down through b, and b's way
    // down to itself through a.
    let crossings = |a: usize, b: usize| within(to[a], b) + within(from[b], a);
    // Returns pairs of list indices, smaller first. Omitted pairs cannot meet or clash.
    let near_pairs = |list: &[usize]| {
        let mut by_lo: Vec<usize> = (0..list.len()).collect();
        by_lo.sort_by(|&a, &b| by(lo[list[a]] - lo[list[b]]));
        let mut pairs = Vec::new();
        for x in 0..by_lo.len() {
            let i = by_lo[x];
            let reach = hi[list[i]] + 4.0 * IN_LINE;
            for &j in by_lo[x + 1..].iter().take_while(|&&j| lo[list[j]] <= reach) {
                pairs.push((i.min(j), i.max(j)));
            }
        }
        pairs
    };

    // Gives the pieces of one gap in `list` their tracks, and each the number of tracks that
    // the pieces it has to do with take between them, counting its own from the first of
    // those. Returns the number of tracks. The pieces come without tracks.
    let lay = |list: &mut [usize], track: &mut [i32], levels: &mut [usize]| {
        // A plain order to start from, and to keep where nothing says otherwise: the pieces
        // that reach furthest go first, so that those of one fan nest.
        list.sort_by(|&a, &b| {
            by(or(
                or(hi[b] - lo[b] - (hi[a] - lo[a]), lo[a] - lo[b]),
                a as f64 - b as f64,
            ))
        });
        let list = &*list;
        let count = list.len();
        let mut above: Vec<Vec<usize>> = Vec::new();
        let mut group: Vec<usize> = (0..count).collect();
        let find = |group: &mut [usize], mut i: usize| {
            while group[i] != i {
                group[i] = group[group[i]];
                i = group[i];
            }
            i
        };
        let weighed = count <= MAX_WEIGHED;
        // Avoid storing quadratically many pairs in dense gaps.
        let pairs = if weighed {
            near_pairs(list)
        } else {
            Vec::new()
        };
        let mut clashes: Vec<Vec<usize>> = if weighed {
            vec![Vec::new(); count]
        } else {
            Vec::new()
        };
        for &(i, j) in &pairs {
            if !clash(list[i], list[j]) {
                continue;
            }
            clashes[i].push(j);
            clashes[j].push(i);
        }
        let mut sequence: Vec<usize> = (0..count).collect();
        if weighed {
            // above[j] lists the pieces that should be above list[j]. The order follows as many
            // of these as it can: a piece is placed once all that should be above it are, and
            // when none is free, the one with the fewest still waiting.
            let mut waiting = vec![0i32; count];
            let mut after: Vec<Vec<usize>> = vec![Vec::new(); count];
            above = vec![Vec::new(); count];
            for &(i, j) in &pairs {
                let a = list[i];
                let b = list[j];
                if !meet(a, b) {
                    continue;
                }
                let root = find(&mut group, i);
                group[root] = find(&mut group, j);
                let ab = crossings(a, b);
                let ba = crossings(b, a);
                if ab == ba {
                    continue;
                }
                let (first, second) = if ab < ba { (i, j) } else { (j, i) };
                above[second].push(first);
                after[first].push(second);
                waiting[second] += 1;
            }
            let mut placed = vec![false; count];
            sequence.clear();
            for _ in 0..count {
                let mut pick: Option<usize> = None;
                for i in 0..count {
                    if placed[i] {
                        continue;
                    }
                    if pick.is_none_or(|p| waiting[i] < waiting[p]) {
                        pick = Some(i);
                    }
                    if waiting[i] == 0 {
                        break;
                    }
                }
                let Some(pick) = pick else { break };
                placed[pick] = true;
                sequence.push(pick);
                for &j in &after[pick] {
                    waiting[j] -= 1;
                }
            }
        }
        let mut taken = HashSet::new();
        let mut on_track: Vec<Vec<usize>> = Vec::new();
        let mut rows = 0;
        for &at in &sequence {
            let k = list[at];
            let mut t = 0;
            for &i in above.get(at).map_or(&[][..], Vec::as_slice) {
                if track[list[i]] >= t {
                    t = track[list[i]] + 1;
                }
            }
            if weighed {
                taken.clear();
                for &i in &clashes[at] {
                    taken.insert(track[list[i]]);
                }
                while taken.contains(&t) {
                    t += 1;
                }
            } else {
                while on_track
                    .get(t as usize)
                    .is_some_and(|row| row.iter().any(|&other| clash(k, other)))
                {
                    t += 1;
                }
                if on_track.len() <= t as usize {
                    on_track.resize_with(t as usize + 1, Vec::new);
                }
                on_track[t as usize].push(k);
            }
            track[k] = t;
            if t >= rows {
                rows = t + 1;
            }
        }
        let mut first = vec![rows; count];
        let mut last = vec![-1; count];
        for i in 0..count {
            let root = find(&mut group, i);
            first[root] = first[root].min(track[list[i]]);
            last[root] = last[root].max(track[list[i]]);
        }
        for i in 0..count {
            let root = find(&mut group, i);
            levels[list[i]] = (last[root] - first[root] + 1) as usize;
            if curved {
                track[list[i]] -= first[root];
            }
        }
        rows as usize
    };

    let sample = |k: usize, h: f64, out: &mut [f64; 2 * SAMPLES + 2]| {
        let c = ease(to[k] - from[k], h);
        for i in 0..=SAMPLES {
            let t = i as f64 / SAMPLES as f64;
            let u = 1.0 - t;
            out[2 * i] = from[k] + (to[k] - from[k]) * t * t * (3.0 - 2.0 * t);
            out[2 * i + 1] = h * (3.0 * c * t * u * u + 3.0 * (1.0 - c) * t * t * u + t * t * t);
        }
    };
    let reaches = |points: &[f64], line: &[f64], limit: f64| {
        // Allow rounding error in the cheap distance check before calling Math.hypot.
        let bound = limit * limit * (1.0 + 1e-9);
        for i in 0..=SAMPLES {
            let px = points[2 * i];
            let py = points[2 * i + 1];
            for j in 0..SAMPLES {
                let dx = line[2 * j + 2] - line[2 * j];
                let dy = line[2 * j + 3] - line[2 * j + 1];
                let t = max(
                    0.0,
                    min(
                        1.0,
                        ((px - line[2 * j]) * dx + (py - line[2 * j + 1]) * dy)
                            / or(dx * dx + dy * dy, 1.0),
                    ),
                );
                let ex = px - line[2 * j] - t * dx;
                let ey = py - line[2 * j + 1] - t * dy;
                if ex * ex + ey * ey < bound && hypot(ex, ey) < limit {
                    return true;
                }
            }
        }
        false
    };
    let near = |a: usize, b: usize, height: f64, limit: f64| {
        let h = max(height / 2.0, height - 2.0 * LEVEL_LEAD);
        let mut line_a = [0.0; 2 * SAMPLES + 2];
        let mut line_b = [0.0; 2 * SAMPLES + 2];
        sample(a, h, &mut line_a);
        sample(b, h, &mut line_b);
        reaches(&line_a, &line_b, limit) || reaches(&line_b, &line_a, limit)
    };

    for (r, mut all) in by_rank {
        if !curved {
            track_count[r] = lay(&mut all, track, levels);
            continue;
        }
        if all.len() > MAX_WEIGHED {
            continue;
        }
        // The pieces that turn onto levels: first those that run far across and have another
        // such piece to be told apart from, then any two that would pass too near each other as
        // plain curves.
        let mut far: Vec<usize> = all.iter().copied().filter(|&k| wide[k]).collect();
        lay(&mut far, track, levels);
        let mut level: HashSet<usize> = far.iter().copied().filter(|&k| levels[k] > 1).collect();
        let pairs = near_pairs(&all);
        for &(i, j) in &pairs {
            let a = all[i];
            let b = all[j];
            if level.contains(&a) && level.contains(&b)
                || !clash(a, b)
                || (from[a] < to[a]) != (from[b] < to[b])
            {
                continue;
            }
            let limit = if wide[a] || wide[b] {
                LEVEL_CLEAR
            } else {
                LEVEL_TOUCH
            };
            if near(a, b, room[r], limit) {
                level.insert(a);
                level.insert(b);
            }
        }
        for &k in &far {
            track[k] = -1;
            levels[k] = 0;
        }
        if level.is_empty() {
            continue;
        }
        let mut crossing = 0;
        let mut alongside = 0;
        let mut beside = HashSet::new();
        for &(i, j) in &pairs {
            let a = all[i];
            let b = all[j];
            if level.contains(&a) && level.contains(&b) {
                if !overlap(a, b) {
                    continue;
                }
                if (from[a] < to[a]) != (from[b] < to[b]) {
                    crossing += 1;
                } else if !mates(a, b) {
                    alongside += 1;
                }
            } else if (level.contains(&a) || level.contains(&b)) && meet(a, b) {
                beside.insert(a);
                beside.insert(b);
            }
        }
        if crossing > alongside {
            for &k in &all {
                if (hi[k] - lo[k]) * CROSSED_SLOPE > tall[r] {
                    tall[r] = (hi[k] - lo[k]) * CROSSED_SLOPE;
                }
            }
            continue;
        }
        // With them go the short pieces that could cross one of them, so that each has its
        // place among the levels and crosses none.
        let mut members: Vec<usize> = all
            .iter()
            .copied()
            .filter(|k| level.contains(k) || !wide[*k] && beside.contains(k))
            .collect();
        lay(&mut members, track, levels);
        let mut most = 0;
        for &k in &members {
            if levels[k] > most {
                most = levels[k];
            }
        }
        track_count[r] = most;
    }
}

/// Moves the bends in ranks that hold only bends onto the straight line between the nearest
/// points of their edges that are held in place, and marks the ones that can be left out of the
/// route, as Pele's straighten does. `first_dummy` and `span` give each edge's run of bends,
/// one in every rank. `sep` is the room kept between the bends of twin edges, or 0 to leave
/// those out as well.
fn straighten(
    g: &Graph,
    xs: &mut [f64],
    empty: &[bool],
    first_dummy: &[i32],
    span: &[usize],
    sep: f64,
    on_line: &mut [bool],
) {
    let Graph {
        max_rank,
        ref ef,
        ref et,
        ref efd,
        ref etd,
        ref succ_start,
        ref succ_e,
        ref pred_start,
        ref pred_e,
        ref layer_start,
        ref order,
        ..
    } = *g;

    let mut want = vec![0.0; xs.len()];
    // The points that each bend's edge runs between, across the ranks that hold only bends.
    let mut from = vec![0.0; xs.len()];
    let mut to = vec![0.0; xs.len()];
    for ei in 0..first_dummy.len() {
        let Ok(d0) = usize::try_from(first_dummy[ei]) else {
            continue;
        };
        let end = d0 + span[ei] - 1;
        // A bend's rank is its place in the run, counted from the rank after the edge's first
        // node.
        let base = g.rank[d0];
        let mut d = d0;
        while d < end {
            if !empty[base + d - d0] {
                d += 1;
                continue;
            }
            let mut last = d;
            while last + 1 < end && empty[base + last + 1 - d0] {
                last += 1;
            }
            let above = pred_e[pred_start[d]];
            let below = succ_e[succ_start[last]];
            let x0 = xs[ef[above]] + efd[above];
            let x1 = xs[et[below]] + etd[below];
            let steps = (last - d + 2) as f64;
            for k in d..=last {
                want[k] = x0 + (x1 - x0) * (k - d + 1) as f64 / steps;
                from[k] = x0;
                to[k] = x1;
            }
            d = last + 1;
        }
    }

    // Nothing in such a rank is in a bend's way, so each one goes onto its line and out of the
    // route. Only twins stay: bends of edges that run between the same two points, which would
    // otherwise be drawn as one. They take places a little apart, around the line.
    let mut twins = Vec::new();
    for r in 1..max_rank {
        if !empty[r] {
            continue;
        }
        let mut row = order[layer_start[r]..layer_start[r + 1]].to_vec();
        row.sort_by(|&a, &b| {
            by(or(
                or(from[a] - from[b], to[a] - to[b]),
                a as f64 - b as f64,
            ))
        });
        let mut i = 0;
        while i < row.len() {
            twins.clear();
            let mut j = i;
            while j < row.len()
                && (from[row[j]] - from[row[i]]).abs() < 1.0
                && (to[row[j]] - to[row[i]]).abs() < 1.0
            {
                twins.push(row[j]);
                j += 1;
            }
            // The twins keep the order the rank gave them, so that two edges do not swap sides
            // on the way.
            twins.sort_by_key(|&d| g.pos[d]);
            for (k, &d) in twins.iter().enumerate() {
                xs[d] = want[d] + (k as f64 - (twins.len() - 1) as f64 / 2.0) * sep;
                if twins.len() == 1 || sep == 0.0 {
                    on_line[d] = true;
                }
            }
            i = j;
        }
    }
}

/// In a rank that holds labels and bends but no nodes, a bend is there to take its edge
/// straight through between the labels, as Pele's pastLabels has it. Where the edge would pass
/// clear of every label anyway, the bend is left out, and the edge turns in the whole height
/// between the ranks on either side.
fn past_labels(
    g: &Graph,
    xs: &mut [f64],
    first_dummy: &[i32],
    span: &[usize],
    clear: f64,
    on_line: &mut [bool],
) {
    let Graph {
        max_rank,
        ref w,
        ref kind,
        ref rank,
        ref ef,
        ref et,
        ref efd,
        ref etd,
        ref succ_start,
        ref succ_e,
        ref pred_start,
        ref pred_e,
        ref layer_start,
        ref order,
        ..
    } = *g;
    let mut labelled = vec![false; max_rank + 1];
    let mut any = false;
    for r in 1..max_rank {
        let mut labels = false;
        let mut others = false;
        for &v in &order[layer_start[r]..layer_start[r + 1]] {
            let kind = kind[v];
            if kind == Kind::Label {
                labels = true;
            } else if kind != Kind::Dummy {
                others = true;
            }
        }
        if labels && !others {
            labelled[r] = true;
            any = true;
        }
    }
    if !any {
        return;
    }
    for ei in 0..first_dummy.len() {
        let Ok(d0) = usize::try_from(first_dummy[ei]) else {
            continue;
        };
        let end = d0 + span[ei] - 1;
        for d in d0..end {
            if kind[d] != Kind::Dummy || !labelled[rank[d]] {
                continue;
            }
            // The points on either side that the edge is held at, past any bends already left
            // out.
            let mut first = d;
            let mut last = d;
            while first > d0 && on_line[first - 1] {
                first -= 1;
            }
            while last + 1 < end && on_line[last + 1] {
                last += 1;
            }
            let above = pred_e[pred_start[first]];
            let below = succ_e[succ_start[last]];
            let from = xs[ef[above]] + efd[above];
            let to = xs[et[below]] + etd[below];
            let lo = min(from, to) - clear;
            let hi = max(from, to) + clear;
            let r = rank[d];
            let free = order[layer_start[r]..layer_start[r + 1]]
                .iter()
                .all(|&label| {
                    !(kind[label] == Kind::Label
                        && xs[label] + w[label] / 2.0 > lo
                        && xs[label] - w[label] / 2.0 < hi)
                });
            if !free {
                continue;
            }
            on_line[d] = true;
            // The bends left out between the two points lie on the line between them.
            let steps = (last - first + 2) as f64;
            for k in first..=last {
                xs[k] = from + (to - from) * (k - first + 1) as f64 / steps;
            }
        }
    }
}

/// Orders the nodes of each rank to cross as few edges as it can, as Pele's reduceCrossings
/// does. Returns the final crossings, and the crossings before grouping or reordering.
fn reduce_crossings(g: &mut Graph, nodes: &[LNode]) -> (usize, usize) {
    let mut order = std::mem::take(&mut g.order);
    let mut pos = std::mem::take(&mut g.pos);
    let Graph {
        n,
        real,
        max_rank,
        ref w,
        ref ef,
        ref et,
        ref efd,
        ref etd,
        ref succ_start,
        ref succ_e,
        ref pred_start,
        ref pred_e,
        ref layer_start,
        offsets,
        ref fixed,
        ref grp,
        ref group_parent,
        ..
    } = *g;
    let found = 'reduce: {
        let mut widest = 0;
        for r in 0..=max_rank {
            widest = widest.max(layer_start[r + 1] - layer_start[r]);
        }
        if widest < 2 {
            break 'reduce (0, 0);
        }
        let segments = ef.len();
        // With offsets, edges that reach one node at different offsets take separate places
        // along it.
        let mut place = offsets.then(|| vec![0usize; segments]);
        // Local offset indices and counts do not depend on node order.
        let mut place_in = vec![0usize; if offsets { segments } else { 0 }];
        let mut places = vec![0usize; if offsets { n } else { 0 }];
        let mut leaving = succ_e.clone();
        let mut mixed = vec![false; if offsets { n } else { 0 }];
        if offsets {
            let mut arriving = pred_e.clone();
            let by_offset = |list: &mut [usize], from: usize, to: usize, dx: &[f64]| {
                for k in from + 1..to {
                    if dx[list[k]] == dx[list[from]] {
                        continue;
                    }
                    list[from..to].sort_by(|&a, &b| by(dx[a] - dx[b]));
                    return true;
                }
                false
            };
            for v in 0..n {
                places[v] = 1;
                if by_offset(&mut arriving, pred_start[v], pred_start[v + 1], etd) {
                    for k in pred_start[v] + 1..pred_start[v + 1] {
                        if etd[arriving[k]] != etd[arriving[k - 1]] {
                            places[v] += 1;
                        }
                        place_in[arriving[k]] = places[v] - 1;
                    }
                }
                if by_offset(&mut leaving, succ_start[v], succ_start[v + 1], efd) {
                    mixed[v] = true;
                }
            }
        }
        let mut size = 1;
        while size < if offsets { widest + segments } else { widest } {
            size <<= 1;
        }
        let mut tree = vec![0usize; 2 * size];
        let mut targets = vec![0usize; segments];

        let mut count = |order: &[usize], pos: &[usize]| {
            let mut total = 0;
            for r in 0..max_rank {
                let mut south_len = layer_start[r + 2] - layer_start[r + 1];
                if let Some(place) = &mut place {
                    let mut next = 0;
                    for &v in &order[layer_start[r + 1]..layer_start[r + 2]] {
                        for &e in &pred_e[pred_start[v]..pred_start[v + 1]] {
                            place[e] = next + place_in[e];
                        }
                        next += places[v];
                    }
                    south_len = next;
                }
                if south_len < 2 {
                    continue;
                }
                let mut t = 0;
                for &v in &order[layer_start[r]..layer_start[r + 1]] {
                    let from = succ_start[v];
                    let to = succ_start[v + 1];
                    let t0 = t;
                    for k in from..to {
                        targets[t] = match &place {
                            Some(place) => place[leaving[k]],
                            None => pos[et[succ_e[k]]],
                        };
                        t += 1;
                    }
                    let d = t - t0;
                    if d < 2 {
                        continue;
                    }
                    if place.is_some() && mixed[v] {
                        // Sort equal-offset edges by destination to avoid counting shared
                        // endpoints as crossings.
                        for a in t0 + 1..t {
                            let item = targets[a];
                            let dx = efd[leaving[from + a - t0]];
                            let mut b = a;
                            while b > t0
                                && efd[leaving[from + b - 1 - t0]] == dx
                                && targets[b - 1] > item
                            {
                                targets[b] = targets[b - 1];
                                b -= 1;
                            }
                            targets[b] = item;
                        }
                    } else if d <= 12 {
                        for a in t0 + 1..t {
                            let item = targets[a];
                            let mut b = a;
                            while b > t0 && targets[b - 1] > item {
                                targets[b] = targets[b - 1];
                                b -= 1;
                            }
                            targets[b] = item;
                        }
                    } else {
                        targets[t0..t].sort_unstable();
                    }
                }
                if t < 2 {
                    continue;
                }
                let mut leaves = 1;
                while leaves < south_len {
                    leaves <<= 1;
                }
                tree[..2 * leaves].fill(0);
                for &target in &targets[..t] {
                    let mut i = target + leaves;
                    tree[i] += 1;
                    while i > 1 {
                        if i & 1 == 0 {
                            total += tree[i + 1];
                        }
                        i >>= 1;
                        tree[i] += 1;
                    }
                }
            }
            total
        };

        let declared = count(&order, &pos);
        let mut bary = vec![0.0; n];
        let mut moving = vec![0usize; widest];
        let mut spare = vec![0usize; widest];

        // Keep groups contiguous, ordering each by its mean key.
        let parent: &[i32] = group_parent.as_deref().unwrap_or(&[]);
        let of: &[i32] = grp.as_deref().unwrap_or(&[]);
        let groups = parent.len();
        let mut depth = vec![0usize; groups];
        for gr in 0..groups {
            let mut p = gr as i32;
            while p >= 0 {
                depth[gr] += 1;
                p = parent[p as usize];
            }
        }
        let mut group_sum = vec![0.0; groups];
        let mut group_count = vec![0usize; groups];
        let mut group_first = vec![0usize; groups];
        let mut arrange = |r: usize, key: &[f64], order: &mut [usize], pos: &mut [usize]| {
            let s = layer_start[r];
            let e = layer_start[r + 1];
            for &v in &order[s..e] {
                let mut gr = of[v];
                while gr >= 0 {
                    group_sum[gr as usize] = 0.0;
                    group_count[gr as usize] = 0;
                    gr = parent[gr as usize];
                }
            }
            for i in s..e {
                let v = order[i];
                let mut gr = of[v];
                while gr >= 0 {
                    let at = gr as usize;
                    if group_count[at] == 0 {
                        group_first[at] = i;
                    }
                    group_sum[at] += key[v];
                    group_count[at] += 1;
                    gr = parent[at];
                }
            }
            // Compare siblings in their nearest common group. Group items use n + group ID.
            let value = |item: usize| {
                if item >= n {
                    group_sum[item - n] / group_count[item - n] as f64
                } else {
                    key[item]
                }
            };
            let first = |item: usize| {
                if item >= n {
                    group_first[item - n]
                } else {
                    s + pos[item]
                }
            };
            let compare = |a: usize, b: usize| {
                let mut ia = a;
                let mut ib = b;
                let mut ca = of[a];
                let mut cb = of[b];
                let mut da = if ca < 0 { 0 } else { depth[ca as usize] };
                let mut db = if cb < 0 { 0 } else { depth[cb as usize] };
                while da > db {
                    ia = n + ca as usize;
                    ca = parent[ca as usize];
                    da -= 1;
                }
                while db > da {
                    ib = n + cb as usize;
                    cb = parent[cb as usize];
                    db -= 1;
                }
                while ca != cb {
                    ia = n + ca as usize;
                    ca = parent[ca as usize];
                    ib = n + cb as usize;
                    cb = parent[cb as usize];
                }
                by(or(
                    value(ia) - value(ib),
                    first(ia) as f64 - first(ib) as f64,
                ))
            };
            let mut row = order[s..e].to_vec();
            row.sort_by(|&a, &b| compare(a, b));
            order[s..e].copy_from_slice(&row);
            for i in s..e {
                pos[order[i]] = i - s;
            }
        };
        if grp.is_some() {
            let mut at = vec![0.0; n];
            for r in 0..=max_rank {
                for i in layer_start[r]..layer_start[r + 1] {
                    at[order[i]] = (i - layer_start[r]) as f64;
                }
                arrange(r, &at, &mut order, &mut pos);
            }
        }
        let mut best = if grp.is_some() {
            count(&order, &pos)
        } else {
            declared
        };
        if best == 0 {
            break 'reduce (0, declared);
        }
        let mut best_order = order.clone();
        // Require improvements over both the grouped and ungrouped starting orders.
        let start = best.min(declared);

        let mut sweep = |down: bool, bias_right: bool, order: &mut [usize], pos: &mut [usize]| {
            let (starts, list, other, other_dx) = if down {
                (pred_start, pred_e, ef, efd)
            } else {
                (succ_start, succ_e, et, etd)
            };
            for step in 1..=max_rank {
                let r = if down { step } else { max_rank - step };
                if fixed[r] {
                    continue;
                }
                let s = layer_start[r];
                let e = layer_start[r + 1];
                let mut movable = 0;
                for i in s..e {
                    let v = order[i];
                    let pin = if v < real { nodes[v].pin } else { 0 };
                    if pin != 0 {
                        bary[v] = f64::from(pin) * 1e9;
                        moving[movable] = v;
                        movable += 1;
                        continue;
                    }
                    let from = starts[v];
                    let to = starts[v + 1];
                    if from == to {
                        if grp.is_some() {
                            // Preserve relative position when there are no neighbours in the
                            // adjacent rank.
                            let across = if down {
                                layer_start[r] - layer_start[r - 1]
                            } else {
                                layer_start[r + 2] - layer_start[r + 1]
                            };
                            bary[v] = ((i - s) * across) as f64 / (e - s) as f64;
                            continue;
                        }
                        bary[v] = -1.0;
                        continue;
                    }
                    let mut sum = 0.0;
                    for &k in &list[from..to] {
                        let u = other[k];
                        sum += pos[u] as f64;
                        if offsets && w[u] > 0.0 {
                            sum += other_dx[k] / w[u];
                        }
                    }
                    bary[v] = sum / (to - from) as f64;
                    moving[movable] = v;
                    movable += 1;
                }
                if grp.is_some() {
                    arrange(r, &bary, order, pos);
                    continue;
                }
                if movable < 2 {
                    continue;
                }
                sort_by_bary(&mut moving, movable, &bary, pos, bias_right, &mut spare);
                let mut k = 0;
                for i in s..e {
                    if bary[order[i]] != -1.0 {
                        order[i] = moving[k];
                        k += 1;
                    }
                }
                for i in s..e {
                    pos[order[i]] = i - s;
                }
            }
        };

        // How many pairs of nodes in one rank are not in the order they were given.
        let out_of_order = |order: &[usize]| {
            let mut total = 0;
            for r in 0..=max_rank {
                let row = &order[layer_start[r]..layer_start[r + 1]];
                for (i, &v) in row.iter().enumerate() {
                    if v >= real {
                        continue;
                    }
                    total += row[i + 1..].iter().filter(|&&u| u < v).count();
                }
            }
            total
        };
        let settle_positions = |order: &[usize], pos: &mut [usize]| {
            for r in 0..=max_rank {
                for i in layer_start[r]..layer_start[r + 1] {
                    pos[order[i]] = i - layer_start[r];
                }
            }
        };

        // Adjacent swaps can remove crossings missed by barycenter sweeps.
        let pin_of = |v: usize| if v < real { nodes[v].pin } else { 0 };
        let at = |pos: &[usize], e: usize, up: bool| {
            let u = if up { ef[e] } else { et[e] };
            if offsets && w[u] > 0.0 {
                pos[u] as f64 + (if up { efd[e] } else { etd[e] }) / w[u]
            } else {
                pos[u] as f64
            }
        };
        // Crossings between a and b, assuming a is left of b.
        let between = |pos: &[usize], a: usize, b: usize| {
            let mut c = 0;
            for &k in &pred_e[pred_start[a]..pred_start[a + 1]] {
                let x = at(pos, k, true);
                for &j in &pred_e[pred_start[b]..pred_start[b + 1]] {
                    if x > at(pos, j, true) {
                        c += 1;
                    }
                }
            }
            for &k in &succ_e[succ_start[a]..succ_start[a + 1]] {
                let x = at(pos, k, false);
                for &j in &succ_e[succ_start[b]..succ_start[b + 1]] {
                    if x > at(pos, j, false) {
                        c += 1;
                    }
                }
            }
            c
        };
        let transpose = |order: &mut [usize], pos: &mut [usize]| {
            for _ in 0..8 {
                let mut improved = false;
                for r in 0..=max_rank {
                    if fixed[r] {
                        continue;
                    }
                    let s = layer_start[r];
                    for i in s..layer_start[r + 1].saturating_sub(1) {
                        let a = order[i];
                        let b = order[i + 1];
                        if pin_of(a) != pin_of(b) || grp.is_some() && of[a] != of[b] {
                            continue;
                        }
                        if between(pos, b, a) >= between(pos, a, b) {
                            continue;
                        }
                        order[i] = b;
                        order[i + 1] = a;
                        pos[b] = i - s;
                        pos[a] = i + 1 - s;
                        improved = true;
                    }
                }
                if !improved {
                    break;
                }
            }
        };

        // The sweeps are run twice from the same start, going down first and going up first.
        // They often end with as few crossings either way but a different order, and then the
        // one that keeps more of the nodes in the order they were given is the less surprising.
        // Only for graphs small enough to be read node by node: a second run costs as much as
        // the first.
        let initial = order.clone();
        let small = n <= 600;
        let mut chosen = best_order.clone();
        let mut chosen_out = usize::MAX;
        for attempt in 0..if small { 2 } else { 1 } {
            if attempt > 0 {
                order.copy_from_slice(&initial);
                settle_positions(&order, &mut pos);
            }
            let mut found = start;
            let mut found_order = initial.clone();
            let mut i = 0;
            let mut stale = 0;
            while stale < 4 && i < MAX_SWEEPS {
                sweep((i + attempt) % 2 == 0, i % 4 >= 2, &mut order, &mut pos);
                let c = count(&order, &pos);
                if c < found {
                    found = c;
                    found_order.copy_from_slice(&order);
                    stale = 0;
                    if c == 0 {
                        break;
                    }
                }
                i += 1;
                stale += 1;
            }
            order.copy_from_slice(&found_order);
            if small && found > 0 {
                settle_positions(&order, &mut pos);
                transpose(&mut order, &mut pos);
                let c = count(&order, &pos);
                if c < found {
                    found = c;
                    found_order.copy_from_slice(&order);
                }
            }
            let out = if small { out_of_order(&order) } else { 0 };
            if attempt == 0 || found < best || found == best && out < chosen_out {
                best = found;
                chosen = found_order;
                chosen_out = out;
            }
        }
        order.copy_from_slice(&chosen);
        settle_positions(&order, &mut pos);
        best_order.copy_from_slice(&chosen);
        // The retained grouped order may exceed the ungrouped threshold, so it is recounted.
        if grp.is_some() {
            best = count(&order, &pos);
        }

        // A drawing and its mirror image cross as often too. Of those two, likewise.
        if !small || nodes[..real].iter().any(|node| node.pin != 0) {
            break 'reduce (best, declared);
        }
        if chosen_out == 0 || fixed[0] || fixed[max_rank] {
            break 'reduce (best, declared);
        }
        for r in 0..=max_rank {
            order[layer_start[r]..layer_start[r + 1]].reverse();
        }
        settle_positions(&order, &mut pos);
        if count(&order, &pos) > best || out_of_order(&order) >= chosen_out {
            order.copy_from_slice(&best_order);
        }
        (best, declared)
    };
    g.order = order;
    g.pos = pos;
    found
}

/// Half the room a node, bend or port takes across the flow, with its share of the gap beside
/// it, as Pele's halfRoom says. A port on a group's border is a point that an edge passes
/// through, and needs no more than a bend.
fn half_room(g: &Graph, opt: &LayeredOptions, v: usize) -> f64 {
    g.w[v] / 2.0
        + (if g.kind[v] == Kind::Node {
            opt.node_sep
        } else {
            opt.edge_sep
        }) / 2.0
}

/// Brandes-Köpf horizontal coordinate assignment, as Pele's position does it: four aligned
/// layouts, balanced. It sorts each node's edges by where their other ends are, as Pele's does
/// in place.
fn position(g: &mut Graph, opt: &LayeredOptions) -> Vec<f64> {
    let n = g.n;
    let segments = g.ef.len();
    let gap: Vec<f64> = (0..n).map(|i| half_room(g, opt, i)).collect();

    // Neighbours in left-to-right order, so the medians can be read off directly.
    let mut key: Vec<usize> = (0..segments).map(|e| g.pos[g.et[e]]).collect();
    for v in 0..n {
        if g.succ_start[v + 1] - g.succ_start[v] > 1 {
            sort_range(&mut g.succ_e, g.succ_start[v], g.succ_start[v + 1], &key);
        }
    }
    for e in 0..segments {
        key[e] = g.pos[g.ef[e]];
    }
    for v in 0..n {
        if g.pred_start[v + 1] - g.pred_start[v] > 1 {
            sort_range(&mut g.pred_e, g.pred_start[v], g.pred_start[v + 1], &key);
        }
    }
    let Graph {
        max_rank,
        ref w,
        ref kind,
        ref ef,
        ref et,
        ref efd,
        ref etd,
        ref succ_start,
        ref succ_e,
        ref pred_start,
        ref pred_e,
        ref layer_start,
        ref order,
        ref pos,
        ..
    } = *g;

    // An edge between two real nodes may not cross a dummy-to-dummy segment. Mark those that
    // would.
    let mut conflict = vec![false; segments];
    for r in 1..=max_rank {
        let prev_len = layer_start[r] - layer_start[r - 1];
        let s = layer_start[r];
        let end = layer_start[r + 1];
        let mut k0 = 0;
        let mut scan = s;
        for i in s..end {
            let v = order[i];
            let mut inner = None;
            if is_dummy(kind[v]) {
                let u = ef[pred_e[pred_start[v]]];
                if is_dummy(kind[u]) {
                    inner = Some(u);
                }
            }
            let k1 = inner.map_or(prev_len, |u| pos[u]);
            if inner.is_some() || i == end - 1 {
                for &w in &order[scan..=i] {
                    let w_dummy = is_dummy(kind[w]);
                    for &e in &pred_e[pred_start[w]..pred_start[w + 1]] {
                        let up = pos[ef[e]];
                        if (up < k0 || k1 < up) && !(w_dummy && is_dummy(kind[ef[e]])) {
                            conflict[e] = true;
                        }
                    }
                }
                scan = i + 1;
                k0 = k1;
            }
        }
    }

    let mut results: Vec<Vec<f64>> = Vec::new();
    let mut root = vec![0usize; n];
    let mut align = vec![0usize; n];
    // Offset of each node from its block's root, so that aligned edges meet at their
    // attachment points.
    let mut shift = vec![0.0; n];
    let mut e_from = vec![0usize; n];
    let mut e_to = vec![0usize; n];
    let mut e_w = vec![0.0; n];
    let mut indeg = vec![0usize; n];
    let mut out_start = vec![0usize; n + 1];
    let mut out_e = vec![0usize; n];
    let mut fill = vec![0usize; n];
    let mut topo = vec![0usize; n];
    let mut x = vec![0.0; n];

    for variant in 0..4 {
        let up = variant < 2;
        let right = variant & 1 == 1;
        let (start, list, other, own_dx, other_dx) = if up {
            (pred_start, pred_e, ef, etd, efd)
        } else {
            (succ_start, succ_e, et, efd, etd)
        };
        for i in 0..n {
            root[i] = i;
            align[i] = i;
            shift[i] = 0.0;
            indeg[i] = 0;
            out_start[i + 1] = 0;
            x[i] = 0.0;
        }

        for step in 0..=max_rank {
            let r = if up { step } else { max_rank - step };
            let s = layer_start[r];
            let end = layer_start[r + 1];
            let adjacent = if up { r.checked_sub(1) } else { Some(r + 1) };
            let adjacent_last = match adjacent {
                Some(a) if a <= max_rank => (layer_start[a + 1] - layer_start[a]) as isize - 1,
                _ => 0,
            };
            let mut prev_idx = -1;
            for k in 0..end - s {
                let i = if right { end - 1 - k } else { s + k };
                let v = order[i];
                let from = start[v];
                let d = start[v + 1] - from;
                if d == 0 {
                    continue;
                }
                let lo = from + ((d - 1) >> 1);
                let hi = from + (d >> 1);
                for pass in 0..2 {
                    if pass == 1 && lo == hi {
                        break;
                    }
                    let e = list[if (pass == 0) != right { lo } else { hi }];
                    let w = other[e];
                    let idx = if right {
                        adjacent_last - pos[w] as isize
                    } else {
                        pos[w] as isize
                    };
                    if align[v] == v && prev_idx < idx && !conflict[e] {
                        align[w] = v;
                        root[v] = root[w];
                        align[v] = root[v];
                        shift[v] = shift[w] + other_dx[e] - own_dx[e];
                        prev_idx = idx;
                    }
                }
            }
        }

        let mut block_edges = 0;
        for r in 0..=max_rank {
            for i in layer_start[r] + 1..layer_start[r + 1] {
                let a = if right { order[i] } else { order[i - 1] };
                let b = if right { order[i - 1] } else { order[i] };
                let sep = gap[a] + gap[b];
                e_from[block_edges] = root[a];
                e_to[block_edges] = root[b];
                e_w[block_edges] = if right {
                    sep - shift[a] + shift[b]
                } else {
                    sep + shift[a] - shift[b]
                };
                indeg[root[b]] += 1;
                out_start[root[a] + 1] += 1;
                block_edges += 1;
            }
        }
        for i in 0..n {
            out_start[i + 1] += out_start[i];
        }
        fill.copy_from_slice(&out_start[..n]);
        for e in 0..block_edges {
            out_e[fill[e_from[e]]] = e;
            fill[e_from[e]] += 1;
        }

        let mut count = 0;
        for i in 0..n {
            if root[i] == i && indeg[i] == 0 {
                topo[count] = i;
                count += 1;
            }
        }
        let mut q = 0;
        while q < count {
            let v = topo[q];
            for &e in &out_e[out_start[v]..out_start[v + 1]] {
                let w = e_to[e];
                if x[v] + e_w[e] > x[w] {
                    x[w] = x[v] + e_w[e];
                }
                indeg[w] -= 1;
                if indeg[w] == 0 {
                    topo[count] = w;
                    count += 1;
                }
            }
            q += 1;
        }
        for &v in topo[..count].iter().rev() {
            let mut least = f64::INFINITY;
            for &e in &out_e[out_start[v]..out_start[v + 1]] {
                let cand = x[e_to[e]] - e_w[e];
                if cand < least {
                    least = cand;
                }
            }
            if least != f64::INFINITY && least > x[v] {
                x[v] = least;
            }
        }
        let xs = (0..n)
            .map(|i| (if right { -x[root[i]] } else { x[root[i]] }) + shift[i])
            .collect();
        results.push(xs);
    }

    let mut narrowest = 0;
    let mut best_width = f64::INFINITY;
    let mut mins = [0.0; 4];
    let mut maxs = [0.0; 4];
    for k in 0..4 {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        let xs = &results[k];
        for i in 0..n {
            let half = w[i] / 2.0;
            if xs[i] - half < lo {
                lo = xs[i] - half;
            }
            if xs[i] + half > hi {
                hi = xs[i] + half;
            }
        }
        mins[k] = lo;
        maxs[k] = hi;
        if hi - lo < best_width {
            best_width = hi - lo;
            narrowest = k;
        }
    }
    for k in 0..4 {
        let delta = if k & 1 == 0 {
            mins[narrowest] - mins[k]
        } else {
            maxs[narrowest] - maxs[k]
        };
        if delta != 0.0 {
            for x in &mut results[k] {
                *x += delta;
            }
        }
    }

    (0..n)
        .map(|i| {
            let mut a = results[0][i];
            let mut b = results[1][i];
            let mut c = results[2][i];
            let mut d = results[3][i];
            // Pele's sorting network, but for the least and the greatest, which are not wanted.
            if a > b {
                (a, b) = (b, a);
            }
            if c > d {
                (c, d) = (d, c);
            }
            if a > c {
                c = a;
            }
            if b > d {
                b = d;
            }
            if b > c {
                (b, c) = (c, b);
            }
            (b + c) / 2.0
        })
        .collect()
}
