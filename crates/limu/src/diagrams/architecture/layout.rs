//! Grid placement, as Pele's layout.ts has it. Mermaid hands the edge sides to a
//! force-directed solver as relative placement constraints. Pele applies them directly: every
//! service and junction takes a cell, an edge `a:R -- L:b` puts b in the cell to the right of
//! a, and a group is a solid rectangle of cells, so a group never encloses a node that is not
//! in it. Each container, the diagram and then every group, is laid out on its own, innermost
//! first, and a finished group is placed in its parent as one block.

use std::collections::HashMap;

use super::db::{ArchEdge, ArchGroup, ArchLayoutHint, ArchNode};

/// Where placeOnGrid puts things, Pele's Grid: the cell of each node, in the order of the
/// nodes passed in, and the cell rectangle and parent of each group. A group with nothing in
/// it takes one cell. A parent of none is the diagram.
pub(crate) struct Grid {
    pub(crate) col: Vec<i64>,
    pub(crate) row: Vec<i64>,
    pub(crate) group_col: Vec<i64>,
    pub(crate) group_row: Vec<i64>,
    pub(crate) group_cols: Vec<i64>,
    pub(crate) group_rows: Vec<i64>,
    pub(crate) group_parent: Vec<Option<usize>>,
    pub(crate) group_empty: Vec<bool>,
    pub(crate) node_parent: Vec<Option<usize>>,
}

pub(crate) const DX: [i64; 4] = [-1, 1, 0, 0];
pub(crate) const DY: [i64; 4] = [0, 0, -1, 1];

/// The index of a side in DX and DY, as Pele's sideIndex gives it.
pub(crate) fn side_index(side: char) -> usize {
    match side {
        'L' => 0,
        'R' => 1,
        'T' => 2,
        _ => 3,
    }
}

/// The most cells a block found through the cell map holds. Larger ones are compared one by
/// one.
const SMALL: i64 = 64;

/// The links between items, each asking for one node to sit beside another: the two nodes,
/// the sides of each it joins, and the children of the container the link belongs to that
/// the nodes are in.
#[derive(Default)]
struct Links {
    a: Vec<usize>,
    b: Vec<usize>,
    side_a: Vec<usize>,
    side_b: Vec<usize>,
    item_a: Vec<usize>,
    item_b: Vec<usize>,
}

/// What placeOnGrid works with while it places one container: where each item is in its
/// container and its size in cells, the items placed, and the cells they take.
struct Placer<'a> {
    n: usize,
    parent: &'a [usize],
    links: &'a Links,
    edge_links: &'a [Vec<usize>],
    x: Vec<i64>,
    y: Vec<i64>,
    w: Vec<i64>,
    h: Vec<i64>,
    placed: Vec<bool>,
    order: Vec<usize>,
    cells: HashMap<(i64, i64), usize>,
    large: Vec<usize>,
    reach: HashMap<[i64; 6], i64>,
    first: usize,
}

