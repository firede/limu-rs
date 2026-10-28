//! The lexer of a state diagram, as Pele's lexer.ts has it: the rules of Mermaid's
//! stateDiagram.jison, tried by hand in the order Jison tries them, in the states it reads
//! them in. Positions are bytes. Where Pele reads a code unit with charCodeAt, at reads the
//! same one, and a character outside ASCII is stepped over whole.

use std::borrow::Cow;

use super::tokens::{DIRECTION_TOKENS, T};
use crate::js::chars::{is_line_end, is_space, is_word, space_run, trim, trim_end, unit, unit_at};

/// A token: its type, its text, and the byte of the text it starts at.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Tok<'a> {
    pub(crate) kind: T,
    pub(crate) text: Cow<'a, str>,
    pub(crate) start: usize,
}

/// What the lexer makes of a text, Pele's Tokens: its tokens, the last of them END or ERROR,
/// and the message of the grammar's own error, where lexing stopped on one rather than on
/// text no rule matches.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Tokens<'a> {
    pub(crate) toks: Vec<Tok<'a>>,
    pub(crate) error: Option<String>,
}

/// The states of the lexer, Pele's S.
#[allow(
    non_camel_case_types,
    clippy::upper_case_acronyms,
    clippy::enum_variant_names
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum S {
    INITIAL,
    r#struct,
    STATE,
    STATE_STRING,
    STATE_ID,
    SCALE,
    acc_title,
    acc_descr,
    acc_descr_multiline,
    CLASSDEF,
    CLASSDEFID,
    CLASS,
    CLASS_STYLE,
    STYLE,
    STYLEDEF_STYLES,
    NOTE,
    NOTE_ID,
    NOTE_TEXT,
    FLOATING_NOTE,
    FLOATING_NOTE_ID,
}

/// Whether s holds the lowercase ASCII word w at byte q, letters of either case, as the
/// grammar's /i flag matches them. Pele's word.
fn word_at(s: &str, w: &str, q: usize) -> bool {
    s.as_bytes().get(q..q + w.len()).is_some_and(|b| {
        b.iter()
            .zip(w.bytes())
            .all(|(&c, a)| c == a || a.is_ascii_lowercase() && c == a - 32)
    })
}

/// The places a pattern matches in a text, in order, to give the first at or after a place,
/// as Pele's finder gives it. Pele searches again only when it is asked from before the last
/// answer or past it. Here each place is found once, up front.
struct Finder(Vec<usize>);

impl Finder {
    /// The places from 0 to the end of s where matches says the pattern starts.
    fn new(s: &str, matches: impl Fn(usize) -> bool) -> Finder {
        Finder(
            s.char_indices()
                .map(|(i, _)| i)
                .filter(|&i| matches(i))
                .collect(),
        )
    }

    /// The first place at or after q.
    fn next(&self, q: usize) -> Option<usize> {
        self.0.get(self.0.partition_point(|&i| i < q)).copied()
    }
}

/// /<<(?:fork|join|choice)>>|\[\[(?:fork|join|choice)\]\]/i at byte i, Pele's RE_STEREOTYPE.
fn stereotype_at(s: &str, i: usize) -> bool {
    let b = s.as_bytes();
    let close = match b.get(i..i + 2) {
        Some(b"<<") => b">>",
        Some(b"[[") => b"]]",
        _ => return false,
    };
    ["fork", "join", "choice"]
        .iter()
        .any(|name| word_at(s, name, i + 2) && b[i + 2 + name.len()..].starts_with(close))
}

