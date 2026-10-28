//! A timeline's text split into tokens, as Pele's lexer.ts splits it, making the decisions
//! the rules of Mermaid's Jison lexer make.

use crate::common::scan::{JisonTokens, accessibility, keyword_at, line_end, skip_trivia};
use crate::js::chars::{is_space, is_word, unit, unit_at};

// The types of token, in the order of Pele's T.
pub(crate) const END: usize = 0;
pub(crate) const EOF: usize = 1;
pub(crate) const INVALID: usize = 2;
pub(crate) const NEWLINE: usize = 3;
pub(crate) const TIMELINE: usize = 4;
pub(crate) const TIMELINE_LR: usize = 5;
pub(crate) const TIMELINE_TD: usize = 6;
pub(crate) const TITLE: usize = 7;
pub(crate) const ACC_TITLE: usize = 8;
pub(crate) const ACC_DESCR: usize = 10;
pub(crate) const ACC_DESCR_MULTILINE_VALUE: usize = 12;
pub(crate) const SECTION: usize = 13;
pub(crate) const PERIOD: usize = 14;
pub(crate) const EVENT: usize = 15;

/// The names of the types of token, as Mermaid's errors give them, Pele's TOKEN_NAMES.
pub(crate) const TOKEN_NAMES: [&str; 16] = [
    "$end",
    "EOF",
    "INVALID",
    "NEWLINE",
    "timeline",
    "timeline_lr",
    "timeline_td",
    "title",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "section",
    "period",
    "event",
];

/// Splits a timeline's text into tokens, as Pele's tokenize does.
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
        let mut e = p + 1;
        if c == 10 {
            while e < n && b[e] == 10 {
                e += 1;
            }
            p = emit(&mut out, p, NEWLINE, e);
            continue;
        }
        if keyword_at(src, p, "timeline") {
            e = p + 8;
            while e < n && (b[e] == 32 || b[e] == 9) {
                e += 1;
            }
            let lr = keyword_at(src, e, "lr");
            if e > p + 8 && (lr || keyword_at(src, e, "td")) && !is_word(unit(src, e + 2)) {
                let kind = if lr { TIMELINE_LR } else { TIMELINE_TD };
                p = emit(&mut out, p, kind, e + 2);
                continue;
            }
            if !is_word(unit(src, p + 8)) {
                p = emit(&mut out, p, TIMELINE, p + 8);
                continue;
            }
        }
        if keyword_at(src, p, "title") {
            // Unlike Mermaid, a bare title is skipped, as no title.
            e = p + 5;
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
                p = emit(&mut out, p, TITLE, line_end(src, p + 5 + size));
                continue;
            }
        }
        e = accessibility(src, p, &mut out, ACC_TITLE);
        if e != p {
            p = e;
            continue;
        }
        // Like Mermaid, a bare section takes the next line as its name.
        if keyword_at(src, p, "section") {
            let (sep, size) = unit_at(src, p + 7);
            if is_space(sep) {
                let from = p + 7 + size;
                e = from;
                while e < n && b[e] != 58 && b[e] != 10 {
                    e += 1;
                }
                if e > from {
                    p = emit(&mut out, p, SECTION, e);
                    continue;
                }
            }
        }
        if c != 58 {
            e = p + size;
            while e < n && b[e] != 35 && b[e] != 58 && b[e] != 10 {
                e += 1;
            }
            p = emit(&mut out, p, PERIOD, e);
            continue;
        }
        // An event runs to the end of the line or to a colon that is followed by white space.
        let (sep, size) = unit_at(src, p + 1);
        if is_space(sep) {
            let from = p + 1 + size;
            e = from;
            while e < n {
                let k = b[e];
                if k == 10 || k == 58 && is_space(unit(src, e + 1)) {
                    break;
                }
                e += 1;
            }
            if e > from {
                p = emit(&mut out, p, EVENT, e);
                continue;
            }
        }
        p = emit(&mut out, p, INVALID, p + 1);
    }

    // Jison marks its lexer done when a rule runs on empty input, so an accessibility value
    // that matches nothing at the very end of the text is followed by no EOF token.
    if out.starts.last().is_none_or(|&s| s < n) {
        p = emit(&mut out, p, EOF, n);
    }
    emit(&mut out, p, END, n);
    out
}