impl Placer<'_> {
    /// Where a node is in an item that holds it, as Pele's anchor adds it up.
    fn anchor(&self, node: usize, item: usize) -> (i64, i64) {
        let (mut ax, mut ay) = (0, 0);
        let mut cur = node;
        while cur != item {
            ax += self.x[cur];
            ay += self.y[cur];
            cur = self.n + self.parent[cur];
        }
        (ax, ay)
    }

    fn put(&mut self, item: usize, px: i64, py: i64) {
        self.x[item] = px;
        self.y[item] = py;
        self.placed[item] = true;
        self.order.push(item);
        if self.w[item] * self.h[item] > SMALL {
            self.large.push(item);
            return;
        }
        for cy in py..py + self.h[item] {
            for cx in px..px + self.w[item] {
                self.cells.insert((cx, cy), item);
            }
        }
    }

    fn overlaps(&self, item: usize, rx: i64, ry: i64, rw: i64, rh: i64) -> bool {
        self.x[item] < rx + rw
            && rx < self.x[item] + self.w[item]
            && self.y[item] < ry + rh
            && ry < self.y[item] + self.h[item]
    }

    fn collide(&self, rx: i64, ry: i64, rw: i64, rh: i64) -> Option<usize> {
        if rw * rh > SMALL {
            return self.order[self.first..]
                .iter()
                .copied()
                .find(|&item| self.overlaps(item, rx, ry, rw, rh));
        }
        for cy in ry..ry + rh {
            for cx in rx..rx + rw {
                if let Some(&hit) = self.cells.get(&(cx, cy)) {
                    return Some(hit);
                }
            }
        }
        self.large
            .iter()
            .copied()
            .find(|&item| self.overlaps(item, rx, ry, rw, rh))
    }

    /// How far a block must slide along (sx, sy) to be clear. Each step jumps past what it
    /// hit, and the result is kept, because a later block starting from the same place cannot
    /// stop sooner.
    fn slide(&mut self, rx: i64, ry: i64, rw: i64, rh: i64, sx: i64, sy: i64) -> i64 {
        let key = [rx, ry, rw, rh, sx, sy];
        let mut k = self.reach.get(&key).copied().unwrap_or(0);
        while let Some(hit) = self.collide(rx + k * sx, ry + k * sy, rw, rh) {
            k = if sx > 0 {
                self.x[hit] + self.w[hit] - rx
            } else if sx < 0 {
                rx + rw - self.x[hit]
            } else if sy > 0 {
                self.y[hit] + self.h[hit] - ry
            } else {
                ry + rh - self.y[hit]
            };
        }
        self.reach.insert(key, k);
        k
    }

    fn follow(&mut self, from: usize, k: usize) {
        let l = self.links;
        let forward = l.item_a[k] == from;
        let to = if forward { l.item_b[k] } else { l.item_a[k] };
        if self.placed[to] {
            return;
        }
        let sp = if forward { l.side_a[k] } else { l.side_b[k] };
        let sq = if forward { l.side_b[k] } else { l.side_a[k] };
        let (pax, pay) = self.anchor(if forward { l.a[k] } else { l.b[k] }, from);
        let (ax, ay) = self.anchor(if forward { l.b[k] } else { l.a[k] }, to);
        let mut dx = DX[sp];
        let mut dy = DY[sp];
        let (mut shift_x, mut shift_y) = (0, 0);
        let facing = DX[sp] + DX[sq] == 0 && DY[sp] + DY[sq] == 0;
        if sp == sq {
            // Like sides: the edge loops around, so the nodes stack across the direction it
            // leaves in.
            dx = if DX[sp] == 0 { 1 } else { 0 };
            dy = 1 - dx;
        } else if !facing {
            // A bend: one step along the side the edge leaves, one step back from the side it
            // enters.
            shift_x = -DX[sq];
            shift_y = -DY[sq];
        }
        let (mut bx, mut by);
        if dx != 0 {
            bx = if dx > 0 {
                self.x[from] + self.w[from]
            } else {
                self.x[from] - self.w[to]
            };
            by = self.y[from] + pay + shift_y - ay;
        } else {
            by = if dy > 0 {
                self.y[from] + self.h[from]
            } else {
                self.y[from] - self.h[to]
            };
            bx = self.x[from] + pax + shift_x - ax;
        }
        if facing {
            // When several placed nodes reach this one through the same pair of sides, line it
            // up with the middle one, not with whichever happened to be visited first.
            let list = &self.edge_links[to];
            if list.len() > 1 {
                let mut wanted = Vec::new();
                for &j in list {
                    let to_is_b = l.item_b[j] == to;
                    let other = if to_is_b { l.item_a[j] } else { l.item_b[j] };
                    let (so, st) = if to_is_b {
                        (l.side_a[j], l.side_b[j])
                    } else {
                        (l.side_b[j], l.side_a[j])
                    };
                    if !self.placed[other] || so != sp || st != sq {
                        continue;
                    }
                    let (ox, oy) = self.anchor(if to_is_b { l.a[j] } else { l.b[j] }, other);
                    let at = if dx != 0 {
                        self.y[other] + oy
                    } else {
                        self.x[other] + ox
                    };
                    let (tx, ty) = self.anchor(if to_is_b { l.b[j] } else { l.a[j] }, to);
                    wanted.push(at - if dx != 0 { ty } else { tx });
                }
                if wanted.len() > 1 {
                    wanted.sort();
                    let middle = wanted[(wanted.len() - 1) >> 1];
                    if dx != 0 {
                        by = middle;
                    } else {
                        bx = middle;
                    }
                }
            }
            // Taken: move sideways, to whichever side is nearer, so the edge can still leave
            // straight.
            let px = if dx == 0 { 1 } else { 0 };
            let py = 1 - px;
            let (tw, th) = (self.w[to], self.h[to]);
            let forth = self.slide(bx, by, tw, th, px, py);
            let back = if forth == 0 {
                0
            } else {
                self.slide(bx, by, tw, th, -px, -py)
            };
            let near = if forth <= back { forth } else { -back };
            bx += px * near;
            by += py * near;
        } else {
            let far = self.slide(bx, by, self.w[to], self.h[to], dx, dy);
            bx += dx * far;
            by += dy * far;
        }
        self.put(to, bx, by);
    }

    fn layout(&mut self, items: &[usize], hint_links: &[Vec<usize>], group: Option<usize>) {
        if items.is_empty() {
            return;
        }
        self.order.clear();
        let mut starts = Vec::new();
        for &start in items {
            if self.placed[start] {
                continue;
            }
            self.first = self.order.len();
            starts.push(self.first);
            self.cells.clear();
            self.large.clear();
            self.reach.clear();
            self.put(start, 0, 0);
            // Declared alignments are followed as soon as a node is placed, ahead of any edge.
            let (mut hint_at, mut edge_at, mut link_at) = (self.first, self.first, 0);
            loop {
                if hint_at < self.order.len() {
                    let from = self.order[hint_at];
                    hint_at += 1;
                    for &k in &hint_links[from] {
                        self.follow(from, k);
                    }
                    continue;
                }
                if edge_at >= self.order.len() {
                    break;
                }
                let from = self.order[edge_at];
                let list = &self.edge_links[from];
                if link_at >= list.len() {
                    edge_at += 1;
                    link_at = 0;
                    continue;
                }
                let k = list[link_at];
                link_at += 1;
                self.follow(from, k);
            }
        }
        starts.push(self.order.len());

        // Unconnected parts go side by side, wrapping into rows that keep the whole roughly
        // square.
        let count = starts.len() - 1;
        let mut min_x = vec![0; count];
        let mut min_y = vec![0; count];
        let mut part_w = vec![0; count];
        let mut part_h = vec![0; count];
        let (mut area, mut widest) = (0, 0);
        for p in 0..count {
            let (mut x0, mut y0, mut x1, mut y1) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
            for &item in &self.order[starts[p]..starts[p + 1]] {
                x0 = x0.min(self.x[item]);
                y0 = y0.min(self.y[item]);
                x1 = x1.max(self.x[item] + self.w[item]);
                y1 = y1.max(self.y[item] + self.h[item]);
            }
            min_x[p] = x0;
            min_y[p] = y0;
            part_w[p] = x1 - x0;
            part_h[p] = y1 - y0;
            area += part_w[p] * part_h[p];
            widest = widest.max(part_w[p]);
        }
        let limit = widest.max(((area * 2) as f64).sqrt().ceil() as i64);
        let (mut cx, mut cy, mut shelf, mut width) = (0, 0, 0, 0);
        for p in 0..count {
            if cx > 0 && cx + part_w[p] > limit {
                cy += shelf;
                cx = 0;
                shelf = 0;
            }
            for i in starts[p]..starts[p + 1] {
                let item = self.order[i];
                self.x[item] += cx - min_x[p];
                self.y[item] += cy - min_y[p];
            }
            cx += part_w[p];
            shelf = shelf.max(part_h[p]);
            width = width.max(cx);
        }
        if let Some(g) = group {
            self.w[self.n + g] = width;
            self.h[self.n + g] = cy + shelf;
        }
    }
}

