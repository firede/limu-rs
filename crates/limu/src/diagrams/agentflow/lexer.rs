//! The lexer of an agent flow, as Pele's lexer.ts has it: the rules of Mermaid's
//! agentflow.jison, tried by hand in the order Jison tries them, in the states it reads them
//! in. Positions are bytes, and each token also has its place as Jison counts it, in lines
//! and in columns of code units. Where Pele reads a code unit with charCodeAt, unit reads the
//! same one, and a character outside ASCII is stepped over whole.

use std::borrow::Cow;

use super::tokens::{DIRECTION_TOKENS, T};
use crate::js::chars::{
    is_line_end, is_space, is_unicode_letter, is_word, len16, space_run, unit, unit_at,
};

/// A token: its type, its text, the byte of the text it starts at, and where it is as Jison
/// counts it: its first line and column, and its last line and column. Lines start at 1, and
/// columns at 0.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Tok<'a> {
    pub(crate) kind: T,
    pub(crate) text: Cow<'a, str>,
    pub(crate) start: usize,
    pub(crate) loc: [usize; 4],
}

/// The states of the lexer, Pele's S.
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum S {
    INITIAL,
    string,
    md_string,
    acc_title,
    acc_descr,
    acc_descr_multiline,
    dir,
    text,
    ellipseText,
    trapText,
    edgeText,
    click,
    callbackname,
    callbackargs,
    shapeData,
    shapeDataStr,
}

/// Whether an ASCII character is one a node's id is made of, Pele's NODE_CHAR.
fn node_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || b"!\"#$%&'*+.`?\\_/".contains(&c)
}

// The regular expressions of Pele's lexer, written out. Each is sticky: it matches at q or
// not at all, and gives the end of its match.

/// /word\s*<sep>\s*/y, Pele's RE_ACC_TITLE, RE_ACC_DESCR and RE_ACC_DESCR_ML.
fn keyword_colon(s: &str, q: usize, word: &str, sep: u8) -> Option<usize> {
    if !s[q..].starts_with(word) {
        return None;
    }
    let j = space_run(s, q + word.len());
    (s.as_bytes().get(j) == Some(&sep)).then(|| space_run(s, j + 1))
}

/// /word\s+/y, Pele's RE_CALL and RE_CLICK.
fn keyword_space(s: &str, q: usize, word: &str) -> Option<usize> {
    if !s[q..].starts_with(word) {
        return None;
    }
    let j = space_run(s, q + word.len());
    (j > q + word.len()).then_some(j)
}

/// /(?:\r?\n)*\s*\n/y, Pele's RE_NODIR: the white space up to the last line feed in it.
fn nodir(s: &str, q: usize) -> Option<usize> {
    let end = space_run(s, q);
    s[q..end].rfind('\n').map(|i| q + i + 1)
}

/// Whether \w matches the code unit at i, the test of the \b in Pele's RE_DIR and RE_DIR_V.
fn word_at(s: &str, i: usize) -> bool {
    is_word(unit(s, i))
}

/// /\s*(?:LR|RL|TB|BT|TD|BR)\b/y, Pele's RE_DIR.
fn dir(s: &str, q: usize) -> Option<usize> {
    let j = space_run(s, q);
    let name = s.get(j..j + 2)?;
    (matches!(name, "LR" | "RL" | "TB" | "BT" | "TD" | "BR") && !word_at(s, j + 2)).then_some(j + 2)
}

/// /\s*[<>^]/y, Pele's RE_DIR_SYM.
fn dir_sym(s: &str, q: usize) -> Option<usize> {
    let j = space_run(s, q);
    matches!(s.as_bytes().get(j), Some(b'<' | b'>' | b'^')).then_some(j + 1)
}

/// /\s*v\b/y, Pele's RE_DIR_V.
fn dir_v(s: &str, q: usize) -> Option<usize> {
    let j = space_run(s, q);
    (s.as_bytes().get(j) == Some(&b'v') && !word_at(s, j + 1)).then_some(j + 1)
}

/// The first line terminator at or after q, or the end, as Pele's tokenize finds it with
/// RE_LINE_END.
fn line_end_from(s: &str, mut q: usize) -> usize {
    while q < s.len() {
        let (c, size) = unit_at(s, q);
        if is_line_end(c) {
            return q;
        }
        q += size;
    }
    s.len()
}

const DIRECTION_NAMES: [&str; 5] = ["TB", "BT", "RL", "LR", "TD"];

