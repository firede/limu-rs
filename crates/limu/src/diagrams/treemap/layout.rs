//! The squarified layout of a treemap, as Pele's layout.ts lays it out.

use std::cmp::Ordering;

use super::model::TreemapModel;
use crate::js::math::{max, min, round};

/// Where a node of the treemap goes, Pele's Box, with its node, its parent and its children
/// by their places. series is the place among the top-level nodes of the branch the box
/// belongs to, which picks its color. value is a leaf's own value, or the sum of the leaves
/// under a section. headed says whether a section keeps room for its header, and header is
/// that room. A section too small for one gives its whole box to its children.
#[derive(Clone, Debug)]
pub(crate) struct Box {
    pub(crate) node: usize,
    pub(crate) parent: Option<usize>,
    pub(crate) series: usize,
    pub(crate) value: f64,
    pub(crate) children: Vec<usize>,
    pub(crate) headed: bool,
    pub(crate) header: f64,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
}

/// Pele's Insets: the gap between boxes, and the room a section keeps for its header, or for
/// a short one where the full one leaves its children too little, and about its children on
/// the other three sides.
#[derive(Clone, Copy)]
pub(crate) struct Insets {
    pub(crate) gap: f64,
    pub(crate) header: f64,
    pub(crate) short: f64,
    pub(crate) side: f64,
}

/// A box for a node, as Pele's box makes it: a leaf's value counts where it is a positive
/// number.
fn new_box(model: &TreemapModel, node: usize, parent: Option<usize>, series: usize) -> Box {
    let n = &model.nodes[node];
    let value = match n.value {
        Some(v) if n.children.is_none() && v.is_finite() && v > 0.0 => v,
        _ => 0.0,
    };
    Box {
        node,
        parent,
        series,
        value,
        children: Vec::new(),
        headed: false,
        header: 0.0,
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    }
}

/// v within low and high, as Pele's clamp gives it: low where v is not at least low.
fn clamp(v: f64, low: f64, high: f64) -> f64 {
    if v >= low { min(v, high) } else { low }
}

/// Lays boxes out in the rectangle as one squarified treemap, as Pele's squarify does: rows
/// are added along the shorter side, and a row stops growing when another box would make its
/// boxes less square.
#[allow(clippy::too_many_arguments)]
fn squarify(
    all: &mut [Box],
    boxes: &[usize],
    total: f64,
    mut x: f64,
    mut y: f64,
    mut w: f64,
    mut h: f64,
    gap: f64,
) {
    let mut rest = total;
    let mut from = 0;
    while from < boxes.len() {
        let scale = (w * h) / rest;
        let side = min(w, h);
        let mut sum = all[boxes[from]].value * scale;
        let largest = sum;
        let mut worst = max(
            (side * side * largest) / (sum * sum),
            (sum * sum) / (side * side * largest),
        );
        let mut to = from + 1;
        while to < boxes.len() {
            let smallest = all[boxes[to]].value * scale;
            let next = sum + smallest;
            let ratio = max(
                (side * side * largest) / (next * next),
                (next * next) / (side * side * smallest),
            );
            if ratio > worst {
                break;
            }
            worst = ratio;
            sum = next;
            to += 1;
        }

        let thickness = sum / side;
        let mut offset = 0.0;
        for i in from..to {
            let b = &mut all[boxes[i]];
            let length = if i == to - 1 {
                side - offset
            } else {
                (b.value * scale) / thickness
            };
            let (bx, by, bw, bh) = if w >= h {
                (x, y + offset, thickness, length)
            } else {
                (x + offset, y, length, thickness)
            };
            // The gap is left out where it would leave the box no size.
            let gx = if round(bx + bw - gap / 2.0) > round(bx + gap / 2.0) {
                gap / 2.0
            } else {
                0.0
            };
            let gy = if round(by + bh - gap / 2.0) > round(by + gap / 2.0) {
                gap / 2.0
            } else {
                0.0
            };
            b.x = round(bx + gx);
            b.y = round(by + gy);
            b.w = round(bx + bw - gx) - b.x;
            b.h = round(by + bh - gy) - b.y;
            offset += length;
            rest -= b.value;
        }
        if w >= h {
            x += thickness;
            w -= thickness;
        } else {
            y += thickness;
            h -= thickness;
        }
        from = to;
    }
}

