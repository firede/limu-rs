//! The lexer of a class diagram, as Pele's lexer.ts has it: the rules of Mermaid's
//! classDiagram.jison, tried by hand in the states it reads them in. Positions are bytes.
//! Where Pele reads a code unit with charCodeAt, unit reads the same one, and a character
//! outside ASCII is stepped over whole.

use super::tokens::T;
use crate::js::chars::{
    is_line_end, is_space, is_unicode_letter, is_word, space_run, unit, unit_at,
};

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
    bqstring,
    generic,
    callbackName,
    callbackArgs,
    accTitle,
    accDescr,
    accDescrMultiline,
    cls,
    classBody,
    namespace,
    namespaceBody,
}

/// The token a state gives at the end of the text, before the parser sees the end itself, as
/// Pele's AT_END has them.
fn at_end(state: S) -> T {
    match state {
        S::INITIAL | S::bqstring | S::cls | S::namespace => T::EOF,
        S::string => T::STR,
        S::generic => T::GENERICTYPE,
        S::callbackName => T::CALLBACK_NAME,
        S::callbackArgs => T::CALLBACK_ARGS,
        S::accTitle => T::acc_title_value,
        S::accDescr => T::acc_descr_value,
        S::accDescrMultiline => T::acc_descr_multiline_value,
        S::classBody | S::namespaceBody => T::EOF_IN_STRUCT,
    }
}

/// The direction statements, in the order Pele's RE_DIRECTION tries them.
const DIRECTIONS: [(&str, T); 4] = [
    ("TB", T::direction_tb),
    ("BT", T::direction_bt),
    ("RL", T::direction_rl),
    ("LR", T::direction_lr),
];

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

/// /.*direction\s+<name>[^\n]*/y at q, one of Pele's RE_DIRECTION, with end the end of the
/// line q is on: the last `direction` on the line that is followed by white space and the
/// name, as the greedy .* backs off to it, and the rest of the line the match ends on.
fn direction_at(s: &str, q: usize, end: usize, name: &str) -> Option<usize> {
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

struct Lexer<'a> {
    src: &'a str,
    toks: Vec<Tok<'a>>,
    stack: Vec<S>,
    state: S,
    p: usize,
    /// Where the next `direction` is, at or after where it was last looked for.
    dir_next: Option<usize>,
    /// The end of the line a direction was last looked for on.
    line_end: Option<usize>,
    /// The end of the last line on which no direction statement starts.
    dir_failed: usize,
}

