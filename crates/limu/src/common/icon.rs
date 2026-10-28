//! A named icon in a square, as Pele's src/diagrams/common/icon.ts draws it, and whether the
//! host has it.

use crate::layout::Options;
use crate::scene::{Picture, PictureSource, Rect};
use crate::text::decode_entities;

/// Whether the host draws the icon of a name, as Pele's iconMarkup finds one: by the name with
/// its entities decoded, as Pele's render asks the host's icons for it.
pub(crate) fn has_icon(o: &Options, name: &str) -> bool {
    o.has_icon(&decode_entities(name))
}

/// The icon of a name in a square side wide with its corner at x, y, as Pele's iconSvg draws
/// it. The host draws it, in the color of text.
pub(crate) fn icon_svg(name: &str, x: f64, y: f64, side: f64) -> Picture {
    Picture {
        source: PictureSource::Icon(decode_entities(name)),
        bounds: Rect::new(x, y, side, side),
    }
}
