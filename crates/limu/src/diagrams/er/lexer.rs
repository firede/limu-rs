//! The lexer of an entity relationship diagram, as Pele's lexer.ts has it: the rules of
//! Mermaid's erDiagram.jison, tried by hand in the order Jison tries them, in the states it
//! reads them in. Positions are bytes. Where Pele reads a code unit with charCodeAt, unit
//! reads the same one, and a character outside ASCII is stepped over whole.

use super::tokens::{DIRECTION_TOKENS, T};
use crate::js::chars::{
    has_prefix_fold, is_digit, is_line_end, is_space, is_word, space_run, unit, unit_at,
};

/// A token: its type, its text, and the byte of the text it starts at.
#[derive(Clone, Debug, PartialEq)]
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
    block,
    block_bq,
    acc_title,
    acc_descr,
    acc_descr_multiline,
    style,
}

// The classes of an ASCII character, what it may be part of, as Pele's CLASS has them.

/// A character of UNICODE_TEXT, outside a block.
const TEXT: u8 = 1;
/// A character of STYLE_TEXT.
const STYLE_TEXT: u8 = 2;
/// The first character of an ATTRIBUTE_WORD.
const ATTR_START: u8 = 4;
/// The other characters of an ATTRIBUTE_WORD.
const ATTR_REST: u8 = 8;

/// The classes of an ASCII character, Pele's CLASS.
fn class(c: u8) -> u8 {
    let mut k = 0;
    if c.is_ascii_alphanumeric() || c == b'_' {
        k |= TEXT | STYLE_TEXT | ATTR_REST;
        if !c.is_ascii_digit() {
            k |= ATTR_START;
        }
    }
    match c {
        b'-' => k |= TEXT | STYLE_TEXT | ATTR_REST,
        b'*' => k |= TEXT | STYLE_TEXT | ATTR_REST | ATTR_START,
        b'.' => k |= TEXT | ATTR_REST,
        b'[' | b']' | b'(' | b')' | b',' => k |= ATTR_REST,
        _ => {}
    }
    k
}

/// Whether code unit c is of the class kind, every code unit past ASCII being of it, as
/// Pele's tokenize tests `c >= 128 || (CLASS[c] & kind) !== 0`. Past the end it is of none.
fn is_class(c: i32, kind: u8) -> bool {
    c >= 128 || (0..128).contains(&c) && class(c as u8) & kind != 0
}

/// Whether code unit c is of the attribute class kind, as Pele's isAttr says. Mermaid's
/// attribute pattern is case-insensitive, which lets the micro sign into its À-￿ range.
fn is_attr(c: i32, kind: u8) -> bool {
    if (0..128).contains(&c) {
        class(c as u8) & kind != 0
    } else {
        c >= 0xc0 || c == 0xb5
    }
}

/// The names after `direction` in Pele's RE_DIRECTION, in its order.
const DIRECTION_NAMES: [&str; 4] = ["TB", "BT", "RL", "LR"];

/// The state of Pele's tokenize.
struct Lexer<'a> {
    src: &'a str,
    n: usize,
    toks: Vec<Tok<'a>>,
    state: S,
    p: usize,
    // The caches that keep the lexer linear: where the next `direction` is, the end of the
    // line it failed on, the end of the line p is on, and for generic types the next tilde,
    // the end of its line and the last tilde before it, and the next white space. Pele's -2
    // and -1 are none, or not yet looked for.
    dir_next: Option<Option<usize>>,
    dir_failed: usize,
    line_end: Option<usize>,
    tilde_next: Option<Option<usize>>,
    tilde_line: Option<usize>,
    tilde_last: Option<usize>,
    ws_next: Option<usize>,
}

