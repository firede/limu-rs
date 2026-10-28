//! Where a cluster's title sits on its top border, clear of the edges that enter there, as
//! Pele's src/svg/cluster.ts places it.

use std::collections::HashMap;

use super::Node;
use crate::js::math::{min, round};

/// The x of every route point, by its y rounded: the edges that cross a cluster's top
/// border, as Pele's Crossings has them.
#[derive(Clone, Debug, Default)]
pub(crate) struct Crossings(HashMap<u64, Vec<f64>>);

impl Crossings {
    /// The key of a y rounded, as a JavaScript Map tells its keys apart: -0 is 0, and every
    /// NaN is the same.
    fn key(y: f64) -> u64 {
        let k = round(y);
        if k.is_nan() {
            f64::NAN.to_bits()
        } else {
            (k + 0.0).to_bits()
        }
    }

    /// The x of the points on y, rounded.
    pub(super) fn at(&self, y: f64) -> Option<&[f64]> {
        self.0.get(&Crossings::key(y)).map(Vec::as_slice)
    }

    /// How many rounded ys have points.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.0.len()
    }
}

/// Adds the points of a route of x, y, axis triples, as Pele's markCrossings does.
pub(crate) fn mark_crossings(crossings: &mut Crossings, route: &[f64]) {
    for &[x, y, _] in route.as_chunks::<3>().0 {
        crossings.0.entry(Crossings::key(y)).or_default().push(x);
    }
}

/// Where along a cluster's top a title width wide is centered, so that the edges that enter
/// there do not cross it, as Pele's clusterTitleX picks it: at the left, in the middle, or at
/// the right, pad in from the sides less 6, the first that is 6 clear of every edge, or else
/// the clearest.
pub(crate) fn cluster_title_x(c: &Node, width: f64, crossings: &Crossings, pad: f64) -> f64 {
    if width == 0.0 {
        return c.x;
    }
    let top = c.y - c.h / 2.0;
    let left = c.x - c.w / 2.0;
    let half = width / 2.0;
    let candidates = [left + pad - 6.0 + half, c.x, left + c.w - pad + 6.0 - half];
    let Some(xs) = crossings.at(top) else {
        return candidates[0];
    };
    let (mut best, mut best_gap) = (candidates[0], -1.0);
    for x in candidates {
        let mut gap = f64::INFINITY;
        for &cx in xs {
            if cx > left && cx < left + c.w {
                gap = min(gap, (cx - x).abs() - half);
            }
        }
        if gap >= 6.0 {
            return x;
        }
        if gap > best_gap + 0.5 {
            (best_gap, best) = (gap, x);
        }
    }
    best
}

/// Whether an edge that enters a cluster at its top runs through its title, width wide and
/// centered at x, as Pele's titleCrossed says.
pub(crate) fn title_crossed(c: &Node, width: f64, x: f64, crossings: &Crossings) -> bool {
    if !(width > 0.0) {
        return false;
    }
    let Some(xs) = crossings.at(c.y - c.h / 2.0) else {
        return false;
    };
    let left = c.x - c.w / 2.0;
    xs.iter()
        .any(|&cx| cx > left && cx < left + c.w && (cx - x).abs() < width / 2.0 + 4.0)
}

/// How wide the outline of a title an edge runs through is. Pele draws such a title over the
/// edges, on an outline in the color of the cluster behind it with round joins, so that only
/// the line is seen to stop short of the letters, as its struckTitle draws it.
pub(crate) const STRUCK_HALO: f64 = 5.0;

/// How many clusters deep the cluster at index is, itself counted and no more than 8, as
/// Pele's struckTitle counts it. parent gives a cluster's parent, or none.
pub(crate) fn struck_depth(parent: impl Fn(usize) -> Option<usize>, index: usize) -> u32 {
    let mut depth = 1;
    let mut p = parent(index);
    while let Some(at) = p
        && depth < 8
    {
        depth += 1;
        p = parent(at);
    }
    depth
}

/// The percentage of the theme's alternate surface over the background in the outline of a
/// struck title depth clusters deep, as each cluster is half its fill over what is behind it.
pub(crate) fn struck_tint(depth: u32) -> f64 {
    100.0 * (1.0 - 0.5f64.powi(depth as i32))
}
