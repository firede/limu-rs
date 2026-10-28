//! The lexer rules that Mermaid's journey and timeline grammars have in common, as Pele's
//! diagrams/common/scan.ts has them, ported from the compiled Jison rules: comments, white
//! space, and the accTitle and accDescr statements.
//!
//! Positions are bytes. Where Pele reads a code unit with charCodeAt, [`unit_at`] reads the
//! same one, and a character outside ASCII is stepped over whole.

use crate::js::chars::{is_space, space_run, unit, unit_at};

/// The tokens of a lexer ported from Jison, the Tokens of Pele's scan.ts: the type of each
/// token, its text, and the byte it starts at.
#[derive(Clone, Debug, Default)]
pub(crate) struct JisonTokens {
    pub(crate) types: Vec<usize>,
    pub(crate) texts: Vec<String>,
    pub(crate) starts: Vec<usize>,
}

impl JisonTokens {
    /// Adds a token of a type that runs from byte start to byte end of src.
    pub(crate) fn emit(&mut self, kind: usize, src: &str, start: usize, end: usize) {
        self.types.push(kind);
        self.texts.push(src[start..end].to_string());
        self.starts.push(start);
    }
}

/// Where the line that byte from is on ends, as Pele's lineEnd finds it.
pub(crate) fn line_end(src: &str, from: usize) -> usize {
    match src.as_bytes()[from..].iter().position(|&b| b == b'\n') {
        Some(at) => from + at,
        None => src.len(),
    }
}

/// Whether word, in lowercase ASCII letters, is at byte p in any case, as Pele's keywordAt
/// finds it: the grammars are case-insensitive. A byte of a character outside ASCII is 0x80
/// or more, as its code unit is, and stays so under `| 32`.
pub(crate) fn keyword_at(src: &str, p: usize, word: &str) -> bool {
    let Some(at) = src.as_bytes().get(p..p + word.len()) else {
        return false;
    };
    at.iter().zip(word.bytes()).all(|(&c, w)| c | 32 == w)
}

/// Skips comments and white space, as Pele's skipTrivia does. It stops at a line break that
/// starts a token, which is a NEWLINE. Jison compiles `\%%` to `%`, so a single `%` starts a
/// comment.
pub(crate) fn skip_trivia(src: &str, from: usize) -> usize {
    let n = src.len();
    let mut p = from;
    while p < n {
        let (c, size) = unit_at(src, p);
        if c == 37 && unit(src, p + 1) != 123 {
            p = line_end(src, p);
        } else if c == 10 {
            return p;
        } else if is_space(c) {
            p = space_run(src, p + size);
        } else if c == 35 {
            p = line_end(src, p);
        } else {
            return p;
        }
    }
    p
}

/// Lexes an accTitle or accDescr statement at byte p and gives where it ends, or p where
/// there is none, as Pele's accessibility does. The token types acc_title, acc_title_value,
/// acc_descr, acc_descr_value and acc_descr_multiline_value are numbered one after another
/// from first.
pub(crate) fn accessibility(src: &str, p: usize, out: &mut JisonTokens, first: usize) -> usize {
    let descr = keyword_at(src, p, "accdescr");
    if !descr && !keyword_at(src, p, "acctitle") {
        return p;
    }
    let n = src.len();
    let mut q = space_run(src, p + 8);
    let c = unit(src, q);
    if c == 58 {
        q = space_run(src, q + 1);
        let end = line_end(src, q);
        let kind = if descr { first + 2 } else { first };
        out.emit(kind, src, p, q);
        out.emit(kind + 1, src, q, end);
        return end;
    }
    if !descr || c != 123 {
        return p;
    }
    q = space_run(src, q + 1);
    while unit(src, q) != 125 {
        let end = match src.as_bytes()[q..].iter().position(|&b| b == b'}') {
            Some(at) => q + at,
            None => n,
        };
        out.emit(first + 4, src, q, end);
        if q == n {
            return n;
        }
        q = end;
    }
    q + 1
}