/// Splits an entity relationship diagram's text into tokens, as Pele's tokenize does, with
/// Jison's end of input after them.
pub(crate) fn tokenize(src: &str) -> Vec<Tok<'_>> {
    let mut l = Lexer {
        src,
        n: src.len(),
        toks: Vec::new(),
        state: S::INITIAL,
        p: 0,
        dir_next: None,
        dir_failed: 0,
        line_end: None,
        tilde_next: None,
        tilde_line: None,
        tilde_last: None,
        ws_next: None,
    };
    while l.p < l.n {
        let (c, size) = unit_at(src, l.p);
        match l.state {
            S::INITIAL => l.initial(c, size),
            S::block => l.block(c, size),
            S::block_bq => {
                if c == 96 {
                    l.state = S::block;
                    l.p += 1;
                } else {
                    let e = l.index_or(l.p, b'`', l.n);
                    l.emit(T::ATTRIBUTE_WORD, l.p, e);
                }
            }
            S::acc_title | S::acc_descr => {
                let kind = if l.state == S::acc_title {
                    T::acc_title_value
                } else {
                    T::acc_descr_value
                };
                let e = l.index_or(l.p, b'\n', l.n);
                l.emit(kind, l.p, e);
                l.state = S::INITIAL;
            }
            S::acc_descr_multiline => {
                if c == 125 {
                    l.state = S::INITIAL;
                    l.p += 1;
                } else {
                    let e = l.index_or(l.p, b'}', l.n);
                    l.emit(T::acc_descr_multiline_value, l.p, e);
                }
            }
            S::style => match c {
                10 => {
                    let e = l.run_of(l.p + 1, b'\n');
                    l.state = S::INITIAL;
                    l.emit(T::NEWLINE, l.p, e);
                }
                _ if is_space(c) => l.p = space_run(src, l.p),
                58 => l.emit(T::COLON, l.p, l.p + 1),
                44 => l.emit(T::COMMA, l.p, l.p + 1),
                35 => l.emit(T::BRKT, l.p, l.p + 1),
                59 => l.emit(T::SEMI, l.p, l.p + 1),
                _ if is_class(c, STYLE_TEXT) => {
                    let e = l.class_run(l.p + size, STYLE_TEXT);
                    l.emit(T::STYLE_TEXT, l.p, e);
                }
                _ => {
                    l.toks.push(Tok {
                        kind: T::ERROR,
                        text: "",
                        start: l.p,
                    });
                    return l.toks;
                }
            },
        }
    }
    // At the end of input Jison's INITIAL state yields EOF, the title and description states
    // yield an empty value, and the block and style states yield nothing.
    let last = match l.state {
        S::INITIAL => Some(T::EOF),
        S::acc_title => Some(T::acc_title_value),
        S::acc_descr => Some(T::acc_descr_value),
        S::acc_descr_multiline => Some(T::acc_descr_multiline_value),
        _ => None,
    };
    let n = l.n;
    if let Some(kind) = last {
        l.toks.push(Tok {
            kind,
            text: "",
            start: n,
        });
    }
    l.toks.push(Tok {
        kind: T::END,
        text: "",
        start: n,
    });
    l.toks
}

impl<'a> Lexer<'a> {
    /// Adds a token of a type for the text from s to e and moves the lexer to e, as the emit
    /// of Pele's tokenize does.
    fn emit(&mut self, kind: T, s: usize, e: usize) {
        self.toks.push(Tok {
            kind,
            text: &self.src[s..e],
            start: s,
        });
        self.p = e;
    }

    /// The code unit at byte q, or -1 past the end.
    fn unit(&self, q: usize) -> i32 {
        unit(self.src, q)
    }

    /// The first byte b at or after q, or none: Pele's src.indexOf(b, q).
    fn index(&self, q: usize, b: u8) -> Option<usize> {
        self.src.as_bytes()[q..]
            .iter()
            .position(|&c| c == b)
            .map(|i| q + i)
    }

    /// [`Lexer::index`], with what Pele puts in place of -1.
    fn index_or(&self, q: usize, b: u8, or: usize) -> usize {
        self.index(q, b).unwrap_or(or)
    }

    /// The end of the run of byte b that starts at q.
    fn run_of(&self, mut q: usize, b: u8) -> usize {
        while self.src.as_bytes().get(q) == Some(&b) {
            q += 1;
        }
        q
    }

