//! The lexer of a requirement diagram, as Pele's lexer.ts has it: the rules of Mermaid's
//! requirementDiagram.jison, tried by hand in the order Jison tries them, in the states it
//! reads them in. Positions are bytes. Where Pele reads a code unit with charCodeAt, unit
//! reads the same one, and a character outside ASCII is stepped over whole.

use super::tokens::{LONGEST_KEYWORD, T, keyword};
use crate::js::chars::{has_prefix_fold, is_line_end, is_space, is_word, space_run, trim, unit_at};

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
    string,
    style,
    acc_title,
    acc_descr,
    acc_descr_multiline,
}

/// The tokens of the directions, in the order of the kinds of direction statement.
const DIRECTIONS: [T; 4] = [
    T::direction_tb,
    T::direction_bt,
    T::direction_rl,
    T::direction_lr,
];

/// Whether an ASCII character ends an unquoted string, Pele's STOP: `:,{<>-=` and line
/// breaks.
fn stops(c: u8) -> bool {
    b":,\r\n{<>-=".contains(&c)
}

/// /word\s*<sep>\s*/iy, Pele's RE_ACC_TITLE, RE_ACC_DESCR and RE_ACC_DESCR_ML: where the match
/// at q ends. The i flag without u folds only ASCII letters.
fn keyword_sep(s: &str, q: usize, word: &str, sep: u8) -> Option<usize> {
    if !has_prefix_fold(&s[q..], word) {
        return None;
    }
    let j = space_run(s, q + word.len());
    (s.as_bytes().get(j) == Some(&sep)).then(|| space_run(s, j + 1))
}

/// Where each kind of direction statement starts and where its keyword ends, as Pele finds
/// them in one pass with RE_DIRECTION, /direction\s+(?:(TB)|(BT)|(RL)|LR)/gi. The kinds are
/// TB, BT, RL and LR, as the groups that match say.
fn directions(s: &str) -> [Vec<(usize, usize)>; 4] {
    let b = s.as_bytes();
    let mut found: [Vec<(usize, usize)>; 4] = Default::default();
    let mut i = 0;
    while i + 9 <= b.len() {
        if !b[i..i + 9].eq_ignore_ascii_case(b"direction") {
            i += 1;
            continue;
        }
        // \s+ takes the whole run, and backing off it leaves a space where a name would be.
        let j = space_run(s, i + 9);
        let kind = match b.get(j..j + 2).map(|n| [n[0] | 32, n[1] | 32]) {
            _ if j == i + 9 => None,
            Some([b't', b'b']) => Some(0),
            Some([b'b', b't']) => Some(1),
            Some([b'r', b'l']) => Some(2),
            Some([b'l', b'r']) => Some(3),
            _ => None,
        };
        match kind {
            Some(k) => {
                found[k].push((i, j + 2));
                i = j + 2;
            }
            None => i += 1,
        }
    }
    found
}

