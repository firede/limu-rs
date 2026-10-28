//! The lexer of an XY chart, as Pele's lexer.ts has it: the rules of Mermaid's xychart.jison,
//! tried by hand in the order Jison tries them, in the states it reads them in. Positions are
//! bytes. Where Pele reads a code unit with charCodeAt, unit reads the same one, and a
//! character outside ASCII is stepped over whole.

use super::tokens::T;
use crate::common::word_at;
use crate::js::chars::{is_digit, is_letter, is_space, is_word, space_run, unit_at};

/// A token: its type, its text, and the byte of the text it starts at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Tok<'a> {
    pub(crate) kind: T,
    pub(crate) text: &'a str,
    pub(crate) start: usize,
}

/// The states of the lexer, Pele's S.
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum S {
    INITIAL,
    axis_data,
    axis_band_data,
    data,
    data_inner,
    string,
    acc_title,
    acc_descr,
    acc_descr_multiline,
}

/// The token a state gives for the empty rest of the input, Pele's AT_END.
fn at_end(s: S) -> T {
    match s {
        S::string => T::STR,
        S::acc_title => T::acc_title_value,
        S::acc_descr => T::acc_descr_value,
        S::acc_descr_multiline => T::acc_descr_multiline_value,
        _ => T::EOF,
    }
}

/// The text Mermaid's rule for a markdown string matches, Pele's BROKEN_RULE, as pieces and
/// how many times each repeats. The grammar means to open a markdown string at `"` followed
/// by a backtick, but its quoting turns the rule into one that matches this text and leaves
/// the current state. Markdown strings are never read.
const BROKEN_RULE: [(&str, usize); 5] = [
    ("`)", 1),
    (" ", 36),
    (
        "{ this.pushstate(md_string); }\n<md_string>(?:(?!`\").)+",
        1,
    ),
    (" ", 18),
    ("{ return md_str; }\n<md_string>(?:`", 1),
];

/// Where BROKEN_RULE ends at p, matched as Pele's wordAt matches it, or none.
fn broken_rule(src: &str, p: usize) -> Option<usize> {
    let mut q = p;
    for (piece, times) in BROKEN_RULE {
        for _ in 0..times {
            if !word_at(src, q, piece) {
                return None;
            }
            q += piece.len();
        }
    }
    Some(q)
}

/// The token of a character that is one by itself, Pele's SINGLE.
fn single(c: i32) -> Option<T> {
    let Ok(c) = u8::try_from(c) else {
        return None;
    };
    Some(match c {
        b':' => T::COLON,
        b'+' => T::PLUS,
        b',' => T::COMMA,
        b'=' => T::EQUALS,
        b'*' => T::MULT,
        b'#' => T::BRKT,
        b'_' => T::UNDERSCORE,
        b'.' => T::DOT,
        b'&' => T::AMP,
        b'-' => T::MINUS,
        b';' => T::SEMI,
        _ => return None,
    })
}

struct Lexer<'a> {
    src: &'a str,
    toks: Vec<Tok<'a>>,
    stack: Vec<S>,
    state: S,
    p: usize,
}

impl<'a> Lexer<'a> {
    fn emit(&mut self, kind: T, s: usize, e: usize) {
        self.toks.push(Tok {
            kind,
            text: &self.src[s..e],
            start: s,
        });
        self.p = e;
    }

    fn push(&mut self, s: S) {
        self.stack.push(self.state);
        self.state = s;
    }

    fn pop(&mut self) {
        if let Some(s) = self.stack.pop() {
            self.state = s;
        }
    }

    /// The byte at q, or -1 past the end. It stands for the code unit wherever only an
    /// ASCII character is looked for.
    fn byte(&self, q: usize) -> i32 {
        self.src.as_bytes().get(q).map_or(-1, |&b| i32::from(b))
    }

    /// The line feed at or after q, or the end.
    fn line_end(&self, q: usize) -> usize {
        self.src[q..].find('\n').map_or(self.src.len(), |e| q + e)
    }

    /// The end of the run of white space at q.
    fn run(&self, q: usize) -> usize {
        space_run(self.src, q)
    }

    /// The end of the run of digits at q.
    fn digits(&self, mut q: usize) -> usize {
        while is_digit(self.byte(q)) {
            q += 1;
        }
        q
    }

    /// Whether word, a keyword, is at p, and no word character follows it.
    fn keyword(&self, word: &str) -> bool {
        word_at(self.src, self.p, word) && !is_word(self.byte(self.p + word.len()))
    }

    /// Takes the keyword at p, as Pele's switch on its first letter does. It is false where
    /// there is none.
    fn keywords(&mut self, c: i32) -> bool {
        let p = self.p;
        let src = self.src;
        let (kind, len, state) = match c | 32 {
            116 if self.keyword("title") => (T::title, 5, None),
            97 if word_at(src, p, "acc") => {
                let descr = word_at(src, p + 3, "descr");
                if !descr && !word_at(src, p + 3, "title") {
                    return false;
                }
                let k = self.run(p + 8);
                let d = self.byte(k);
                if d == 58 {
                    let (kind, state) = if descr {
                        (T::acc_descr, S::acc_descr)
                    } else {
                        (T::acc_title, S::acc_title)
                    };
                    self.emit(kind, p, self.run(k + 1));
                    self.push(state);
                    return true;
                }
                if d == 123 && descr {
                    self.p = self.run(k + 1);
                    self.push(S::acc_descr_multiline);
                    return true;
                }
                return false;
            }
            120 if self.keyword("xychart-beta") => (T::XYCHART, 12, None),
            120 if self.keyword("xychart") => (T::XYCHART, 7, None),
            120 if self.keyword("x-axis") => (T::X_AXIS, 6, Some(S::axis_data)),
            121 if self.keyword("y-axis") => (T::Y_AXIS, 6, Some(S::axis_data)),
            118 if word_at(src, p, "vertical") => (T::CHART_ORIENTATION, 8, None),
            104 if word_at(src, p, "horizontal") => (T::CHART_ORIENTATION, 10, None),
            108 if self.keyword("line") => (T::LINE, 4, Some(S::data)),
            98 if self.keyword("bar") => (T::BAR, 3, Some(S::data)),
            _ => return false,
        };
        self.emit(kind, p, p + len);
        if let Some(state) = state {
            self.push(state);
        }
        true
    }

