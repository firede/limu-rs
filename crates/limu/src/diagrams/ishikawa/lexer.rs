//! An Ishikawa diagram's text split into tokens, as Pele's lexer.ts splits it, making the
//! decisions the rules of Mermaid's Jison lexer make.

use crate::js::chars::{is_line_end, is_space, is_word, unit, unit_at};

// The types of token, in the order of Pele's T.
pub(crate) const END: usize = 0;
pub(crate) const EOF: usize = 1;
pub(crate) const SPACELINE: usize = 2;
pub(crate) const NL: usize = 3;
pub(crate) const ISHIKAWA: usize = 4;
pub(crate) const SPACELIST: usize = 5;
pub(crate) const TEXT: usize = 6;

/// The names of the types of token, as Mermaid's errors give them, Pele's TOKEN_NAMES.
pub(crate) const TOKEN_NAMES: [&str; 7] = [
    "$end",
    "EOF",
    "SPACELINE",
    "NL",
    "ISHIKAWA",
    "SPACELIST",
    "TEXT",
];

/// The tokens of a text, Pele's Tokens: the type of each, and the bytes it starts and ends
/// at. They end with EOF and END, both empty, at the end of the text.
#[derive(Clone, Debug, Default)]
pub(crate) struct Tokens {
    pub(crate) types: Vec<usize>,
    pub(crate) starts: Vec<usize>,
    pub(crate) ends: Vec<usize>,
}

/// The length of the keyword at byte at of src, or 0 if there is none, as Pele's keyword
/// reads it: `ishikawa-beta`, or failing that `ishikawa`, in any case, either one ending at
/// a word boundary.
fn keyword(src: &str, at: usize) -> usize {
    let word = b"ishikawa-beta";
    let mut k = 0;
    while k < 13 {
        let c = unit(src, at + k);
        let w = i32::from(word[k]);
        if c != w && (c + 32 != w || !(65..=90).contains(&c)) {
            break;
        }
        k += 1;
    }
    if k == 13 && !is_word(unit(src, at + 13)) {
        return 13;
    }
    if k >= 8 && !is_word(unit(src, at + 8)) {
        8
    } else {
        0
    }
}

/// Splits an Ishikawa diagram's text into tokens, as Pele's tokenize does, where the first
/// rule that matches wins: white space up to a comment or up to the last line break in it is
/// a blank line, a single line break before text is a new line, other white space is
/// indentation, and the rest of a line is text.
pub(crate) fn tokenize(src: &str) -> Tokens {
    let mut t = Tokens::default();
    let n = src.len();
    let mut i = 0;
    while i < n {
        let mut j = i;
        let mut last_break = None;
        loop {
            let (c, size) = unit_at(src, j);
            if !is_space(c) {
                break;
            }
            if c == 10 {
                last_break = Some(j);
            }
            j += size;
        }
        let (kind, end);
        if unit(src, j) == 37 && unit(src, j + 1) == 37 {
            kind = SPACELINE;
            let mut e = j + 2;
            loop {
                let (c, size) = unit_at(src, e);
                if size == 0 || is_line_end(c) {
                    break;
                }
                e += size;
            }
            end = e;
        } else if j == i {
            let length = keyword(src, i);
            if length > 0 {
                kind = ISHIKAWA;
                end = i + length;
            } else {
                kind = TEXT;
                end = src[i..].find('\n').map_or(n, |at| i + at);
            }
        } else if last_break.is_some_and(|b| b > i) {
            kind = SPACELINE;
            end = last_break.unwrap_or(i) + 1;
        } else if last_break == Some(i) {
            kind = NL;
            end = i + 1;
        } else {
            kind = SPACELIST;
            end = j;
        }
        t.types.push(kind);
        t.starts.push(i);
        t.ends.push(end);
        i = end;
    }
    t.types.extend([EOF, END]);
    t.starts.extend([n, n]);
    t.ends.extend([n, n]);
    t
}
