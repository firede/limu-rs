//! Layout for swimlanes, as Pele's src/layout/lanes.ts lays them out. Every top-level group is
//! a lane: a band that runs the length of the diagram along the flow. Ranks are shared by all
//! lanes, so a step sits level with the steps that happen at the same time in other lanes, and
//! each node stays inside its own lane.

use std::collections::HashSet;

use super::compound::{CEdge, CNode, CompoundResult, Dir};
use super::layered::{Kind, LEdge, LNode, LayeredOptions, ledge, lnode, orient, rank};
use crate::js::math::{by, max, min, round, sign};

const LANE_HEAD: f64 = 34.0;
/// The room a lane keeps around its items, unless its caller says otherwise.
pub(crate) const LANE_PAD: f64 = 24.0;
const SWEEPS: usize = 8;
const MAX_DUMMIES: usize = 20000;
const MAX_HOPS: usize = 10000;
const ROUTE_BUDGET: usize = 2000000;
// Parallel lines closer than this are drawn as one.
const NEAR: f64 = 4.0;
const ROUNDS: usize = 3;
const RELAY: usize = 8;
const TURN: f64 = 20.0;
// The room a lane keeps beside a handoff along its margin, and how much of it is between the
// handoff and the lane's items.
const MARGIN: f64 = 20.0;
const LEG: f64 = 14.0;
const CLOSE: f64 = 3.0;

/// What a place of the lanes gives: the size, which relations took a simple route of their
/// own, the lanes whose margins a handoff runs along, and how much work routing was.
struct Placed {
    width: f64,
    height: f64,
    routed: Vec<bool>,
    margins: Vec<bool>,
    work: usize,
}

/// The runs of a rank's items that share a lane, first and last, as the eachRun of Pele's
/// laneLayout visits them.
fn runs(row: &[usize], lane: &[usize]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut from = 0;
    while from < row.len() {
        let mut to = from;
        while to + 1 < row.len() && lane[row[to + 1]] == lane[row[from]] {
            to += 1;
        }
        out.push((from, to));
        from = to + 1;
    }
    out
}

