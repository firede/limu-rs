//! A venn diagram's text read into tokens one at a time, as Pele's VennLexer in lexer.ts reads
//! it, making the decisions the rules of Mermaid's Jison lexer make.

use super::db::IndentHost;
use crate::js::chars::{is_digit, is_hex, is_space, is_word, trim, unit, unit_at};

// The types of token, in the order of Pele's T.
pub(crate) const END: usize = 0;
pub(crate) const ERROR: usize = 1;
pub(crate) const EOF: usize = 2;
pub(crate) const NEWLINE: usize = 3;
pub(crate) const VENN: usize = 4;
pub(crate) const TITLE: usize = 5;
pub(crate) const SET: usize = 6;
pub(crate) const UNION: usize = 7;
pub(crate) const TEXT: usize = 8;
pub(crate) const INDENT_TEXT: usize = 9;
pub(crate) const STYLE: usize = 10;
pub(crate) const BRACKET_LABEL: usize = 11;
pub(crate) const NUMERIC: usize = 12;
pub(crate) const HEXCOLOR: usize = 13;
pub(crate) const RGBCOLOR: usize = 14;
pub(crate) const RGBACOLOR: usize = 15;
pub(crate) const IDENTIFIER: usize = 16;
pub(crate) const STRING: usize = 17;
pub(crate) const COMMA: usize = 18;
pub(crate) const COLON: usize = 19;

/// The names of the types of token, as Mermaid's errors give them, Pele's TOKEN_NAMES.
pub(crate) const TOKEN_NAMES: [&str; 20] = [
    "$end",
    "INVALID",
    "EOF",
    "NEWLINE",
    "VENN",
    "TITLE",
    "SET",
    "UNION",
    "TEXT",
    "INDENT_TEXT",
    "STYLE",
    "BRACKET_LABEL",
    "NUMERIC",
    "HEXCOLOR",
    "RGBCOLOR",
    "RGBACOLOR",
    "IDENTIFIER",
    "STRING",
    "COMMA",
    "COLON",
];

/// The keywords, as Pele's KEYWORDS, each with the type of its token.
const KEYWORDS: [(&str, usize); 5] = [
    ("venn-beta", VENN),
    ("set", SET),
    ("union", UNION),
    ("text", TEXT),
    ("style", STYLE),
];

/// Reads tokens one at a time, as Mermaid's Jison lexer for venn diagrams does: every rule
/// ignores case, the first rule that matches wins, and a keyword must end at a word
/// boundary. Positions are bytes.
pub(crate) struct VennLexer<'a> {
    src: &'a str,
    /// The text of the token [`next`](VennLexer::next) gave last.
    pub(crate) text: String,
    /// Where that token starts.
    pub(crate) start: usize,
    at: usize,
    line_start: bool,
    ended: bool,
}

