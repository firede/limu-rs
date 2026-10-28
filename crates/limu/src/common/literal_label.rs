//! Text that must keep every character, as Pele's diagrams/common/literal-label.ts lays it
//! out.

use crate::js::chars::{is_space_char, trim_end};
use crate::js::math::{max, round};
use crate::scene::text::{Face, Label, Span};
use crate::text::{LINE_HEIGHT, Setting};

/// Lays out text that must keep every character but the spaces its lines break at, as a
/// filename or a field of a protocol, as Pele's literalLabel does. A line breaks at the last
/// space that lets it fit room, or, in a word longer than that, between any two characters.
/// Nothing in it is read as markup or entities.
pub(crate) fn literal_label(s: &str, set: &Setting, size: f64, room: f64, face: Face) -> Label {
    let mut lines: Vec<String> = Vec::new();
    for raw in s.split('\n') {
        let chars: Vec<char> = raw.chars().collect();
        if chars.is_empty() {
            lines.push(String::new());
        }
        let mut from = 0;
        while from < chars.len() {
            let fits = |length: usize| {
                let piece: String = chars[from..from + length].iter().collect();
                set.width(&piece, size, face) <= room
            };
            let mut hi = 1;
            while hi < chars.len() - from && fits(hi) {
                hi = (hi * 2).min(chars.len() - from);
            }
            let mut lo = 1;
            while lo < hi {
                let mid = (lo + hi).div_ceil(2);
                if fits(mid) {
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
            let mut take = lo;
            let wraps = from + take < chars.len();
            if wraps && !is_space_char(chars[from + take]) {
                for i in (1..take).rev() {
                    if is_space_char(chars[from + i]) {
                        take = i + 1;
                        break;
                    }
                }
            }
            let line: String = chars[from..from + take].iter().collect();
            lines.push(match trim_end(&line) {
                trimmed if wraps && !trimmed.is_empty() => trimmed.to_string(),
                _ => line,
            });
            from += take;
            while wraps && from < chars.len() && is_space_char(chars[from]) {
                from += 1;
            }
        }
    }
    if s.is_empty() {
        lines.clear();
    }
    let widths: Vec<f64> = lines.iter().map(|l| set.width(l, size, face)).collect();
    let width = widths.iter().fold(0.0, |w, &x| max(w, x));
    let line_height = round(size * LINE_HEIGHT);
    Label {
        height: lines.len() as f64 * line_height,
        lines: lines
            .into_iter()
            .zip(&widths)
            .map(|(text, &width)| {
                vec![Span {
                    text,
                    face,
                    icon: None,
                    width,
                }]
            })
            .collect(),
        widths,
        width,
        size,
        line_height,
    }
}

/// Breaks each line of a label that is wider than room between any two characters, so that
/// no line but one holding a single character or an icon is wider, as Pele's breakLabel
/// does. Lines that fit are kept as they are.
pub(crate) fn break_label(label: Label, set: &Setting, room: f64) -> Label {
    if label.width <= room {
        return label;
    }
    let mut lines: Vec<Vec<Span>> = Vec::new();
    let mut widths: Vec<f64> = Vec::new();
    for (i, line) in label.lines.iter().enumerate() {
        if label.widths[i] <= room {
            lines.push(line.clone());
            widths.push(label.widths[i]);
            continue;
        }
        let mut current: Vec<Span> = Vec::new();
        let mut width = 0.0;
        let commit = |current: &mut Vec<Span>,
                      width: &mut f64,
                      lines: &mut Vec<Vec<Span>>,
                      widths: &mut Vec<f64>| {
            lines.push(std::mem::take(current));
            widths.push(*width);
            *width = 0.0;
        };
        for span in line {
            if span.icon.is_some() {
                if !current.is_empty() && width + span.width > room {
                    commit(&mut current, &mut width, &mut lines, &mut widths);
                }
                current.push(span.clone());
                width += span.width;
                continue;
            }
            for c in span.text.chars() {
                let ch = c.to_string();
                let joined = current
                    .last()
                    .is_some_and(|last| last.icon.is_none() && last.face == span.face);
                let mut added = match current.last() {
                    Some(last) if joined => {
                        set.width(&format!("{}{ch}", last.text), label.size, span.face) - last.width
                    }
                    _ => set.width(&ch, label.size, span.face),
                };
                let mut joins = joined;
                if !current.is_empty() && width + added > room {
                    commit(&mut current, &mut width, &mut lines, &mut widths);
                    joins = false;
                    added = set.width(&ch, label.size, span.face);
                }
                match current.last_mut() {
                    Some(last) if joins => {
                        last.text.push(c);
                        last.width += added;
                    }
                    _ => current.push(Span {
                        text: ch,
                        face: span.face,
                        icon: None,
                        width: added,
                    }),
                }
                width += added;
            }
        }
        if !current.is_empty() || line.is_empty() {
            commit(&mut current, &mut width, &mut lines, &mut widths);
        }
    }
    let width = widths.iter().fold(0.0, |w, &x| max(w, x));
    Label {
        height: lines.len() as f64 * label.line_height,
        lines,
        widths,
        width,
        ..label
    }
}
