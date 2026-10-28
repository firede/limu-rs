//! A user journey's text split into tokens, as Pele's lexer.ts splits it, making the
//! decisions the rules of Mermaid's Jison lexer make.

use crate::common::scan::{JisonTokens, accessibility, keyword_at, skip_trivia};
use crate::js::chars::{is_space, is_word, unit, unit_at};

// The types of token, in the order of Pele's T.
pub(crate) const END: usize = 0;
pub(crate) const EOF: usize = 1;
pub(crate) const INVALID: usize = 2;
pub(crate) const NEWLINE: usize = 3;
pub(crate) const JOURNEY: usize = 4;
pub(crate) const TITLE: usize = 5;
pub(crate) const ACC_TITLE: usize = 6;
pub(crate) const ACC_DESCR: usize = 8;
pub(crate) const ACC_DESCR_MULTILINE_VALUE: usize = 10;
pub(crate) const SECTION: usize = 11;
pub(crate) const TASK_NAME: usize = 12;
pub(crate) const TASK_DATA: usize = 13;
pub(crate) const COLON: usize = 14;

/// The names of the types of token, as Mermaid's errors give them, Pele's TOKEN_NAMES.
pub(crate) const TOKEN_NAMES: [&str; 15] = [
    "$end",
    "EOF",
    "INVALID",
    "NEWLINE",
    "journey",
    "title",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "section",
    "taskName",
    "taskData",
    ":",
];

/// Where text from byte from ends, as Pele's textEnd finds it: `#`, a line break and `;` end
/// every kind of text, and a colon also ends a task name or a section.
fn text_end(src: &str, from: usize, colon: bool) -> usize {
    let b = src.as_bytes();
    let mut e = from;
    while e < b.len() {
        let c = b[e];
        if c == b'#' || c == b'\n' || c == b';' || colon && c == b':' {
            break;
        }
        e += 1;
    }
    e
}

/// Splits a user journey's text into tokens, as Pele's tokenize does.
pub(crate) fn tokenize(src: &str) -> JisonTokens {
    let n = src.len();
    let b = src.as_bytes();
    let mut out = JisonTokens::default();
    let mut p = 0;
    // Adds a token from p to e, and gives where the next starts.
    let emit = |out: &mut JisonTokens, p: usize, kind: usize, e: usize| {
        out.emit(kind, src, p, e);
        e
    };

    loop {
        p = skip_trivia(src, p);
        if p >= n {
            break;
        }
        let (c, size) = unit_at(src, p);
        if c == 10 {
            let mut e = p + 1;
            while e < n && b[e] == 10 {
                e += 1;
            }
            p = emit(&mut out, p, NEWLINE, e);
            continue;
        }
        if keyword_at(src, p, "journey") && !is_word(unit(src, p + 7)) {
            p = emit(&mut out, p, JOURNEY, p + 7);
            continue;
        }
        if keyword_at(src, p, "title") {
            // Unlike Mermaid, a bare title is skipped, as no title.
            let mut e = p + 5;
            loop {
                let (c, size) = unit_at(src, e);
                if c == 10 || !is_space(c) {
                    break;
                }
                e += size;
            }
            if e == n || b[e] == 10 {
                p = e;
                continue;
            }
            let (sep, size) = unit_at(src, p + 5);
            if sep != 13 && is_space(sep) {
                let from = p + 5 + size;
                let e = text_end(src, from, false);
                if e > from {
                    p = emit(&mut out, p, TITLE, e);
                    continue;
                }
            }
        }
        let e = accessibility(src, p, &mut out, ACC_TITLE);
        if e != p {
            p = e;
            continue;
        }
        // Like Mermaid, a bare section takes the next line as its name.
        if keyword_at(src, p, "section") {
            let (sep, size) = unit_at(src, p + 7);
            if is_space(sep) {
                let from = p + 7 + size;
                let e = text_end(src, from, true);
                if e > from {
                    p = emit(&mut out, p, SECTION, e);
                    continue;
                }
            }
        }
        p = match c {
            58 => {
                let e = text_end(src, p + 1, false);
                if e > p + 1 {
                    emit(&mut out, p, TASK_DATA, e)
                } else {
                    emit(&mut out, p, COLON, p + 1)
                }
            }
            59 => emit(&mut out, p, INVALID, p + 1),
            _ => emit(&mut out, p, TASK_NAME, text_end(src, p + size, true)),
        };
    }

    // Jison marks its lexer done when a rule runs on empty input, so an accessibility value
    // that matches nothing at the very end of the text is followed by no EOF token.
    if out.starts.last().is_none_or(|&s| s < n) {
        p = emit(&mut out, p, EOF, n);
    }
    emit(&mut out, p, END, n);
    out
}
