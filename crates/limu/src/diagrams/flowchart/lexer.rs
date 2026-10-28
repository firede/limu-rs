//! The lexer of a flowchart, as Pele's lexer.ts has it: the rules of Mermaid's flow.jison,
//! tried by hand in the order Jison tries them, in the states it reads them in. Positions
//! are bytes. Where Pele reads a code unit with charCodeAt, unit reads the same one, and a
//! character outside ASCII is stepped over whole.

use std::borrow::Cow;

use super::tokens::{DIRECTION_TOKENS, T};
use crate::js::chars::{
    is_line_end, is_space, is_unicode_letter, is_word, space_run, unit, unit_at,
};

/// A token: its type, its text, and the byte of the text it starts at.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Tok<'a> {
    pub(crate) kind: T,
    pub(crate) text: Cow<'a, str>,
    pub(crate) start: usize,
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
    thickEdgeText,
    dottedEdgeText,
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

/// Whether the byte at i is one of set, a character class of Pele's expressions for links.
fn is_one_of(s: &str, i: usize, set: &[u8]) -> bool {
    s.as_bytes().get(i).is_some_and(|c| set.contains(c))
}

/// /\s*[xo<]?<c><c>+[<c>xo>]\s*/y for c of - or =, Pele's RE_LINK for - and RE_THICK_LINK
/// for =.
fn solid_link(s: &str, q: usize, c: u8) -> Option<usize> {
    let mut j = space_run(s, q);
    if is_one_of(s, j, b"xo<") {
        j += 1;
    }
    let mut k = j;
    while s.as_bytes().get(k) == Some(&c) {
        k += 1;
    }
    // The last of the run may be the [<c>xo>] that ends the link.
    match k - j {
        m if m >= 2 && is_one_of(s, k, b"xo>") => k += 1,
        m if m >= 3 => {}
        _ => return None,
    }
    Some(space_run(s, k))
}

/// /\s*[xo<]?<c><c>\s*/y, Pele's RE_START_LINK for - and RE_THICK_START for =.
fn start_link(s: &str, q: usize, c: u8) -> Option<usize> {
    let mut j = space_run(s, q);
    if is_one_of(s, j, b"xo<") {
        j += 1;
    }
    s.as_bytes()[j..]
        .starts_with(&[c, c])
        .then(|| space_run(s, j + 2))
}

/// /\s*[xo<]?-?\.+-[xo>]?\s*/y, Pele's RE_DOTTED_LINK.
fn dotted_link(s: &str, q: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut j = space_run(s, q);
    if is_one_of(s, j, b"xo<") {
        j += 1;
    }
    if b.get(j) == Some(&b'-') {
        j += 1;
    }
    let mut k = j;
    while b.get(k) == Some(&b'.') {
        k += 1;
    }
    if k == j || b.get(k) != Some(&b'-') {
        return None;
    }
    k += 1;
    if is_one_of(s, k, b"xo>") {
        k += 1;
    }
    Some(space_run(s, k))
}

/// /\s*[xo<]?-\.\s*/y, Pele's RE_DOTTED_START.
fn dotted_start(s: &str, q: usize) -> Option<usize> {
    let mut j = space_run(s, q);
    if is_one_of(s, j, b"xo<") {
        j += 1;
    }
    s[j..].starts_with("-.").then(|| space_run(s, j + 2))
}

/// /\s*~~~+\s*/y, Pele's RE_INVIS_LINK.
fn invisible(s: &str, q: usize) -> Option<usize> {
    let j = space_run(s, q);
    let k = j + s[j..].bytes().take_while(|&c| c == b'~').count();
    (k - j >= 3).then(|| space_run(s, k))
}

/// /(?:\r?\n)+/y, Pele's RE_NEWLINE.
fn newline(s: &str, q: usize) -> Option<usize> {
    let mut j = q;
    loop {
        if s[j..].starts_with("\r\n") {
            j += 2;
        } else if s[j..].starts_with('\n') {
            j += 1;
        } else {
            break;
        }
    }
    (j > q).then_some(j)
}

