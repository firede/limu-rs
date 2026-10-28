//! The steps between the marks of an axis, as Pele's ticks.ts finds them.

use crate::js::math::{log10, max, pow10, round};

/// A step of 1, 2 or 5 times a power of ten that divides span into about count parts, as
/// Pele's tickStep finds it. A step below one comes back as the negative of its inverse,
/// which is a whole number, so that tick values are found by division and print without
/// rounding noise (0.3, not 0.30000000000000004).
pub(crate) fn tick_step(span: f64, count: f64) -> f64 {
    let raw = span / max(count, 1.0);
    let power = log10(raw).floor();
    let unit = raw / pow10(power);
    let factor = if unit >= 7.07 {
        10.0
    } else if unit >= 3.16 {
        5.0
    } else if unit >= 1.41 {
        2.0
    } else {
        1.0
    };
    if power < 0.0 {
        -pow10(-power) / factor
    } else {
        factor * pow10(power)
    }
}

/// How many steps a value is from zero, as Pele's tickIndex counts them.
pub(crate) fn tick_index(step: f64, value: f64) -> f64 {
    if step > 0.0 {
        value / step
    } else {
        value * -step
    }
}

/// The value a number of steps from zero, as Pele's tickValue gives it.
pub(crate) fn tick_value(step: f64, index: f64) -> f64 {
    if step > 0.0 {
        index * step
    } else {
        index / -step
    }
}

/// The multiples of a step from lo to hi, both included, at most 100 of them, as Pele's
/// ticksBetween gives them.
pub(crate) fn ticks_between(lo: f64, hi: f64, step: f64) -> Vec<f64> {
    let mut first = round(tick_index(step, lo));
    if tick_value(step, first) < lo {
        first += 1.0;
    }
    let mut last = round(tick_index(step, hi));
    if tick_value(step, last) > hi {
        last -= 1.0;
    }
    let mut out = Vec::new();
    let mut i = first;
    while i <= last && out.len() < 100 {
        out.push(tick_value(step, i));
        i += 1.0;
    }
    out
}