impl<'a> VennLexer<'a> {
    pub(crate) fn new(src: &'a str) -> VennLexer<'a> {
        VennLexer {
            src,
            text: String::new(),
            start: 0,
            at: 0,
            line_start: false,
            ended: false,
        }
    }

    /// Whether word, in lowercase ASCII, is at byte at in any case.
    fn word(&self, at: usize, word: &str) -> bool {
        word.bytes().enumerate().all(|(k, w)| {
            let c = unit(self.src, at + k);
            let w = i32::from(w);
            c == w || c + 32 == w && (65..=90).contains(&c)
        })
    }

    /// Whether word is at byte at, in any case, ending at a word boundary.
    fn keyword(&self, at: usize, word: &str) -> bool {
        self.word(at, word) && !is_word(unit(self.src, at + word.len()))
    }

    /// The end of `rgb(1, 2, 3)` or `rgba(1, 2, 3, 0.5)` whose parenthesis opens at byte at,
    /// as Pele's color finds it.
    fn color(&self, at: usize, parts: usize) -> Option<usize> {
        let src = self.src;
        let spaces = |mut i: usize| {
            loop {
                let (c, size) = unit_at(src, i);
                if !is_space(c) {
                    return i;
                }
                i += size;
            }
        };
        let mut i = at + 1;
        for part in 0..parts {
            i = spaces(i);
            let from = i;
            while is_digit(unit(src, i)) || unit(src, i) == 46 {
                i += 1;
            }
            if i == from {
                return None;
            }
            i = spaces(i);
            if unit(src, i) != if part == parts - 1 { 41 } else { 44 } {
                return None;
            }
            i += 1;
        }
        Some(i)
    }

    /// Whether only spaces and tabs stand between a line feed and byte at.
    fn after_line_feed(&self, at: usize) -> bool {
        let b = self.src.as_bytes();
        let mut k = at;
        while k > 0 && (b[k - 1] == 32 || b[k - 1] == 9) {
            k -= 1;
        }
        k > 0 && b[k - 1] == 10
    }

    fn line_end(&self, from: usize) -> usize {
        self.src[from..]
            .find('\n')
            .map_or(self.src.len(), |e| from + e)
    }

    /// The next token's type, as Pele's next reads it. Its text and start are left in the
    /// lexer.
    pub(crate) fn next(&mut self, host: &mut dyn IndentHost) -> usize {
        let src = self.src;
        let n = src.len();
        loop {
            let mut i = self.at;
            self.start = i;
            self.text.clear();
            if i >= n {
                if self.ended {
                    return END;
                }
                self.ended = true;
                return EOF;
            }
            let c = unit(src, i);
            if c == 37 && unit(src, i + 1) == 37 && unit(src, i + 2) != 123 {
                self.at = self.line_end(i + 2);
                continue;
            }
            if self.line_start
                && self.word(i, "title")
                && self.after_line_feed(i)
                && trim(&src[i + 5..self.line_end(i + 5)]).is_empty()
            {
                // A bare title is skipped as a comment is, so that it does not end indented
                // text.
                let mut j = self.line_end(i + 5);
                while unit(src, j) == 10 || unit(src, j) == 13 {
                    j += 1;
                }
                self.at = j;
                continue;
            }
            if self.line_start {
                if c == 32 || c == 9 {
                    let mut j = i + 1;
                    while unit(src, j) == 32 || unit(src, j) == 9 {
                        j += 1;
                    }
                    self.at = j;
                    if self.keyword(j, "text") && host.indent_mode() {
                        // The word `text` itself is dropped. The indentation stands for it.
                        self.at = j + 4;
                        self.line_start = false;
                        self.text = src[i..j].to_string();
                        return INDENT_TEXT;
                    }
                    continue;
                }
                if c != 10 && c != 13 {
                    host.set_indent_mode(false);
                    self.line_start = false;
                }
            }
            if c == 10 || c == 13 {
                i += 1;
                while unit(src, i) == 10 || unit(src, i) == 13 {
                    i += 1;
                }
                self.line_start = true;
                return self.token(NEWLINE, i);
            }
            if c == 37 && unit(src, i + 1) == 37 {
                self.at = self.line_end(i);
                continue;
            }
            if c == 32 || c == 9 {
                i += 1;
                while unit(src, i) == 32 || unit(src, i) == 9 {
                    i += 1;
                }
                self.at = i;
                continue;
            }

            if ((c | 32) >= 97 && (c | 32) <= 122) || c == 95 {
                // The text of a title is on its line: a line break after the word is no
                // space. Its end is past one code unit after the word.
                let (sep, size) = unit_at(src, i + 5);
                if self.word(i, "title") && is_space(sep) && sep != 10 && sep != 13 {
                    let from = i + 5 + size;
                    let mut end = from;
                    while end < n {
                        let ch = src.as_bytes()[end];
                        if ch == 35 || ch == 10 || ch == 59 {
                            break;
                        }
                        end += 1;
                    }
                    if end > from {
                        return self.token(TITLE, end);
                    }
                }
                for (word, kind) in KEYWORDS {
                    // A hyphen continues a name, as in `set-theory`.
                    if self.keyword(i, word) && unit(src, i + word.len()) != 45 {
                        return self.token(kind, i + word.len());
                    }
                }
                if self.word(i, "rgb") {
                    let alpha = (unit(src, i + 3) | 32) == 97;
                    let open = i + if alpha { 4 } else { 3 };
                    let end = if unit(src, open) == 40 {
                        self.color(open, if alpha { 4 } else { 3 })
                    } else {
                        None
                    };
                    if let Some(end) = end {
                        return self.token(if alpha { RGBACOLOR } else { RGBCOLOR }, end);
                    }
                }
                i += 1;
                while is_word(unit(src, i)) || unit(src, i) == 45 {
                    i += 1;
                }
                return self.token(IDENTIFIER, i);
            }
            if c == 91 {
                if unit(src, i + 1) == 34 {
                    let Some(quote) = src[i + 2..].find('"').map(|q| i + 2 + q) else {
                        return ERROR;
                    };
                    if unit(src, quote + 1) != 93 {
                        return ERROR;
                    }
                    self.at = quote + 2;
                    self.text = src[i + 2..quote].to_string();
                    return BRACKET_LABEL;
                }
                let b = src.as_bytes();
                let mut end = i + 1;
                while end < n && b[end] != 93 && b[end] != 34 {
                    end += 1;
                }
                if end == i + 1 || unit(src, end) != 93 {
                    return ERROR;
                }
                self.at = end + 1;
                self.text = trim(&src[i + 1..end]).to_string();
                return BRACKET_LABEL;
            }
            if is_digit(c) || c == 46 || c == 43 || c == 45 {
                if c == 43 || c == 45 {
                    i += 1;
                }
                let from = i;
                while is_digit(unit(src, i)) {
                    i += 1;
                }
                if i > from || unit(src, i) == 46 && is_digit(unit(src, i + 1)) {
                    if unit(src, i) == 46 && is_digit(unit(src, i + 1)) {
                        i += 2;
                        while is_digit(unit(src, i)) {
                            i += 1;
                        }
                    }
                } else {
                    return ERROR;
                }
                return self.token(NUMERIC, i);
            }
            if c == 35 {
                let mut end = i + 1;
                while end < i + 9 && is_hex(unit(src, end)) {
                    end += 1;
                }
                return if end >= i + 4 {
                    self.token(HEXCOLOR, end)
                } else {
                    ERROR
                };
            }
            if c == 34 {
                return match src[i + 1..].find('"') {
                    Some(q) => self.token(STRING, i + 1 + q + 1),
                    None => ERROR,
                };
            }
            if c == 44 {
                return self.token(COMMA, i + 1);
            }
            if c == 58 {
                return self.token(COLON, i + 1);
            }
            return ERROR;
        }
    }

    fn token(&mut self, kind: usize, end: usize) -> usize {
        self.text = self.src[self.at..end].to_string();
        self.at = end;
        kind
    }
}