struct Lexer<'a> {
    src: &'a str,
    n: usize,
    toks: Vec<Tok<'a>>,
    stack: Vec<S>,
    state: S,
    p: usize,
    error: Option<String>,
    next_direction: Finder,
    next_stereotype: Finder,
    next_line_end: Finder,
    next_percent: Finder,
    next_brace: Finder,
    /// The bytes the kinds of `direction <dir>` below were found between, the last of each
    /// kind found there, and where its keyword ends.
    dir_span: Option<(usize, usize)>,
    dir_at: [Option<usize>; 4],
    dir_end: [usize; 4],
    /// The same for <<fork>>, <<join>>, <<choice>> and their [[ ]] spellings.
    stereo_span: Option<(usize, usize)>,
    stereo_at: [Option<usize>; 6],
}

/// Reads src into tokens, as Pele's tokenize does.
pub(crate) fn tokenize(src: &str) -> Tokens<'_> {
    let b = src.as_bytes();
    let mut l = Lexer {
        src,
        n: src.len(),
        toks: Vec::new(),
        stack: Vec::new(),
        state: S::INITIAL,
        p: 0,
        error: None,
        next_direction: Finder::new(src, |i| word_at(src, "direction", i)),
        next_stereotype: Finder::new(src, |i| stereotype_at(src, i)),
        next_line_end: Finder::new(src, |i| is_line_end(unit(src, i))),
        next_percent: Finder::new(src, |i| b[i..].starts_with(b"%%")),
        next_brace: Finder::new(src, |i| b[i] == b'{'),
        dir_span: None,
        dir_at: [None; 4],
        dir_end: [0; 4],
        stereo_span: None,
        stereo_at: [None; 6],
    };
    l.run();
    Tokens {
        toks: l.toks,
        error: l.error,
    }
}

impl<'a> Lexer<'a> {
    /// The code unit at byte i, -1 past the end: Pele's src.charCodeAt(i).
    fn at(&self, i: usize) -> i32 {
        unit(self.src, i)
    }

    /// Byte i and the character there, Pele's i + 1.
    fn next(&self, i: usize) -> usize {
        i + unit_at(self.src, i).1.max(1)
    }

    fn emit(&mut self, kind: T, s: usize, e: usize) {
        self.toks.push(Tok {
            kind,
            text: Cow::Borrowed(&self.src[s..e]),
            start: s,
        });
        self.p = e;
    }