struct Lexer<'a> {
    src: &'a str,
    toks: Vec<Tok<'a>>,
    stack: Vec<S>,
    state: S,
    p: usize,
    /// The direction statements of each kind, as starts and the ends of their keywords.
    dirs: [Vec<(usize, usize)>; 4],
    /// How many statements of each kind start on or before the end of the line.
    dir_seen: [usize; 4],
    any_dir: bool,
    /// The end of the line the statements were last counted to.
    line_end: Option<usize>,
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

    fn unit(&self, q: usize) -> i32 {
        unit_at(self.src, q).0
    }

    /// The line feed at or after q, or the end.
    fn line_break(&self, q: usize) -> usize {
        self.src[q..].find('\n').map_or(self.src.len(), |e| q + e)
    }

    /// `.*direction\s+XX[^\n]*`: the last such statement that starts on this line decides the
    /// token, as Pele's direction finds it.
    fn direction(&mut self) -> bool {
        let p = self.p;
        if self.line_end.is_none_or(|e| e < p) {
            // RE_LINE_END, /[\n\r  ]/g.
            let mut q = p;
            let end = loop {
                let (c, size) = unit_at(self.src, q);
                if c < 0 {
                    break self.src.len();
                }
                if is_line_end(c) {
                    break q;
                }
                q += size;
            };
            self.line_end = Some(end);
            for k in 0..4 {
                let list = &self.dirs[k];
                let mut j = self.dir_seen[k];
                while j < list.len() && list[j].0 <= end {
                    j += 1;
                }
                self.dir_seen[k] = j;
            }
        }
        let found = (0..4).find_map(|k| {
            let j = self.dir_seen[k].checked_sub(1)?;
            let (start, end) = self.dirs[k][j];
            (start >= p).then_some((DIRECTIONS[k], end))
        });
        let Some((kind, end)) = found else {
            return false;
        };
        let e = self.line_break(end);
        self.emit(kind, p, e);
        true
    }

    fn initial(&mut self, c: i32) -> bool {
        let (src, p, n) = (self.src, self.p, self.src.len());
        let b = src.as_bytes();
        let lower = c | 32;
        if lower == 116
            && b.get(p + 1..p + 5)
                .is_some_and(|w| w.eq_ignore_ascii_case(b"itle"))
            && is_space(self.unit(p + 5))
        {
            let from = p + 5 + unit_at(src, p + 5).1;
            let mut q = from;
            while q < n && !matches!(b[q], b'#' | b'\n' | b';') {
                q += 1;
            }
            if q > from {
                self.emit(T::title, p, q);
                return true;
            }
        }
        if lower == 97 && self.unit(p + 1) | 32 == 99 && self.unit(p + 2) | 32 == 99 {
            if let Some(e) = keyword_sep(src, p, "accTitle", b':') {
                self.push(S::acc_title);
                self.emit(T::acc_title, p, e);
                return true;
            }
            if let Some(e) = keyword_sep(src, p, "accDescr", b':') {
                self.push(S::acc_descr);
                self.emit(T::acc_descr, p, e);
                return true;
            }
            if let Some(e) = keyword_sep(src, p, "accDescr", b'{') {
                self.push(S::acc_descr_multiline);
                self.p = e;
                return true;
            }
        }
        if self.any_dir && !is_line_end(c) && self.direction() {
            return true;
        }

        if c == 10 || (c == 13 && b.get(p + 1) == Some(&b'\n')) {
            let mut q = p;
            while q < n {
                if b[q] == b'\n' {
                    q += 1;
                } else if b[q] == b'\r' && b.get(q + 1) == Some(&b'\n') {
                    q += 2;
                } else {
                    break;
                }
            }
            self.emit(T::NEWLINE, p, q);
            return true;
        }
        if is_space(c) {
            self.p = space_run(src, p);
            return true;
        }
        if c == 35 || c == 37 {
            self.p = self.line_break(p);
            return true;
        }

        if is_word(c) {
            let mut q = p + 1;
            while q < n && is_word(i32::from(b[q])) {
                q += 1;
            }
            if q - p <= LONGEST_KEYWORD
                && let Some(k) = keyword(&src[p..q].to_ascii_lowercase())
            {
                if matches!(k, T::STYLE | T::CLASSDEF | T::CLASS) {
                    self.push(S::style);
                }
                self.emit(k, p, q);
                return true;
            }
            while q < n && !(b[q] < 128 && stops(b[q])) {
                q += 1;
            }
            self.toks.push(Tok {
                kind: T::unqString,
                text: trim(&src[p..q]),
                start: p,
            });
            self.p = q;
            return true;
        }

        match c {
            123 => self.emit(T::STRUCT_START, p, p + 1),
            125 => self.emit(T::STRUCT_STOP, p, p + 1),
            58 => {
                if b.get(p + 1..p + 3) == Some(b"::") {
                    self.emit(T::STYLE_SEPARATOR, p, p + 3);
                } else {
                    self.emit(T::COLONSEP, p, p + 1);
                }
            }
            60 => {
                if b.get(p + 1) != Some(&b'-') {
                    return false;
                }
                self.emit(T::END_ARROW_L, p, p + 2);
            }
            45 => {
                if b.get(p + 1) == Some(&b'>') {
                    self.emit(T::END_ARROW_R, p, p + 2);
                } else {
                    self.emit(T::LINE, p, p + 1);
                }
            }
            34 => {
                self.push(S::string);
                self.p += 1;
            }
            44 => self.emit(T::COMMA, p, p + 1),
            _ => return false,
        }
        true
    }

    fn style(&mut self, c: i32) -> bool {
        let p = self.p;
        if is_word(c) {
            let b = self.src.as_bytes();
            let mut q = p + 1;
            while q < b.len() && is_word(i32::from(b[q])) {
                q += 1;
            }
            self.emit(T::ALPHA, p, q);
            return true;
        }
        match c {
            58 => self.emit(T::COLON, p, p + 1),
            59 => self.emit(T::SEMICOLON, p, p + 1),
            37 => self.emit(T::PERCENT, p, p + 1),
            45 => self.emit(T::MINUS, p, p + 1),
            35 => self.emit(T::BRKT, p, p + 1),
            32 => self.p += 1,
            34 => {
                self.push(S::string);
                self.p += 1;
            }
            10 => {
                self.pop();
                self.p += 1;
            }
            44 => self.emit(T::COMMA, p, p + 1),
            _ => return false,
        }
        true
    }

    /// From p up to the next of a character, or the end, as a token of a type.
    fn until(&mut self, kind: T, stop: char) {
        let p = self.p;
        let e = self.src[p..].find(stop).map_or(self.src.len(), |e| p + e);
        self.emit(kind, p, e);
    }
}

