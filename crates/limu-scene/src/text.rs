//! Text laid out: labels of lines of spans, already measured, and the measurer layout asks.

use std::ops::{BitAnd, BitOr, BitOrAssign};

/// How a run of text is set: bold, italic, monospaced, lowered or raised a little and
/// smaller, underlined, or a mix. Its bits are those of Pele's Style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Face(u8);

impl Face {
    /// The regular face.
    pub const REGULAR: Face = Face(0);
    /// Bold.
    pub const BOLD: Face = Face(1);
    /// Italic.
    pub const ITALIC: Face = Face(1 << 1);
    /// The monospace face.
    pub const MONO: Face = Face(1 << 2);
    /// Set lower and smaller, as `<sub>` is.
    pub const SUB: Face = Face(1 << 3);
    /// Set higher and smaller, as `<sup>` is.
    pub const SUP: Face = Face(1 << 4);
    /// Underlined.
    pub const UNDERLINE: Face = Face(1 << 5);

    /// Whether the face has every bit of other.
    pub const fn contains(self, other: Face) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether the face has any bit of other.
    pub const fn intersects(self, other: Face) -> bool {
        self.0 & other.0 != 0
    }

    /// The face without the bits of other.
    pub const fn without(self, other: Face) -> Face {
        Face(self.0 & !other.0)
    }

    /// The face's bits, as Pele's Style numbers them.
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// The face of Pele's bits.
    pub const fn from_bits(bits: u8) -> Face {
        Face(bits & 0x3f)
    }

    /// The size a span in the face is set at, in a label set at size: a [`SUB`](Face::SUB)
    /// or [`SUP`](Face::SUP) span is set smaller, at size / 1.2, as Pele's measurers take
    /// font-size="smaller" to be.
    pub fn size_at(self, size: f64) -> f64 {
        if self.intersects(Face::SUB | Face::SUP) {
            size / SMALLER
        } else {
            size
        }
    }
}

impl BitOr for Face {
    type Output = Face;
    fn bitor(self, rhs: Face) -> Face {
        Face(self.0 | rhs.0)
    }
}

impl BitOrAssign for Face {
    fn bitor_assign(&mut self, rhs: Face) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for Face {
    type Output = Face;
    fn bitand(self, rhs: Face) -> Face {
        Face(self.0 & rhs.0)
    }
}

/// How much smaller a sub or sup span is set.
const SMALLER: f64 = 1.2;

/// Gives the advance width of text set in a face at a size, in the units the diagram is laid
/// out in. Layout depends on it, so a diagram is drawn in the faces it was measured in.
/// Layout asks it only of the faces [`BOLD`](Face::BOLD), [`ITALIC`](Face::ITALIC) and
/// [`MONO`](Face::MONO), and their mixes: it measures a sub or sup span at the smaller size it
/// is set at, and an underline takes no room.
pub trait Measurer {
    /// The advance width of s in the face f at size.
    fn width(&self, s: &str, size: f64, f: Face) -> f64;
}

impl<M: Measurer + ?Sized> Measurer for &M {
    fn width(&self, s: &str, size: f64, f: Face) -> f64 {
        (**self).width(s, size, f)
    }
}

/// How wide m measures s in the face f at size, as a label's span is measured: a sub or sup
/// span at the size it is set at, and m asked only of bold, italic and mono.
pub fn width(m: &dyn Measurer, s: &str, size: f64, f: Face) -> f64 {
    m.width(
        s,
        f.size_at(size),
        f & (Face::BOLD | Face::ITALIC | Face::MONO),
    )
}

/// A run of a line in one face: text, or an icon in its place.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Span {
    /// The text. It is empty in the span of an icon.
    pub text: String,
    /// The face it is set in.
    pub face: Face,
    /// The icon's name, as "fa:fa-car", where the span is an icon.
    pub icon: Option<String>,
    /// How wide it was measured.
    pub width: f64,
}

/// Text laid out in lines to a width, measured.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Label {
    /// The lines, each its spans in turn.
    pub lines: Vec<Vec<Span>>,
    /// The width of each line.
    pub widths: Vec<f64>,
    /// The width of the widest line.
    pub width: f64,
    /// The height of the lines together.
    pub height: f64,
    /// The size the label is set at.
    pub size: f64,
    /// The height of a line.
    pub line_height: f64,
}

impl Label {
    /// Whether the label has nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The label's text, its lines joined by new lines, without its icons.
    pub fn text(&self) -> String {
        let lines: Vec<String> = self
            .lines
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect())
            .collect();
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Measures a character a size wide, and bold ones twice that, and says if it is asked of
    // a face layout never asks of.
    struct Fixed;

    impl Measurer for Fixed {
        fn width(&self, s: &str, size: f64, f: Face) -> f64 {
            assert!(
                !f.intersects(Face::SUB | Face::SUP | Face::UNDERLINE),
                "asked of {f:?}"
            );
            let n = s.encode_utf16().count() as f64;
            if f.contains(Face::BOLD) {
                2.0 * n * size
            } else {
                n * size
            }
        }
    }

    #[test]
    fn widths() {
        assert_eq!(width(&Fixed, "ab", 12.0, Face::REGULAR), 24.0);
        assert_eq!(
            width(&Fixed, "ab", 12.0, Face::BOLD | Face::UNDERLINE),
            48.0
        );
        assert_eq!(width(&Fixed, "ab", 12.0, Face::SUB), 20.0);
        assert_eq!(width(&Fixed, "a", 12.0, Face::SUP | Face::BOLD), 20.0);
    }

    #[test]
    fn faces() {
        let f = Face::BOLD | Face::ITALIC;
        assert!(f.contains(Face::BOLD) && !f.contains(Face::BOLD | Face::MONO));
        assert_eq!(f.without(Face::BOLD), Face::ITALIC);
        assert_eq!(Face::from_bits(f.bits()), f);
    }
}