    /// Where a number with an optional sign and decimals ends at p, as Pele reads one in the
    /// data of an axis or a series, or none.
    fn number(&self, c: i32) -> Option<usize> {
        let q = if c == 43 || c == 45 {
            self.p + 1
        } else {
            self.p
        };
        let d = self.byte(q);
        if is_digit(d) {
            let mut e = self.digits(q + 1);
            if self.byte(e) == 46 && is_digit(self.byte(e + 1)) {
                e = self.digits(e + 2);
            }
            return Some(e);
        }
        (d == 46 && is_digit(self.byte(q + 1))).then(|| self.digits(q + 2))
    }

    /// Takes a token at p, as Pele's loop does. It is false where no rule matches, and the
    /// lexer stops.
    fn step(&mut self) -> bool {
        let p = self.p;
        let n = self.src.len();
        let (c, size) = unit_at(self.src, p);

        match self.state {
            S::string => {
                if c == 34 {
                    self.p += 1;
                    self.pop();
                } else {
                    let e = self.src[p..].find('"').map_or(n, |e| p + e);
                    self.emit(T::STR, p, e);
                }
                return true;
            }
            S::acc_title | S::acc_descr => {
                self.emit(at_end(self.state), p, self.line_end(p));
                self.pop();
                return true;
            }
            S::acc_descr_multiline => {
                if c == 125 {
                    self.p += 1;
                    self.pop();
                } else {
                    let e = self.src[p..].find('}').map_or(n, |e| p + e);
                    self.emit(T::acc_descr_multiline_value, p, e);
                }
                return true;
            }
            _ => {}
        }

        let next = self.byte(p + 1);
        if c == 37 && next == 37 {
            self.p = self.line_end(p + 2);
            return true;
        }
        if c == 10 || c == 13 {
            // Only a line break that directly follows an axis or a series ends it. One after
            // spaces is skipped with them, and the state carries on to the next line.
            if matches!(self.state, S::axis_data | S::data) && (c == 10 || next == 10) {
                self.emit(T::NEWLINE, p, if c == 10 { p + 1 } else { p + 2 });
                self.pop();
                return true;
            }
            let mut e = p + 1;
            while matches!(self.byte(e), 10 | 13) {
                e += 1;
            }
            self.emit(T::NEWLINE, p, e);
            return true;
        }

        if self.keywords(c) {
            return true;
        }

        if c == 91 {
            match self.state {
                S::axis_data => self.push(S::axis_band_data),
                S::data => self.push(S::data_inner),
                _ => {}
            }
            self.emit(T::SQUARE_BRACES_START, p, p + 1);
            return true;
        }
        if c == 93 {
            if matches!(self.state, S::data_inner | S::axis_band_data) {
                self.pop();
            }
            self.emit(T::SQUARE_BRACES_END, p, p + 1);
            return true;
        }
        if self.state == S::axis_data && c == 45 && next == 45 && self.byte(p + 2) == 62 {
            self.emit(T::ARROW_DELIMITER, p, p + 3);
            return true;
        }
        if matches!(self.state, S::axis_data | S::data_inner)
            && let Some(e) = self.number(c)
        {
            self.emit(T::NUMBER_WITH_DECIMAL, p, e);
            return true;
        }
        if c == 96
            && let Some(e) = broken_rule(self.src, p)
        {
            self.p = e;
            self.pop();
            return true;
        }
        if c == 34 {
            self.p += 1;
            self.push(S::string);
            return true;
        }
        if is_letter(c) {
            let mut e = p + 1;
            while is_letter(self.byte(e)) {
                e += 1;
            }
            self.emit(T::ALPHA, p, e);
            return true;
        }
        if is_digit(c) {
            let e = self.digits(p + 1);
            self.emit(T::NUM, p, e);
            return true;
        }
        if is_space(c) {
            self.p = self.run(p + size);
            return true;
        }
        if let Some(kind) = single(c) {
            self.emit(kind, p, p + 1);
            return true;
        }
        false
    }
}

/// The tokens of an XY chart's text, as Pele's tokenize gives them. A text the lexer cannot
/// read ends with an ERROR token where it stopped. Any other ends with the token its state
/// gives at the end, and END.
pub(crate) fn tokenize(src: &str) -> Vec<Tok<'_>> {
    let n = src.len();
    let mut l = Lexer {
        src,
        toks: Vec::new(),
        stack: Vec::new(),
        state: S::INITIAL,
        p: 0,
    };
    while l.p < n && l.step() {}

    let mut toks = l.toks;
    if l.p < n {
        toks.push(Tok {
            kind: T::ERROR,
            text: "",
            start: l.p,
        });
        return toks;
    }
    // Jison tries the rules of the current state once more against the empty rest of the
    // input.
    toks.push(Tok {
        kind: at_end(l.state),
        text: "",
        start: n,
    });
    toks.push(Tok {
        kind: T::END,
        text: "",
        start: n,
    });
    toks
}
