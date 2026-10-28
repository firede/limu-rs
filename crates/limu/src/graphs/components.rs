//! The wrapping of a laid-out graph's disconnected parts into rows, as Pele's
//! src/layout/components.ts wraps them.

use super::compound::{CEdge, CNode, CompoundResult};
use crate::js::math::{max, min};

/// A disconnected part of a graph and its bounds, as Pele's Part is: its nodes and edges, by
/// their indices.
struct Part {
    nodes: Vec<usize>,
    edges: Vec<usize>,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl Part {
    /// Grows the bounds to take in a box centered at x, y, as the include of Pele's
    /// wrapComponents does.
    fn include(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.left = min(self.left, x - w / 2.0);
        self.top = min(self.top, y - h / 2.0);
        self.right = max(self.right, x + w / 2.0);
        self.bottom = max(self.bottom, y + h / 2.0);
    }
}

/// Moves disconnected parts into rows no wider than `width`, `gap` apart, without changing the
/// layout inside a part, as Pele's wrapComponents does. Gives the new size, or none where there
/// is one part or wrapping would make the layout no narrower.
pub(crate) fn wrap_components(
    nodes: &mut [CNode],
    edges: &mut [CEdge],
    width: f64,
    gap: f64,
) -> Option<CompoundResult> {
    let mut parent: Vec<usize> = (0..nodes.len()).collect();
    let root = |parent: &mut [usize], mut i: usize| {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    };
    let join = |parent: &mut [usize], a: usize, b: usize| {
        let ra = root(parent, a);
        parent[ra] = root(parent, b);
    };
    for i in 0..nodes.len() {
        if nodes[i].parent >= 0 {
            join(&mut parent, i, nodes[i].parent as usize);
        }
    }
    for e in edges.iter() {
        join(&mut parent, e.src, e.dst);
    }

    // Pele keeps the parts in a Map, which goes over them in the order they were first put in.
    let mut parts: Vec<Part> = Vec::new();
    let mut slot = vec![usize::MAX; nodes.len()];
    for (i, c) in nodes.iter().enumerate() {
        let id = root(&mut parent, i);
        if slot[id] == usize::MAX {
            slot[id] = parts.len();
            parts.push(Part {
                nodes: Vec::new(),
                edges: Vec::new(),
                left: f64::INFINITY,
                top: f64::INFINITY,
                right: f64::NEG_INFINITY,
                bottom: f64::NEG_INFINITY,
            });
        }
        let part = &mut parts[slot[id]];
        part.nodes.push(i);
        part.include(c.x, c.y, c.w, c.h);
    }
    if parts.len() < 2 {
        return None;
    }
    for (ei, e) in edges.iter().enumerate() {
        let part = &mut parts[slot[root(&mut parent, e.src)]];
        part.edges.push(ei);
        for point in e.route.chunks(3) {
            part.include(point[0], point[1], 0.0, 0.0);
        }
        if e.label_w > 0.0 {
            part.include(e.label_x, e.label_y, e.label_w, e.label_h);
        }
    }

    let (mut x, mut y, mut row_height, mut right) = (0.0, 0.0, 0.0, 0.0);
    let (mut from, mut to) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut shifts = Vec::new();
    for part in &parts {
        let w = part.right - part.left;
        let h = part.bottom - part.top;
        if x > 0.0 && x + w > width {
            y += row_height + gap;
            x = 0.0;
            row_height = 0.0;
        }
        shifts.push((x - part.left, y - part.top));
        from = min(from, part.left);
        to = max(to, part.right);
        right = max(right, x + w);
        row_height = max(row_height, h);
        x += w + gap;
    }
    if right >= to - from {
        return None;
    }
    // Each part moves as Pele's shiftLayout moves a layout.
    for (part, (ox, oy)) in parts.iter().zip(shifts) {
        for &i in &part.nodes {
            nodes[i].x += ox;
            nodes[i].y += oy;
        }
        for &ei in &part.edges {
            let e = &mut edges[ei];
            for point in e.route.chunks_mut(3) {
                point[0] += ox;
                point[1] += oy;
            }
            e.label_x += ox;
            e.label_y += oy;
        }
    }
    Some(CompoundResult {
        width: right,
        height: y + row_height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphs::compound::cnode;

    fn placed(x: f64, y: f64) -> CNode {
        let mut c = cnode(100.0, 40.0, -1);
        c.x = x;
        c.y = y;
        c
    }

    #[test]
    fn wraps_parts_into_rows() {
        let mut nodes = vec![placed(50.0, 20.0), placed(250.0, 20.0)];
        let size = wrap_components(&mut nodes, &mut [], 150.0, 10.0);
        assert_eq!(
            size,
            Some(CompoundResult {
                width: 100.0,
                height: 90.0
            })
        );
        assert_eq!((nodes[0].x, nodes[0].y), (50.0, 20.0));
        assert_eq!((nodes[1].x, nodes[1].y), (50.0, 70.0));
        assert_eq!(wrap_components(&mut nodes[..1], &mut [], 150.0, 10.0), None);
        // Parts already in one row, gap apart, are left where they are.
        let mut nodes = vec![placed(50.0, 20.0), placed(160.0, 20.0)];
        assert_eq!(wrap_components(&mut nodes, &mut [], 1000.0, 10.0), None);
    }
}