/// Places the nodes and groups on a grid as the edges and the align directives ask, as Pele's
/// placeOnGrid does.
pub(crate) fn place_on_grid(
    nodes: &[&ArchNode],
    groups: &[&ArchGroup],
    edges: &[ArchEdge],
    hints: &[ArchLayoutHint],
) -> Grid {
    let n = nodes.len();
    let g_count = groups.len();
    let root = g_count;
    let mut node_index: HashMap<&str, usize> = HashMap::new();
    let mut group_index: HashMap<&str, usize> = HashMap::new();
    // Items are nodes (0..n) then groups (n..n + g_count). Containers are groups
    // (0..g_count) then the root.
    let mut parent = vec![root; n + g_count];
    let mut depth = vec![0usize; g_count + 1];
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); g_count + 1];
    for (g, group) in groups.iter().enumerate() {
        group_index.insert(&group.id, g);
        match group.parent.as_deref().and_then(|p| group_index.get(p)) {
            Some(&p) if p != g => {
                parent[n + g] = p;
                depth[g] = depth[p] + 1;
            }
            _ => depth[g] = 1,
        }
    }
    for (i, node) in nodes.iter().enumerate() {
        node_index.insert(&node.id, i);
        if let Some(&p) = node.parent.as_deref().and_then(|p| group_index.get(p)) {
            parent[i] = p;
        }
        kids[parent[i]].push(i);
    }
    for g in 0..g_count {
        kids[parent[n + g]].push(n + g);
    }

    // A link asks for one node to sit beside another. It belongs to the innermost container
    // that holds both ends, and there it relates the two children of that container the ends
    // are in.
    let mut links = Links::default();
    let mut hint_links: Vec<Vec<usize>> = vec![Vec::new(); n + g_count];
    let mut edge_links: Vec<Vec<usize>> = vec![Vec::new(); n + g_count];
    let mut add_link = |a: usize, b: usize, sa: usize, sb: usize, lists: &mut [Vec<usize>]| {
        if a == b {
            return;
        }
        let (mut ia, mut ib) = (a, b);
        let (mut ca, mut cb) = (parent[a], parent[b]);
        while ca != cb {
            if depth[ca] >= depth[cb] {
                ia = n + ca;
                ca = parent[ia];
            } else {
                ib = n + cb;
                cb = parent[ib];
            }
        }
        let k = links.a.len();
        links.a.push(a);
        links.b.push(b);
        links.side_a.push(sa);
        links.side_b.push(sb);
        links.item_a.push(ia);
        links.item_b.push(ib);
        lists[ia].push(k);
        lists[ib].push(k);
    };
    for hint in hints {
        let row = hint.direction == "row";
        for pair in hint.members.windows(2) {
            if let (Some(&a), Some(&b)) = (
                node_index.get(pair[0].as_str()),
                node_index.get(pair[1].as_str()),
            ) {
                let (sa, sb) = if row { (1, 0) } else { (3, 2) };
                add_link(a, b, sa, sb, &mut hint_links);
            }
        }
    }
    // An edge between two like sides says least about where its ends go, so those come last.
    for like_sides in [false, true] {
        for edge in edges {
            if (edge.lhs_dir == edge.rhs_dir) != like_sides {
                continue;
            }
            if let (Some(&a), Some(&b)) = (
                node_index.get(edge.lhs_id.as_str()),
                node_index.get(edge.rhs_id.as_str()),
            ) {
                let (sa, sb) = (side_index(edge.lhs_dir), side_index(edge.rhs_dir));
                add_link(a, b, sa, sb, &mut edge_links);
            }
        }
    }

    let mut placer = Placer {
        n,
        parent: &parent,
        links: &links,
        edge_links: &edge_links,
        x: vec![0; n + g_count],
        y: vec![0; n + g_count],
        w: vec![1; n + g_count],
        h: vec![1; n + g_count],
        placed: vec![false; n + g_count],
        order: Vec::new(),
        cells: HashMap::new(),
        large: Vec::new(),
        reach: HashMap::new(),
        first: 0,
    };
    for c in (0..g_count).rev() {
        placer.layout(&kids[c], &hint_links, Some(c));
    }
    placer.layout(&kids[root], &hint_links, None);

    let (x, y) = (&placer.x, &placer.y);
    let mut grid = Grid {
        col: vec![0; n],
        row: vec![0; n],
        group_col: vec![0; g_count],
        group_row: vec![0; g_count],
        group_cols: placer.w[n..].to_vec(),
        group_rows: placer.h[n..].to_vec(),
        group_parent: vec![None; g_count],
        group_empty: vec![false; g_count],
        node_parent: vec![None; n],
    };
    for g in 0..g_count {
        let p = parent[n + g];
        grid.group_col[g] = x[n + g];
        grid.group_row[g] = y[n + g];
        if p != root {
            grid.group_parent[g] = Some(p);
            grid.group_col[g] += grid.group_col[p];
            grid.group_row[g] += grid.group_row[p];
        }
        grid.group_empty[g] = kids[g].is_empty();
    }
    for i in 0..n {
        let p = parent[i];
        grid.col[i] = x[i];
        grid.row[i] = y[i];
        if p != root {
            grid.node_parent[i] = Some(p);
            grid.col[i] += grid.group_col[p];
            grid.row[i] += grid.group_row[p];
        }
    }
    grid
}