    /// The end of the run of characters of class kind that starts at q, as the loops of
    /// Pele's tokenize that read UNICODE_TEXT and STYLE_TEXT find it.
    fn class_run(&self, mut q: usize, kind: u8) -> usize {
        while q < self.n {
            let (c, size) = unit_at(self.src, q);
            if !is_class(c, kind) {
                break;
            }
            q += size;
        }
        q
    }

    /// Whether a lower-case keyword is at p, its case ignored, as the kw of Pele's tokenize
    /// says.
    fn kw(&self, word: &str) -> bool {
        has_prefix_fold(&self.src[self.p..], word)
    }

    /// Whether a keyword is at p that no word character follows, as the kwb of Pele's
    /// tokenize says.
    fn kwb(&self, word: &str) -> bool {
        self.kw(word) && !is_word(self.unit(self.p + word.len()))
    }

    /// The code unit at q, lowered if it is an ASCII capital, as the lower of Pele's tokenize
    /// gives it.
    fn lower(&self, q: usize) -> i32 {
        let c = self.unit(q);
        if (65..=90).contains(&c) { c + 32 } else { c }
    }

    /// The first line terminator at or after q, or the end, as the nextLineEnd of Pele's
    /// tokenize finds it with RE_LINE_END.
    fn next_line_end(&self, mut q: usize) -> usize {
        while q < self.n {
            let (c, size) = unit_at(self.src, q);
            if is_line_end(c) {
                return q;
            }
            q += size;
        }
        self.n
    }

    /// The end of the run of ASCII digits from q, as the digits of Pele's tokenize gives it.
    fn digits(&self, mut q: usize) -> usize {
        while self.src.as_bytes().get(q).is_some_and(u8::is_ascii_digit) {
            q += 1;
        }
        q
    }

    /// The first white space at or after q, or the end, as the loops in the generic of Pele's
    /// tokenize find it.
    fn space_next(&self, mut q: usize) -> usize {
        while q < self.n {
            let (c, size) = unit_at(self.src, q);
            if is_space(c) {
                break;
            }
            q += size;
        }
        q
    }

    /// The first "direction" at or after q, its case ignored, as Pele's RE_DIR_WORD finds it.
    fn index_fold(&self, mut q: usize) -> Option<usize> {
        while q + 9 <= self.n {
            if has_prefix_fold(&self.src[q..], "direction") {
                return Some(q);
            }
            q += unit_at(self.src, q).1;
        }
        None
    }

    /// /<word>\s*<sep>\s*/iy at q, Pele's RE_ACC_TITLE, RE_ACC_DESCR and RE_ACC_DESCR_ML: the
    /// end of the match.
    fn word_fold(&self, q: usize, word: &str, sep: u8) -> Option<usize> {
        if !has_prefix_fold(&self.src[q..], word) {
            return None;
        }
        let j = space_run(self.src, q + word.len());
        (self.src.as_bytes().get(j) == Some(&sep)).then(|| space_run(self.src, j + 1))
    }

    /// /.*direction\s+<name>[^\n]*/iy at q, one of Pele's RE_DIRECTION: the last `direction`
    /// on the line that white space and the name follow, as the greedy .* backs off to it,
    /// and the rest of the line its match ends on. The white space may run onto further
    /// lines.
    fn direction_at(&self, q: usize, name: &str) -> Option<usize> {
        let end = self.next_line_end(q);
        (q..end).rev().find_map(|x| {
            if !self.src.as_bytes()[x..]
                .get(..9)
                .is_some_and(|w| w.eq_ignore_ascii_case(b"direction"))
            {
                return None;
            }
            let j = space_run(self.src, x + 9);
            if j == x + 9 || !has_prefix_fold(&self.src[j..], name) {
                return None;
            }
            Some(self.index_or(j + 2, b'\n', self.n))
        })
    }

