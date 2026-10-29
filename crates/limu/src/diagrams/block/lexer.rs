//! A block diagram's text split into tokens, as Pele's lexer.ts and tokens.ts split it,
//! making the decisions the rules of the lexer Jison compiles from block.jison make, written
//! out by hand. On text no rule matches it stops with an ERROR token.
//!
//! Positions are bytes. Where Pele reads a code unit with charCodeAt, [`unit_at`] reads the
//! same one, and a character outside ASCII is stepped over whole.

use crate::js::chars::{is_space, is_word, space_run, unit, unit_at};

// The types of token, in the order of Pele's T.
pub(crate) const END: usize = 0;
pub(crate) const ERROR: usize = 1;
pub(crate) const KEY: usize = 2;
pub(crate) const ID_BLOCK: usize = 3;
pub(crate) const COLUMNS: usize = 4;
pub(crate) const MD_STR: usize = 5;
pub(crate) const STR: usize = 6;
pub(crate) const SPACE_BLOCK: usize = 7;
pub(crate) const DEFAULT: usize = 8;
pub(crate) const LINKSTYLE: usize = 9;
pub(crate) const INTERPOLATE: usize = 10;
pub(crate) const CLASSDEF: usize = 11;
pub(crate) const DEFAULT_CLASSDEF_ID: usize = 12;
pub(crate) const CLASSDEF_ID: usize = 13;
pub(crate) const CLASSDEF_STYLEOPTS: usize = 14;
pub(crate) const CLASS: usize = 15;
pub(crate) const CLASSENTITY_IDS: usize = 16;
pub(crate) const STYLECLASS: usize = 17;
pub(crate) const STYLE: usize = 18;
pub(crate) const STYLE_ENTITY_IDS: usize = 19;
pub(crate) const STYLE_DEFINITION_DATA: usize = 20;
pub(crate) const ACC_TITLE: usize = 21;
pub(crate) const ACC_TITLE_VALUE: usize = 22;
pub(crate) const ACC_DESCR: usize = 23;
pub(crate) const ACC_DESCR_VALUE: usize = 24;
pub(crate) const ACC_DESCR_MULTILINE_VALUE: usize = 25;
pub(crate) const END_KEYWORD: usize = 26;
pub(crate) const NODE_DSTART: usize = 27;
pub(crate) const NODE_DEND: usize = 28;
pub(crate) const BLOCK_ARROW_START: usize = 29;
pub(crate) const BLOCK_ARROW_END: usize = 30;
pub(crate) const DIR: usize = 31;
pub(crate) const NODE_ID: usize = 32;
pub(crate) const LINK: usize = 33;
pub(crate) const START_LINK: usize = 34;
pub(crate) const LINK_LABEL: usize = 35;
pub(crate) const SIZE: usize = 36;
pub(crate) const EOF: usize = 37;

/// The names of the types of token, as Mermaid's errors give them, Pele's TOKEN_NAMES.
pub(crate) const TOKEN_NAMES: [&str; 38] = [
    "$end",
    "INVALID",
    "BLOCK_DIAGRAM_KEY",
    "id-block",
    "COLUMNS",
    "MD_STR",
    "STR",
    "SPACE_BLOCK",
    "DEFAULT",
    "LINKSTYLE",
    "INTERPOLATE",
    "classDef",
    "DEFAULT_CLASSDEF_ID",
    "CLASSDEF_ID",
    "CLASSDEF_STYLEOPTS",
    "class",
    "CLASSENTITY_IDS",
    "STYLECLASS",
    "style",
    "STYLE_ENTITY_IDS",
    "STYLE_DEFINITION_DATA",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "end",
    "NODE_DSTART",
    "NODE_DEND",
    "BLOCK_ARROW_START",
    "BLOCK_ARROW_END",
    "DIR",
    "NODE_ID",
    "LINK",
    "START_LINK",
    "LINK_LABEL",
    "SIZE",
    "EOF",
];

/// The tokens of a text, Pele's Tokens: the type of each, its text, and the byte it starts
/// at.
pub(crate) struct Tokens {
    pub(crate) types: Vec<usize>,
    pub(crate) texts: Vec<String>,
    pub(crate) starts: Vec<usize>,
}

