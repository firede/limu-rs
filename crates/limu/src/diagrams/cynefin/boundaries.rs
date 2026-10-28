//! The geometry of Mermaid's Cynefin boundaries, as Pele's boundaries.ts has it: seeded wavy
//! lines between the domains and the S-shaped cliff between Clear and Chaotic. Curves are flat
//! lists of numbers: a start point, then six numbers, two control points and an end point,
//! for each cubic segment.
//!
//! The seeded numbers are worked out with JavaScript's conversions between numbers and 32-bit
//! integers, which wrap: `| 0` is to_int32, Math.imul is wrapping_mul, and `>>>` shifts the
//! integer's bits as an unsigned one.

use crate::scene::Path;

/// JavaScript's ToInt32: the whole part of x, wrapped to 32 bits, and 0 for NaN and the
/// infinities.
fn to_int32(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    (x.trunc() % 4294967296.0) as i64 as u32 as i32
}

/// `a >>> n`, as a signed integer again, which is what `^` makes of it.
fn ushr(a: i32, n: u32) -> i32 {
    ((a as u32) >> n) as i32
}

/// One step of mulberry32 from seed, a number in [0, 1), as Pele's seededRandom works it out.
pub(crate) fn seeded_random(seed: f64) -> f64 {
    let mut t = to_int32(seed + 1831565813.0);
    t = (t ^ ushr(t, 15)).wrapping_mul(t | 1);
    t ^= t.wrapping_add((t ^ ushr(t, 7)).wrapping_mul(t | 61));
    f64::from((t ^ ushr(t, 14)) as u32) / 4294967296.0
}

/// Java's hash of a string's UTF-16 code units, in 32 bits, as Pele's hashString works it
/// out.
pub(crate) fn hash_string(s: &str) -> i32 {
    let mut hash: i32 = 0;
    for c in s.encode_utf16() {
        hash = (hash << 5).wrapping_sub(hash).wrapping_add(i32::from(c));
    }
    hash
}

/// The seed of the boundaries, as Pele's resolveSeed gives it: a configured seed other than
/// zero wins, and otherwise the seed is the hash of the diagram's id.
pub(crate) fn resolve_seed(configured: f64, id: &str) -> f64 {
    if configured.is_finite() && configured != 0.0 {
        return configured;
    }
    f64::from(hash_string(id))
}

/// How a wavy line is drawn, Pele's WaveOptions: its segments, whether it runs down, the
/// multipliers Mermaid varies the seed by for each point and each control point, and whether
/// its ends stay on its center line.
#[derive(Clone, Copy)]
pub(crate) struct WaveOptions {
    pub(crate) segments: usize,
    pub(crate) vertical: bool,
    pub(crate) point_step: f64,
    pub(crate) control_step: f64,
    pub(crate) control_shift: f64,
    pub(crate) pin_start: bool,
    pub(crate) pin_end: bool,
}

/// Pele's FOLD, the line down between the top quadrants.
pub(crate) const FOLD: WaveOptions = WaveOptions {
    segments: 7,
    vertical: true,
    point_step: 17.0,
    control_step: 31.0,
    control_shift: 7.0,
    pin_start: false,
    pin_end: false,
};

/// Pele's HORIZON, the lines across between the top quadrants and the bottom ones.
pub(crate) const HORIZON: WaveOptions = WaveOptions {
    segments: 7,
    vertical: false,
    point_step: 23.0,
    control_step: 37.0,
    control_shift: 11.0,
    pin_start: false,
    pin_end: false,
};

/// A wavy line along one axis from `from` to `to`, centered on center in the other axis, as
/// Pele's wave draws it. Its points jitter by up to the amplitude, and a pinned end stays on
/// the center line.
pub(crate) fn wave(
    from: f64,
    to: f64,
    center: f64,
    seed: f64,
    amplitude: f64,
    o: WaveOptions,
) -> Vec<f64> {
    let step = (to - from) / o.segments as f64;
    let mut along = Vec::with_capacity(o.segments + 1);
    let mut across = Vec::with_capacity(o.segments + 1);
    for i in 0..=o.segments {
        let fi = i as f64;
        let pinned = (i == 0 && o.pin_start) || (i == o.segments && o.pin_end);
        along.push(from + fi * step);
        across.push(if pinned {
            center
        } else {
            center + seeded_random(seed + fi * o.point_step) * amplitude * 2.0 - amplitude
        });
    }
    let mut out = if o.vertical {
        vec![across[0], along[0]]
    } else {
        vec![along[0], across[0]]
    };
    for i in 0..o.segments {
        let mid = (along[i] + along[i + 1]) / 2.0;
        let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
        let r = seeded_random(seed + i as f64 * o.control_step + o.control_shift);
        let offset = amplitude * 1.5 * sign * r;
        if o.vertical {
            out.extend([
                across[i] + offset,
                mid,
                across[i + 1] - offset,
                mid,
                across[i + 1],
                along[i + 1],
            ]);
        } else {
            out.extend([
                mid,
                across[i] + offset,
                mid,
                across[i + 1] - offset,
                along[i + 1],
                across[i + 1],
            ]);
        }
    }
    out
}

/// The cliff, a steep S from (cx, top) down to (cx, bottom), as Pele's cliff draws it.
pub(crate) fn cliff(cx: f64, top: f64, bottom: f64, amplitude: f64) -> Vec<f64> {
    let h = bottom - top;
    vec![
        cx,
        top,
        cx + amplitude,
        top + h * 0.2,
        cx - amplitude * 1.5,
        top + h * 0.55,
        cx + amplitude * 0.5,
        top + h * 0.75,
        cx - amplitude,
        top + h * 0.85,
        cx + amplitude * 0.3,
        top + h * 0.95,
        cx,
        bottom,
    ]
}

/// Adds to p the curves from a curve's start point to its end, as the commands of Pele's
/// forward draw them.
pub(crate) fn forward(p: &mut Path, curve: &[f64]) {
    for &[c1x, c1y, c2x, c2y, x, y] in curve[2..].as_chunks::<6>().0 {
        p.cube_to(c1x, c1y, c2x, c2y, x, y);
    }
}

/// Adds to p the curves from a curve's end point back to its start, as the commands of Pele's
/// backward draw them.
pub(crate) fn backward(p: &mut Path, curve: &[f64]) {
    for i in (2..curve.len()).step_by(6).rev() {
        p.cube_to(
            curve[i + 2],
            curve[i + 3],
            curve[i],
            curve[i + 1],
            curve[i - 2],
            curve[i - 1],
        );
    }
}

/// A curve drawn on its own, as Pele's curvePath draws it.
pub(crate) fn curve_path(curve: &[f64]) -> Path {
    let mut p = Path::new();
    p.move_to(curve[0], curve[1]);
    forward(&mut p, curve);
    p
}

/// The outline of an ellipse, as Pele's ellipsePath draws it: two arcs from its left end, over
/// its top and back under it.
pub(crate) fn ellipse_path(cx: f64, cy: f64, rx: f64, ry: f64) -> Path {
    let mut p = Path::new();
    p.move_to(cx - rx, cy)
        .arc_to(rx, ry, 0.0, true, true, cx + rx, cy)
        .arc_to(rx, ry, 0.0, true, true, cx - rx, cy)
        .close();
    p
}