/// /.*direction\s+<name>[^\n]*/y, one of Pele's RE_DIRECTION: the last `direction` on the
/// line that is followed by white space and the name, as the greedy .* backs off to it, and
/// the rest of the line the match ends on.
fn direction(s: &str, q: usize, name: &str) -> Option<usize> {
    let end = line_end_from(s, q);
    (q..end).rev().find_map(|x| {
        if !s.as_bytes()[x..].starts_with(b"direction") {
            return None;
        }
        let j = space_run(s, x + 9);
        if j == x + 9 || !s[j..].starts_with(name) {
            return None;
        }
        Some(s[j + 2..].find('\n').map_or(s.len(), |nl| j + 2 + nl))
    })
}

/// /\(\s*\)/y, Pele's RE_EMPTY_CALL.
fn empty_call(s: &str, q: usize) -> Option<usize> {
    if !s[q..].starts_with('(') {
        return None;
    }
    let j = space_run(s, q + 1);
    s[j..].starts_with(')').then_some(j + 1)
}

/// s.replace(/\n\s*/g, '<br/>'), as Pele's tokenize writes the line breaks of a string in a
/// node's metadata.
fn breaks(s: &str) -> Cow<'_, str> {
    if !s.contains('\n') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while let Some(n) = s[i..].find('\n') {
        out.push_str(&s[i..i + n]);
        out.push_str("<br/>");
        i = space_run(s, i + n + 1);
    }
    out.push_str(&s[i..]);
    Cow::Owned(out)
}

struct Lexer<'a> {
    src: &'a str,
    n: usize,
    toks: Vec<Tok<'a>>,
    stack: Vec<S>,
    state: S,
    p: usize,
    /// Where p is as Jison counts it: its line, from 1, and its column in code units, from 0.
    line: usize,
    col: usize,
    first_graph: bool,
    dir_next: Option<usize>,
    at_next: Option<usize>,
    line_end: Option<usize>,
    dir_failed: usize,
    run_end: usize,
    run_at: Option<usize>,
    /// The last run of white space wsEnd read, from and to.
    ws: Option<(usize, usize)>,
    /// The last run of spaces and tabs blankEnd read, from and to.
    blank: Option<(usize, usize)>,
    /// Where link and arrowEnd last found no link after the white space, Pele's noLink and
    /// noArrow.
    no_link: Option<usize>,
    no_arrow: Option<usize>,
}

/// Reads src into tokens, as Pele's tokenize does. The last token is END, or ERROR where no
/// token starts.
pub(crate) fn tokenize(src: &str) -> Vec<Tok<'_>> {
    let mut l = Lexer {
        src,
        n: src.len(),
        toks: Vec::new(),
        stack: Vec::new(),
        state: S::INITIAL,
        p: 0,
        line: 1,
        col: 0,
        first_graph: true,
        dir_next: src.find("direction"),
        at_next: src.find('@'),
        line_end: None,
        dir_failed: 0,
        run_end: 0,
        run_at: None,
        ws: None,
        blank: None,
        no_link: None,
        no_arrow: None,
    };
    l.run();
    l.toks
}

impl<'a> Lexer<'a> {
    /// The code unit at byte i, -1 past the end: Pele's src.charCodeAt(i). Pele's at, which
    /// runs an expression, is the functions above.
    fn at(&self, i: usize) -> i32 {
        unit(self.src, i)
    }

    /// Byte i and the character there, Pele's i + 1.
    fn next(&self, i: usize) -> usize {
        i + unit_at(self.src, i).1.max(1)
    }

    /// Moves p over the text to e, as Pele's advance does. Jison counts \r\n, \r and \n as
    /// one line break each, and after a break its column stops at the first U+2028 or
    /// U+2029.
    fn advance(&mut self, e: usize) {
        let b = self.src.as_bytes();
        let mut brk = None;
        let mut k = self.p;
        while k < e {
            match b[k] {
                b'\n' => {
                    self.line += 1;
                    brk = Some(k);
                }
                b'\r' => {
                    if k + 1 < e && b[k + 1] == b'\n' {
                        k += 1;
                    }
                    self.line += 1;
                    brk = Some(k);
                }
                _ => {}
            }
            k += 1;
        }
        match brk {
            None => self.col += len16(&self.src[self.p..e]),
            Some(brk) => {
                let rest = &self.src[brk + 1..e];
                let stop = rest.find(['\u{2028}', '\u{2029}']).unwrap_or(rest.len());
                self.col = len16(&rest[..stop]);
            }
        }
        self.p = e;
    }

