//! Text laid out as Mermaid's journey and timeline diagrams draw it, as Pele's
//! diagrams/common/fit.ts lays it out.

use crate::js::chars::{is_word, len16, space_run, trim};
use crate::scene::text::{Face, Label};
use crate::text::{Setting, layout_label};

/// The end of the `<br>` tag at byte i of s, as /<br\s*\/?>/i matches it, or none.
fn br_end(s: &str, i: usize) -> Option<usize> {
    let b = s.as_bytes();
    if i + 3 > b.len() || b[i] != b'<' || b[i + 1] | 32 != b'b' || b[i + 2] | 32 != b'r' {
        return None;
    }
    let mut j = space_run(s, i + 3);
    if b.get(j) == Some(&b'/') {
        j += 1;
    }
    (b.get(j) == Some(&b'>')).then_some(j + 1)
}

/// The end of the piece of text at byte i that fit_label may cut a word before, as Pele's
/// RE_UNIT, /<br\s*\/?>|ﬂ°°?\w+¶ß|&#?\w+;|[\ud800-\udbff][\udc00-\udfff]|[\s\S]/gi, finds
/// it: a line break, an encoded entity, an entity, or one character, a surrogate pair whole.
fn unit_end(s: &str, i: usize) -> usize {
    if let Some(e) = br_end(s, i) {
        return e;
    }
    let b = s.as_bytes();
    let words = |mut j: usize| {
        while j < b.len() && is_word(i32::from(b[j])) {
            j += 1;
        }
        j
    };
    // ﬂ°°?\w+¶ß, an entity encoded. \w cannot take °, so the second ° is the optional one
    // whenever it is there.
    const OPEN: &str = "ﬂ°";
    const DEGREE: &str = "°";
    const CLOSE: &str = "¶ß";
    if s[i..].starts_with(OPEN) {
        let mut j = i + OPEN.len();
        if s[j..].starts_with(DEGREE) {
            j += DEGREE.len();
        }
        let k = words(j);
        if k > j && s[k..].starts_with(CLOSE) {
            return k + CLOSE.len();
        }
    }
    // &#?\w+;, where \w cannot take #, so #? takes one whenever it is there.
    if b[i] == b'&' {
        let mut j = i + 1;
        if b.get(j) == Some(&b'#') {
            j += 1;
        }
        let k = words(j);
        if k > j && b.get(k) == Some(&b';') {
            return k + 1;
        }
    }
    i + s[i..].chars().next().map_or(1, char::len_utf8)
}

/// Lays out text as plain text in which only `<br>` is markup, so that other tags and icon
/// names show as written instead of being read as formatting, as Pele's plainLabel does. A <
/// that starts no `<br>` is written &lt;, as /<(?!br\s*\/?>)/gi replaces it, and :fa- is
/// written &colon;fa-.
pub(crate) fn plain_label(raw: &str, set: &Setting, size: f64, max: f64, base: Face) -> Label {
    let mut text = std::borrow::Cow::Borrowed(raw);
    if text.contains('<') {
        let mut out = String::with_capacity(text.len());
        for (i, c) in text.char_indices() {
            if c == '<' && br_end(&text, i).is_none() {
                out.push_str("&lt;");
            } else {
                out.push(c);
            }
        }
        text = out.into();
    }
    if text.contains(":fa-") {
        text = text.replace(":fa-", "&colon;fa-").into();
    }
    layout_label(&text, false, set, size, max, base)
}

/// Lays out text as plain_label does, and also cuts any word too long to fit max, for a box
/// of fixed width, as Pele's fitLabel does. layout_label wraps at spaces only.
pub(crate) fn fit_label(raw: &str, set: &Setting, size: f64, max: f64, base: Face) -> Label {
    let label = plain_label(raw, set, size, max, base);
    if label.width <= max {
        return label;
    }
    let mut out = String::with_capacity(raw.len());
    let mut width = 0.0;
    let mut i = 0;
    while i < raw.len() {
        let end = unit_end(raw, i);
        let unit = &raw[i..end];
        i = end;
        let n = len16(unit);
        if trim(unit).is_empty() || n > 2 && unit.starts_with('<') {
            width = 0.0;
        } else {
            let w = set.width(if n > 2 { "x" } else { unit }, size, base);
            if width > 0.0 && width + w > max {
                out.push('\n');
                width = 0.0;
            }
            width += w;
        }
        out.push_str(unit);
    }
    plain_label(&out, set, size, max, base)
}
