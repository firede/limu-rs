//! The lexer of sankey diagrams, as Pele's lexer.ts reads them: the CSV that Mermaid's Jison
//! grammar lexes.

use crate::js::chars::{is_word, to_lower, unit};

// The kinds of token, in Pele's T, which is Jison's order.
pub(crate) const T_END: usize = 0;
/// Not a Jison token: the reference lexer would return the same empty token forever from
/// here.
pub(crate) const T_STUCK: usize = 1;
pub(crate) const T_SANKEY: usize = 2;
pub(crate) const T_NEWLINE: usize = 3;
pub(crate) const T_COMMA: usize = 4;
pub(crate) const T_DQUOTE: usize = 5;
pub(crate) const T_NON_ESCAPED_TEXT: usize = 6;
pub(crate) const T_ESCAPED_TEXT: usize = 7;
pub(crate) const T_EOF: usize = 8;

/// The names of the kinds, Pele's TOKEN_NAMES.
pub(crate) const TOKEN_NAMES: [&str; 9] = [
    "$end",
    "STUCK",
    "SANKEY",
    "NEWLINE",
    "COMMA",
    "DQUOTE",
    "NON_ESCAPED_TEXT",
    "ESCAPED_TEXT",
    "EOF",
];

/// The tokens of a text, Pele's Tokens: each one's kind, its text, and the byte it starts
/// at.
#[derive(Debug, Default)]
pub(crate) struct Tokens<'a> {
    pub(crate) types: Vec<usize>,
    pub(crate) texts: Vec<&'a str>,
    pub(crate) starts: Vec<usize>,
}

// The lexer's start conditions, Pele's S.
const S_INITIAL: usize = 0;
const S_CSV: usize = 1;
const S_ESCAPED_TEXT: usize = 2;

/// Whether c is RFC 4180's TEXTDATA, printable ASCII but for the double quote and the comma,
/// extended to Unicode but for controls and the characters XML does not allow, as Pele's
/// isTextData says.
fn is_text_data(c: char) -> bool {
    let c = c as u32;
    (32..=126).contains(&c) && c != 34 && c != 44
        || (0xa0..=0xd7ff).contains(&c)
        || (0xe000..=0xfffd).contains(&c)
        || (0x10000..=0x10ffff).contains(&c)
}

/// Whether word, in lowercase ASCII, is at byte p of src, in any case and not followed by a
/// letter, a digit or an underscore, as Pele's keyword reads it: the next word.len() code
/// units, lowered as toLowerCase lowers them, are the word. The Kelvin sign lowers to a k, so
/// it can stand for one. It gives the byte the word ends at.
fn keyword(src: &str, p: usize, word: &str) -> Option<usize> {
    let (mut end, mut units) = (p, 0);
    for c in src[p..].chars() {
        if units >= word.len() {
            break;
        }
        end += c.len_utf8();
        units += c.len_utf16();
    }
    (units == word.len() && to_lower(&src[p..end]) == word && !is_word(unit(src, end)))
        .then_some(end)
}

/// Splits a text into tokens, as Pele's tokenize does.
pub(crate) fn tokenize<'a>(src: &'a str) -> Tokens<'a> {
    let n = src.len();
    let b = src.as_bytes();
    let mut t = Tokens::default();
    let mut stack = Vec::new();
    let mut state = S_INITIAL;
    let mut p = 0;
    let emit = |t: &mut Tokens<'a>, kind: usize, s: usize, e: usize| {
        t.types.push(kind);
        t.texts.push(&src[s..e]);
        t.starts.push(s);
        e
    };

    while p < n {
        let c = b[p];
        let start = p;
        let mut q = p;
        if state == S_ESCAPED_TEXT {
            if c == b'"' && b.get(p + 1) != Some(&b'"') {
                if let Some(s) = stack.pop() {
                    state = s;
                }
                p = emit(&mut t, T_DQUOTE, p, p + 1);
                continue;
            }
            while let Some(d) = src[q..].chars().next() {
                if is_text_data(d) || matches!(d, ',' | '\r' | '\n') {
                    q += d.len_utf8();
                } else if d == '"' && b.get(q + 1) == Some(&b'"') {
                    q += 2;
                } else {
                    break;
                }
            }
            p = emit(&mut t, T_ESCAPED_TEXT, p, q);
        } else {
            if state == S_INITIAL && c | 32 == b's' {
                let end = keyword(src, p, "sankey-beta").or_else(|| keyword(src, p, "sankey"));
                if let Some(end) = end {
                    stack.push(state);
                    state = S_CSV;
                    p = emit(&mut t, T_SANKEY, p, end);
                    continue;
                }
            }
            if c == b'\n' || c == b'\r' && b.get(p + 1) == Some(&b'\n') {
                let end = if c == b'\n' { p + 1 } else { p + 2 };
                p = emit(&mut t, T_NEWLINE, p, end);
                continue;
            }
            if c == b',' {
                p = emit(&mut t, T_COMMA, p, p + 1);
                continue;
            }
            if c == b'"' {
                stack.push(state);
                state = S_ESCAPED_TEXT;
                p = emit(&mut t, T_DQUOTE, p, p + 1);
                continue;
            }
            while let Some(d) = src[q..].chars().next() {
                if !is_text_data(d) {
                    break;
                }
                q += d.len_utf8();
            }
            p = emit(&mut t, T_NON_ESCAPED_TEXT, p, q);
        }
        // A character outside the CSV alphabet matches as empty text and is never consumed.
        if q == start {
            emit(&mut t, T_STUCK, p, p);
            return t;
        }
    }

    let last = if state == S_ESCAPED_TEXT {
        T_ESCAPED_TEXT
    } else {
        T_EOF
    };
    emit(&mut t, last, n, n);
    emit(&mut t, T_END, n, n);
    t
}