    /// Reads a direction statement, which swallows its whole line from wherever the lexer
    /// stands on it, as the direction of Pele's tokenize does.
    fn direction(&mut self) -> bool {
        if self.dir_next == Some(None) {
            return false;
        }
        if self.dir_next.is_none_or(|d| d.is_some_and(|d| d < self.p)) {
            let next = self.index_fold(self.p);
            self.dir_next = Some(next);
            if next.is_none() {
                return false;
            }
        }
        if self.p < self.dir_failed {
            return false;
        }
        if self.line_end.is_none_or(|e| e < self.p) {
            self.line_end = Some(self.next_line_end(self.p));
        }
        let line_end = self.line_end.unwrap_or(self.n);
        if self.dir_next.flatten().is_some_and(|d| d >= line_end) {
            return false;
        }
        for (i, name) in DIRECTION_NAMES.iter().enumerate() {
            if let Some(e) = self.direction_at(self.p, name) {
                self.emit(DIRECTION_TOKENS[i], self.p, e);
                return true;
            }
        }
        self.dir_failed = line_end;
        false
    }

    /// Reads a double quote at p, as the quoted of Pele's tokenize does: an ENTITY_NAME
    /// through the next quote when the text between is not empty and holds none of % \ \r \n
    /// \v \b, a WORD through it when it does, and a CHAR when no quote closes it.
    fn quoted(&mut self) {
        let p = self.p;
        let Some(e) = self.index(p + 1, b'"') else {
            self.emit(T::CHAR, p, p + 1);
            return;
        };
        let name = e > p + 1
            && !self.src.as_bytes()[p + 1..e]
                .iter()
                .any(|c| matches!(c, b'%' | b'\\' | b'\r' | b'\n' | 0x0b | 0x08));
        self.emit(if name { T::ENTITY_NAME } else { T::WORD }, p, e + 1);
    }

    /// Reads what starts with the digit c at p, as the number of Pele's tokenize does: the
    /// cardinalities 1+ and 0+, a DECIMAL_NUM, a 1 that is a cardinality, or a NUM.
    fn number(&mut self, c: i32) {
        let p = self.p;
        let (d, dsize) = unit_at(self.src, p + 1);
        if d == 43 && (c == 49 || c == 48) {
            let kind = if c == 49 {
                T::ONE_OR_MORE
            } else {
                T::ZERO_OR_MORE
            };
            self.emit(kind, p, p + 2);
            return;
        }
        let q = self.digits(p + 1);
        if self.unit(q) == 46 && is_digit(self.unit(q + 1)) {
            let e = self.digits(q + 2);
            self.emit(T::DECIMAL_NUM, p, e);
            return;
        }
        if c == 49 {
            if is_space(d) {
                let f = self.unit(space_run(self.src, p + 1 + dsize));
                if is_word(f) || f == 34 || f == 39 {
                    self.emit(T::ONLY_ONE, p, p + 1);
                    return;
                }
            } else if d == 45 || d == 46 {
                let f = self.unit(p + 2);
                if f == 45 || f == 46 {
                    self.emit(T::ONLY_ONE, p, p + 1);
                    return;
                }
            }
            if !is_word(d) {
                self.emit(T::ENTITY_ONE, p, p + 1);
                return;
            }
        }
        self.emit(T::NUM, p, q);
    }