    fn emit_text(&mut self, kind: T, text: &'a str, s: usize, e: usize) {
        self.toks.push(Tok {
            kind,
            text: Cow::Borrowed(text),
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

    fn word(&self, w: &str, q: usize) -> bool {
        word_at(self.src, w, q)
    }

    fn ws_end(&self, q: usize) -> usize {
        space_run(self.src, q)
    }

    fn word_end(&self, q: usize) -> usize {
        q + self.src.as_bytes()[q.min(self.n)..]
            .iter()
            .take_while(|&&c| is_word(i32::from(c)))
            .count()
    }

    /// The first line feed at or after q, or the end.
    fn eol(&self, q: usize) -> usize {
        self.src[q..].find('\n').map_or(self.n, |k| q + k)
    }

    /// The first line terminator at or after q, or the end.
    fn line_end(&self, q: usize) -> usize {
        self.next_line_end.next(q).unwrap_or(self.n)
    }

    /// The end of the run of white space other than line feeds that starts at q.
    fn blank_run(&self, mut j: usize) -> usize {
        while j < self.n {
            let (d, size) = unit_at(self.src, j);
            if d == 10 || !is_space(d) {
                break;
            }
            j += size;
        }
        j
    }

    /// The rules `.*direction\s+TB[^\n]*` and their three siblings: the first kind, in
    /// grammar order, found between q and the end of its line, as Pele's direction finds it.
    fn direction(&mut self, q: usize) -> Option<usize> {
        if self.dir_span.is_none_or(|(from, to)| q < from || q > to) {
            let first = self.next_direction.next(q);
            let end = if first.is_none() {
                self.n
            } else {
                self.line_end(q)
            };
            self.dir_span = Some((q, end));
            self.dir_at = [None; 4];
            let mut d = first;
            while let Some(at) = d.filter(|&d| d < end) {
                let w = self.ws_end(at + 9);
                let kind = if w == at + 9 {
                    None
                } else {
                    ["tb", "bt", "rl", "lr"]
                        .iter()
                        .position(|name| self.word(name, w))
                };
                if let Some(kind) = kind {
                    self.dir_at[kind] = Some(at);
                    self.dir_end[kind] = w + 2;
                }
                d = self.next_direction.next(at + 1);
            }
        }
        (0..4).find(|&kind| self.dir_at[kind].is_some_and(|at| at >= q))
    }

    /// The first of <<fork>>, <<join>>, <<choice>>, [[fork]], [[join]] and [[choice]], in
    /// that order, found between q and the end of its line, as Pele's stereotype finds it.
    fn stereotype(&mut self, q: usize) -> Option<usize> {
        if self.stereo_span.is_none_or(|(from, to)| q < from || q > to) {
            let first = self.next_stereotype.next(q);
            let end = if first.is_none() {
                self.n
            } else {
                self.line_end(q)
            };
            self.stereo_span = Some((q, end));
            self.stereo_at = [None; 6];
            let b = self.src.as_bytes();
            let mut d = first;
            while let Some(at) = d.filter(|&d| d < end) {
                let letter = b[at + 2] | 32;
                let base = if b[at] == b'<' { 0 } else { 3 };
                let kind = match letter {
                    b'f' => 0,
                    b'j' => 1,
                    _ => 2,
                };
                self.stereo_at[base + kind] = Some(at);
                d = self.next_stereotype.next(at + 1);
            }
        }
        (0..6).find(|&kind| self.stereo_at[kind].is_some_and(|at| at >= q))
    }

    /// The grammar's processId(), as Pele's emitId does it: an id ends at an inline `%%`, and
    /// a text that starts with one is dropped. It says whether it gave a token.
    fn emit_id(&mut self, s: usize, e: usize) -> bool {
        match self.next_percent.next(s) {
            Some(at) if at + 2 <= e => {
                if at == s {
                    self.p = e;
                    return false;
                }
                self.emit(T::ID, s, at);
            }
            _ => self.emit(T::ID, s, e),
        }
        true
    }

    fn finish(&mut self, kind: T) {
        self.toks.push(Tok {
            kind,
            text: Cow::Borrowed(""),
            start: self.p,
        });
    }

    fn run(&mut self) {
        let n = self.n;
        loop {
            let p = self.p;
            if p >= n {
                // Jison tries the current state's rules once more on the empty remainder.
                let kind = match self.state {
                    S::INITIAL => Some(T::NL),
                    S::acc_title => Some(T::acc_title_value),
                    S::acc_descr => Some(T::acc_descr_value),
                    S::acc_descr_multiline => Some(T::acc_descr_multiline_value),
                    S::CLASSDEFID => Some(T::CLASSDEF_STYLEOPTS),
                    S::CLASS_STYLE => Some(T::STYLECLASS),
                    S::STYLEDEF_STYLES => Some(T::STYLEDEF_STYLEOPTS),
                    S::STATE_STRING => Some(T::STATE_DESCR),
                    S::FLOATING_NOTE => Some(T::NOTE_TEXT),
                    S::STATE_ID | S::FLOATING_NOTE_ID => Some(T::ID),
                    _ => None,
                };
                if let Some(kind) = kind {
                    self.emit(kind, n, n);
                }
                return self.finish(T::END);
            }

            let c = self.at(p);
            match self.state {
                S::INITIAL | S::r#struct => {
                    if !self.initial(c) {
                        return self.finish(T::ERROR);
                    }
                }

                S::STATE => {
                    if c != 10 && is_space(c) {
                        self.p = self.blank_run(self.next(p));
                        continue;
                    }
                    if c == 35 || (c == 37 && self.at(p + 1) == 37 && self.at(p + 2) != 123) {
                        self.p = self.eol(p);
                        continue;
                    }
                    if let Some(kind) = self.stereotype(p) {
                        let at = self.stereo_at[kind].unwrap_or(p);
                        let choice = kind == 2 || kind == 5;
                        let token = if choice {
                            T::CHOICE
                        } else if kind % 3 == 0 {
                            T::FORK
                        } else {
                            T::JOIN
                        };
                        let text = trim(&self.src[p..at]);
                        self.emit_text(token, text, p, at + if choice { 10 } else { 8 });
                        self.pop();
                        continue;
                    }
                    if c == 34 {
                        self.p = p + 1;
                        self.push(S::STATE_STRING);
                        continue;
                    }
                    let q = if c == 10 { self.ws_end(p) } else { p };
                    if self.word("as", q) {
                        let e = self.ws_end(q + 2);
                        if e > q + 2 {
                            self.emit(T::AS, p, e);
                            self.push(S::STATE_ID);
                            continue;
                        }
                    }
                    if c == 10 {
                        self.p = p + 1;
                        self.pop();
                        continue;
                    }
                    if c == 123 {
                        self.emit(T::STRUCT_START, p, p + 1);
                        self.pop();
                        self.push(S::r#struct);
                        continue;
                    }
                    if is_word(c) {
                        let first = self.word_end(p);
                        let second = self.ws_end(first);
                        if second > first
                            && is_word(self.at(second))
                            && let Some(brace) = self.next_brace.next(second + 1)
                            && brace < self.line_end(second)
                        {
                            self.error = Some(format!(
                                "Error: State name must be a single word. Found: \"{}\"",
                                trim(&self.src[p..brace + 1])
                            ));
                            return self.finish(T::ERROR);
                        }
                    }
                    let mut j = self.next(p);
                    while j < n {
                        let (d, size) = unit_at(self.src, j);
                        if d == 123 || is_space(d) {
                            break;
                        }
                        j += size;
                    }
                    self.emit(T::COMPOSIT_STATE, p, j);
                }

                S::STATE_STRING => {
                    if c == 34 {
                        self.p = p + 1;
                        self.pop();
                        continue;
                    }
                    let close = self.src[p..].find('"').map_or(n, |k| p + k);
                    self.emit(T::STATE_DESCR, p, close);
                }

                S::STATE_ID => {
                    let b = self.src.as_bytes();
                    let mut j = p;
                    while j < n && b[j] != b'\n' && b[j] != b'{' {
                        j += 1;
                    }
                    if self.emit_id(p, j) {
                        self.pop();
                    }
                }

                S::SCALE => {
                    if (48..=57).contains(&c) {
                        let b = self.src.as_bytes();
                        let mut j = p + 1;
                        while j < n && b[j].is_ascii_digit() {
                            j += 1;
                        }
                        self.emit(T::WIDTH, p, j);
                        continue;
                    }
                    let q = self.ws_end(p);
                    if q > p && self.word("width", q) && !is_word(self.at(q + 5)) {
                        self.p = q + 5;
                        self.pop();
                        continue;
                    }
                    return self.finish(T::ERROR);
                }

                S::acc_title => {
                    self.emit(T::acc_title_value, p, self.eol(p));
                    self.pop();
                }

                S::acc_descr => {
                    self.emit(T::acc_descr_value, p, self.eol(p));
                    self.pop();
                }

                S::acc_descr_multiline => {
                    if c == 125 {
                        self.p = p + 1;
                        self.pop();
                        continue;
                    }
                    let close = self.src[p..].find('}').map_or(n, |k| p + k);
                    self.emit(T::acc_descr_multiline_value, p, close);
                }

                S::CLASSDEF => {
                    let is_default = self.word("default", p) && self.ws_end(p + 7) > p + 7;
                    let j = if is_default { p + 7 } else { self.word_end(p) };
                    let e = self.ws_end(j);
                    if j == p || e == j {
                        return self.finish(T::ERROR);
                    }
                    let kind = if is_default {
                        T::DEFAULT_CLASSDEF_ID
                    } else {
                        T::CLASSDEF_ID
                    };
                    self.emit(kind, p, e);
                    self.pop();
                    self.push(S::CLASSDEFID);
                }

                S::CLASSDEFID => {
                    self.emit(T::CLASSDEF_STYLEOPTS, p, self.eol(p));
                    self.pop();
                }

                S::CLASS => {
                    let mut j = self.word_end(p);
                    if j == p {
                        return self.finish(T::ERROR);
                    }
                    while self.at(j) == 44 {
                        let k = self.ws_end(j + 1);
                        let e = self.word_end(k);
                        if e == k {
                            break;
                        }
                        j = e;
                    }
                    self.emit(T::CLASSENTITY_IDS, p, j);
                    self.pop();
                    self.push(S::CLASS_STYLE);
                }

                S::CLASS_STYLE => {
                    self.emit(T::STYLECLASS, p, self.eol(p));
                    self.pop();
                }

                S::STYLE => {
                    let b = self.src.as_bytes();
                    let mut j = p;
                    while j < n && (is_word(i32::from(b[j])) || b[j] == b',') {
                        j += 1;
                    }
                    let e = self.ws_end(j);
                    if j == p || e == j {
                        return self.finish(T::ERROR);
                    }
                    self.emit(T::STYLE_IDS, p, e);
                    self.pop();
                    self.push(S::STYLEDEF_STYLES);
                }

                S::STYLEDEF_STYLES => {
                    self.emit(T::STYLEDEF_STYLEOPTS, p, self.eol(p));
                    self.pop();
                }

                S::NOTE => {
                    if self.word("left of", p) && !is_word(self.at(p + 7)) {
                        self.emit(T::left_of, p, p + 7);
                        self.pop();
                        self.push(S::NOTE_ID);
                        continue;
                    }
                    if self.word("right of", p) && !is_word(self.at(p + 8)) {
                        self.emit(T::right_of, p, p + 8);
                        self.pop();
                        self.push(S::NOTE_ID);
                        continue;
                    }
                    if c == 34 {
                        self.p = p + 1;
                        self.pop();
                        self.push(S::FLOATING_NOTE);
                        continue;
                    }
                    return self.finish(T::ERROR);
                }

                S::NOTE_ID => {
                    let q = self.ws_end(p);
                    let mut j = q;
                    while j < n {
                        let (d, size) = unit_at(self.src, j);
                        if d == 58 || d == 45 || is_space(d) {
                            break;
                        }
                        j += size;
                    }
                    if j == q {
                        return self.finish(T::ERROR);
                    }
                    if self.emit_id(p, j) {
                        self.pop();
                        self.push(S::NOTE_TEXT);
                    }
                }

                S::NOTE_TEXT => {
                    let q = self.ws_end(p);
                    if self.at(q) == 58 {
                        let b = self.src.as_bytes();
                        let mut j = q + 1;
                        while j < n && !matches!(b[j], b':' | b'\n' | b';') {
                            j += 1;
                        }
                        if j > q + 1 {
                            // Mermaid drops two code units here, expecting " :". With no space
                            // before the colon the first character of the note goes too. The
                            // first unit is a colon or a space, and so a character of its own,
                            // but the second may be the first half of a character beyond the
                            // Basic Multilingual Plane. Pele keeps the second half then, which
                            // a Rust string cannot hold, and it is written as U+FFFD, as UTF-8
                            // writes a lone surrogate.
                            let second = self.next(p).min(j);
                            let from = self.next(second).min(j);
                            if from - second == 4 {
                                self.toks.push(Tok {
                                    kind: T::NOTE_TEXT,
                                    text: Cow::Owned(format!(
                                        "\u{fffd}{}",
                                        trim_end(&self.src[from..j])
                                    )),
                                    start: p,
                                });
                                self.p = j;
                                self.pop();
                                continue;
                            }
                            self.emit_text(T::NOTE_TEXT, trim(&self.src[from..j]), p, j);
                            self.pop();
                            continue;
                        }
                    }
                    let mut k = self.src[p..].find('\n').map(|k| p + k);
                    let mut e = 0;
                    while let Some(at) = k {
                        e = self.ws_end(at);
                        if self.word("end note", e) && !is_word(self.at(e + 8)) {
                            break;
                        }
                        k = self.src[e..].find('\n').map(|k| e + k);
                    }
                    if k.is_none() {
                        return self.finish(T::ERROR);
                    }
                    self.emit_text(T::NOTE_TEXT, trim(&self.src[p..e]), p, e + 8);
                    self.pop();
                }

                S::FLOATING_NOTE => {
                    let q = self.ws_end(p);
                    if self.word("as", q) {
                        let e = self.ws_end(q + 2);
                        self.emit(T::AS, p, e);
                        self.pop();
                        self.push(S::FLOATING_NOTE_ID);
                        continue;
                    }
                    if c == 34 {
                        self.p = p + 1;
                        continue;
                    }
                    let close = self.src[p..].find('"').map_or(n, |k| p + k);
                    self.emit(T::NOTE_TEXT, p, close);
                }

                S::FLOATING_NOTE_ID => {
                    if self.emit_id(p, self.eol(p)) {
                        self.pop();
                    }
                }
            }
        }
    }

    /// The rules of the root document and of a composite's, at p, where the code unit is c,
    /// as Pele's tokenize tries them. It says whether one matched, which is false only where
    /// a composite's text has no token at p.
    fn initial(&mut self, c: i32) -> bool {
        let n = self.n;
        let p = self.p;
        let root = self.state == S::INITIAL;
        let lc = c | 32;
        if root {
            if lc == 99 && self.word("click", p) && !is_word(self.at(p + 5)) {
                self.emit(T::CLICK, p, p + 5);
                return true;
            }
            if lc == 104 && self.word("href", p) && !is_word(self.at(p + 4)) {
                self.emit(T::HREF, p, p + 4);
                return true;
            }
            if c == 34
                && let Some(close) = self.src[p + 1..].find('"')
            {
                self.emit(T::STRING, p, p + 1 + close + 1);
                return true;
            }
            if lc == 100 && self.word("default", p) && !is_word(self.at(p + 7)) {
                self.emit(T::DEFAULT, p, p + 7);
                return true;
            }
            if let Some(kind) = self.direction(p) {
                let e = self.eol(self.dir_end[kind]);
                self.emit(DIRECTION_TOKENS[kind], p, e);
                return true;
            }
            if c == 10 {
                let b = self.src.as_bytes();
                let mut j = p + 1;
                while b.get(j) == Some(&b'\n') {
                    j += 1;
                }
                self.emit(T::NL, p, j);
                return true;
            }
            if is_space(c) {
                self.p = self.ws_end(p);
                return true;
            }
        } else if c != 10 && is_space(c) {
            self.p = self.blank_run(self.next(p));
            return true;
        }
        if c == 35 || (c == 37 && self.at(p + 1) == 37 && self.at(p + 2) != 123) {
            self.p = self.eol(p);
            return true;
        }
        if lc == 115 {
            if root && self.word("scale", p) {
                let e = self.ws_end(p + 5);
                if e > p + 5 {
                    self.emit(T::scale, p, e);
                    self.push(S::SCALE);
                    return true;
                }
            }
            if self.word("style", p) {
                let e = self.ws_end(p + 5);
                if e > p + 5 {
                    self.emit(T::style, p, e);
                    self.push(S::STYLE);
                    return true;
                }
            }
            if self.word("state", p) {
                let e = self.ws_end(p + 5);
                if e > p + 5 {
                    self.p = e;
                    self.push(S::STATE);
                    return true;
                }
                if root && self.word("diagram", p + 5) {
                    let from = if self.word("-v2", p + 12) {
                        p + 15
                    } else {
                        p + 12
                    };
                    let e = self.ws_end(from);
                    if e > from {
                        self.emit(T::SD, p, e);
                        return true;
                    }
                }
            }
        } else if lc == 99 {
            if self.word("class", p) {
                let def = self.word("def", p + 5);
                let from = if def { p + 8 } else { p + 5 };
                let e = self.ws_end(from);
                if e > from {
                    if def {
                        self.emit(T::classDef, p, e);
                        self.push(S::CLASSDEF);
                    } else {
                        self.emit(T::class, p, e);
                        self.push(S::CLASS);
                    }
                    return true;
                }
            }
        } else if lc == 97 && root && (self.word("acctitle", p) || self.word("accdescr", p)) {
            let title = (self.at(p + 3) | 32) == 116;
            let q = self.ws_end(p + 8);
            let d = self.at(q);
            if d == 58 {
                let e = self.ws_end(q + 1);
                if title {
                    self.emit(T::acc_title, p, e);
                    self.push(S::acc_title);
                } else {
                    self.emit(T::acc_descr, p, e);
                    self.push(S::acc_descr);
                }
                return true;
            }
            if d == 123 && !title {
                self.p = self.ws_end(q + 1);
                self.push(S::acc_descr_multiline);
                return true;
            }
        }
        if root {
            if c == 123 {
                self.emit(T::STRUCT_START, p, p + 1);
                self.push(S::r#struct);
                return true;
            }
        } else {
            if let Some(kind) = self.direction(p) {
                let e = self.eol(self.dir_end[kind]);
                self.emit(DIRECTION_TOKENS[kind], p, e);
                return true;
            }
            if c == 125 {
                self.emit(T::STRUCT_STOP, p, p + 1);
                self.pop();
                return true;
            }
            if c == 10 {
                self.p = p + 1;
                return true;
            }
        }
        if lc == 110 && self.word("note", p) {
            let e = self.ws_end(p + 4);
            if e > p + 4 {
                self.emit(T::note, p, e);
                self.push(S::NOTE);
                return true;
            }
        }
        if root && lc == 104 && self.word("hide empty description", p) && !is_word(self.at(p + 22))
        {
            self.emit(T::HIDE_EMPTY, p, p + 22);
            return true;
        }
        if c == 91 && self.at(p + 1) == 42 && self.at(p + 2) == 93 {
            self.emit(T::EDGE_STATE, p, p + 3);
            return true;
        }
        let mut j = p;
        while j < n {
            let (d, size) = unit_at(self.src, j);
            if d == 58 || d == 45 || d == 123 || is_space(d) {
                break;
            }
            j += size;
        }
        if j > p {
            self.emit_id(p, j);
            return true;
        }
        if c == 58 {
            let mut j = p + 1;
            while j < n {
                let d = self.at(j);
                if d == 10 || d == 59 {
                    break;
                }
                if d == 58 {
                    let after = self.at(j + 1);
                    if j + 1 >= n || after == 58 || after == 10 || after == 59 {
                        break;
                    }
                    j += 1;
                }
                j += 1;
            }
            if j > p + 1 {
                self.emit_text(T::DESCR, trim(&self.src[p..j]), p, j);
                return true;
            }
            if self.at(p + 1) == 58 && self.at(p + 2) == 58 {
                self.emit(T::STYLE_SEPARATOR, p, p + 3);
                return true;
            }
        } else if c == 45 && self.at(p + 1) == 45 {
            if self.at(p + 2) == 62 {
                self.emit(T::ARROW, p, p + 3);
                return true;
            }
            if !root {
                self.emit(T::CONCURRENT, p, p + 2);
                return true;
            }
        }
        if !root {
            return false;
        }
        self.emit(T::INVALID, p, self.next(p));
        true
    }
}
