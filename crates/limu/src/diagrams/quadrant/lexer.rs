//! The lexer of a quadrant chart, as Pele's lexer.ts has it: the rules of Mermaid's
//! quadrant.jison, tried by hand in the order Jison tries them, in the states it reads them
//! in. Positions are bytes. Where Pele reads a code unit with charCodeAt, unit reads the same
//! one, and a character outside ASCII is stepped over whole.

use super::tokens::{QUADRANTS, T};
use crate::common::word_at;
use crate::js::chars::{is_digit, is_letter, is_line_end, is_space, is_word, space_run, unit_at};

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
    title,
    acc_title,
    acc_descr,
    acc_descr_multiline,
    string,
    md_string,
    class_name,
    point_start,
    point_x,
    point_y,
}

/// The token a state gives for the empty rest of the input, where it gives one, Pele's
/// AT_END.
fn at_end(s: S) -> Option<T> {
    match s {
        S::INITIAL => Some(T::EOF),
        S::title => Some(T::title_value),
        S::acc_title => Some(T::acc_title_value),
        S::acc_descr => Some(T::acc_descr_value),
        S::acc_descr_multiline => Some(T::acc_descr_multiline_value),
        S::string => Some(T::STR),
        _ => None,
    }
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
        b' ' | b'\t' | 0x0b | 0x0c => T::SPACE,
        b';' => T::SEMI,
        b'!' | b'$' | b'%' | b'\'' | b'`' | b'?' | b'\\' | b'/' => T::PUNCTUATION,
        _ => return None,
    })
}