    /// Reads a keyword, a cardinality or a relationship's mark at p, whose first code unit
    /// lowered is lc, as the keyword of Pele's tokenize does, and says whether it found one.
    fn keyword(&mut self, lc: i32) -> bool {
        let p = self.p;
        let d = self.unit(p + 1);
        let (kind, e) = match lc {
            101 => {
                if self.kwb("erdiagram") {
                    (T::ER_DIAGRAM, p + 9)
                } else if self.kwb("end") {
                    (T::END_KW, space_run(self.src, p + 3))
                } else {
                    return false;
                }
            }
            115 => {
                if self.kwb("style") {
                    self.state = S::style;
                    (T::STYLE, p + 5)
                } else if self.kwb("subgraph") {
                    (T::SUBGRAPH, p + 8)
                } else {
                    return false;
                }
            }
            99 => {
                if self.kwb("classdef") {
                    self.state = S::style;
                    (T::CLASSDEF, p + 8)
                } else if self.kwb("class") {
                    (T::CLASS, p + 5)
                } else {
                    return false;
                }
            }
            111 => {
                if self.kwb("one or zero") {
                    (T::ZERO_OR_ONE, p + 11)
                } else if self.kwb("one or more") || self.kwb("one or many") {
                    (T::ONE_OR_MORE, p + 11)
                } else if self.kwb("one") {
                    (T::ONLY_ONE, p + 3)
                } else if self.kwb("only one") {
                    (T::ONLY_ONE, p + 8)
                } else if d == 124 {
                    (T::ZERO_OR_ONE, p + 2)
                } else if d == 123 {
                    (T::ZERO_OR_MORE, p + 2)
                } else if self.kwb("optionally to") {
                    (T::NON_IDENTIFYING, p + 13)
                } else {
                    return false;
                }
            }
            122 => {
                if self.kwb("zero or one") {
                    (T::ZERO_OR_ONE, p + 11)
                } else if self.kwb("zero or more") || self.kwb("zero or many") {
                    (T::ZERO_OR_MORE, p + 12)
                } else {
                    return false;
                }
            }
            109 => {
                if self.kw("many(0)") {
                    (T::ZERO_OR_MORE, p + 7)
                } else if self.kw("many(1)") {
                    (T::ONE_OR_MORE, p + 7)
                } else if self.kwb("many") {
                    (T::ZERO_OR_MORE, p + 4)
                } else {
                    return false;
                }
            }
            116 => {
                if !self.kwb("to") {
                    return false;
                }
                (T::IDENTIFYING, p + 2)
            }
            117 => {
                if d != 46 && d != 45 && d != 124 {
                    return false;
                }
                (T::MD_PARENT, p + 1)
            }
            124 => {
                if self.lower(p + 1) == 111 && !is_word(self.unit(p + 2)) {
                    (T::ZERO_OR_ONE, p + 2)
                } else if d == 124 {
                    (T::ONLY_ONE, p + 2)
                } else if d == 123 {
                    (T::ONE_OR_MORE, p + 2)
                } else {
                    return false;
                }
            }
            125 => {
                if self.lower(p + 1) == 111 && !is_word(self.unit(p + 2)) {
                    (T::ZERO_OR_MORE, p + 2)
                } else if d == 124 {
                    (T::ONE_OR_MORE, p + 2)
                } else {
                    return false;
                }
            }
            46 => {
                if d != 46 && d != 45 {
                    return false;
                }
                (T::NON_IDENTIFYING, p + 2)
            }
            45 => match d {
                45 => (T::IDENTIFYING, p + 2),
                46 => (T::NON_IDENTIFYING, p + 2),
                _ => return false,
            },
            _ => return false,
        };
        self.emit(kind, p, e);
        true
    }