// The states of the lexer, as Pele's S.
#[derive(Clone, Copy, PartialEq, Eq)]
enum S {
    Initial,
    String,
    MdString,
    Node,
    BlockArrow,
    ArrowDir,
    LLabel,
    ClassDef,
    ClassDefId,
    Class,
    ClassStyle,
    StyleStmnt,
    StyleDefinition,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
}

/// The directions an arrow block may point, Pele's DIRECTIONS, in the order they are tried.
const DIRECTIONS: [&str; 6] = ["right", "left", "x", "y", "up", "down"];

/// Whether an ASCII code unit ends a node id, Pele's ID_STOP: ( ) [ { } < > - : = and white
/// space.
fn id_stop(c: i32) -> bool {
    matches!(
        c,
        9 | 10 | 11 | 12 | 13 | 32 | 40 | 41 | 45 | 58 | 60 | 61 | 62 | 91 | 123 | 125
    )
}

/// What the lexer has read: the tokens so far, where it is, and the stack of its states.
struct Lexer<'a> {
    src: &'a str,
    out: Tokens,
    stack: Vec<S>,
    state: S,
    p: usize,
}

impl Lexer<'_> {
    fn n(&self) -> usize {
        self.src.len()
    }

    fn at(&self, i: usize) -> i32 {
        unit(self.src, i)
    }

    fn starts(&self, text: &str, at: usize) -> bool {
        self.src.get(at..).is_some_and(|s| s.starts_with(text))
    }

    fn put(&mut self, kind: usize, text: &str, end: usize) -> bool {
        self.out.types.push(kind);
        self.out.texts.push(text.to_string());
        self.out.starts.push(self.p);
        self.p = end;
        true
    }

    fn emit(&mut self, kind: usize, end: usize) -> bool {
        let text = &self.src[self.p..end];
        self.put(kind, text, end)
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

    fn ws(&self, q: usize) -> usize {
        space_run(self.src, q)
    }

    fn digits(&self, mut q: usize) -> usize {
        while q < self.n() && self.src.as_bytes()[q].is_ascii_digit() {
            q += 1;
        }
        q
    }

    fn word(&self, mut q: usize) -> usize {
        while q < self.n() && is_word(self.at(q)) {
            q += 1;
        }
        q
    }

    fn keyword(&self, text: &str) -> bool {
        self.starts(text, self.p) && !is_word(self.at(self.p + text.len()))
    }

    fn quote(&mut self) -> bool {
        if self.at(self.p + 1) == 96 {
            self.push(S::MdString);
            self.p += 2;
        } else {
            self.push(S::String);
            self.p += 1;
        }
        true
    }

    /// The end of `.+-[xo>]?` at q, or none.
    fn dotted(&self, mut q: usize) -> Option<usize> {
        if self.at(q) != 46 {
            return None;
        }
        q += 1;
        while self.at(q) == 46 {
            q += 1;
        }
        if self.at(q) != 45 {
            return None;
        }
        let c = self.at(q + 1);
        Some(if c == 120 || c == 111 || c == 62 {
            q + 2
        } else {
            q + 1
        })
    }

    /// The end of a complete link at q, before its trailing white space, or none.
    fn link_end(&self, mut q: usize) -> Option<usize> {
        let c = self.at(q);
        if c == 120 || c == 111 || c == 60 {
            q += 1;
        }
        let line = self.at(q);
        if line == 46 {
            return self.dotted(q);
        }
        if line != 45 && line != 61 {
            return None;
        }
        let mut k = q + 1;
        while self.at(k) == line {
            k += 1;
        }
        if k - q == 1 {
            return if line == 45 { self.dotted(k) } else { None };
        }
        let head = self.at(k);
        if head == 120 || head == 111 || head == 62 {
            return Some(k + 1);
        }
        (k - q >= 3).then_some(k)
    }

    fn link(&mut self) -> bool {
        if let Some(e) = self.link_end(self.p) {
            let end = self.ws(e);
            return self.emit(LINK, end);
        }
        let q = if self.at(self.p) == 60 {
            self.p + 1
        } else {
            self.p
        };
        let (a, b) = (self.at(q), self.at(q + 1));
        if (a == 45 && (b == 45 || b == 46)) || (a == 61 && b == 61) {
            self.push(S::LLabel);
            let end = self.ws(q + 2);
            return self.emit(START_LINK, end);
        }
        false
    }

    fn line_state(&mut self, kind: usize) -> bool {
        let e = self.src[self.p..]
            .find('\n')
            .map_or(self.n(), |i| self.p + i);
        self.pop();
        self.emit(kind, e)
    }

    /// `\w+(,\s*\w+)*`
    fn id_list(&mut self, kind: usize, next: S) -> bool {
        let mut k = self.word(self.p);
        if k == self.p {
            return false;
        }
        while self.at(k) == 44 {
            let from = self.ws(k + 1);
            let to = self.word(from);
            if to == from {
                break;
            }
            k = to;
        }
        self.state = next;
        self.emit(kind, k)
    }

    fn initial(&mut self, c: i32) -> bool {
        let p = self.p;
        if is_space(c) {
            self.p = self.ws(p);
            return true;
        }
        match c {
            98 => {
                if self.starts("block", p) {
                    if self.keyword("block-beta") {
                        return self.emit(KEY, p + 10);
                    }
                    if self.at(p + 5) == 58 {
                        return self.emit(ID_BLOCK, p + 6);
                    }
                    if !is_word(self.at(p + 5)) {
                        return self.emit(KEY, p + 5);
                    }
                }
            }
            99 => {
                if self.starts("columns", p) {
                    let q = self.ws(p + 7);
                    if q > p + 7 {
                        if self.starts("auto", q) && !is_word(self.at(q + 4)) {
                            return self.put(COLUMNS, "-1", q + 4);
                        }
                        let e = self.digits(q);
                        if e > q {
                            let text = &self.src[q..e];
                            return self.put(COLUMNS, text, e);
                        }
                    }
                } else if self.starts("class", p) {
                    let def = self.starts("Def", p + 5);
                    let from = if def { p + 8 } else { p + 5 };
                    let q = self.ws(from);
                    if q > from {
                        self.push(if def { S::ClassDef } else { S::Class });
                        return self.emit(if def { CLASSDEF } else { CLASS }, q);
                    }
                }
            }
            34 => return self.quote(),
            115 => {
                if self.starts("space", p) {
                    if self.at(p + 5) == 58 {
                        let e = self.digits(p + 6);
                        if e > p + 6 {
                            let text = &self.src[p + 6..e];
                            return self.put(SPACE_BLOCK, text, e);
                        }
                    }
                    if !is_word(self.at(p + 5)) {
                        return self.put(SPACE_BLOCK, "1", p + 5);
                    }
                } else if self.starts("style", p) {
                    let q = self.ws(p + 5);
                    if q > p + 5 {
                        self.push(S::StyleStmnt);
                        return self.emit(STYLE, q);
                    }
                }
            }
            100 => {
                if self.keyword("default") {
                    return self.emit(DEFAULT, p + 7);
                }
            }
            108 => {
                if self.keyword("linkStyle") {
                    return self.emit(LINKSTYLE, p + 9);
                }
            }
            105 => {
                if self.keyword("interpolate") {
                    return self.emit(INTERPOLATE, p + 11);
                }
            }
            97 => {
                if self.starts("accTitle", p) {
                    let q = self.ws(p + 8);
                    if self.at(q) == 58 {
                        self.push(S::AccTitle);
                        let end = self.ws(q + 1);
                        return self.emit(ACC_TITLE, end);
                    }
                } else if self.starts("accDescr", p) {
                    let q = self.ws(p + 8);
                    let d = self.at(q);
                    if d == 58 {
                        self.push(S::AccDescr);
                        let end = self.ws(q + 1);
                        return self.emit(ACC_DESCR, end);
                    }
                    if d == 123 {
                        self.push(S::AccDescrMultiline);
                        self.p = self.ws(q + 1);
                        return true;
                    }
                }
            }
            101 => {
                if self.keyword("end") {
                    let end = self.ws(p + 3);
                    return self.emit(END_KEYWORD, end);
                }
            }
            45 => {
                if self.at(p + 1) == 41 {
                    self.push(S::Node);
                    return self.emit(NODE_DSTART, p + 2);
                }
                return self.link();
            }
            61 => return self.link(),
            60 => {
                if self.at(p + 1) == 91 {
                    self.push(S::BlockArrow);
                    return self.emit(BLOCK_ARROW_START, p + 2);
                }
                return self.link();
            }
            40 => {
                let d = self.at(p + 1);
                let len = if d == 45 || d == 91 {
                    2
                } else if d == 40 {
                    if self.at(p + 2) == 40 { 3 } else { 2 }
                } else {
                    1
                };
                self.push(S::Node);
                return self.emit(NODE_DSTART, p + len);
            }
            41 | 123 => {
                self.push(S::Node);
                let len = if self.at(p + 1) == c { 2 } else { 1 };
                return self.emit(NODE_DSTART, p + len);
            }
            62 => {
                self.push(S::Node);
                return self.emit(NODE_DSTART, p + 1);
            }
            91 => {
                let d = self.at(p + 1);
                self.push(S::Node);
                let len = if matches!(d, 91 | 124 | 40 | 92 | 47) {
                    2
                } else {
                    1
                };
                return self.emit(NODE_DSTART, p + len);
            }
            58 => {
                let e = self.digits(p + 1);
                if e > p + 1 {
                    let text = &self.src[p + 1..e];
                    return self.put(SIZE, text, e);
                }
                return false;
            }
            _ => {}
        }

        let mut q = p;
        while q < self.n() {
            let (ch, size) = unit_at(self.src, q);
            if if ch < 128 { id_stop(ch) } else { is_space(ch) } {
                break;
            }
            q += size;
        }
        q > p && self.emit(NODE_ID, q)
    }

    fn node_end(&mut self, c: i32) -> bool {
        let p = self.p;
        let d = self.at(p + 1);
        let len = match c {
            34 => return self.quote(),
            40 => {
                if d == 45 {
                    2
                } else if d == 40 {
                    if self.at(p + 2) == 40 { 3 } else { 2 }
                } else {
                    1
                }
            }
            41 => {
                if d == 41 {
                    if self.at(p + 2) == 41 { 3 } else { 2 }
                } else if d == 93 {
                    2
                } else {
                    1
                }
            }
            125 => {
                if d == 125 {
                    2
                } else {
                    1
                }
            }
            45 if d == 41 => 2,
            93 => {
                if d == 93 || d == 41 || d == 62 {
                    2
                } else {
                    1
                }
            }
            92 | 47 if d == 93 => 2,
            _ => 0,
        };
        if len == 0 {
            return false;
        }
        self.pop();
        self.emit(NODE_DEND, p + len)
    }

    fn arrow_dir(&mut self, c: i32) -> bool {
        let p = self.p;
        if c == 41 {
            self.pop();
            self.pop();
            let end = self.ws(p + 1);
            return self.put(BLOCK_ARROW_END, "]>", end);
        }
        let from = self.ws(if c == 44 { p + 1 } else { p });
        for dir in DIRECTIONS {
            if self.starts(dir, from) {
                let e = self.ws(from + dir.len());
                let text = &self.src[if c == 44 { from } else { p }..e];
                return self.put(DIR, text, e);
            }
        }
        false
    }

    /// Reads one token or a part of one in the current state, and whether a rule matched.
    fn step(&mut self, c: i32) -> bool {
        let p = self.p;
        match self.state {
            S::Initial => return self.initial(c),
            S::String => {
                if c == 34 {
                    self.pop();
                    self.p += 1;
                } else {
                    let e = self.src[p..].find('"').map_or(self.n(), |i| p + i);
                    self.emit(STR, e);
                }
            }
            S::MdString => {
                if c == 96 {
                    let ok = self.at(p + 1) == 34;
                    if ok {
                        self.pop();
                        self.p += 2;
                    }
                    return ok;
                } else if c == 34 {
                    return false;
                }
                let bytes = self.src.as_bytes();
                let mut q = p + unit_at(self.src, p).1;
                while q < self.n() && bytes[q] != b'`' && bytes[q] != b'"' {
                    q += 1;
                }
                self.emit(MD_STR, q);
            }
            S::Node => return self.node_end(c),
            S::BlockArrow => {
                if c == 34 {
                    self.quote();
                } else if c == 93 && self.at(p + 1) == 62 {
                    let q = self.ws(p + 2);
                    if self.at(q) != 40 {
                        return false;
                    }
                    self.push(S::ArrowDir);
                    self.p = q + 1;
                } else {
                    return false;
                }
            }
            S::ArrowDir => return self.arrow_dir(c),
            S::LLabel => {
                if c == 34 {
                    if self.at(p + 1) == 96 {
                        self.quote();
                    } else {
                        self.emit(LINK_LABEL, p + 1);
                        self.push(S::String);
                    }
                } else {
                    let Some(e) = self.link_end(self.ws(p)) else {
                        return false;
                    };
                    self.pop();
                    let end = self.ws(e);
                    self.emit(LINK, end);
                }
            }
            S::ClassDef => {
                let mut kind = DEFAULT_CLASSDEF_ID;
                let mut k = if self.starts("DEFAULT", p) { p + 7 } else { p };
                let mut q = self.ws(k);
                if k == p || q == k {
                    kind = CLASSDEF_ID;
                    k = self.word(p);
                    q = self.ws(k);
                }
                if !(k > p && q > k) {
                    return false;
                }
                self.state = S::ClassDefId;
                self.emit(kind, q);
            }
            S::ClassDefId => {
                self.line_state(CLASSDEF_STYLEOPTS);
            }
            S::Class => return self.id_list(CLASSENTITY_IDS, S::ClassStyle),
            S::ClassStyle => {
                self.line_state(STYLECLASS);
            }
            S::StyleStmnt => return self.id_list(STYLE_ENTITY_IDS, S::StyleDefinition),
            S::StyleDefinition => {
                self.line_state(STYLE_DEFINITION_DATA);
            }
            S::AccTitle => {
                self.line_state(ACC_TITLE_VALUE);
            }
            S::AccDescr => {
                self.line_state(ACC_DESCR_VALUE);
            }
            S::AccDescrMultiline => {
                if c == 125 {
                    self.pop();
                    self.p += 1;
                } else {
                    let e = self.src[p..].find('}').map_or(self.n(), |i| p + i);
                    self.emit(ACC_DESCR_MULTILINE_VALUE, e);
                }
            }
        }
        true
    }
}