    /// The token from p to e, which may hold line breaks, Pele's emit.
    fn emit(&mut self, kind: T, e: usize) {
        let (p, line, col) = (self.p, self.line, self.col);
        self.advance(e);
        self.toks.push(Tok {
            kind,
            text: Cow::Borrowed(&self.src[p..e]),
            start: p,
            loc: [line, col, self.line, self.col],
        });
    }

    /// The token from p to e, which holds no line break, Pele's emitFlat.
    fn emit_flat(&mut self, kind: T, e: usize) {
        let (p, line, col) = (self.p, self.line, self.col);
        self.col += len16(&self.src[p..e]);
        self.p = e;
        self.toks.push(Tok {
            kind,
            text: Cow::Borrowed(&self.src[p..e]),
            start: p,
            loc: [line, col, self.line, self.col],
        });
    }

    /// The token from p to e of a run that Jison matches one character at a time, where \r\n
    /// counts as two line breaks, Pele's emitChars.
    fn emit_chars(&mut self, kind: T, e: usize) {
        let (p, line, col) = (self.p, self.line, self.col);
        for c in self.src[p..e].chars() {
            if c == '\n' || c == '\r' {
                self.line += 1;
                self.col = 0;
            } else {
                self.col += c.len_utf16();
            }
        }
        self.p = e;
        self.toks.push(Tok {
            kind,
            text: Cow::Borrowed(&self.src[p..e]),
            start: p,
            loc: [line, col, self.line, self.col],
        });
    }

    /// A token of a text of its own that stands for the text from p to e, Pele's emitText.
    fn emit_text(&mut self, kind: T, text: Cow<'a, str>, e: usize) {
        let (p, line, col) = (self.p, self.line, self.col);
        self.advance(e);
        self.toks.push(Tok {
            kind,
            text,
            start: p,
            loc: [line, col, self.line, self.col],
        });
    }