/// The token stream of Mermaid's requirement lexer, as Pele's tokenize makes it. Its rules
/// are tried in order and the first match wins, so a line that holds a direction statement
/// anywhere is one token. A character no rule reads ends the stream with an ERROR token.
pub(crate) fn tokenize(src: &str) -> Vec<Tok<'_>> {
    let dirs = directions(src);
    let any_dir = dirs.iter().any(|d| !d.is_empty());
    let mut lx = Lexer {
        src,
        toks: Vec::new(),
        stack: Vec::new(),
        state: S::INITIAL,
        p: 0,
        dirs,
        dir_seen: [0; 4],
        any_dir,
        line_end: None,
    };
    let n = src.len();
    while lx.p < n {
        let c = lx.unit(lx.p);
        let ok = match lx.state {
            S::INITIAL => lx.initial(c),
            S::style => lx.style(c),
            S::string => {
                if c == 34 {
                    lx.pop();
                    lx.p += 1;
                } else {
                    lx.until(T::qString, '"');
                }
                true
            }
            S::acc_title => {
                lx.pop();
                let e = lx.line_break(lx.p);
                lx.emit(T::acc_title_value, lx.p, e);
                true
            }
            S::acc_descr => {
                lx.pop();
                let e = lx.line_break(lx.p);
                lx.emit(T::acc_descr_value, lx.p, e);
                true
            }
            S::acc_descr_multiline => {
                if c == 125 {
                    lx.pop();
                    lx.p += 1;
                } else {
                    lx.until(T::acc_descr_multiline_value, '}');
                }
                true
            }
        };
        if !ok {
            lx.toks.push(Tok {
                kind: T::ERROR,
                text: "",
                start: lx.p,
            });
            return lx.toks;
        }
    }

    // At the end of input Jison still tries the rules that can match nothing, once.
    let last = match lx.state {
        S::INITIAL => Some(T::EOF),
        S::string => Some(T::qString),
        S::acc_title => Some(T::acc_title_value),
        S::acc_descr => Some(T::acc_descr_value),
        S::acc_descr_multiline => Some(T::acc_descr_multiline_value),
        S::style => None,
    };
    if let Some(last) = last {
        lx.emit(last, n, n);
    }
    lx.emit(T::END, n, n);
    lx.toks
}