    /// Reads one token in the INITIAL state from code unit c, size bytes long, at p, as the
    /// initial of Pele's tokenize does.
    fn initial(&mut self, c: i32, size: usize) {
        let p = self.p;
        if (c == 97 || c == 65) && self.kw("acc") {
            if let Some(e) = self.word_fold(p, "accTitle", b':') {
                self.state = S::acc_title;
                self.emit(T::acc_title, p, e);
                return;
            }
            if let Some(e) = self.word_fold(p, "accDescr", b':') {
                self.state = S::acc_descr;
                self.emit(T::acc_descr, p, e);
                return;
            }
            if let Some(e) = self.word_fold(p, "accDescr", b'{') {
                self.state = S::acc_descr_multiline;
                self.p = e;
                return;
            }
        }
        if self.direction() {
            return;
        }
        match c {
            32 | 9 | 13 => {
                let mut e = p + 1;
                while matches!(self.src.as_bytes().get(e), Some(b' ' | b'\t' | b'\r')) {
                    e += 1;
                }
                self.p = e;
            }
            10 => {
                let e = self.run_of(p + 1, b'\n');
                self.emit(T::NEWLINE, p, e);
            }
            34 => self.quoted(),
            123 => {
                self.state = S::block;
                self.emit(T::BLOCK_START, p, p + 1);
            }
            35 => self.emit(T::BRKT, p, p + 1),
            44 => self.emit(T::COMMA, p, p + 1),
            58 => {
                if self.src[p..].starts_with(":::") {
                    self.emit(T::STYLE_SEPARATOR, p, p + 3);
                } else {
                    self.emit(T::COLON, p, p + 1);
                }
            }
            91 => self.emit(T::SQS, p, p + 1),
            93 => self.emit(T::SQE, p, p + 1),
            _ if is_digit(c) => self.number(c),
            _ => {
                let lc = if (65..=90).contains(&c) { c + 32 } else { c };
                if self.keyword(lc) {
                    return;
                }
                if is_class(c, TEXT) {
                    let e = self.class_run(p + size, TEXT);
                    self.emit(T::UNICODE_TEXT, p, e);
                    return;
                }
                self.emit(T::CHAR, p, p + size);
            }
        }
    }

    /// Reads Mermaid's generic type rule: a run of non-space characters holding a `~`,
    /// through the last `~` on the line, as the generic of Pele's tokenize does.
    fn generic(&mut self) -> bool {
        if self.tilde_next == Some(None) {
            return false;
        }
        if self
            .tilde_next
            .is_none_or(|t| t.is_some_and(|t| t < self.p))
        {
            let next = self.index(self.p, b'~');
            self.tilde_next = Some(next);
            if next.is_none() {
                return false;
            }
        }
        let Some(Some(tilde_next)) = self.tilde_next else {
            return false;
        };
        if self.ws_next.is_none_or(|w| w < self.p) {
            self.ws_next = Some(self.space_next(self.p));
        }
        if self.ws_next.is_some_and(|w| tilde_next >= w) {
            return false;
        }
        if self.tilde_line.is_none_or(|t| t < self.p) {
            let line = self.next_line_end(self.p);
            self.tilde_line = Some(line);
            self.tilde_last = self.src.as_bytes()[..line].iter().rposition(|&c| c == b'~');
        }
        let Some(last) = self.tilde_last.filter(|&t| t > tilde_next) else {
            return false;
        };
        let e = self.space_next(last + 1);
        self.emit(T::ATTRIBUTE_WORD, self.p, e);
        true
    }

    /// Reads one token in an entity's block of attributes from code unit c, size bytes long,
    /// at p, as the block of Pele's tokenize does.
    fn block(&mut self, c: i32, size: usize) {
        let p = self.p;
        if is_space(c) {
            self.p = space_run(self.src, p);
            return;
        }
        let lc = c | 32;
        if (lc == 112 || lc == 102 || lc == 117)
            && self.unit(p + 1) | 32 == 107
            && !is_word(self.unit(p + 2))
        {
            self.emit(T::ATTRIBUTE_KEY, p, p + 2);
            return;
        }
        if self.generic() {
            return;
        }
        if is_attr(c, ATTR_START) {
            let mut e = p + size;
            while e < self.n {
                let (d, dsize) = unit_at(self.src, e);
                if !is_attr(d, ATTR_REST) {
                    break;
                }
                e += dsize;
            }
            self.emit(T::ATTRIBUTE_WORD, p, e);
            return;
        }
        match c {
            96 => {
                self.state = S::block_bq;
                self.p += 1;
                return;
            }
            34 => {
                if let Some(e) = self.index(p + 1, b'"') {
                    self.emit(T::COMMENT, p, e + 1);
                    return;
                }
            }
            125 => {
                self.state = S::INITIAL;
                self.emit(T::BLOCK_STOP, p, p + 1);
                return;
            }
            _ => {}
        }
        self.emit(T::CHAR, p, p + size);
    }
}