    /// A token with no text at p, as Pele ends its tokens.
    fn mark(&mut self, kind: T) {
        let (line, col) = (self.line, self.col);
        self.toks.push(Tok {
            kind,
            text: Cow::Borrowed(""),
            start: self.p,
            loc: [line, col, line, col],
        });
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

    /// Whether the text at p is word, and no word character follows, as Pele's kw says.
    fn kw(&self, word: &str) -> bool {
        self.src[self.p..].starts_with(word) && !is_word(self.at(self.p + word.len()))
    }

    /// The end of the run of white space from q, Pele's wsEnd. The last run is kept, so
    /// asking again from inside it costs nothing.
    fn ws_end(&mut self, q: usize) -> usize {
        if let Some((from, to)) = self.ws
            && q >= from
            && q <= to
        {
            return to;
        }
        let j = space_run(self.src, q);
        self.ws = Some((q, j));
        j
    }

    /// The end of the run of spaces and tabs from q, Pele's blankEnd.
    fn blank_end(&mut self, q: usize) -> usize {
        if let Some((from, to)) = self.blank
            && q >= from
            && q <= to
        {
            return to;
        }
        let j = blanks(self.src, q);
        self.blank = Some((q, j));
        j
    }

    /// The end of `\s*--+[x>]\s*` from q, Pele's arrowEnd.
    fn arrow_end(&mut self, q: usize) -> Option<usize> {
        let j = self.ws_end(q);
        if Some(j) == self.no_arrow || self.at(j) != 45 || self.at(j + 1) != 45 {
            return None;
        }
        let mut k = j + 2;
        while self.at(k) == 45 {
            k += 1;
        }
        if matches!(self.at(k), 120 | 62) {
            return Some(self.ws_end(k + 1));
        }
        self.no_arrow = Some(j);
        None
    }

    /// A link at p, or the start of one with a label in it, Pele's link.
    fn link(&mut self) -> bool {
        let j = self.ws_end(self.p);
        if Some(j) == self.no_link || self.at(j) != 45 {
            return false;
        }
        let d = self.at(j + 1);
        let mut k = j + 2;
        if d == 45 {
            while self.at(k) == 45 {
                k += 1;
            }
            if matches!(self.at(k), 120 | 62) {
                let e = self.ws_end(k + 1);
                self.emit(T::LINK, e);
            } else {
                self.push(S::edgeText);
                let e = self.ws_end(j + 2);
                self.emit(T::START_LINK, e);
            }
            return true;
        }
        if d == 46 {
            while self.at(k) == 46 {
                k += 1;
            }
            if self.at(k) == 45 {
                let e = self.ws_end(k + 1);
                self.emit(T::LINK, e);
                return true;
            }
        }
        self.no_link = Some(j);
        false
    }

    /// The end of a comment at q, `%%` not followed by `{`, through the end of its line,
    /// Pele's commentEnd.
    fn comment_end(&self, q: usize) -> Option<usize> {
        if self.at(q) != 37 || self.at(q + 1) != 37 || self.at(q + 2) == 123 {
            return None;
        }
        Some(self.src[q + 2..].find('\n').map_or(self.n, |i| q + 2 + i))
    }

    fn quote(&mut self) {
        if self.at(self.p + 1) == 96 {
            self.push(S::md_string);
            self.advance(self.p + 2);
        } else {
            self.push(S::string);
            self.advance(self.p + 1);
        }
    }

    fn opener(&mut self, c: i32) -> bool {
        let p = self.p;
        let d = self.at(p + 1);
        match c {
            40 => {
                if d == 45 {
                    self.push(S::ellipseText);
                    self.emit_flat(T::ELLIPSE_START, p + 2);
                } else if d == 91 {
                    self.push(S::text);
                    self.emit_flat(T::STADIUMSTART, p + 2);
                } else if d == 40 && self.at(p + 2) == 40 {
                    self.push(S::text);
                    self.emit_flat(T::DOUBLECIRCLESTART, p + 3);
                } else {
                    self.push(S::text);
                    self.emit_flat(T::PS, p + 1);
                }
                true
            }
            91 => {
                let (state, kind, len) = match d {
                    91 => (S::text, T::SUBROUTINESTART, 2),
                    40 => (S::text, T::CYLINDERSTART, 2),
                    47 => (S::trapText, T::TRAPSTART, 2),
                    92 => (S::trapText, T::INVTRAPSTART, 2),
                    _ => (S::text, T::SQS, 1),
                };
                self.push(state);
                self.emit_flat(kind, p + len);
                true
            }
            124 => {
                self.push(S::text);
                self.emit_flat(T::PIPE, p + 1);
                true
            }
            123 => {
                self.push(S::text);
                self.emit_flat(T::DIAMOND_START, p + 1);
                true
            }
            _ => false,
        }
    }

    fn node_string(&mut self) -> bool {
        let b = self.src.as_bytes();
        let mut q = self.p;
        while q < self.n {
            let ch = b[q];
            if ch < 128 && node_char(ch) {
                q += 1;
            } else if ch == b'-' && q + 1 < self.n {
                if matches!(b[q + 1], b'>' | b'-' | b'.') {
                    break;
                }
                q += 1;
            } else {
                break;
            }
        }
        if q == self.p {
            return false;
        }
        self.emit_flat(T::NODE_STRING, q);
        true
    }

    fn initial(&mut self, c: i32) -> bool {
        let (src, p, n) = (self.src, self.p, self.n);
        match c {
            97 => {
                if src[p..].starts_with("acc") {
                    if let Some(e) = keyword_colon(src, p, "accTitle", b':') {
                        self.push(S::acc_title);
                        self.emit(T::acc_title, e);
                        return true;
                    }
                    if let Some(e) = keyword_colon(src, p, "accDescr", b':') {
                        self.push(S::acc_descr);
                        self.emit(T::acc_descr, e);
                        return true;
                    }
                    if let Some(e) = keyword_colon(src, p, "accDescr", b'{') {
                        self.push(S::acc_descr_multiline);
                        self.advance(e);
                        return true;
                    }
                }
                if self.kw("agentflow-beta") {
                    if self.first_graph {
                        self.first_graph = false;
                        self.push(S::dir);
                    }
                    self.emit_flat(T::GRAPH, p + 14);
                    return true;
                }
            }
            64 if self.at(p + 1) == 123 => {
                self.push(S::shapeData);
                self.emit_text(T::SHAPE_DATA, Cow::Borrowed(""), p + 2);
                return true;
            }
            99 => {
                if let Some(e) = keyword_space(src, p, "call") {
                    self.push(S::callbackname);
                    self.advance(e);
                    return true;
                }
                if self.kw("classDef") {
                    self.emit_flat(T::CLASSDEF, p + 8);
                    return true;
                }
                if self.kw("class") {
                    self.emit_flat(T::CLASS, p + 5);
                    return true;
                }
                if let Some(e) = keyword_space(src, p, "click") {
                    self.push(S::click);
                    self.advance(e);
                    return true;
                }
                if self.kw("connector") {
                    self.emit_flat(T::connector, p + 9);
                    return true;
                }
            }
            34 => {
                self.quote();
                return true;
            }
            115 if self.kw("style") => {
                self.emit_flat(T::STYLE, p + 5);
                return true;
            }
            100 if self.kw("default") => {
                self.emit_flat(T::DEFAULT, p + 7);
                return true;
            }
            108 if self.kw("linkStyle") => {
                self.emit_flat(T::LINKSTYLE, p + 9);
                return true;
            }
            105 if self.kw("interpolate") => {
                self.emit_flat(T::INTERPOLATE, p + 11);
                return true;
            }
            // href and the white space character after it.
            104 if src[p..].starts_with("href") && is_space(self.at(p + 4)) => {
                let e = self.next(p + 4);
                self.emit(T::HREF, e);
                return true;
            }
            102 if self.kw("flow") => {
                self.emit_flat(T::flow, p + 4);
                return true;
            }
            103 if self.kw("global") => {
                let e = blanks(src, p + 6);
                self.emit_flat(T::global, e);
                return true;
            }
            101 if self.kw("end") => {
                let e = blanks(src, p + 3);
                self.emit_flat(T::end, e);
                return true;
            }
            95 => {
                for word in ["_self", "_blank", "_parent", "_top"] {
                    if self.kw(word) {
                        self.emit_flat(T::LINK_TARGET, p + word.len());
                        return true;
                    }
                }
            }
            _ => {}
        }

        if let Some(mut dir_next) = self.dir_next
            && p >= self.dir_failed
        {
            if dir_next < p {
                self.dir_next = src[p..].find("direction").map(|i| p + i);
            }
            if let Some(next) = self.dir_next {
                dir_next = next;
                let line_end = match self.line_end {
                    Some(e) if e >= p => e,
                    _ => {
                        let e = line_end_from(src, p);
                        self.line_end = Some(e);
                        e
                    }
                };
                if dir_next < line_end {
                    for (i, name) in DIRECTION_NAMES.iter().enumerate() {
                        if let Some(e) = direction(src, p, name) {
                            self.emit(DIRECTION_TOKENS[i], e);
                            return true;
                        }
                    }
                    // No direction statement starts anywhere in the rest of this line either.
                    self.dir_failed = line_end;
                }
            }
        }

        // A link id is a run of characters other than white space up to its last `@` that
        // is not followed by `{` or `"`.
        if self.at_next.is_some() && c != 34 && !is_space(c) {
            if p >= self.run_end {
                if self.at_next.is_some_and(|a| a < p) {
                    self.at_next = src[p..].find('@').map(|i| p + i);
                }
                let mut q = p;
                while q < n {
                    let (ch, size) = unit_at(src, q);
                    if ch == 34 || is_space(ch) {
                        break;
                    }
                    q += size;
                }
                self.run_end = q;
                self.run_at = None;
                if let Some(at_next) = self.at_next.filter(|&a| a < q) {
                    // From the code unit before the run's end, or, where the run reaches the
                    // end of the text, from the one before its last: Pele's
                    // Math.min(q, n - 1) - 1. Only an @ is looked for, so the bytes between
                    // stand for the code units.
                    let from = if q < n {
                        q
                    } else {
                        src[..n].char_indices().next_back().map_or(0, |(i, _)| i)
                    };
                    let b = src.as_bytes();
                    self.run_at = (at_next..from)
                        .rev()
                        .find(|&k| b[k] == b'@' && !matches!(self.at(k + 1), 123 | 34));
                }
            }
            if let Some(run_at) = self.run_at
                && run_at > p
            {
                self.emit_flat(T::LINK_ID, run_at + 1);
                return true;
            }
        }

        if (48..=57).contains(&c) {
            let q = p + src[p..].bytes().take_while(u8::is_ascii_digit).count();
            self.emit_flat(T::NUM, q);
            return true;
        }

        match c {
            35 => {
                self.emit_flat(T::BRKT, p + 1);
                return true;
            }
            58 => {
                if src[p..].starts_with(":::") {
                    self.emit_flat(T::STYLE_SEPARATOR, p + 3);
                } else {
                    self.emit_flat(T::COLON, p + 1);
                }
                return true;
            }
            38 | 59 | 44 | 42 | 60 => {
                let kind = match c {
                    38 => T::AMP,
                    59 => T::SEMI,
                    44 => T::COMMA,
                    42 => T::MULT,
                    _ => T::TAGSTART,
                };
                self.emit_flat(kind, p + 1);
                return true;
            }
            45 => {
                if !(self.link() || self.node_string()) {
                    self.emit_flat(T::MINUS, p + 1);
                }
                return true;
            }
            40 => return self.opener(c),
            91 => {
                if self.at(p + 1) == 124 {
                    self.emit_flat(T::VERTEX_WITH_PROPS_START, p + 2);
                    return true;
                }
                return self.opener(c);
            }
            124 | 123 => return self.opener(c),
            62 => {
                self.push(S::text);
                self.emit_flat(T::TAGEND, p + 1);
                return true;
            }
            94 => {
                self.emit_flat(T::UP, p + 1);
                return true;
            }
            92 => {
                if self.at(p + 1) == 124 {
                    self.emit_flat(T::SEP, p + 2);
                    return true;
                }
                return self.node_string();
            }
            118 => {
                if !is_word(self.at(p + 1)) {
                    self.emit_flat(T::DOWN, p + 1);
                    return true;
                }
                return self.node_string();
            }
            37 => {
                if let Some(e) = self.comment_end(p) {
                    self.emit(T::COMMENT, e);
                    return true;
                }
                return self.node_string();
            }
            _ => {}
        }

        if is_space(c) {
            if self.link() {
                return true;
            }
            if c == 32 || c == 9 {
                let b = self.blank_end(p);
                if let Some(e) = self.comment_end(b) {
                    self.emit(T::COMMENT, e);
                    return true;
                }
            }
            let mut e = p;
            loop {
                if src[e..].starts_with('\n') {
                    e += 1;
                } else if src[e..].starts_with("\r\n") {
                    e += 2;
                } else {
                    break;
                }
            }
            if e > p {
                self.emit(T::NEWLINE, e);
            } else {
                let e = self.next(p);
                self.emit(T::SPACE, e);
            }
            return true;
        }

        if c < 128 {
            return self.node_string();
        }
        if is_unicode_letter(c) {
            let e = self.next(p);
            self.emit_flat(T::UNICODE_TEXT, e);
            return true;
        }
        false
    }

    /// The end of the run from from of characters other than those stop is true of.
    fn run_to(&self, from: usize, stop: impl Fn(i32) -> bool) -> usize {
        let mut q = from;
        while q < self.n {
            let (ch, size) = unit_at(self.src, q);
            if stop(ch) {
                break;
            }
            q += size;
        }
        q
    }

    /// The first of byte ch from p, or the end.
    fn find_or_end(&self, ch: char) -> usize {
        self.src[self.p..].find(ch).map_or(self.n, |i| self.p + i)
    }

    fn run(&mut self) {
        loop {
            let (src, p, n) = (self.src, self.p, self.n);
            if p >= n {
                let kind = match self.state {
                    S::INITIAL => Some(T::EOF),
                    S::acc_title => Some(T::acc_title_value),
                    S::acc_descr => Some(T::acc_descr_value),
                    S::acc_descr_multiline => Some(T::acc_descr_multiline_value),
                    S::click => Some(T::CLICK),
                    S::callbackname => Some(T::CALLBACKNAME),
                    S::callbackargs => Some(T::CALLBACKARGS),
                    _ => None,
                };
                if let Some(kind) = kind {
                    self.emit_flat(kind, n);
                }
                break;
            }

            let c = self.at(p);
            let mut ok = true;
            match self.state {
                S::INITIAL => ok = self.initial(c),

                S::string => {
                    if c == 34 {
                        if self.at(p + 1) == 96 {
                            self.push(S::md_string);
                            self.advance(p + 2);
                        } else {
                            self.pop();
                            self.advance(p + 1);
                        }
                    } else {
                        let e = self.find_or_end('"');
                        self.emit(T::STR, e);
                    }
                }

                S::md_string => {
                    if c == 96 {
                        if self.at(p + 1) == 34 {
                            self.pop();
                            self.advance(p + 2);
                        } else {
                            ok = false;
                        }
                    } else if c == 34 {
                        self.quote();
                    } else {
                        let q = self.run_to(self.next(p), |ch| ch == 96 || ch == 34);
                        self.emit(T::MD_STR, q);
                    }
                }

                S::acc_title | S::acc_descr => {
                    let e = self.find_or_end('\n');
                    let kind = if self.state == S::acc_title {
                        T::acc_title_value
                    } else {
                        T::acc_descr_value
                    };
                    self.pop();
                    self.emit(kind, e);
                }

                S::acc_descr_multiline => {
                    if c == 125 {
                        self.pop();
                        self.advance(p + 1);
                    } else {
                        let e = self.find_or_end('}');
                        self.emit(T::acc_descr_multiline_value, e);
                    }
                }

                S::dir => {
                    if c == 34 {
                        self.quote();
                    } else if let Some(e) = nodir(src, p) {
                        self.pop();
                        self.emit(T::NODIR, e);
                    } else if let Some(e) = dir(src, p)
                        .or_else(|| dir_sym(src, p))
                        .or_else(|| dir_v(src, p))
                    {
                        self.pop();
                        self.emit(T::DIR, e);
                    } else {
                        ok = self.opener(c);
                    }
                }

                S::text => ok = self.text(c),
                S::ellipseText => ok = self.ellipse_text(c),
                S::trapText => ok = self.trap_text(c),
                S::edgeText => ok = self.edge_text(c),

                S::click => {
                    if c == 34 {
                        self.quote();
                    } else if is_space(c) {
                        self.pop();
                        let e = self.next(p);
                        self.advance(e);
                    } else {
                        let q = self.run_to(self.next(p), is_space);
                        self.emit_flat(T::CLICK, q);
                    }
                }

                S::callbackname => {
                    if c == 40 {
                        let e = empty_call(src, p);
                        self.pop();
                        match e {
                            Some(e) => self.advance(e),
                            None => {
                                self.push(S::callbackargs);
                                self.advance(p + 1);
                            }
                        }
                    } else {
                        let e = self.find_or_end('(');
                        self.emit(T::CALLBACKNAME, e);
                    }
                }

                S::callbackargs => {
                    if c == 41 {
                        self.pop();
                        self.advance(p + 1);
                    } else {
                        let e = self.find_or_end(')');
                        self.emit(T::CALLBACKARGS, e);
                    }
                }

                S::shapeData => {
                    if c == 34 {
                        self.push(S::shapeDataStr);
                        self.emit_flat(T::SHAPE_DATA, p + 1);
                    } else if c == 125 {
                        self.pop();
                        self.advance(p + 1);
                    } else if c == 94 {
                        ok = false;
                    } else {
                        let q = self.run_to(self.next(p), |ch| ch == 125 || ch == 94 || ch == 34);
                        self.emit(T::SHAPE_DATA, q);
                    }
                }

                S::shapeDataStr => {
                    if c == 34 {
                        self.pop();
                        self.emit_flat(T::SHAPE_DATA, p + 1);
                    } else {
                        let e = self.find_or_end('"');
                        self.emit_text(T::SHAPE_DATA, breaks(&src[p..e]), e);
                    }
                }
            }

            if !ok {
                self.mark(T::ERROR);
                return;
            }
        }
        self.mark(T::END);
    }

    /// The text of a node's label, Pele's case S.text.
    fn text(&mut self, c: i32) -> bool {
        let p = self.p;
        let d = self.at(p + 1);
        match c {
            34 => self.quote(),
            40 | 91 | 123 => {
                self.opener(c);
            }
            93 => {
                self.pop();
                match d {
                    41 => self.emit_flat(T::STADIUMEND, p + 2),
                    93 => self.emit_flat(T::SUBROUTINEEND, p + 2),
                    _ => self.emit_flat(T::SQE, p + 1),
                }
            }
            41 => {
                self.pop();
                if d == 93 {
                    self.emit_flat(T::CYLINDEREND, p + 2);
                } else if d == 41 && self.at(p + 2) == 41 {
                    self.emit_flat(T::DOUBLECIRCLEEND, p + 3);
                } else {
                    self.emit_flat(T::PE, p + 1);
                }
            }
            124 => {
                self.pop();
                self.emit_flat(T::PIPE, p + 1);
            }
            125 => {
                self.pop();
                self.emit_flat(T::DIAMOND_STOP, p + 1);
            }
            // Jison drops the lone `%` from Mermaid's text pattern, so only a comment may
            // start with one.
            37 => match self.comment_end(p) {
                Some(e) => self.advance(e),
                None => return false,
            },
            _ => {
                let q = self.run_to(self.next(p), |ch| {
                    matches!(ch, 91 | 93 | 40 | 41 | 123 | 125 | 124 | 34 | 37)
                });
                self.emit(T::TEXT, q);
            }
        }
        true
    }

    /// The text of an ellipse's label, Pele's case S.ellipseText.
    fn ellipse_text(&mut self, c: i32) -> bool {
        let (p, n) = (self.p, self.n);
        let d = self.at(p + 1);
        if c == 34 {
            self.quote();
        } else if let Some(e) = (c == 37).then(|| self.comment_end(p)).flatten() {
            self.advance(e);
        } else if (c == 45 || c == 47 || c == 41) && d == 41 {
            self.pop();
            self.emit_flat(T::ELLIPSE_END, p + 2);
        } else if c == 40 || c == 91 || c == 123 {
            self.opener(c);
        } else if c == 41 || c == 93 || c == 125 {
            return false;
        } else {
            let mut q = self.next(p);
            while q < n {
                let ch = self.at(q);
                if matches!(ch, 34 | 40 | 41 | 91 | 93 | 123 | 125) {
                    break;
                }
                if (ch == 45 || ch == 47) && self.at(q + 1) == 41 {
                    break;
                }
                if ch == 37 && self.at(q + 1) == 37 && self.at(q + 2) != 123 {
                    break;
                }
                q = self.next(q);
            }
            self.emit_chars(T::TEXT, q);
        }
        true
    }

    /// The text of a trapezoid's label, Pele's case S.trapText.
    fn trap_text(&mut self, c: i32) -> bool {
        let p = self.p;
        let d = self.at(p + 1);
        if c == 34 {
            self.quote();
            return true;
        }
        match c {
            40 => {
                if d == 45 || d == 91 || (d == 40 && self.at(p + 2) == 40) {
                    self.opener(c);
                } else if d == 93 {
                    self.pop();
                    self.emit_flat(T::TRAPEND, p + 2);
                } else {
                    self.opener(c);
                }
            }
            91 | 123 => {
                self.opener(c);
            }
            92 | 47 => {
                if d == 93 {
                    self.pop();
                    self.emit_flat(if c == 92 { T::TRAPEND } else { T::INVTRAPEND }, p + 2);
                } else {
                    self.emit_flat(T::TEXT, p + 1);
                }
            }
            93 | 41 => {
                if d != 93 {
                    return false;
                }
                self.pop();
                self.emit_flat(T::TRAPEND, p + 2);
            }
            125 => return false,
            37 => match self.comment_end(p) {
                Some(e) => self.advance(e),
                None => return false,
            },
            _ => {
                if (c == 63 || c == 61) && d == 93 {
                    self.pop();
                    self.emit_flat(T::TRAPEND, p + 2);
                    return true;
                }
                let q = self.run_to(self.next(p), |ch| {
                    matches!(ch, 92 | 91 | 93 | 40 | 41 | 123 | 125 | 47 | 37)
                });
                self.emit(T::TEXT, q);
            }
        }
        true
    }

    /// The label inside a link, up to the arrow that ends it, Pele's case S.edgeText.
    fn edge_text(&mut self, c: i32) -> bool {
        let (p, n) = (self.p, self.n);
        if c == 34 {
            self.quote();
            return true;
        }
        if let Some(e) = self.arrow_end(p) {
            self.pop();
            self.emit(T::LINK, e);
            return true;
        }
        if c == 45 && self.at(p + 1) == 45 {
            return false;
        }
        let mut q = self.next(p);
        while q < n {
            let ch = self.at(q);
            if ch == 34 {
                break;
            }
            if ch == 45 {
                if self.at(q + 1) == 45 {
                    break;
                }
            } else if is_space(ch) && self.arrow_end(q).is_some() {
                break;
            }
            q = self.next(q);
        }
        self.emit_chars(T::EDGE_TEXT, q);
        true
    }
}

/// The end of the run of spaces and tabs from q, as Pele's blanks gives it.
fn blanks(s: &str, q: usize) -> usize {
    q + s[q..]
        .bytes()
        .take_while(|&c| c == b' ' || c == b'\t')
        .count()
}
