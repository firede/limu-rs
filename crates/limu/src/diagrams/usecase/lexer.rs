//! The lexer of a use case diagram, as Pele's lexer.ts has it: the tokens of Mermaid's
//! Chevrotain lexer, tried by hand. Positions are bytes. Every character the lexer looks for
//! is ASCII, so a byte stands for the code unit Pele reads at the same place, and a run of
//! characters outside ASCII is one symbol, as it is to Pele.

use super::tokens::T;
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode, written};
use crate::js::chars::{is_blank, is_digit, is_hex, is_letter, len16, trim};

/// The tokens of a text: the type of each, and the bytes it starts and ends at, Pele's
/// Tokens. They end with three of type EOF, at the end of the text.
#[derive(Clone, Debug, Default)]
pub(crate) struct Tokens {
    pub(crate) types: Vec<T>,
    pub(crate) starts: Vec<usize>,
    pub(crate) ends: Vec<usize>,
}

impl Tokens {
    fn push(&mut self, kind: T, start: usize, end: usize) {
        self.types.push(kind);
        self.starts.push(start);
        self.ends.push(end);
    }
}

/// The lengths of the keywords, by their token type, Pele's KEYWORD_LENGTH.
const KEYWORD_LENGTH: [usize; 21] = [
    0, 12, 5, 14, 3, 9, 2, 2, 2, 2, 2, 4, 3, 4, 8, 5, 5, 7, 6, 4, 5,
];

/// The byte at i, or -1 past the end, as charCodeAt gives NaN there.
fn at(src: &str, i: usize) -> i32 {
    src.as_bytes().get(i).map_or(-1, |&b| i32::from(b))
}

/// Whether a character is one a word is made of, Pele's isWord: an ASCII letter, a digit or
/// an underscore.
fn is_word(c: i32) -> bool {
    crate::js::chars::is_word(c)
}

/// Whether a character is the punctuation Pele's CLASS marks so.
fn is_punctuation(c: i32) -> bool {
    u8::try_from(c).is_ok_and(|c| b"!#$&*+/=?^|~".contains(&c))
}

/// Whether the text at `at` starts with an ASCII word, letters compared without regard to
/// case, as Pele's startsWithNoCase does. Mermaid's INCLUDE and EXTEND are the only keywords
/// matched so.
fn starts_with_no_case(src: &str, at: usize, word: &str) -> bool {
    word.bytes()
        .enumerate()
        .all(|(k, w)| src.as_bytes().get(at + k).is_some_and(|&b| b | 32 == w))
}

/// The keyword that is a prefix of the text at `at`, tried in the order of Mermaid's token
/// list, as Pele's keywordAt finds it.
fn keyword_at(src: &str, at: usize, c: u8) -> Option<T> {
    let s = &src[at..];
    let pick = |word: &str, t: T| s.starts_with(word).then_some(t);
    match c {
        b'a' => pick("actor", T::ACTOR),
        b's' => pick("systemBoundary", T::SYSTEM_BOUNDARY).or_else(|| pick("style", T::STYLE)),
        b'e' => pick("end", T::END)
            .or_else(|| starts_with_no_case(src, at, "extend").then_some(T::EXTEND)),
        b'E' => starts_with_no_case(src, at, "extend").then_some(T::EXTEND),
        b'd' => pick("direction", T::DIRECTION),
        b'T' => pick("TD", T::TD).or_else(|| pick("TB", T::TB)),
        b'B' => pick("BT", T::BT),
        b'L' => pick("LR", T::LR),
        b'R' => pick("RL", T::RL),
        b'n' => pick("note", T::NOTE),
        b'f' => pick("for", T::FOR).or_else(|| pick("false", T::FALSE)),
        b'j' => pick("json", T::JSON),
        b'c' => pick("classDef", T::CLASS_DEF).or_else(|| pick("class", T::CLASS)),
        b'i' | b'I' => starts_with_no_case(src, at, "include").then_some(T::INCLUDE),
        b't' => pick("true", T::TRUE),
        b'u' => pick("usecase-beta", T::USECASE),
        _ => None,
    }
}

/// The line and column of byte offset in the text its author wrote, both from 1, the column
/// in UTF-16 code units, as Pele's lineColumn counts them. A line ends at a line feed, a
/// carriage return, or both.
pub(crate) fn line_column(src: &str, offset: usize) -> (usize, usize) {
    let (src, offset) = written(src, offset);
    let b = src.as_bytes();
    let (mut line, mut line_start) = (1, 0);
    let mut i = 0;
    while i < offset {
        let c = b[i];
        if c == b'\n' || c == b'\r' {
            if c == b'\r' && i + 1 < offset && b[i + 1] == b'\n' {
                i += 1;
            }
            line += 1;
            line_start = i + 1;
        }
        i += 1;
    }
    (line, len16(&src[line_start..offset]) + 1)
}