/// Lays boxes of children out in a rectangle, as Pele's place does: those of positive value,
/// largest first, each kept within the rectangle, and added to placed.
#[allow(clippy::too_many_arguments)]
fn place(
    all: &mut [Box],
    placed: &mut Vec<usize>,
    children: &[usize],
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    gap: f64,
) {
    let mut shown: Vec<usize> = children
        .iter()
        .copied()
        .filter(|&b| all[b].value > 0.0)
        .collect();
    // b.value - a.value, in a stable sort.
    shown.sort_by(|&a, &b| {
        (all[b].value - all[a].value)
            .partial_cmp(&0.0)
            .unwrap_or(Ordering::Equal)
    });
    let mut total = 0.0;
    for &b in &shown {
        total += all[b].value;
    }
    if shown.is_empty() || !total.is_finite() {
        return;
    }
    let w = max(0.0, w);
    let h = max(0.0, h);
    if w > 0.0 && h > 0.0 {
        squarify(
            all,
            &shown,
            total,
            x - gap / 2.0,
            y - gap / 2.0,
            w + gap,
            h + gap,
            gap,
        );
    }
    for &i in &shown {
        let b = &mut all[i];
        let x0 = clamp(b.x, x, x + w);
        let y0 = clamp(b.y, y, y + h);
        b.w = clamp(b.x + b.w, x0, x + w) - x0;
        b.h = clamp(b.y + b.h, y0, y + h) - y0;
        b.x = x0;
        b.y = y0;
        placed.push(i);
    }
}

/// The boxes of the model's nodes, and those of positive value among them by their places,
/// those with no size too, parents before their children, as Pele's layoutTreemap gives
/// them.
pub(crate) fn layout_treemap(
    model: &TreemapModel,
    width: f64,
    height: f64,
    insets: &Insets,
) -> (Vec<Box>, Vec<usize>) {
    let mut all: Vec<Box> = model
        .roots
        .iter()
        .enumerate()
        .map(|(index, &node)| new_box(model, node, None, index))
        .collect();
    let top: Vec<usize> = (0..all.len()).collect();
    let mut i = 0;
    while i < all.len() {
        let (node, series) = (all[i].node, all[i].series);
        for &child in model.nodes[node].children.as_deref().unwrap_or(&[]) {
            let k = all.len();
            all.push(new_box(model, child, Some(i), series));
            all[i].children.push(k);
        }
        i += 1;
    }
    for i in (0..all.len()).rev() {
        if let Some(parent) = all[i].parent {
            all[parent].value += all[i].value;
        }
    }

    let Insets {
        gap,
        header,
        short,
        side,
    } = *insets;
    let mut placed = Vec::new();
    place(&mut all, &mut placed, &top, 0.0, 0.0, width, height, gap);
    let mut i = 0;
    while i < placed.len() {
        let k = placed[i];
        i += 1;
        if all[k].children.is_empty() {
            continue;
        }
        let b = &mut all[k];
        let top = if b.h - header - side >= 12.0 {
            header
        } else {
            short
        };
        b.headed = b.w > max(24.0, 2.0 * side) && b.h - top - side >= 8.0;
        b.header = if b.headed { top } else { 0.0 };
        let children = std::mem::take(&mut b.children);
        let (x, y, w, h) = (b.x, b.y, b.w, b.h);
        if b.headed {
            place(
                &mut all,
                &mut placed,
                &children,
                x + side,
                y + top,
                w - 2.0 * side,
                h - top - side,
                gap,
            );
        } else {
            place(&mut all, &mut placed, &children, x, y, w, h, gap);
        }
        all[k].children = children;
    }
    (all, placed)
}