/// Splits a block diagram's text into tokens, as Pele's tokenize does.
pub(crate) fn tokenize(src: &str) -> Tokens {
    let mut l = Lexer {
        src,
        out: Tokens {
            types: Vec::new(),
            texts: Vec::new(),
            starts: Vec::new(),
        },
        stack: Vec::new(),
        state: S::Initial,
        p: 0,
    };
    let n = src.len();
    loop {
        if l.p >= n {
            let kind = match l.state {
                S::Initial => Some(EOF),
                S::String => Some(STR),
                S::ClassDefId => Some(CLASSDEF_STYLEOPTS),
                S::ClassStyle => Some(STYLECLASS),
                S::StyleDefinition => Some(STYLE_DEFINITION_DATA),
                S::AccTitle => Some(ACC_TITLE_VALUE),
                S::AccDescr => Some(ACC_DESCR_VALUE),
                S::AccDescrMultiline => Some(ACC_DESCR_MULTILINE_VALUE),
                _ => None,
            };
            l.p = n;
            if let Some(kind) = kind {
                l.put(kind, "", n);
            }
            break;
        }
        let c = l.at(l.p);
        if !l.step(c) {
            l.out.types.push(ERROR);
            l.out.texts.push(String::new());
            l.out.starts.push(l.p);
            return l.out;
        }
    }
    l.out.types.push(END);
    l.out.texts.push(String::new());
    l.out.starts.push(n);
    l.out
}