/// Where byte offset of src is in the text its author wrote, in UTF-16 code units.
pub(crate) fn written16(src: &str, offset: usize) -> usize {
    let (src, offset) = written(src, offset);
    len16(&src[..offset])
}

/// Where the bytes start..end of src are, as Pele's locationText writes it.
pub(crate) fn location_text(src: &str, start: usize, end: usize) -> String {
    let (line, column) = line_column(src, start);
    format!(
        "line {line}, column {column} [{},{})",
        written16(src, start),
        written16(src, end)
    )
}

/// The end of a quoted string that opens at `at`, or none where the line ends first, as
/// Pele's stringEnd finds it.
fn string_end(src: &str, at: usize, quote: u8) -> Option<usize> {
    for (k, &c) in src.as_bytes().iter().enumerate().skip(at + 1) {
        if c == quote {
            return Some(k + 1);
        }
        if c == b'\n' || c == b'\r' {
            return None;
        }
    }
    None
}

/// Whether a token starts at `at`, as Pele's startsToken says.
fn starts_token(src: &str, at: usize) -> bool {
    let c = src.as_bytes()[at];
    if c != b'"' && c != b'\'' {
        return true;
    }
    (c == b'"' && self::at(src, at + 1) == 96) || string_end(src, at, c).is_some()
}

/// The error of a character no token starts with, as Pele's lexError words it: Mermaid
/// reports the character, and how many characters its lexer skipped before a token started.
fn lex_error(src: &str, offset: usize, length: usize) -> Error {
    let (line, column) = line_column(src, offset);
    let (from, to) = (written16(src, offset), written16(src, offset + length));
    let c = src[offset..]
        .chars()
        .next()
        .map(String::from)
        .unwrap_or_default();
    let message = format!(
        "Error lexing usecase diagram: unexpected character: ->{c}<- at offset: {from}, skipped {} characters. at line {line}, column {column} [{from},{to})",
        to - from
    );
    placed(message, ErrorCode::Syntax, line, column)
}

/// An error of a use case diagram at a line and column, with no snippet, as Pele's
/// PeleError is made with a type, a line and a column.
pub(crate) fn placed(message: String, code: ErrorCode, line: usize, column: usize) -> Error {
    let mut e = Error::new(message, code);
    e.diagram = Some(DiagramType::UseCase);
    (e.line, e.column) = (line, column);
    e
}