impl<'a> Lexer<'a> {
    fn emit(&mut self, kind: T, s: usize, e: usize) -> bool {
        self.toks.push(Tok {
            kind,
            text: &self.src[s..e],
            start: s,
        });
        self.p = e;
        true
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

    /// Whether a keyword starts here and no word goes on after it, as Pele's kw says.
    fn kw(&self, word: &str) -> bool {
        self.src[self.p..].starts_with(word) && !is_word(unit(self.src, self.p + word.len()))
    }

    fn starts(&self, at: usize, s: &str) -> bool {
        self.src.as_bytes()[at.min(self.src.len())..].starts_with(s.as_bytes())
    }

    /// Where the text from here reaches ch, or its end, as Pele's until finds it.
    fn until(&self, ch: char) -> usize {
        self.src[self.p..]
            .find(ch)
            .map_or(self.src.len(), |e| self.p + e)
    }

    /// A run of white space, as Pele's space reads it: a NEWLINE up to its last line feed,
    /// and the rest skipped. leaves says that a NEWLINE leaves the state.
    fn space(&mut self, leaves: bool) {
        let mut q = self.p;
        let mut k = None;
        loop {
            let (c, size) = unit_at(self.src, q);
            if !is_space(c) {
                break;
            }
            if c == 10 {
                k = Some(q);
            }
            q += size;
        }
        let Some(k) = k else {
            self.p = q;
            return;
        };
        if leaves {
            self.pop();
        }
        self.emit(T::NEWLINE, self.p, k + 1);
    }

    /// The keywords that every state reads, even inside generics and backtick names, as
    /// Pele's keyword reads them.
    fn keyword(&mut self, c: i32) -> bool {
        let p = self.p;
        match c {
            99 => {
                if self.kw("cssClass") {
                    return self.emit(T::CSSCLASS, p, p + 8);
                }
                if self.kw("callback") {
                    return self.emit(T::CALLBACK, p, p + 8);
                }
                if self.kw("click") {
                    return self.emit(T::CLICK, p, p + 5);
                }
            }
            108 if self.kw("link") => return self.emit(T::LINK, p, p + 4),
            110 => {
                if self.kw("note for") {
                    return self.emit(T::NOTE_FOR, p, p + 8);
                }
                if self.kw("note") {
                    return self.emit(T::NOTE, p, p + 4);
                }
            }
            60 if unit(self.src, p + 1) == 60 => return self.emit(T::ANNOTATION_START, p, p + 2),
            62 if unit(self.src, p + 1) == 62 => return self.emit(T::ANNOTATION_END, p, p + 2),
            104 if self.kw("href") => return self.emit(T::HREF, p, p + 4),
            _ => {}
        }
        false
    }

    /// What the states share, as Pele's common reads it.
    fn common(&mut self, c: i32) -> bool {
        if self.keyword(c) {
            return true;
        }
        let p = self.p;
        let d = unit(self.src, p + 1);
        match c {
            34 => {
                self.push(S::string);
                self.p += 1;
                return true;
            }
            126 => {
                self.push(S::generic);
                self.p += 1;
                return true;
            }
            96 => {
                self.push(S::bqstring);
                self.p += 1;
                return true;
            }
            95 => {
                for target in ["_self", "_blank", "_parent", "_top"] {
                    if self.kw(target) {
                        return self.emit(T::LINK_TARGET, p, p + target.len());
                    }
                }
            }
            60 => {
                return if d == 124 {
                    self.emit(T::EXTENSION, p, p + 2)
                } else {
                    self.emit(T::DEPENDENCY, p, p + 1)
                };
            }
            124 => return d == 62 && self.emit(T::EXTENSION, p, p + 2),
            62 => return self.emit(T::DEPENDENCY, p, p + 1),
            42 => return self.emit(T::COMPOSITION, p, p + 1),
            111 if !is_word(d) => return self.emit(T::AGGREGATION, p, p + 1),
            40 => return d == 41 && self.emit(T::LOLLIPOP, p, p + 2),
            45 => {
                return if d == 45 {
                    self.emit(T::LINE, p, p + 2)
                } else {
                    self.emit(T::MINUS, p, p + 1)
                };
            }
            46 => {
                return if d == 46 {
                    self.emit(T::DOTTED_LINE, p, p + 2)
                } else {
                    self.emit(T::DOT, p, p + 1)
                };
            }
            58 => {
                let b = self.src.as_bytes();
                let mut q = p + 1;
                while q < b.len() && !matches!(b[q], b':' | b'\n' | b';') {
                    q += 1;
                }
                if q > p + 1 {
                    return self.emit(T::LABEL, p, q);
                }
                return d == 58
                    && unit(self.src, p + 2) == 58
                    && self.emit(T::STYLE_SEPARATOR, p, p + 3);
            }
            43 => return self.emit(T::PLUS, p, p + 1),
            37 => return self.emit(T::PCT, p, p + 1),
            61 => return self.emit(T::EQUALS, p, p + 1),
            91 => return self.emit(T::SQS, p, p + 1),
            93 => return self.emit(T::SQE, p, p + 1),
            33 | 35 | 36 | 38 | 39 | 44 | 63 | 92 | 47 => {
                return self.emit(T::PUNCTUATION, p, p + 1);
            }
            _ => {}
        }
        if is_word(c) {
            let mut q = p + 1;
            while is_word(unit(self.src, q)) {
                q += 1;
            }
            return self.emit(T::ALPHA, p, q);
        }
        is_unicode_letter(c) && self.emit(T::UNICODE_TEXT, p, p + unit_at(self.src, p).1)
    }

    /// A direction statement that starts here, as Pele's direction finds it. A line on which
    /// none starts is not looked at again.
    fn direction(&mut self) -> bool {
        let p = self.p;
        let Some(mut next) = self.dir_next else {
            return false;
        };
        if p < self.dir_failed {
            return false;
        }
        if next < p {
            match self.src[p..].find("direction") {
                Some(at) => next = p + at,
                None => {
                    self.dir_next = None;
                    return false;
                }
            }
            self.dir_next = Some(next);
        }
        let end = match self.line_end {
            Some(e) if e >= p => e,
            _ => {
                let e = line_end_from(self.src, p);
                self.line_end = Some(e);
                e
            }
        };
        if next >= end {
            return false;
        }
        for (name, kind) in DIRECTIONS {
            if let Some(e) = direction_at(self.src, p, end, name) {
                return self.emit(kind, p, e);
            }
        }
        // No direction statement starts anywhere in the rest of this line either.
        self.dir_failed = end;
        false
    }

    /// The rules of the INITIAL state, as Pele's initial reads them.
    fn initial(&mut self, c: i32) -> bool {
        if self.direction() {
            return true;
        }
        if is_space(c) {
            self.space(false);
            return true;
        }
        let p = self.p;
        let n = self.src.len();
        match c {
            37 if unit(self.src, p + 1) == 37 => {
                let b = self.src.as_bytes();
                let mut e = self.until('\n');
                while e < n && matches!(b[e], b'\n' | b'\r') {
                    e += 1;
                }
                self.p = e;
                return true;
            }
            97 => {
                let title = self.starts(p, "accTitle");
                if title || self.starts(p, "accDescr") {
                    let q = space_run(self.src, p + 8);
                    let d = unit(self.src, q);
                    if d == 58 {
                        self.emit(
                            if title { T::acc_title } else { T::acc_descr },
                            p,
                            space_run(self.src, q + 1),
                        );
                        self.push(if title { S::accTitle } else { S::accDescr });
                        return true;
                    }
                    if d == 123 && !title {
                        self.p = space_run(self.src, q + 1);
                        self.push(S::accDescrMultiline);
                        return true;
                    }
                }
            }
            99 => {
                if self.kw("classDiagram-v2") {
                    return self.emit(T::CLASS_DIAGRAM, p, p + 15);
                }
                if self.kw("classDiagram") {
                    return self.emit(T::CLASS_DIAGRAM, p, p + 12);
                }
                if self.starts(p, "call") && is_space(unit(self.src, p + 4)) {
                    self.p = space_run(self.src, p + 4);
                    self.push(S::callbackName);
                    return true;
                }
                if self.kw("classDef") {
                    return self.emit(T::CLASSDEF, p, p + 8);
                }
                if self.kw("class") {
                    self.push(S::cls);
                    return self.emit(T::CLASS, p, p + 5);
                }
            }
            91 if self.starts(p + 1, "*]") => return self.emit(T::EDGE_STATE, p, p + 3),
            115 if self.kw("style") => return self.emit(T::STYLE, p, p + 5),
            110 if self.kw("namespace") => {
                self.push(S::namespace);
                return self.emit(T::NAMESPACE, p, p + 9);
            }
            44 => return self.emit(T::COMMA, p, p + 1),
            35 => return self.emit(T::BRKT, p, p + 1),
            _ => {}
        }
        self.common(c) || (c == 58 && self.emit(T::COLON, p, p + 1))
    }

    /// Reads one token, or skips what gives none, in the state the lexer is in. It is false
    /// where no rule reads what is here.
    fn step(&mut self, c: i32) -> bool {
        let p = self.p;
        match self.state {
            S::INITIAL => return self.initial(c),
            S::cls => {
                if is_space(c) {
                    self.space(true);
                } else if c == 125 {
                    self.pop();
                    self.pop();
                    self.emit(T::STRUCT_STOP, p, p + 1);
                } else if c == 123 {
                    self.push(S::classBody);
                    self.emit(T::STRUCT_START, p, p + 1);
                } else {
                    return self.common(c);
                }
            }
            S::classBody => {
                if c == 34 {
                    self.push(S::string);
                    self.p += 1;
                } else if c == 125 {
                    self.pop();
                    self.emit(T::STRUCT_STOP, p, p + 1);
                } else if c == 91 && self.starts(p + 1, "*]") {
                    self.emit(T::EDGE_STATE, p, p + 3);
                } else if c == 123 {
                    self.emit(T::OPEN_IN_STRUCT, p, p + 1);
                } else if c == 10 {
                    self.p += 1;
                } else {
                    let b = self.src.as_bytes();
                    let mut q = p + 1;
                    while q < b.len() && !matches!(b[q], b'{' | b'}' | b'\n') {
                        q += 1;
                    }
                    self.emit(T::MEMBER, p, q);
                }
            }
            S::namespace => {
                if is_space(c) {
                    self.space(true);
                } else if c == 123 {
                    self.push(S::namespaceBody);
                    self.emit(T::STRUCT_START, p, p + 1);
                } else if c == 125 {
                    // The brace is read again in the state the namespace was opened from.
                    self.pop();
                } else if c == 110 && self.kw("namespace") {
                    self.push(S::namespace);
                    self.emit(T::NAMESPACE, p, p + 9);
                } else {
                    return self.common(c);
                }
            }
            S::namespaceBody => {
                if is_space(c) {
                    self.space(false);
                } else if c == 125 {
                    self.pop();
                    self.emit(T::STRUCT_STOP, p, p + 1);
                } else if c == 110 && self.kw("namespace") {
                    self.push(S::namespace);
                    self.emit(T::NAMESPACE, p, p + 9);
                } else if c == 91 && self.starts(p + 1, "*]") {
                    self.emit(T::EDGE_STATE, p, p + 3);
                } else if c == 99 && self.kw("class") {
                    self.push(S::cls);
                    self.emit(T::CLASS, p, p + 5);
                } else {
                    return self.common(c);
                }
            }
            S::string => {
                if c == 34 {
                    self.pop();
                    self.p += 1;
                } else {
                    let e = self.until('"');
                    self.emit(T::STR, p, e);
                }
            }
            S::generic => {
                if c == 34 {
                    self.push(S::string);
                    self.p += 1;
                } else if self.keyword(c) {
                    // A keyword is read here too, as a token of its own.
                } else if c == 126 {
                    self.pop();
                    self.p += 1;
                } else {
                    let e = self.until('~');
                    self.emit(T::GENERICTYPE, p, e);
                }
            }
            S::bqstring => {
                if c == 34 {
                    self.push(S::string);
                    self.p += 1;
                } else if self.keyword(c) {
                    // A keyword is read here too, as a token of its own.
                } else if c == 126 {
                    self.push(S::generic);
                    self.p += 1;
                } else if c == 96 {
                    self.pop();
                    self.p += 1;
                } else {
                    let e = self.until('`');
                    self.emit(T::BQUOTE_STR, p, e);
                }
            }
            S::callbackName => {
                if c == 40 {
                    let q = space_run(self.src, p + 1);
                    self.pop();
                    if unit(self.src, q) == 41 {
                        self.p = q + 1;
                    } else {
                        self.push(S::callbackArgs);
                        self.p += 1;
                    }
                } else {
                    let e = self.until('(');
                    self.emit(T::CALLBACK_NAME, p, e);
                }
            }
            S::callbackArgs => {
                if c == 41 {
                    self.pop();
                    self.p += 1;
                } else {
                    let e = self.until(')');
                    self.emit(T::CALLBACK_ARGS, p, e);
                }
            }
            S::accTitle | S::accDescr => {
                let kind = if self.state == S::accTitle {
                    T::acc_title_value
                } else {
                    T::acc_descr_value
                };
                let e = self.until('\n');
                self.emit(kind, p, e);
                self.pop();
            }
            S::accDescrMultiline => {
                if c == 125 {
                    self.pop();
                    self.p += 1;
                } else {
                    let e = self.until('}');
                    self.emit(T::acc_descr_multiline_value, p, e);
                }
            }
        }
        true
    }
}

/// The tokens of a class diagram's text, as Pele's tokenize reads them. They end with END,
/// or, where no rule reads what is next, with ERROR at where that is.
pub(crate) fn tokenize(src: &str) -> Vec<Tok<'_>> {
    let mut lx = Lexer {
        src,
        toks: Vec::new(),
        stack: Vec::new(),
        state: S::INITIAL,
        p: 0,
        dir_next: src.find("direction"),
        line_end: None,
        dir_failed: 0,
    };
    let n = src.len();
    loop {
        if lx.p >= n {
            lx.toks.push(Tok {
                kind: at_end(lx.state),
                text: "",
                start: n,
            });
            break;
        }
        let c = unit(src, lx.p);
        if !lx.step(c) {
            let start = lx.p;
            lx.toks.push(Tok {
                kind: T::ERROR,
                text: "",
                start,
            });
            return lx.toks;
        }
    }
    lx.toks.push(Tok {
        kind: T::END,
        text: "",
        start: n,
    });
    lx.toks
}
