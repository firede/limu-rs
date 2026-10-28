//! A measurer for hosts with no font at hand, from Pele's src/text/measurer.ts and
//! metrics.ts.

use crate::scene::text::{Face, Measurer};

/// Estimates widths from a table of Arial's advances, as Pele's metricsMeasurer does, to the
/// last bit. Wide East Asian characters take an em, other characters past ASCII 0.58 of one,
/// and monospaced text 0.6 of an em a character. It is what Pele measures with where it has
/// no canvas, and so what its tests were laid out with.
#[derive(Clone, Copy, Debug, Default)]
pub struct Arial;

/// Advance widths of printable ASCII in thousandths of an em, two base-36 digits each.
const REGULAR: &str = "7q7q9vfgfgopij5b9999atg87q997q7qfgfgfgfgfgfgfgfgfgfg7q7qg8g8g8fgs7ijijk2k2ijgzlmk27qdwijfgn5k2lmijlmk2ijgzk2ijq8ijijgz7q7q7qd1fg99fgfgdwfgfg7qfgfg6666dw66n5fgfgfgfg99dw7qfgdwk2dwdwdw9a789ag8";
const BOLD: &str = "7q99d6fgfgopk26m9999atg87q997q7qfgfgfgfgfgfgfgfgfgfg9999g8g8g8gzr3k2k2k2k2ijgzlmk27qfgk2gzn5k2lmijlmk2ijgzk2ijq8ijijgz997q99g8fg99fggzfggzfg99gzgz7q7qfg7qopgzgzgzgzatfg99gzfglmfgfgdwat7satg8";

/// The advance of printable ASCII character c in a table, as Pele's unpack reads it.
fn advance(table: &str, c: u32) -> u32 {
    let i = 2 * (c - 32) as usize;
    u32::from_str_radix(&table[i..i + 2], 36).unwrap_or(0)
}

fn is_wide(c: u32) -> bool {
    matches!(c, 0x1100..=0x115f | 0x2e80..=0xa4cf | 0xac00..=0xd7a3 | 0xf900..=0xfaff
        | 0xfe30..=0xfe4f | 0xff00..=0xff60 | 0xffe0..=0xffe6)
}

fn is_zero_width(c: u32) -> bool {
    matches!(c, 0xad | 0x300..=0x36f | 0x200b..=0x200f | 0xfe00..=0xfe0f | 0xfeff)
}

impl Measurer for Arial {
    /// Counts as Pele does, by UTF-16 code unit: a character past the Basic Multilingual Plane
    /// is a high surrogate, which is wide, and a low one, of no width.
    fn width(&self, s: &str, size: f64, f: Face) -> f64 {
        let mono = f.contains(Face::MONO);
        let table = if f.contains(Face::BOLD) {
            BOLD
        } else {
            REGULAR
        };
        let mut units = 0;
        for c in s.chars().map(u32::from) {
            units += match c {
                0..32 => 0,
                32..127 if mono => 600,
                32..127 => advance(table, c),
                _ if is_zero_width(c) => 0,
                _ if c > 0xffff || is_wide(c) => {
                    if mono {
                        1200
                    } else {
                        1000
                    }
                }
                _ if mono => 600,
                _ => 580,
            };
        }
        f64::from(units) * size / 1000.0
    }
}