/// The tokens of Mermaid's Chevrotain lexer for use case diagrams, as Pele's tokenize
/// produces them: the first pattern in its list that matches wins, a keyword gives way to a
/// longer identifier, and `json id@` and `<<` switch to modes that read a JSON object and a
/// stereotype.
pub(crate) fn tokenize(src: &str) -> Result<Tokens, Error> {
    let mut out = Tokens::default();
    let b = src.as_bytes();
    let n = b.len();
    let mut i = 0;
    // Whether only blanks came before on this line, where comments and accessibility lines
    // may start.
    let mut line_start = true;
    let mut no_brace = false;

    while i < n {
        let c = b[i];
        if c == b' ' || c == b'\t' {
            i += 1;
            continue;
        }
        let mut kind: Option<T> = None;
        let mut end = i + 1;
        let mut fresh = false;

        if is_word(i32::from(c)) {
            if c == b'a' && line_start && src[i..].starts_with("acc") {
                let descr = src[i + 3..].starts_with("Descr");
                if descr || src[i + 3..].starts_with("Title") {
                    let mut j = i + 8;
                    while is_blank(at(src, j)) {
                        j += 1;
                    }
                    let next = at(src, j);
                    if next == 58 {
                        kind = Some(if descr {
                            T::ACC_DESCR_LINE
                        } else {
                            T::ACC_TITLE_LINE
                        });
                        end = j + 1;
                        while end < n && b[end] != b'\n' && b[end] != b'\r' {
                            end += 1;
                        }
                    } else if next == 123 && descr && !no_brace {
                        match src[j + 1..].find('}') {
                            None => no_brace = true,
                            Some(close) => {
                                kind = Some(T::ACC_DESCR_BLOCK);
                                end = j + 1 + close + 1;
                            }
                        }
                    }
                }
            } else if c == b'j' && src[i..].starts_with("json") && is_blank(at(src, i + 4)) {
                let mut j = i + 5;
                while is_blank(at(src, j)) {
                    j += 1;
                }
                let id = j;
                while is_word(at(src, j)) {
                    j += 1;
                }
                if j > id {
                    while is_blank(at(src, j)) {
                        j += 1;
                    }
                    if at(src, j) == 64 {
                        j += 1;
                        while is_blank(at(src, j)) {
                            j += 1;
                        }
                        if at(src, j) == 123 {
                            out.push(T::JSON_DECLARATION_START, i, j);
                            let mut depth = 0;
                            let (mut quoted, mut escaped) = (false, false);
                            let mut k = j;
                            while k < n {
                                let ch = b[k];
                                if quoted {
                                    if escaped {
                                        escaped = false;
                                    } else if ch == b'\\' {
                                        escaped = true;
                                    } else if ch == b'"' {
                                        quoted = false;
                                    }
                                } else if ch == b'"' {
                                    quoted = true;
                                } else if ch == b'{' {
                                    depth += 1;
                                } else if ch == b'}' {
                                    depth -= 1;
                                    if depth == 0 {
                                        break;
                                    }
                                }
                                k += 1;
                            }
                            let closed = k < n;
                            i = if closed { k + 1 } else { n };
                            out.push(
                                if closed {
                                    T::JSON_OBJECT_LITERAL
                                } else {
                                    T::UNCLOSED_JSON_OBJECT_LITERAL
                                },
                                j,
                                i,
                            );
                            line_start = false;
                            continue;
                        }
                    }
                }
            }
            if kind.is_none() {
                while end < n && is_word(i32::from(b[end])) {
                    end += 1;
                }
                if let Some(keyword) = keyword_at(src, i, c) {
                    // A keyword that is only the start of a longer word is an identifier.
                    let length = KEYWORD_LENGTH[keyword as usize];
                    if keyword == T::USECASE || end - i == length {
                        kind = Some(keyword);
                        end = i + length;
                    } else {
                        kind = Some(T::IDENTIFIER);
                    }
                } else if (c == b'o' || c == b'x') && at(src, i + 1) == 45 && at(src, i + 2) == 45 {
                    kind = Some(if c == b'o' {
                        T::BACKWARD_CIRCLE
                    } else {
                        T::BACKWARD_CROSS
                    });
                    end = i + 3;
                } else if is_digit(i32::from(c)) {
                    // `1mg` is an identifier, and `1.5px` is a number because the identifier
                    // stops at the dot.
                    let mut digits = i + 1;
                    while is_digit(at(src, digits)) {
                        digits += 1;
                    }
                    if at(src, digits) == 46 && is_digit(at(src, digits + 1)) {
                        kind = Some(T::NUMBER);
                        end = digits + 2;
                        while is_digit(at(src, end)) {
                            end += 1;
                        }
                        while is_letter(at(src, end)) {
                            end += 1;
                        }
                    } else {
                        kind = Some(T::IDENTIFIER);
                    }
                } else {
                    kind = Some(T::IDENTIFIER);
                    while at(src, end) == 45 && is_word(at(src, end + 1)) {
                        kind = Some(T::CSS_IDENTIFIER);
                        end += 2;
                        while is_word(at(src, end)) {
                            end += 1;
                        }
                    }
                }
            }
        } else if c == b'\n' {
            kind = Some(T::NEWLINE);
            fresh = true;
        } else if c == b'\r' {
            kind = Some(T::NEWLINE);
            fresh = true;
            if at(src, end) == 10 {
                end += 1;
            }
        } else if c == b'"' || c == b'\'' {
            if c == b'"' && at(src, i + 1) == 96 {
                let close = src[i + 2..].find("`\"").map(|k| i + 2 + k);
                kind = Some(if close.is_none() {
                    T::UNCLOSED_MARKDOWN_STRING
                } else {
                    T::MARKDOWN_STRING
                });
                end = close.map_or(n, |k| k + 2);
            } else {
                kind = Some(T::PLAIN_STRING);
                match string_end(src, i, c) {
                    Some(e) => end = e,
                    None => {
                        let mut resume = i + 1;
                        while resume < n && !starts_token(src, resume) {
                            resume += 1;
                        }
                        return Err(lex_error(src, i, resume - i));
                    }
                }
            }
        } else if c == b'%' {
            if line_start && at(src, end) == 37 {
                kind = Some(T::COMMENT);
                while end < n && b[end] != b'\n' && b[end] != b'\r' {
                    end += 1;
                }
            } else {
                kind = Some(T::PERCENT);
            }
        } else if c == b'-' {
            if at(src, end) != 45 {
                kind = Some(T::DASH);
            } else {
                while at(src, end) == 45 {
                    end += 1;
                }
                let next = at(src, end);
                let two = end - i == 2;
                if two && next == 124 && at(src, end + 1) == 62 {
                    kind = Some(T::GENERALIZATION);
                    end += 2;
                } else if next == 62 {
                    kind = Some(T::FORWARD_SOLID);
                    end += 1;
                } else if two && (next == 111 || next == 120) {
                    kind = Some(if next == 111 {
                        T::FORWARD_CIRCLE
                    } else {
                        T::FORWARD_CROSS
                    });
                    end += 1;
                } else {
                    kind = Some(T::MARKERLESS_SOLID);
                }
            }
        } else if c == b'.' {
            let next = at(src, end);
            if next == 46 && at(src, end + 1) == 62 {
                kind = Some(T::DEPENDENCY_ARROW);
                end += 2;
            } else if is_digit(next) {
                kind = Some(T::NUMBER);
                while is_digit(at(src, end)) {
                    end += 1;
                }
                while is_letter(at(src, end)) {
                    end += 1;
                }
            } else {
                kind = Some(T::DOT);
            }
        } else if c == b'<' {
            if at(src, end) == 60 {
                // Pele clears lineStart here too, which the token after the stereotype's start
                // sets again before it is read.
                out.push(T::STEREOTYPE_START, i, i + 2);
                i += 2;
                if i >= n {
                    break;
                }
                if at(src, i) == 62 && at(src, i + 1) == 62 {
                    kind = Some(T::STEREOTYPE_END);
                    end = i + 2;
                } else {
                    let mut close = i;
                    let mut found = false;
                    while close < n {
                        let ch = b[close];
                        if ch == b'\n' || ch == b'\r' {
                            break;
                        }
                        if ch == b'>' && at(src, close + 1) == 62 {
                            found = true;
                            break;
                        }
                        close += 1;
                    }
                    if found && !trim(&src[i..close]).is_empty() {
                        out.push(T::STEREOTYPE_TEXT, i, close);
                        kind = Some(T::STEREOTYPE_END);
                        i = close;
                        end = close + 2;
                    } else {
                        while close < n && b[close] != b'\n' && b[close] != b'\r' {
                            close += 1;
                        }
                        if close == i {
                            while close < n && (b[close] == b'\n' || b[close] == b'\r') {
                                close += 1;
                            }
                            return Err(lex_error(src, i, close - i));
                        }
                        kind = Some(T::UNCLOSED_STEREOTYPE_TEXT);
                        end = close;
                    }
                }
            } else if at(src, end) == 45 && at(src, end + 1) == 45 {
                kind = Some(T::BACKWARD_SOLID);
                end += 2;
                while at(src, end) == 45 {
                    end += 1;
                }
            } else {
                kind = Some(T::LABEL_PUNCTUATION);
            }
        } else if c == b':' {
            if at(src, end) == 58 && at(src, end + 1) == 58 {
                kind = Some(T::CLASS_SEPARATOR);
                end += 2;
            } else {
                kind = Some(T::COLON);
            }
        } else if c == b'@' {
            if at(src, end) == 123 {
                kind = Some(T::METADATA_START);
                end += 1;
            } else {
                kind = Some(T::AT);
            }
        } else if c == b'{' {
            kind = Some(T::LBRACE);
        } else if c == b'}' {
            kind = Some(T::RBRACE);
        } else if c == b'[' {
            kind = Some(T::LBRACKET);
        } else if c == b']' {
            kind = Some(T::RBRACKET);
        } else if c == b'(' {
            kind = Some(T::LPAREN);
        } else if c == b')' {
            kind = Some(T::RPAREN);
        } else if c == b',' {
            kind = Some(T::COMMA);
        } else if c == b'#' {
            while is_hex(at(src, end)) {
                end += 1;
            }
            kind = Some(if end > i + 1 {
                T::HASH_COLOR
            } else {
                T::CSS_PUNCTUATION
            });
        } else if c == b'\\' {
            if at(src, end) == 44 {
                kind = Some(T::CSS_ESCAPED_COMMA);
                end += 1;
            } else {
                kind = Some(T::LABEL_PUNCTUATION);
            }
        } else if c < 127 && c > 32 {
            kind = Some(if is_punctuation(i32::from(c)) {
                T::CSS_PUNCTUATION
            } else {
                T::LABEL_PUNCTUATION
            });
        } else {
            // A run of control characters and of characters outside ASCII, whose bytes are
            // all 0x80 or more.
            kind = Some(T::LABEL_SYMBOL);
            while end < n {
                let ch = b[end];
                if (32..127).contains(&ch) || ch == b'\t' || ch == b'\n' || ch == b'\r' {
                    break;
                }
                end += 1;
            }
        }

        out.push(kind.unwrap_or(T::EOF), i, end);
        i = end;
        line_start = fresh;
    }

    for _ in 0..3 {
        out.push(T::EOF, n, n);
    }
    Ok(out)
}
