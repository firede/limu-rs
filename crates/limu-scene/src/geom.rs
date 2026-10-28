//! Points and boxes, in a diagram's units, y down.

/// A place in a diagram, in its units, y down.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Point {
    #[allow(missing_docs)]
    pub x: f64,
    #[allow(missing_docs)]
    pub y: f64,
}

impl Point {
    /// The point at x, y.
    pub const fn new(x: f64, y: f64) -> Point {
        Point { x, y }
    }
}

/// A box: its top left corner, its width and its height.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Rect {
    #[allow(missing_docs)]
    pub x: f64,
    #[allow(missing_docs)]
    pub y: f64,
    #[allow(missing_docs)]
    pub w: f64,
    #[allow(missing_docs)]
    pub h: f64,
}

impl Rect {
    /// The box with its top left corner at x, y, w wide and h high.
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    /// The middle of the box.
    pub fn center(&self) -> Point {
        Point::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    /// Whether p is in the box, its sides included.
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.x + self.w && p.y >= self.y && p.y <= self.y + self.h
    }

    /// Whether the box is empty: no wider and no higher than nothing, at the origin, as a
    /// box nothing was set in is.
    pub fn is_empty(&self) -> bool {
        *self == Rect::default()
    }

    /// The box moved by dx, dy.
    pub fn moved(&self, dx: f64, dy: f64) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}