/// Lays out swimlanes, as Pele's laneLayout does. `lane_pad` is the room a lane keeps around
/// its items, which Pele's callers leave at LANE_PAD unless they say otherwise.
pub(crate) fn lane_layout(
    nodes: &mut [CNode],
    edges: &mut [CEdge],
    dir: Dir,
    opt: &LayeredOptions,
    lane_pad: f64,
) -> CompoundResult {
    let n = nodes.len();
    if n == 0 {
        return CompoundResult::default();
    }
    let sideways = dir == Dir::LR || dir == Dir::RL;
    let flipped = dir == Dir::BT || dir == Dir::RL;

    // Lanes in the order they were written. The graph lists groups last-declared first.
    let lanes: Vec<usize> = (0..n)
        .rev()
        .filter(|&i| nodes[i].parent < 0 && nodes[i].is_group)
        .collect();
    let lane_count = lanes.len().max(1);
    let mut lane_slot = vec![-1i32; n];
    for (k, &id) in lanes.iter().enumerate() {
        lane_slot[id] = k as i32;
    }
    let mut has_child = vec![false; n];
    for node in nodes.iter() {
        if node.parent >= 0 {
            has_child[node.parent as usize] = true;
        }
    }
    // Room a lane keeps around its items, so that the groups drawn around them stay inside it.
    let mut lane_pads = vec![lane_pad; lane_count];
    let mut end_pad = lane_pad;
    let mut lane_of = |nodes: &[CNode], i: usize| {
        let mut t = i;
        let mut room = lane_pad;
        let mut hops = 0;
        while nodes[t].parent >= 0 && hops < MAX_HOPS {
            t = nodes[t].parent as usize;
            if lane_slot[t] < 0 {
                room += max(max(nodes[t].pad_x, nodes[t].pad_top), nodes[t].pad_bottom);
            }
            hops += 1;
        }
        let lane = lane_slot[t].max(0) as usize;
        if room > lane_pads[lane] {
            lane_pads[lane] = room;
        }
        if room > end_pad {
            end_pad = room;
        }
        lane
    };

    // What gets ranked and placed: nodes, and groups that are empty and so act as nodes.
    let mut item_of = vec![-1i32; n];
    let mut real = Vec::new();
    for i in 0..n {
        if lane_slot[i] >= 0 {
            continue;
        }
        if !nodes[i].is_group || !has_child[i] {
            item_of[i] = real.len() as i32;
            real.push(i);
        }
    }
    let along = |c: &CNode| if sideways { c.w } else { c.h };
    let cross = |c: &CNode| if sideways { c.h } else { c.w };

    let mut lnodes: Vec<LNode> = real
        .iter()
        .map(|&i| lnode(cross(&nodes[i]), along(&nodes[i]), Kind::Node))
        .collect();
    let mut ledges: Vec<LEdge> = Vec::new();
    let mut edge_of = Vec::new();
    let mut step = 1;
    {
        let mut seen = HashSet::new();
        for (ei, e) in edges.iter().enumerate() {
            let (Ok(a), Ok(b)) = (
                usize::try_from(item_of[e.src]),
                usize::try_from(item_of[e.dst]),
            ) else {
                continue;
            };
            if a == b {
                continue;
            }
            let key = a.min(b) * real.len() + a.max(b);
            if e.label_w > 0.0 || seen.contains(&key) {
                step = 2;
            }
            seen.insert(key);
            let (label_w, label_h) = if sideways {
                (e.label_h, e.label_w)
            } else {
                (e.label_w, e.label_h)
            };
            ledges.push(ledge(a, b, e.minlen, label_w, label_h));
            edge_of.push(ei);
        }
    }
    orient(&lnodes, &mut ledges);
    rank(&mut lnodes, &ledges, step);

    let m = ledges.len();
    let item_lane: Vec<usize> = real.iter().map(|&i| lane_of(nodes, i)).collect();
    let lane_pads = lane_pads;
    let end_pad = end_pad;
    // Lays the ranked items out and routes the relations. A plain relation is a handoff that
    // takes a simple route of its own, and so keeps no room through the ranks it passes.
    let place = |nodes: &mut [CNode], edges: &mut [CEdge], plain: &[bool], margins: &[bool]| {
        let pad: Vec<f64> = lane_pads
            .iter()
            .zip(margins)
            .map(|(&room, &margin)| if margin { max(room, MARGIN) } else { room })
            .collect();
        let mut used = vec![false; lane_count];
        // Items: the real ones, then a dummy for every rank an edge passes through.
        let mut dummies = 0;
        let mut span = vec![0usize; m];
        for k in 0..m {
            span[k] = lnodes[ledges[k].head]
                .rank
                .abs_diff(lnodes[ledges[k].tail].rank);
            if !plain[k] {
                dummies += span[k] - 1;
            }
        }
        let direct = dummies > MAX_DUMMIES;
        let total = real.len() + if direct { 0 } else { dummies };
        let mut rank = vec![0usize; total];
        let mut lane = vec![0usize; total];
        let mut size = vec![0.0; total];
        let mut depth = vec![0.0; total];
        let mut dummy = vec![false; total];
        let mut max_rank = 0;
        for k in 0..real.len() {
            rank[k] = lnodes[k].rank;
            lane[k] = item_lane[k];
            size[k] = lnodes[k].w;
            depth[k] = lnodes[k].h;
            if rank[k] > max_rank {
                max_rank = rank[k];
            }
        }
        let mut first_dummy = vec![-1i32; m];
        let mut label_item = vec![-1i32; m];
        let mut links = Vec::new();
        {
            let mut next = real.len();
            for k in 0..m {
                let e = &ledges[k];
                let (a, b) = if e.reversed {
                    (e.head, e.tail)
                } else {
                    (e.tail, e.head)
                };
                if direct || span[k] < 2 || plain[k] {
                    links.push((a, b));
                    continue;
                }
                let ra = rank[a];
                let rb = rank[b];
                let mut label_rank: isize = -1;
                if e.label_w > 0.0 {
                    label_rank = (ra + ((rb - ra) >> 1)) as isize;
                    if step == 2 && label_rank & 1 == 0 {
                        label_rank -= 1;
                    }
                }
                first_dummy[k] = next as i32;
                let mut prev = a;
                for r in ra + 1..rb {
                    let d = next;
                    next += 1;
                    rank[d] = r;
                    // The edge crosses from its first lane to its last one evenly along the way.
                    let across = (lane[b] as f64 - lane[a] as f64) * (r - ra) as f64;
                    lane[d] = (lane[a] as f64 + round(across / (rb - ra) as f64)) as usize;
                    dummy[d] = true;
                    if r as isize == label_rank {
                        size[d] = e.label_w;
                        depth[d] = e.label_h;
                        label_item[k] = d as i32;
                    }
                    links.push((prev, d));
                    prev = d;
                }
                links.push((prev, b));
            }
        }

        let mut up_start = vec![0usize; total + 1];
        let mut down_start = vec![0usize; total + 1];
        for &(a, b) in &links {
            down_start[a + 1] += 1;
            up_start[b + 1] += 1;
        }
        for i in 0..total {
            up_start[i + 1] += up_start[i];
            down_start[i + 1] += down_start[i];
        }
        let mut up = vec![0usize; links.len()];
        let mut down = vec![0usize; links.len()];
        {
            let mut uf = up_start[..total].to_vec();
            let mut df = down_start[..total].to_vec();
            for &(a, b) in &links {
                down[df[a]] = b;
                df[a] += 1;
                up[uf[b]] = a;
                uf[b] += 1;
            }
        }

        // Order within each rank: by lane first, then by where the item's neighbours sit.
        let mut rows: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
        for i in 0..total {
            rows[rank[i]].push(i);
        }
        let mut pos = vec![0.0; total];
        let mut key = vec![0.0; total];
        let settle = |row: &mut Vec<usize>, key: &[f64], pos: &mut [f64]| {
            row.sort_by(|&a, &b| {
                lane[a]
                    .cmp(&lane[b])
                    .then(by(key[a] - key[b]))
                    .then(a.cmp(&b))
            });
            for (k, &v) in row.iter().enumerate() {
                pos[v] = k as f64;
            }
        };
        for row in &mut rows {
            for (k, &v) in row.iter().enumerate() {
                key[v] = k as f64;
            }
            settle(row, &key, &mut pos);
        }
        let mut sweep = 0;
        while sweep < SWEEPS && max_rank > 0 {
            let downward = sweep & 1 == 0;
            for s in 1..=max_rank {
                let row = &mut rows[if downward { s } else { max_rank - s }];
                let start = if downward { &up_start } else { &down_start };
                let list = if downward { &up } else { &down };
                for &i in row.iter() {
                    let mut sum = 0.0;
                    let count = start[i + 1] - start[i];
                    for &u in &list[start[i]..start[i + 1]] {
                        sum += pos[u];
                    }
                    key[i] = if count > 0 {
                        sum / count as f64
                    } else {
                        pos[i]
                    };
                }
                settle(row, &key, &mut pos);
            }
            sweep += 1;
        }

        // The items of one rank that share a lane sit side by side as a run.
        let gap = |a: usize, b: usize| {
            if dummy[a] || dummy[b] {
                opt.edge_sep
            } else {
                opt.node_sep
            }
        };
        let run_size = |row: &[usize], from: usize, to: usize| {
            let mut size_of = size[row[from]];
            for k in from..to {
                size_of += gap(row[k], row[k + 1]) + size[row[k + 1]];
            }
            size_of
        };

        // Lane breadth: enough for its fullest rank, and for its title.
        let mut breadth = vec![0.0; lane_count];
        let mut head = if lanes.is_empty() { 0.0 } else { LANE_HEAD };
        for (k, &id) in lanes.iter().enumerate() {
            let lane = &nodes[id];
            breadth[k] = max(0.0, lane.min_w - 2.0 * pad[k]);
            if lane.pad_top > head {
                head = lane.pad_top;
            }
        }
        for row in &rows {
            for (from, to) in runs(row, &lane) {
                let size_of = run_size(row, from, to);
                if size_of > breadth[lane[row[from]]] {
                    breadth[lane[row[from]]] = size_of;
                }
            }
        }
        let mut lane_start = vec![0.0; lane_count + 1];
        for k in 0..lane_count {
            lane_start[k + 1] = lane_start[k] + breadth[k] + 2.0 * pad[k];
        }

        // Across the flow: each lane's items start centered, then line up with their
        // neighbours in the same lane as far as the lane and the items beside them allow.
        let mut c = vec![0.0; total];
        let mut want = vec![0.0; total];
        let center = |c: &mut [f64], row: &[usize], from: usize, to: usize| {
            let l = lane[row[from]];
            let mut at = (lane_start[l] + pad[l] + (lane_start[l + 1] - pad[l])
                - run_size(row, from, to))
                / 2.0;
            for k in from..=to {
                c[row[k]] = at + size[row[k]] / 2.0;
                if k < to {
                    at += size[row[k]] + gap(row[k], row[k + 1]);
                }
            }
        };
        let align = |c: &mut [f64], want: &[f64], row: &[usize], from: usize, to: usize| {
            let l = lane[row[from]];
            let mut floor = lane_start[l] + pad[l];
            for k in from..=to {
                let i = row[k];
                c[i] = max(want[i], floor + size[i] / 2.0);
                if k < to {
                    floor = c[i] + size[i] / 2.0 + gap(i, row[k + 1]);
                }
            }
            let mut ceil = lane_start[l + 1] - pad[l];
            for k in (from..=to).rev() {
                let i = row[k];
                c[i] = min(c[i], ceil - size[i] / 2.0);
                if k > from {
                    ceil = c[i] - size[i] / 2.0 - gap(row[k - 1], i);
                }
            }
        };
        for row in &rows {
            for (from, to) in runs(row, &lane) {
                center(&mut c, row, from, to);
            }
        }
        let mut pass = 0;
        while pass < 4 && max_rank > 0 {
            let downward = pass & 1 == 0;
            for s in 1..=max_rank {
                let row = &rows[if downward { s } else { max_rank - s }];
                let start = if downward { &up_start } else { &down_start };
                let list = if downward { &up } else { &down };
                for &i in row {
                    let mut sum = 0.0;
                    let mut count = 0;
                    for &u in &list[start[i]..start[i + 1]] {
                        if lane[u] != lane[i] {
                            continue;
                        }
                        sum += c[u];
                        count += 1;
                    }
                    want[i] = if count > 0 { sum / count as f64 } else { c[i] };
                }
                for (from, to) in runs(row, &lane) {
                    align(&mut c, &want, row, from, to);
                }
            }
            pass += 1;
        }

        // Along the flow: one band per rank, after the lane titles.
        let mut band_start = vec![0.0; max_rank + 1];
        let mut band_end = vec![0.0; max_rank + 1];
        // The room after a rank is for the relations that turn there or are only as long as
        // they were asked to be. Where just plain handoffs and straight lines pass, the ranks
        // close up, as far as the label of a handoff between them leaves it a line to sit on.
        let step_f = step as f64;
        let mut sep = vec![opt.rank_sep / step_f; max_rank + 1];
        if plain.iter().any(|&on| on) {
            let full = opt.rank_sep / step_f;
            sep.fill(full / CLOSE);
            for k in 0..m {
                let e = &ledges[k];
                let (a, b) = if e.reversed {
                    (e.head, e.tail)
                } else {
                    (e.tail, e.head)
                };
                if plain[k] {
                    if e.label_h <= 0.0 || span[k] > step {
                        continue;
                    }
                    let room = min(
                        full,
                        (e.label_h + 2.0 * TURN - min(depth[a], depth[b]) / 2.0) / span[k] as f64,
                    );
                    for r in rank[a]..rank[b] {
                        if room > sep[r] {
                            sep[r] = room;
                        }
                    }
                    continue;
                }
                let d0 = match usize::try_from(first_dummy[k]) {
                    Ok(d0) if !(span[k] as f64 <= step_f * e.minlen) => d0,
                    _ => {
                        for r in rank[a]..rank[b] {
                            sep[r] = full;
                        }
                        continue;
                    }
                };
                for d in d0 - 1..d0 + span[k] - 1 {
                    let u = if d < d0 { a } else { d };
                    let v = if d + 1 < d0 + span[k] - 1 { d + 1 } else { b };
                    if lane[u] != lane[v] || (c[u] - c[v]).abs() >= 0.5 {
                        sep[rank[u]] = full;
                    }
                }
            }
        }
        let mut at = head + end_pad;
        for r in 0..=max_rank {
            let mut deepest = 0.0;
            for &i in &rows[r] {
                if depth[i] > deepest {
                    deepest = depth[i];
                }
            }
            band_start[r] = at;
            band_end[r] = at + deepest;
            at += deepest + if r == max_rank { 0.0 } else { sep[r] };
        }
        let length = at + end_pad;
        let breadth_total = lane_start[lane_count];
        let flip = |a: f64| if flipped { head + length - a } else { a };
        let px = |c: f64, a: f64| if sideways { flip(a) } else { c };
        let py = |c: f64, a: f64| if sideways { c } else { flip(a) };
        let axis = if sideways { 1.0 } else { 0.0 };

        for k in 0..real.len() {
            let node = &mut nodes[real[k]];
            let a = (band_start[rank[k]] + band_end[rank[k]]) / 2.0;
            node.x = px(c[k], a);
            node.y = py(c[k], a);
        }
        for (k, &id) in lanes.iter().enumerate() {
            let lane = &mut nodes[id];
            let middle = (lane_start[k] + lane_start[k + 1]) / 2.0;
            let size_of = lane_start[k + 1] - lane_start[k];
            lane.x = if sideways { length / 2.0 } else { middle };
            lane.y = if sideways { middle } else { length / 2.0 };
            lane.w = if sideways { length } else { size_of };
            lane.h = if sideways { size_of } else { length };
            lane.pad_top = head;
        }

        // Groups inside a lane are drawn around their members, deepest first.
        let mut depth_of = vec![0usize; n];
        let mut inner = Vec::new();
        for i in 0..n {
            if !nodes[i].is_group || lane_slot[i] >= 0 || item_of[i] >= 0 {
                continue;
            }
            let mut d = 0;
            let mut p = nodes[i].parent;
            while p >= 0 && d < MAX_HOPS {
                d += 1;
                p = nodes[p as usize].parent;
            }
            depth_of[i] = d;
            inner.push(i);
        }
        inner.sort_by(|&a, &b| depth_of[b].cmp(&depth_of[a]).then(a.cmp(&b)));
        let mut boxes = vec![
            [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY
            ];
            n
        ];
        let grow = |boxes: &mut [[f64; 4]], g: usize, c: &CNode| {
            let b = &mut boxes[g];
            if c.x - c.w / 2.0 < b[0] {
                b[0] = c.x - c.w / 2.0;
            }
            if c.y - c.h / 2.0 < b[1] {
                b[1] = c.y - c.h / 2.0;
            }
            if c.x + c.w / 2.0 > b[2] {
                b[2] = c.x + c.w / 2.0;
            }
            if c.y + c.h / 2.0 > b[3] {
                b[3] = c.y + c.h / 2.0;
            }
        };
        let in_lane_group = |p: i32| p >= 0 && lane_slot[p as usize] < 0;
        for &i in &real {
            if in_lane_group(nodes[i].parent) {
                grow(&mut boxes, nodes[i].parent as usize, &nodes[i]);
            }
        }
        for &g in &inner {
            let b = boxes[g];
            if b[0] == f64::INFINITY {
                continue;
            }
            let c = &mut nodes[g];
            let w = max(b[2] - b[0] + 2.0 * c.pad_x, c.min_w);
            c.x = (b[0] + b[2]) / 2.0;
            c.w = w;
            c.y = (b[1] - c.pad_top + b[3] + c.pad_bottom) / 2.0;
            c.h = b[3] - b[1] + c.pad_top + c.pad_bottom;
            if in_lane_group(c.parent) {
                let parent = c.parent as usize;
                grow(&mut boxes, parent, &nodes[g]);
            }
        }

        for e in edges.iter_mut() {
            e.route = Vec::new();
        }
        for k in 0..m {
            let e = &ledges[k];
            let (a, b) = if e.reversed {
                (e.head, e.tail)
            } else {
                (e.tail, e.head)
            };
            let mut pts = vec![
                c[a],
                band_end[rank[a]] - (band_end[rank[a]] - band_start[rank[a]] - depth[a]) / 2.0,
            ];
            if let Ok(d0) = usize::try_from(first_dummy[k]) {
                for d in d0..d0 + span[k] - 1 {
                    pts.extend([c[d], band_start[rank[d]]]);
                    if band_end[rank[d]] > band_start[rank[d]] {
                        pts.extend([c[d], band_end[rank[d]]]);
                    }
                }
            }
            pts.extend([
                c[b],
                band_start[rank[b]] + (band_end[rank[b]] - band_start[rank[b]] - depth[b]) / 2.0,
            ]);
            let mut route = Vec::new();
            for p in pts.chunks(2) {
                route.extend([px(p[0], p[1]), py(p[0], p[1]), axis]);
            }
            if e.reversed {
                let points = route.len() / 3;
                for i in 0..points / 2 {
                    let j = points - 1 - i;
                    route.swap(3 * i, 3 * j);
                    route.swap(3 * i + 1, 3 * j + 1);
                }
            }
            let out = &mut edges[edge_of[k]];
            if let Ok(label) = usize::try_from(label_item[k]) {
                let mid = (band_start[rank[label]] + band_end[rank[label]]) / 2.0;
                out.label_x = px(c[label], mid);
                out.label_y = py(c[label], mid);
            } else {
                out.label_x = (route[0] + route[route.len() - 3]) / 2.0;
                out.label_y = (route[1] + route[route.len() - 2]) / 2.0;
            }
            out.route = route;
        }

        // An edge that starts or ends at a lane or a group runs straight to its border.
        for e in edges.iter_mut() {
            if !e.route.is_empty() || e.src == e.dst {
                continue;
            }
            let a = &nodes[e.src];
            let b = &nodes[e.dst];
            let (ax, ay) = border(a, b);
            let (bx, by) = border(b, a);
            let flat = if (bx - ax).abs() > (by - ay).abs() {
                1.0
            } else {
                0.0
            };
            e.route = vec![ax, ay, flat, bx, by, flat];
            e.label_x = (ax + bx) / 2.0;
            e.label_y = (ay + by) / 2.0;
        }

        // A handoff can leave towards the other lane, then arrive along the flow (or vice
        // versa). In particular, a decision's return branch must not leave through its incoming
        // side and retrace the path past the preceding step. Try the two single-corner routes,
        // keeping the ranked route when a node or another label blocks them.
        let mut routed = vec![false; m];
        let route_points: usize = edges.iter().map(|edge| edge.route.len() / 3).sum();
        let work = m * (real.len() + edges.len() + route_points);
        if work <= ROUTE_BUDGET {
            let amid = |i: usize| (band_start[rank[i]] + band_end[rank[i]]) / 2.0;
            let mut labels: Vec<[f64; 2]> = edges
                .iter()
                .map(|e| {
                    if sideways {
                        [e.label_y, flip(e.label_x)]
                    } else {
                        [e.label_x, flip(e.label_y)]
                    }
                })
                .collect();
            let overlaps =
                |c0: f64, a0: f64, c1: f64, a1: f64, lc: f64, la: f64, cw: f64, ah: f64| {
                    c1 > lc - cw / 2.0
                        && c0 < lc + cw / 2.0
                        && a1 > la - ah / 2.0
                        && a0 < la + ah / 2.0
                };
            let clear = |edges: &[CEdge],
                         labels: &[[f64; 2]],
                         (c0, a0, c1, a1): (f64, f64, f64, f64),
                         ends: Option<(usize, usize)>,
                         ei: usize| {
                for i in 0..real.len() {
                    let end = ends.is_some_and(|(src, dst)| i == src || i == dst);
                    if !end
                        && overlaps(c0, a0, c1, a1, c[i], amid(i), size[i] + 8.0, depth[i] + 8.0)
                    {
                        return false;
                    }
                }
                for (i, e) in edges.iter().enumerate() {
                    // A step's own loop keeps its label inside the step's box, and has no place
                    // here.
                    if i != ei
                        && e.label_w > 0.0
                        && e.src != e.dst
                        && overlaps(
                            c0,
                            a0,
                            c1,
                            a1,
                            labels[i][0],
                            labels[i][1],
                            if sideways { e.label_h } else { e.label_w },
                            if sideways { e.label_w } else { e.label_h },
                        )
                    {
                        return false;
                    }
                }
                true
            };
            // How many other relations have a stretch inside a box. A stretch's whole box
            // counts, including its curve.
            let crossings = |edges: &[CEdge], x0: f64, y0: f64, x1: f64, y1: f64, ei: usize| {
                let mut count = 0;
                for (i, edge) in edges.iter().enumerate() {
                    if i == ei {
                        continue;
                    }
                    let route = &edge.route;
                    for j in (3..route.len()).step_by(3) {
                        if max(route[j - 3], route[j]) > x0
                            && min(route[j - 3], route[j]) < x1
                            && max(route[j - 2], route[j + 1]) > y0
                            && min(route[j - 2], route[j + 1]) < y1
                        {
                            count += 1;
                            break;
                        }
                    }
                }
                count
            };
            let meets = |edges: &[CEdge], x: f64, y: f64, ei: usize| {
                let mut count = 0;
                for (i, edge) in edges.iter().enumerate() {
                    let route = &edge.route;
                    if i == ei || route.is_empty() {
                        continue;
                    }
                    let last = route.len() - 3;
                    if (route[0] - x).abs() < 1.0 && (route[1] - y).abs() < 1.0
                        || (route[last] - x).abs() < 1.0 && (route[last + 1] - y).abs() < 1.0
                    {
                        count += 1;
                    }
                }
                count
            };
            // Two relations drawn along one line read as a single path. Those that leave the
            // same step or arrive at the same step may share a trunk, and any other pair must
            // not.
            let retraces = |edges: &[CEdge], x0: f64, y0: f64, x1: f64, y1: f64, ei: usize| {
                let upright = (x1 - x0).abs() < 0.5;
                let lo = if upright { min(y0, y1) } else { min(x0, x1) };
                let hi = if upright { max(y0, y1) } else { max(x0, x1) };
                let line = if upright { x0 } else { y0 };
                let (across, along) = if upright { (0, 1) } else { (1, 0) };
                for (i, other) in edges.iter().enumerate() {
                    if i == ei || other.src == edges[ei].src || other.dst == edges[ei].dst {
                        continue;
                    }
                    let route = &other.route;
                    for j in (3..route.len()).step_by(3) {
                        let u0 = route[j - 3 + across];
                        let u1 = route[j + across];
                        if (u0 - line).abs() >= NEAR || (u1 - line).abs() >= NEAR {
                            continue;
                        }
                        let v0 = route[j - 3 + along];
                        let v1 = route[j + along];
                        if min(hi, max(v0, v1)) - max(lo, min(v0, v1)) > 1.0 {
                            return true;
                        }
                    }
                }
                false
            };
            for k in 0..m {
                let e = &ledges[k];
                let ei = edge_of[k];
                let a = e.tail;
                let b = e.head;
                if lane[a] == lane[b] || (c[b] - c[a]).abs() < 1.0 {
                    continue;
                }
                let ac = c[a];
                let bc = c[b];
                let aa = amid(a);
                let ba = amid(b);
                let dc = sign(bc - ac);
                let da = sign(ba - aa);
                let attempt =
                    |edges: &mut [CEdge], labels: &mut [[f64; 2]], pts: &[f64], margin: f64| {
                        for p in (2..pts.len()).step_by(2) {
                            let bounds = (
                                min(pts[p - 2], pts[p]) - margin,
                                min(pts[p - 1], pts[p + 1]) - margin,
                                max(pts[p - 2], pts[p]) + margin,
                                max(pts[p - 1], pts[p + 1]) + margin,
                            );
                            if !clear(edges, labels, bounds, Some((a, b)), ei) {
                                return false;
                            }
                            let (x0, y0) = (px(pts[p - 2], pts[p - 1]), py(pts[p - 2], pts[p - 1]));
                            let (x1, y1) = (px(pts[p], pts[p + 1]), py(pts[p], pts[p + 1]));
                            if retraces(edges, x0, y0, x1, y1, ei) {
                                return false;
                            }
                        }
                        let out_label_w = edges[ei].label_w;
                        let out_label_h = edges[ei].label_h;
                        let mut lc = 0.0;
                        let mut la = 0.0;
                        let mut longest = -1.0;
                        if out_label_w > 0.0 {
                            let cw = e.label_w;
                            let ah = e.label_h;
                            for p in (2..pts.len()).step_by(2) {
                                let cc = (pts[p - 2] + pts[p]) / 2.0;
                                let ca = (pts[p - 1] + pts[p + 1]) / 2.0;
                                let across = (pts[p] - pts[p - 2]).abs();
                                let along = (pts[p + 1] - pts[p - 1]).abs();
                                let run = across + along;
                                let short = if across > 0.0 {
                                    across < cw + 8.0
                                } else {
                                    along < ah + 8.0
                                };
                                if run <= longest || short {
                                    continue;
                                }
                                if cc - cw / 2.0 < 0.0
                                    || cc + cw / 2.0 > breadth_total
                                    || ca - ah / 2.0 < 0.0
                                    || ca + ah / 2.0 > length
                                {
                                    continue;
                                }
                                // Include the ends here: the label must clear both shapes it
                                // connects.
                                let bounds =
                                    (cc - cw / 2.0, ca - ah / 2.0, cc + cw / 2.0, ca + ah / 2.0);
                                if !clear(edges, labels, bounds, None, ei) {
                                    continue;
                                }
                                // A label moved onto a handoff must not cover a return path that
                                // keeps its ranked route. A segment's whole box is kept clear,
                                // including its curve.
                                let lx = px(cc, ca);
                                let ly = py(cc, ca);
                                if crossings(
                                    edges,
                                    lx - out_label_w / 2.0,
                                    ly - out_label_h / 2.0,
                                    lx + out_label_w / 2.0,
                                    ly + out_label_h / 2.0,
                                    ei,
                                ) > 0
                                {
                                    continue;
                                }
                                lc = cc;
                                la = ca;
                                longest = run;
                            }
                            if longest < 0.0 {
                                return false;
                            }
                        }
                        // A point short of each corner and one past it keep the turn close to the
                        // corner, so that the line stays under its label and the handoff takes
                        // little room.
                        let mut way = vec![pts[0], pts[1]];
                        for p in (2..pts.len()).step_by(2) {
                            let dx = pts[p] - pts[p - 2];
                            let dy = pts[p + 1] - pts[p - 1];
                            let run = dx.abs() + dy.abs();
                            let first = if p > 2 { TURN } else { 0.0 };
                            let last = if p + 2 < pts.len() { TURN } else { 0.0 };
                            if run > first + last + 1.0 {
                                if first != 0.0 {
                                    way.extend([
                                        pts[p - 2] + (dx / run) * first,
                                        pts[p - 1] + (dy / run) * first,
                                    ]);
                                }
                                if last != 0.0 {
                                    way.extend([
                                        pts[p] - (dx / run) * last,
                                        pts[p + 1] - (dy / run) * last,
                                    ]);
                                }
                            }
                            way.extend([pts[p], pts[p + 1]]);
                        }
                        let mut route = Vec::new();
                        for p in (0..way.len()).step_by(2) {
                            let crosswise = if p == 0 {
                                way[0] != way[2]
                            } else {
                                way[p] != way[p - 2]
                            };
                            route.extend([
                                px(way[p], way[p + 1]),
                                py(way[p], way[p + 1]),
                                if crosswise { 1.0 - axis } else { axis },
                            ]);
                        }
                        let out = &mut edges[ei];
                        out.route = route;
                        if out.label_w > 0.0 {
                            out.label_x = px(lc, la);
                            out.label_y = py(lc, la);
                            labels[ei] = [lc, la];
                        }
                        true
                    };
                let corner = [
                    ac + dc * size[a] / 2.0,
                    aa,
                    bc,
                    aa,
                    bc,
                    ba - da * depth[b] / 2.0,
                ];
                if attempt(edges, &mut labels, &corner, 2.0) {
                    routed[k] = true;
                    continue;
                }
                let corner = [
                    ac,
                    aa + da * depth[a] / 2.0,
                    ac,
                    ba,
                    bc - dc * size[b] / 2.0,
                    ba,
                ];
                if attempt(edges, &mut labels, &corner, 2.0) {
                    routed[k] = true;
                    continue;
                }
                // With both corners blocked, as for a return past the steps between, the
                // handoff goes around: out to the margin of a lane, along it, and back in level
                // with its end. It takes the margin that crosses the fewest other relations and
                // shares the fewest ends with them, then the shortest way.
                if work > ROUTE_BUDGET / RELAY {
                    continue;
                }
                let mut around: Vec<(Vec<f64>, f64, usize)> = Vec::new();
                let pair = if lane[a] < lane[b] {
                    [lane[b], lane[a]]
                } else {
                    [lane[a], lane[b]]
                };
                for l in pair {
                    // A lane that keeps no margin yet has less room there, and the handoff stays
                    // inside it.
                    let inset = max(pad[l] - LEG, min(pad[l] / 2.0, MARGIN - LEG));
                    for ch in [lane_start[l] + inset, lane_start[l + 1] - inset] {
                        if (ch - ac).abs() < size[a] / 2.0 + 4.0
                            || (ch - bc).abs() < size[b] / 2.0 + 4.0
                        {
                            continue;
                        }
                        let pts = vec![
                            ac + sign(ch - ac) * size[a] / 2.0,
                            aa,
                            ch,
                            aa,
                            ch,
                            ba,
                            bc + sign(ch - bc) * size[b] / 2.0,
                            ba,
                        ];
                        let meet = meets(edges, px(pts[0], pts[1]), py(pts[0], pts[1]), ei)
                            + meets(edges, px(pts[6], pts[7]), py(pts[6], pts[7]), ei);
                        let mut cost = meet as f64 * 1e6;
                        for p in (2..pts.len()).step_by(2) {
                            let x0 = px(pts[p - 2], pts[p - 1]);
                            let y0 = py(pts[p - 2], pts[p - 1]);
                            let x1 = px(pts[p], pts[p + 1]);
                            let y1 = py(pts[p], pts[p + 1]);
                            let crossed = crossings(
                                edges,
                                min(x0, x1),
                                min(y0, y1),
                                max(x0, x1),
                                max(y0, y1),
                                ei,
                            );
                            cost += crossed as f64 * 1e6 + (x1 - x0).abs() + (y1 - y0).abs();
                        }
                        around.push((pts, cost, l));
                    }
                }
                around.sort_by(|p, q| by(p.1 - q.1));
                for (pts, _, l) in &around {
                    if !attempt(edges, &mut labels, pts, 1.0) {
                        continue;
                    }
                    routed[k] = true;
                    used[*l] = true;
                    break;
                }
            }
        }

        Placed {
            width: if sideways { length } else { breadth_total },
            height: if sideways { breadth_total } else { length },
            routed,
            margins: used,
            work,
        }
    };

    let mut plain = vec![false; m];
    let mut margins = vec![false; lane_count];
    let mut placed = place(nodes, edges, &plain, &margins);
    // A handoff that was given a simple route needs no room kept for it through the ranks it
    // passes, so the lanes are laid out again without it. A handoff that then loses its route
    // goes back to the ranked one. Routing is most of the work, so a large graph is laid out
    // once.
    let mut round_ = 0;
    while round_ < ROUNDS && placed.work <= ROUTE_BUDGET / RELAY {
        let next: Vec<bool> = if round_ == 0 {
            placed.routed.clone()
        } else {
            plain
                .iter()
                .zip(&placed.routed)
                .map(|(&on, &routed)| on && routed)
                .collect()
        };
        if next == plain && placed.margins == margins {
            break;
        }
        plain = next;
        margins = placed.margins.clone();
        placed = place(nodes, edges, &plain, &margins);
        round_ += 1;
    }
    if plain
        .iter()
        .zip(&placed.routed)
        .any(|(&on, &routed)| on && !routed)
    {
        placed = place(nodes, edges, &vec![false; m], &vec![false; lane_count]);
    }
    CompoundResult {
        width: placed.width,
        height: placed.height,
    }
}

/// The point where the line from one box's center to another's leaves the first box, as Pele's
/// border finds it. When the other center is inside the box, the point is on the nearest side
/// instead.
fn border(from: &CNode, to: &CNode) -> (f64, f64) {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let hw = max(from.w / 2.0, 0.5);
    let hh = max(from.h / 2.0, 0.5);
    if dx.abs() < hw && dy.abs() < hh {
        let to_side = hw - dx.abs();
        let to_end = hh - dy.abs();
        return if to_side < to_end {
            (from.x + if dx < 0.0 { -hw } else { hw }, to.y)
        } else {
            (to.x, from.y + if dy < 0.0 { -hh } else { hh })
        };
    }
    let scale = min(
        if dx != 0.0 {
            hw / dx.abs()
        } else {
            f64::INFINITY
        },
        if dy != 0.0 {
            hh / dy.abs()
        } else {
            f64::INFINITY
        },
    );
    (from.x + dx * scale, from.y + dy * scale)
}