/// /[^\S\n\r]+/y, Pele's RE_SPACE.
fn blanks(s: &str, q: usize) -> Option<usize> {
    let mut j = q;
    while j < s.len() {
        let (c, size) = unit_at(s, j);
        if !is_space(c) || c == 10 || c == 13 {
            break;
        }
        j += size;
    }
    (j > q).then_some(j)
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
    first_graph: bool,
    dir_next: Option<usize>,
    at_next: Option<usize>,
    line_end: Option<usize>,
    dir_failed: usize,
    run_end: usize,
    run_at: Option<usize>,
    /// The last run of white space linkChar or invisAhead read, from and to.
    ws: Option<(usize, usize)>,
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
        first_graph: true,
        dir_next: src.find("direction"),
        at_next: src.find('@'),
        line_end: None,
        dir_failed: 0,
        run_end: 0,
        run_at: None,
        ws: None,
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

    fn emit(&mut self, kind: T, s: usize, e: usize) {
        self.toks.push(Tok {
            kind,
            text: Cow::Borrowed(&self.src[s..e]),
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

    /// Whether the text at p is word, and no word character follows, as Pele's kw says.
    fn kw(&self, word: &str) -> bool {
        self.src[self.p..].starts_with(word) && !is_word(self.at(self.p + word.len()))
    }

    /// The end of the white space from q, kept for the next call from within it: the part
    /// Pele's linkChar and invisAhead each write out.
    fn space_end(&mut self, q: usize) -> usize {
        if let Some((from, to)) = self.ws
            && q >= from
            && q <= to
        {
            return to;
        }
        if is_space(self.at(q)) {
            let j = space_run(self.src, q);
            self.ws = Some((q, j));
            return j;
        }
        q
    }

    /// The first character after any run of white space from q, past one x, o or <, as
    /// Pele's linkChar gives it.
    fn link_char(&mut self, q: usize) -> i32 {
        let j = self.space_end(q);
        match self.at(j) {
            120 | 111 | 60 => self.at(j + 1),
            d => d,
        }
    }

    /// Whether the first character after the white space from q is a tilde, as Pele's
    /// invisAhead says.
    fn invis_ahead(&mut self, q: usize) -> bool {
        let j = self.space_end(q);
        self.at(j) == 126
    }

    fn quote(&mut self) {
        if self.at(self.p + 1) == 96 {
            self.push(S::md_string);
            self.p += 2;
        } else {
            self.push(S::string);
            self.p += 1;
        }
    }

    fn invis_link(&mut self) -> bool {
        if !self.invis_ahead(self.p) {
            return false;
        }
        let Some(e) = invisible(self.src, self.p) else {
            return false;
        };
        self.emit(T::LINK, self.p, e);
        true
    }

    fn opener(&mut self, c: i32) -> bool {
        let p = self.p;
        let d = self.at(p + 1);
        match c {
            40 => {
                if d == 45 {
                    self.push(S::ellipseText);
                    self.emit(T::ELLIPSE_START, p, p + 2);
                } else if d == 91 {
                    self.push(S::text);
                    self.emit(T::STADIUMSTART, p, p + 2);
                } else if d == 40 && self.at(p + 2) == 40 {
                    self.push(S::text);
                    self.emit(T::DOUBLECIRCLESTART, p, p + 3);
                } else {
                    self.push(S::text);
                    self.emit(T::PS, p, p + 1);
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
                self.emit(kind, p, p + len);
                true
            }
            124 => {
                self.push(S::text);
                self.emit(T::PIPE, p, p + 1);
                true
            }
            123 => {
                self.push(S::text);
                self.emit(T::DIAMOND_START, p, p + 1);
                true
            }
            _ => false,
        }
    }

    fn graph(&mut self, len: usize) {
        if self.first_graph {
            self.first_graph = false;
            self.push(S::dir);
        }
        self.emit(T::GRAPH, self.p, self.p + len);
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
        self.emit(T::NODE_STRING, self.p, q);
        true
    }

    fn link(&mut self, c: i32) -> bool {
        let p = self.p;
        let d = self.link_char(p);
        let src = self.src;
        if d == 45 {
            if let Some(e) = solid_link(src, p, b'-') {
                self.emit(T::LINK, p, e);
                return true;
            }
            if let Some(e) = start_link(src, p, b'-') {
                self.push(S::edgeText);
                self.emit(T::START_LINK, p, e);
                return true;
            }
            if let Some(e) = dotted_link(src, p) {
                self.emit(T::LINK, p, e);
                return true;
            }
            if let Some(e) = dotted_start(src, p) {
                self.push(S::dottedEdgeText);
                self.emit(T::START_LINK, p, e);
                return true;
            }
        } else if d == 61 {
            if let Some(e) = solid_link(src, p, b'=') {
                self.emit(T::LINK, p, e);
                return true;
            }
            if let Some(e) = start_link(src, p, b'=') {
                self.push(S::thickEdgeText);
                self.emit(T::START_LINK, p, e);
                return true;
            }
        } else if d == 46
            && let Some(e) = dotted_link(src, p)
        {
            self.emit(T::LINK, p, e);
            return true;
        }
        c != 45 && c != 61 && c != 46 && self.invis_link()
    }

    fn initial(&mut self, c: i32) -> bool {
        let (src, p, n) = (self.src, self.p, self.n);
        match c {
            97 if src[p..].starts_with("acc") => {
                if let Some(e) = keyword_colon(src, p, "accTitle", b':') {
                    self.push(S::acc_title);
                    self.emit(T::acc_title, p, e);
                    return true;
                }
                if let Some(e) = keyword_colon(src, p, "accDescr", b':') {
                    self.push(S::acc_descr);
                    self.emit(T::acc_descr, p, e);
                    return true;
                }
                if let Some(e) = keyword_colon(src, p, "accDescr", b'{') {
                    self.push(S::acc_descr_multiline);
                    self.p = e;
                    return true;
                }
            }
            64 if self.at(p + 1) == 123 => {
                self.push(S::shapeData);
                self.toks.push(Tok {
                    kind: T::SHAPE_DATA,
                    text: Cow::Borrowed(""),
                    start: p,
                });
                self.p += 2;
                return true;
            }
            99 => {
                if let Some(e) = keyword_space(src, p, "call") {
                    self.push(S::callbackname);
                    self.p = e;
                    return true;
                }
                if self.kw("classDef") {
                    self.emit(T::CLASSDEF, p, p + 8);
                    return true;
                }
                if self.kw("class") {
                    self.emit(T::CLASS, p, p + 5);
                    return true;
                }
                if let Some(e) = keyword_space(src, p, "click") {
                    self.push(S::click);
                    self.p = e;
                    return true;
                }
            }
            34 => {
                self.quote();
                return true;
            }
            115 => {
                if self.kw("style") {
                    self.emit(T::STYLE, p, p + 5);
                    return true;
                }
                if self.kw("swimlane-beta") {
                    self.graph(13);
                    return true;
                }
                if self.kw("subgraph") {
                    self.emit(T::subgraph, p, p + 8);
                    return true;
                }
            }
            100 if self.kw("default") => {
                self.emit(T::DEFAULT, p, p + 7);
                return true;
            }
            108 if self.kw("linkStyle") => {
                self.emit(T::LINKSTYLE, p, p + 9);
                return true;
            }
            105 if self.kw("interpolate") => {
                self.emit(T::INTERPOLATE, p, p + 11);
                return true;
            }
            // href and the white space character after it.
            104 if src[p..].starts_with("href") && is_space(self.at(p + 4)) => {
                let e = self.next(p + 4);
                self.emit(T::HREF, p, e);
                return true;
            }
            102 => {
                if self.kw("flowchart-elk") {
                    self.graph(13);
                    return true;
                }
                if self.kw("flowchart") {
                    self.graph(9);
                    return true;
                }
            }
            103 if self.kw("graph") => {
                self.graph(5);
                return true;
            }
            101 if self.kw("end") => {
                let e = space_run(src, p + 3);
                self.emit(T::end, p, e);
                return true;
            }
            95 => {
                for word in ["_self", "_blank", "_parent", "_top"] {
                    if self.kw(word) {
                        self.emit(T::LINK_TARGET, p, p + word.len());
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
                            self.emit(DIRECTION_TOKENS[i], p, e);
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
                self.emit(T::LINK_ID, p, run_at + 1);
                return true;
            }
        }

        if (48..=57).contains(&c) {
            let q = p + src[p..].bytes().take_while(u8::is_ascii_digit).count();
            self.emit(T::NUM, p, q);
            return true;
        }

        match c {
            35 => {
                self.emit(T::BRKT, p, p + 1);
                return true;
            }
            58 => {
                if src[p..].starts_with(":::") {
                    self.emit(T::STYLE_SEPARATOR, p, p + 3);
                } else {
                    self.emit(T::COLON, p, p + 1);
                }
                return true;
            }
            38 | 59 | 44 | 42 => {
                let kind = match c {
                    38 => T::AMP,
                    59 => T::SEMI,
                    44 => T::COMMA,
                    _ => T::MULT,
                };
                self.emit(kind, p, p + 1);
                return true;
            }
            120 | 111 => return self.link(c) || self.node_string(),
            60 => {
                if !self.link(c) {
                    self.emit(T::TAGSTART, p, p + 1);
                }
                return true;
            }
            45 => {
                if !(self.link(c) || self.node_string()) {
                    self.emit(T::MINUS, p, p + 1);
                }
                return true;
            }
            61 => return self.link(c),
            46 => return self.link(c) || self.node_string(),
            126 => return self.invis_link(),
            40 => return self.opener(c),
            91 => {
                if self.at(p + 1) == 124 {
                    self.emit(T::VERTEX_WITH_PROPS_START, p, p + 2);
                    return true;
                }
                return self.opener(c);
            }
            124 | 123 => return self.opener(c),
            62 => {
                self.push(S::text);
                self.emit(T::TAGEND, p, p + 1);
                return true;
            }
            94 => {
                self.emit(T::UP, p, p + 1);
                return true;
            }
            92 => {
                if self.at(p + 1) == 124 {
                    self.emit(T::SEP, p, p + 2);
                    return true;
                }
                return self.node_string();
            }
            118 => {
                if !is_word(self.at(p + 1)) {
                    self.emit(T::DOWN, p, p + 1);
                    return true;
                }
                return self.node_string();
            }
            _ => {}
        }

        if is_space(c) {
            if self.link(c) {
                return true;
            }
            if let Some(e) = newline(src, p) {
                self.emit(T::NEWLINE, p, e);
                return true;
            }
            if let Some(e) = blanks(src, p) {
                self.emit(T::SPACE, p, e);
                return true;
            }
            return false;
        }

        if c < 128 {
            return self.node_string();
        }
        if is_unicode_letter(c) {
            let e = self.next(p);
            self.emit(T::UNICODE_TEXT, p, e);
            return true;
        }
        false
    }

    /// Reads in a link's label: a string, the end of the link that link_re matches, of
    /// characters a and b, or the text up to either, as Pele's edgeText does.
    fn edge_text(
        &mut self,
        c: i32,
        link_re: fn(&str, usize) -> Option<usize>,
        a: i32,
        b: i32,
    ) -> bool {
        let (src, p, n) = (self.src, self.p, self.n);
        if c == 34 {
            self.quote();
            return true;
        }
        let d = self.link_char(p);
        if (d == a || d == b)
            && let Some(e) = link_re(src, p)
        {
            self.pop();
            self.emit(T::LINK, p, e);
            return true;
        }
        if c == a && (a != 45 || self.at(p + 1) == 45) {
            return false;
        }
        let mut q = self.next(p);
        while q < n {
            let ch = self.at(q);
            if ch == 34 {
                break;
            }
            if ch == a {
                if a != 45 || self.at(q + 1) == 45 {
                    break;
                }
            } else if ch == 120 || ch == 111 || ch == 60 || ch == b || is_space(ch) {
                let dd = self.link_char(q);
                if (dd == a || dd == b) && link_re(src, q).is_some() {
                    break;
                }
            }
            q = self.next(q);
        }
        self.emit(T::EDGE_TEXT, p, q);
        true
    }

    /// The end of the run from p+1 of characters other than those stop is true of.
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
                    self.emit(kind, n, n);
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
                            self.p += 2;
                        } else {
                            self.pop();
                            self.p += 1;
                        }
                    } else {
                        let e = self.find_or_end('"');
                        self.emit(T::STR, p, e);
                    }
                }

                S::md_string => {
                    if c == 96 {
                        if self.at(p + 1) == 34 {
                            self.pop();
                            self.p += 2;
                        } else {
                            ok = false;
                        }
                    } else if c == 34 {
                        self.quote();
                    } else {
                        let q = self.run_to(self.next(p), |ch| ch == 96 || ch == 34);
                        self.emit(T::MD_STR, p, q);
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
                    self.emit(kind, p, e);
                }

                S::acc_descr_multiline => {
                    if c == 125 {
                        self.pop();
                        self.p += 1;
                    } else {
                        let e = self.find_or_end('}');
                        self.emit(T::acc_descr_multiline_value, p, e);
                    }
                }

                S::dir => {
                    if c == 34 {
                        self.quote();
                    } else if let Some(e) = nodir(src, p) {
                        self.pop();
                        self.emit(T::NODIR, p, e);
                    } else if let Some(e) = dir(src, p)
                        .or_else(|| dir_sym(src, p))
                        .or_else(|| dir_v(src, p))
                    {
                        self.pop();
                        self.emit(T::DIR, p, e);
                    } else {
                        ok = self.invis_link() || self.opener(c);
                    }
                }

                S::text => {
                    let d = self.at(p + 1);
                    match c {
                        34 => self.quote(),
                        40 | 91 | 123 => {
                            self.opener(c);
                        }
                        93 => {
                            self.pop();
                            match d {
                                41 => self.emit(T::STADIUMEND, p, p + 2),
                                93 => self.emit(T::SUBROUTINEEND, p, p + 2),
                                _ => self.emit(T::SQE, p, p + 1),
                            }
                        }
                        41 => {
                            self.pop();
                            if d == 93 {
                                self.emit(T::CYLINDEREND, p, p + 2);
                            } else if d == 41 && self.at(p + 2) == 41 {
                                self.emit(T::DOUBLECIRCLEEND, p, p + 3);
                            } else {
                                self.emit(T::PE, p, p + 1);
                            }
                        }
                        124 => {
                            self.pop();
                            self.emit(T::PIPE, p, p + 1);
                        }
                        125 => {
                            self.pop();
                            self.emit(T::DIAMOND_STOP, p, p + 1);
                        }
                        _ => {
                            if !((c == 126 || is_space(c)) && self.invis_link()) {
                                let q = self.run_to(self.next(p), |ch| {
                                    matches!(ch, 91 | 93 | 40 | 41 | 123 | 125 | 124 | 34)
                                });
                                self.emit(T::TEXT, p, q);
                            }
                        }
                    }
                }

                S::ellipseText => {
                    let d = self.at(p + 1);
                    if c == 34 {
                        self.quote();
                    } else if (c == 126 || is_space(c)) && self.invis_link() {
                    } else if (c == 45 || c == 47 || c == 41) && d == 41 {
                        self.pop();
                        self.emit(T::ELLIPSE_END, p, p + 2);
                    } else if c == 40 || c == 91 || c == 123 {
                        self.opener(c);
                    } else if c == 41 || c == 93 || c == 125 {
                        ok = false;
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
                            if (ch == 126 || is_space(ch))
                                && self.invis_ahead(q)
                                && invisible(src, q).is_some()
                            {
                                break;
                            }
                            q = self.next(q);
                        }
                        self.emit(T::TEXT, p, q);
                    }
                }

                S::trapText => ok = self.trap_text(c),

                S::edgeText => ok = self.edge_text(c, |s, q| solid_link(s, q, b'-'), 45, -1),
                S::thickEdgeText => ok = self.edge_text(c, |s, q| solid_link(s, q, b'='), 61, -1),
                S::dottedEdgeText => ok = self.edge_text(c, dotted_link, 46, 45),

                S::click => {
                    if c == 34 {
                        self.quote();
                    } else if is_space(c) {
                        self.pop();
                        self.p = self.next(p);
                    } else {
                        let q = self.run_to(self.next(p), is_space);
                        self.emit(T::CLICK, p, q);
                    }
                }

                S::callbackname => {
                    if c == 40 {
                        let e = empty_call(src, p);
                        self.pop();
                        match e {
                            Some(e) => self.p = e,
                            None => {
                                self.push(S::callbackargs);
                                self.p += 1;
                            }
                        }
                    } else {
                        let e = self.find_or_end('(');
                        self.emit(T::CALLBACKNAME, p, e);
                    }
                }

                S::callbackargs => {
                    if c == 41 {
                        self.pop();
                        self.p += 1;
                    } else {
                        let e = self.find_or_end(')');
                        self.emit(T::CALLBACKARGS, p, e);
                    }
                }

                S::shapeData => {
                    if c == 34 {
                        self.push(S::shapeDataStr);
                        self.emit(T::SHAPE_DATA, p, p + 1);
                    } else if c == 125 {
                        self.pop();
                        self.p += 1;
                    } else if c == 94 {
                        ok = false;
                    } else {
                        let q = self.run_to(self.next(p), |ch| ch == 125 || ch == 94 || ch == 34);
                        self.emit(T::SHAPE_DATA, p, q);
                    }
                }

                S::shapeDataStr => {
                    if c == 34 {
                        self.pop();
                        self.emit(T::SHAPE_DATA, p, p + 1);
                    } else {
                        let e = self.find_or_end('"');
                        self.toks.push(Tok {
                            kind: T::SHAPE_DATA,
                            text: breaks(&src[p..e]),
                            start: p,
                        });
                        self.p = e;
                    }
                }
            }

            if !ok {
                self.toks.push(Tok {
                    kind: T::ERROR,
                    text: Cow::Borrowed(""),
                    start: self.p,
                });
                return;
            }
        }
        self.toks.push(Tok {
            kind: T::END,
            text: Cow::Borrowed(""),
            start: self.n,
        });
    }

    /// The text of a trapezoid's label, Pele's case S.trapText.
    fn trap_text(&mut self, c: i32) -> bool {
        let p = self.p;
        let d = self.at(p + 1);
        if c == 34 {
            self.quote();
            return true;
        }
        if (c == 126 || is_space(c)) && self.invis_link() {
            return true;
        }
        match c {
            40 => {
                if d == 45 || d == 91 || (d == 40 && self.at(p + 2) == 40) {
                    self.opener(c);
                } else if d == 93 {
                    self.pop();
                    self.emit(T::TRAPEND, p, p + 2);
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
                    self.emit(if c == 92 { T::TRAPEND } else { T::INVTRAPEND }, p, p + 2);
                } else {
                    self.emit(T::TEXT, p, p + 1);
                }
            }
            93 | 41 => {
                if d != 93 {
                    return false;
                }
                self.pop();
                self.emit(T::TRAPEND, p, p + 2);
            }
            125 => return false,
            _ => {
                if (c == 63 || c == 61) && d == 93 {
                    self.pop();
                    self.emit(T::TRAPEND, p, p + 2);
                    return true;
                }
                let q = self.run_to(self.next(p), |ch| {
                    matches!(ch, 92 | 91 | 93 | 40 | 41 | 123 | 125 | 47)
                });
                self.emit(T::TEXT, p, q);
            }
        }
        true
    }
}
