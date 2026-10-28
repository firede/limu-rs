//! One line of text fitted into a width, as Pele's diagrams/common/fit-text.ts fits it.

use crate::js::chars::trim_end;
use crate::js::math::max;
use crate::scene::text::Face;
use crate::text::Setting;

/// One line of text fitted into a width, Pele's Fitted: the text, perhaps cut short, the
/// size of type it is written in, and its width.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Fitted {
    pub(crate) text: String,
    pub(crate) size: f64,
    pub(crate) width: f64,
}

/// Fits one line of text into a width, as Pele's fitText does: first by shrinking the type
/// down to least, then by cutting the text and adding an ellipsis. It is none where not even
/// one character fits.
pub(crate) fn fit_text(
    text: &str,
    room: f64,
    size: f64,
    least: f64,
    set: &Setting,
    face: Face,
) -> Option<Fitted> {
    if text.is_empty() || room <= 0.0 {
        return None;
    }
    let mut width = set.width(text, size, face);
    if width <= room {
        return Some(Fitted {
            text: text.to_string(),
            size,
            width,
        });
    }

    // Width grows with the size of type, so one division lands on the right size or just
    // above it.
    let mut fitted = max(least, (size * room / width).floor());
    while fitted >= least {
        width = set.width(text, fitted, face);
        if width <= room {
            return Some(Fitted {
                text: text.to_string(),
                size: fitted,
                width,
            });
        }
        fitted -= 1.0;
    }

    let chars: Vec<char> = text.chars().collect();
    let cut = |n: usize| {
        let head: String = chars[..n].iter().collect();
        format!("{}…", trim_end(&head))
    };
    let (mut low, mut high) = (0, chars.len() - 1);
    while low < high {
        let mid = (low + high + 1) >> 1;
        if set.width(&cut(mid), least, face) <= room {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    if low == 0 {
        return None;
    }
    let short = cut(low);
    width = set.width(&short, least, face);
    (width <= room).then_some(Fitted {
        text: short,
        size: least,
        width,
    })
}