struct Lexer<'a> {
    src: &'a str,
    toks: Vec<Tok<'a>>,
    stack: Vec<S>,
    state: S,
    p: usize,
    // The last run scanned of each kind, so that a long run is not scanned again from each
    // of its characters.
    space_from: usize,
    space_to: usize,
    dash_from: usize,
    dash_to: usize,
    ws_from: usize,
    ws_to: usize,
    gap_from: usize,
    gap_to: usize,
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

    /// The end of the run of spaces at q.
    fn spaces(&self, mut q: usize) -> usize {
        while self.byte(q) == 32 {
            q += 1;
        }
        q
    }

    /// The end of the run of white space at q.
    fn run(&self, q: usize) -> usize {
        space_run(self.src, q)
    }

    /// Where Mermaid's `(1)|(0(.\d+)?)` ends at q, where the dot is any character within a
    /// line, as Pele's coordinate finds it.
    fn coordinate(&self, q: usize) -> Option<usize> {
        let c = self.byte(q);
        if c == 49 {
            return Some(q + 1);
        }
        if c != 48 {
            return None;
        }
        let mut e = q + 1;
        let (d, size) = unit_at(self.src, e);
        // The dot takes one code unit, so a character of two is followed by its second, not
        // by a digit.
        let pair = (0xd800..0xdc00).contains(&d);
        if d >= 0 && !is_line_end(d) && !pair && is_digit(self.byte(e + size)) {
            e += size + 1;
            while is_digit(self.byte(e)) {
                e += 1;
            }
        }
        Some(e)
    }

    /// The token of an axis, quadrant or chart keyword or of the axis arrow at p, and where
    /// it ends, the spaces before it taken.
    fn keyword(&mut self, c: i32) -> Option<(T, usize)> {
        let p = self.p;
        let mut j = p;
        if c == 32 {
            if p < self.space_from || p >= self.space_to {
                self.space_from = p;
                self.space_to = self.spaces(p);
            }
            j = self.space_to;
        }
        let src = self.src;
        let d = self.byte(j);
        let dl = d | 32;
        if dl == 120 && word_at(src, j, "x-axis") {
            return Some((T::X_AXIS, j + 6));
        }
        if dl == 121 && word_at(src, j, "y-axis") {
            return Some((T::Y_AXIS, j + 6));
        }
        if d == 45 {
            if j < self.dash_from || j >= self.dash_to {
                self.dash_from = j;
                self.dash_to = j;
                while self.byte(self.dash_to) == 45 {
                    self.dash_to += 1;
                }
            }
            if self.dash_to - j >= 2 && self.byte(self.dash_to) == 62 {
                return Some((T::DELIMITER, self.dash_to + 1));
            }
            return None;
        }
        if dl == 113 && word_at(src, j, "quadrant") {
            let q = self.byte(j + 9);
            if self.byte(j + 8) == 45 && (49..=52).contains(&q) {
                return Some((QUADRANTS[(q - 49) as usize], j + 10));
            }
            if word_at(src, j + 8, "chart") {
                return Some((T::QUADRANT, j + 13));
            }
        }
        None
    }

    /// Where a point's `: [` at p ends, with any white space before, between and after, line
    /// breaks included, or none.
    fn point_start(&mut self, c: i32) -> Option<usize> {
        let p = self.p;
        let mut k = p;
        if c != 58 {
            if p < self.ws_from || p >= self.ws_to {
                self.ws_from = p;
                self.ws_to = self.run(p);
            }
            k = self.ws_to;
        }
        if self.byte(k) != 58 {
            return None;
        }
        if k + 1 < self.gap_from || k + 1 >= self.gap_to {
            self.gap_from = k + 1;
            self.gap_to = self.run(k + 1);
        }
        (self.byte(self.gap_to) == 91).then(|| self.run(self.gap_to + 1))
    }

    /// Takes the token of the state at p. It is false where the state has no rule that
    /// matches, and the lexer stops.
    fn state_rule(&mut self, c: i32, next: i32) -> Option<bool> {
        let p = self.p;
        let n = self.src.len();
        match self.state {
            S::INITIAL => return None,
            S::title | S::acc_title | S::acc_descr => {
                let kind = at_end(self.state).unwrap_or(T::EOF);
                self.emit(kind, p, self.line_end(p));
                self.pop();
            }
            S::acc_descr_multiline => {
                if c == 125 {
                    self.p += 1;
                    self.pop();
                } else {
                    let e = self.src[p..].find('}').map_or(n, |e| p + e);
                    self.emit(T::acc_descr_multiline_value, p, e);
                }
            }
            S::string => {
                if c == 34 {
                    self.p += 1;
                    self.pop();
                } else {
                    let e = self.src[p..].find('"').map_or(n, |e| p + e);
                    self.emit(T::STR, p, e);
                }
            }
            S::md_string => {
                if c == 96 && next == 34 {
                    self.p += 2;
                    self.pop();
                } else if c != 96 && c != 34 {
                    // The bytes of a character outside ASCII are none of the two.
                    let mut e = p + 1;
                    while e < n && !matches!(self.byte(e), 96 | 34) {
                        e += 1;
                    }
                    self.emit(T::MD_STR, p, e);
                } else {
                    return Some(false);
                }
            }
            S::class_name => {
                if !is_word(c) {
                    return Some(false);
                }
                let mut e = p + 1;
                while is_word(self.byte(e)) {
                    e += 1;
                }
                self.emit(T::class_name, p, e);
                self.pop();
            }
            S::point_start => {
                if let Some(e) = self.coordinate(p) {
                    self.emit(T::point_x, p, e);
                    self.push(S::point_x);
                } else {
                    let k = self.run(p);
                    if self.byte(k) != 93 {
                        return Some(false);
                    }
                    self.p = self.spaces(k + 1);
                    self.pop();
                }
            }
            S::point_x => {
                let k = self.run(p);
                if self.byte(k) != 44 {
                    return Some(false);
                }
                self.p = self.run(k + 1);
                self.state = S::point_y;
            }
            S::point_y => {
                let Some(e) = self.coordinate(p) else {
                    return Some(false);
                };
                self.emit(T::point_y, p, e);
                self.pop();
            }
        }
        Some(true)
    }

    /// Takes a token of the initial state at p, as Pele's rules after its switch do. It is
    /// false where none matches.
    fn initial_rule(&mut self, c: i32, next: i32) -> bool {
        let p = self.p;
        let src = self.src;
        if c == 37 && next == 37 {
            self.p = self.line_end(p + 2);
            return true;
        }
        if c == 10 || c == 13 {
            let mut e = p + 1;
            while matches!(self.byte(e), 10 | 13) {
                e += 1;
            }
            self.emit(T::NEWLINE, p, e);
            return true;
        }

        let lower = c | 32;
        if lower == 116 && word_at(src, p, "title") && !is_word(self.byte(p + 5)) {
            self.emit(T::title, p, p + 5);
            self.push(S::title);
            return true;
        }
        if lower == 97 && word_at(src, p, "acc") {
            let descr = word_at(src, p + 3, "descr");
            if descr || word_at(src, p + 3, "title") {
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
            }
        }

        // Axis, quadrant and chart keywords and the axis arrow take the spaces around them.
        if let Some((kind, end)) = self.keyword(c) {
            self.emit(kind, p, self.spaces(end));
            return true;
        }

        if lower == 99 && word_at(src, p, "classdef") && !is_word(self.byte(p + 8)) {
            self.emit(T::CLASSDEF, p, p + 8);
            return true;
        }
        if c == 34 {
            if next == 96 {
                self.p += 2;
                self.push(S::md_string);
            } else {
                self.p += 1;
                self.push(S::string);
            }
            return true;
        }
        if c == 58 && next == 58 && self.byte(p + 2) == 58 {
            self.p += 3;
            self.push(S::class_name);
            return true;
        }
        if (c == 58 || is_space(c))
            && let Some(e) = self.point_start(c)
        {
            self.emit(T::point_start, p, e);
            self.push(S::point_start);
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
        if c >= 128 {
            // Every byte of a character outside ASCII is 128 or more, as each of its code
            // units is.
            let mut e = p + 1;
            while self.byte(e) >= 128 {
                e += 1;
            }
            self.emit(T::UNICODE_TEXT, p, e);
            return true;
        }
        if is_digit(c) {
            let mut e = p + 1;
            while is_digit(self.byte(e)) {
                e += 1;
            }
            self.emit(T::NUM, p, e);
            return true;
        }
        if let Some(kind) = single(c) {
            self.emit(kind, p, p + 1);
            return true;
        }
        false
    }
}

/// The tokens of a quadrant chart's text, as Pele's tokenize gives them. A text the lexer
/// cannot read ends with an ERROR token where it stopped. Any other ends with the token its
/// state gives at the end, where it gives one, and END.
pub(crate) fn tokenize(src: &str) -> Vec<Tok<'_>> {
    let n = src.len();
    let mut l = Lexer {
        src,
        toks: Vec::new(),
        stack: Vec::new(),
        state: S::INITIAL,
        p: 0,
        space_from: 0,
        space_to: 0,
        dash_from: 0,
        dash_to: 0,
        ws_from: 0,
        ws_to: 0,
        gap_from: 0,
        gap_to: 0,
    };
    while l.p < n {
        let c = unit_at(src, l.p).0;
        let next = l.byte(l.p + 1);
        let taken = match l.state_rule(c, next) {
            Some(taken) => taken,
            None => l.initial_rule(c, next),
        };
        if !taken {
            break;
        }
    }

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
    if let Some(kind) = at_end(l.state) {
        toks.push(Tok {
            kind,
            text: "",
            start: n,
        });
    }
    toks.push(Tok {
        kind: T::END,
        text: "",
        start: n,
    });
    toks
}
