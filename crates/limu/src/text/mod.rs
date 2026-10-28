//! Text laid out as Pele lays it out: labels in lines of measured spans, the entities in
//! them, and a measurer for hosts with no font at hand.

mod entities;
mod label;
mod measurer;

pub use measurer::Arial;

pub(crate) use entities::decode_entities;
pub(crate) use label::{LINE_HEIGHT, layout_label};

use crate::scene::text::{self, Face, Measurer};

/// What text is set with: the host's measurer, and which icons it can draw, as Pele's
/// withIcons sets them for a render. A label leaves room for an icon only where the host has
/// it.
#[derive(Clone, Copy)]
pub(crate) struct Setting<'a> {
    pub(crate) measurer: &'a dyn Measurer,
    pub(crate) icons: &'a dyn Fn(&str) -> bool,
}

impl Setting<'_> {
    /// How wide s is set in face f at size, as a label's span is measured.
    pub(crate) fn width(&self, s: &str, size: f64, f: Face) -> f64 {
        text::width(self.measurer, s, size, f)
    }
}

#[cfg(test)]
mod tests;
