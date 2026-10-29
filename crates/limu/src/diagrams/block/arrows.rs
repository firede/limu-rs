//! The arrow blocks of a block diagram, as Pele's arrows.ts sizes and draws them.

use crate::js::chars::trim;
use crate::js::math::{max, min};
use crate::scene::Path;

// The directions an arrow block points, as bits of a mask.
pub(crate) const RIGHT: u8 = 1;
pub(crate) const LEFT: u8 = 2;
pub(crate) const UP: u8 = 4;
pub(crate) const DOWN: u8 = 8;

/// How far a head reaches past each side of its shaft, Pele's OVERHANG.
pub(crate) const OVERHANG: f64 = 7.0;
const MAX_DEPTH: f64 = 20.0;
const NUB: f64 = 12.0;

/// The size of an arrow block, and how far its label is shifted from its center, away from
/// its heads, Pele's ArrowSize.
pub(crate) struct ArrowSize {
    pub(crate) w: f64,
    pub(crate) h: f64,
    pub(crate) dx: f64,
    pub(crate) dy: f64,
}

/// The directions of an arrow block as a mask, as Pele's arrowMask reads them. Mermaid
/// compares directions exactly, so `( up )` draws no arrow there. Here spaces are ignored.
pub(crate) fn arrow_mask(directions: Option<&[String]>) -> u8 {
    let mut mask = 0;
    for direction in directions.unwrap_or_default() {
        mask |= match trim(direction) {
            "right" => RIGHT,
            "left" => LEFT,
            "up" => UP,
            "down" => DOWN,
            "x" => RIGHT | LEFT,
            "y" => UP | DOWN,
            _ => 0,
        };
    }
    mask
}

fn bit(mask: u8, flag: u8) -> f64 {
    if mask & flag != 0 { 1.0 } else { 0.0 }
}

/// The axis an arrow points along, as Pele's arrowAxis gives it: 'x' or 'y' for one with a
/// shaft and heads wider than it, and none for one that points along both axes, a box with a
/// small point on each side it names.
pub(crate) fn arrow_axis(mask: u8) -> Option<char> {
    if mask != 0 && mask & (UP | DOWN) == 0 {
        return Some('x');
    }
    if mask != 0 && mask & (RIGHT | LEFT) == 0 {
        return Some('y');
    }
    None
}

/// The size of an arrow block about a label tw by th, as Pele's arrowSize gives it.
pub(crate) fn arrow_size(mask: u8, tw: f64, th: f64) -> ArrowSize {
    let axis = arrow_axis(mask);
    let across = bit(mask, LEFT) - bit(mask, RIGHT);
    let along = bit(mask, UP) - bit(mask, DOWN);
    if axis == Some('x') {
        let h = max(th + 8.0, 14.0) + 2.0 * OVERHANG;
        let depth = min(h / 2.0, MAX_DEPTH);
        let heads = if mask == RIGHT | LEFT { 2.0 } else { 1.0 };
        return ArrowSize {
            w: max(tw + 12.0, 16.0) + depth * heads,
            h,
            dx: (across * depth) / 2.0,
            dy: 0.0,
        };
    }
    if axis == Some('y') {
        let w = max(tw + 16.0, 14.0) + 2.0 * OVERHANG;
        let depth = min(w / 2.0, MAX_DEPTH);
        let heads = if mask == UP | DOWN { 2.0 } else { 1.0 };
        return ArrowSize {
            w,
            h: max(th + 8.0, 16.0) + depth * heads,
            dx: 0.0,
            dy: (along * depth) / 2.0,
        };
    }
    ArrowSize {
        w: max(tw + 16.0, 24.0) + NUB * (bit(mask, LEFT) + bit(mask, RIGHT)),
        h: max(th + 8.0, 24.0) + NUB * (bit(mask, UP) + bit(mask, DOWN)),
        dx: (across * NUB) / 2.0,
        dy: (along * NUB) / 2.0,
    }
}

/// The outline of an arrow block w by h centered on cx, cy, as Pele's drawArrow draws its
/// polygon about the block's center.
pub(crate) fn draw_arrow(mask: u8, cx: f64, cy: f64, w: f64, h: f64) -> Path {
    let x = w / 2.0;
    let y = h / 2.0;
    let mut p: Vec<f64>;
    match arrow_axis(mask) {
        Some('x') => {
            let depth = min(y, MAX_DEPTH);
            let s = y - OVERHANG;
            let x0 = if mask & LEFT != 0 { -x + depth } else { -x };
            let x1 = if mask & RIGHT != 0 { x - depth } else { x };
            p = vec![x0, -s, x1, -s];
            if mask & RIGHT != 0 {
                p.extend([x1, -y, x, 0.0, x1, y]);
            }
            p.extend([x1, s, x0, s]);
            if mask & LEFT != 0 {
                p.extend([x0, y, -x, 0.0, x0, -y]);
            }
        }
        Some(_) => {
            let depth = min(x, MAX_DEPTH);
            let s = x - OVERHANG;
            let y0 = if mask & UP != 0 { -y + depth } else { -y };
            let y1 = if mask & DOWN != 0 { y - depth } else { y };
            p = vec![s, y0, s, y1];
            if mask & DOWN != 0 {
                p.extend([x, y1, 0.0, y, -x, y1]);
            }
            p.extend([-s, y1, -s, y0]);
            if mask & UP != 0 {
                p.extend([-x, y0, 0.0, -y, x, y0]);
            }
        }
        None => {
            let x0 = if mask & LEFT != 0 { -x + NUB } else { -x };
            let x1 = if mask & RIGHT != 0 { x - NUB } else { x };
            let y0 = if mask & UP != 0 { -y + NUB } else { -y };
            let y1 = if mask & DOWN != 0 { y - NUB } else { y };
            let mx = (x0 + x1) / 2.0;
            let my = (y0 + y1) / 2.0;
            let kx = min(10.0, (x1 - x0) / 2.0);
            let ky = min(10.0, (y1 - y0) / 2.0);
            p = vec![x0, y0];
            if mask & UP != 0 {
                p.extend([mx - kx, y0, mx, -y, mx + kx, y0]);
            }
            p.extend([x1, y0]);
            if mask & RIGHT != 0 {
                p.extend([x1, my - ky, x, my, x1, my + ky]);
            }
            p.extend([x1, y1]);
            if mask & DOWN != 0 {
                p.extend([mx + kx, y1, mx, y, mx - kx, y1]);
            }
            p.extend([x0, y1]);
            if mask & LEFT != 0 {
                p.extend([x0, my + ky, -x, my, x0, my - ky]);
            }
        }
    }
    let points: Vec<(f64, f64)> = p.chunks(2).map(|a| (a[0] + cx, a[1] + cy)).collect();
    let mut path = Path::new();
    path.polygon(&points);
    path
}
