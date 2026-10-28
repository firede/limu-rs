//! The lexer of a sequence diagram, as Pele's lexer.ts has it: the rules of Mermaid's
//! sequenceDiagram.jison, tried by hand in the order Jison tries them, in the states it reads
//! them in. Positions are bytes. Where Pele reads a code unit with charCodeAt, code reads the
//! same one, and a character outside ASCII is stepped over whole, as no rule tells the two
//! halves of a surrogate pair apart.

use super::tokens::T;
use crate::js::chars::{is_digit, is_space, is_word, trim, unit_at};

/// A token: its type, its text, and the byte of the text it starts at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Tok<'a> {
    pub(crate) kind: T,
    pub(crate) text: &'a str,
    pub(crate) start: usize,
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
    ID,
    ALIAS,
    LINE,
    CONFIG,
    acc_title,
    acc_descr,
    acc_descr_multiline,
}

// The character classes of Mermaid's lexer rules, Pele's C.
const ACTOR_START: u8 = 1;
const ACTOR_REST: u8 = 2;
const ID: u8 = 4;
const ID_WIDE: u8 = 8;
const LINE: u8 = 16;
const MENU: u8 = 32;

/// The classes of each ASCII character, Pele's CLASS. Other characters are tested by is_space.
const CLASS: [u8; 128] = {
    const fn holds(set: &[u8], c: u8) -> bool {
        let mut i = 0;
        while i < set.len() {
            if set[i] == c {
                return true;
            }
            i += 1;
        }
        false
    }
    let mut out = [0u8; 128];
    let mut c = 0u8;
    while c < 128 {
        let ws = c == 32 || (c >= 9 && c <= 13);
        let mut bits = 0;
        if !holds(b"/\\+()<->:\n,;", c) {
            bits |= ACTOR_START;
        }
        if !holds(b"+<->:\n,;", c) {
            bits |= ACTOR_REST;
        }
        if !holds(b"<>:\n,;@", c) {
            bits |= ID_WIDE | if ws { 0 } else { ID };
        }
        if !holds(b"#\n;", c) {
            bits |= LINE;
        }
        if !holds(b"/\\+()<>:\n,;-", c) && !ws {
            bits |= MENU;
        }
        out[c as usize] = bits;
        c += 1;
    }
    out
};

struct Lexer<'a> {
    src: &'a str,
    n: usize,
    toks: Vec<Tok<'a>>,
    stack: Vec<S>,
    state: S,
    p: usize,
    next_nl: Option<usize>,
}

impl<'a> Lexer<'a> {
    /// The code unit at byte q, -1 past the end, as charCodeAt reads it.
    fn code(&self, q: usize) -> i32 {
        unit_at(self.src, q).0
    }

    /// The byte after the character at q.
    fn after(&self, q: usize) -> usize {
        q + unit_at(self.src, q).1.max(1)
    }

    fn emit(&mut self, kind: T, s: usize, e: usize) {
        let text = &self.src[s..e];
        self.emit_text(kind, s, e, text);
    }

    fn emit_text(&mut self, kind: T, s: usize, e: usize, text: &'a str) {
        self.toks.push(Tok {
            kind,
            text,
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

    /// The first line feed at or after q, or the end, as Pele's eol finds it.
    fn eol(&mut self, q: usize) -> usize {
        if self.next_nl.is_none_or(|nl| nl < q) {
            self.next_nl = Some(self.src[q..].find('\n').map_or(self.n, |i| q + i));
        }
        self.next_nl.unwrap_or(self.n)
    }

    /// Whether the character at q is of a class. Past the end, where charCodeAt is NaN, it is
    /// of every class, as Pele's has finds.
    fn has(&self, q: usize, bit: u8) -> bool {
        let c = self.code(q);
        if (0..128).contains(&c) {
            return CLASS[c as usize] & bit != 0;
        }
        if bit == ID || bit == MENU {
            !is_space(c)
        } else {
            true
        }
    }

    /// Whether the lowercase literal word is at q, without regard to the case of the source,
    /// as Pele's ci matches it.
    fn ci(&self, q: usize, word: &str) -> bool {
        let b = self.src.as_bytes();
        word.bytes().enumerate().all(|(k, w)| {
            let Some(&c) = b.get(q + k) else {
                return false;
            };
            if w.is_ascii_lowercase() {
                c | 32 == w
            } else {
                c == w
            }
        })
    }

    fn word(&self, lit: &str) -> bool {
        self.ci(self.p, lit) && !is_word(self.code(self.p + lit.len()))
    }

    fn skip_ws(&self, mut q: usize) -> usize {
        while q < self.n && is_space(self.code(q)) {
            q = self.after(q);
        }
        q
    }

    fn skip_same_line(&mut self) -> bool {
        let mut q = self.p;
        while q < self.n {
            let c = self.code(q);
            if c == 10 || !is_space(c) {
                break;
            }
            q = self.after(q);
        }
        if q == self.p {
            return false;
        }
        self.p = q;
        true
    }

    fn line_run(&self, mut q: usize) -> usize {
        while q < self.n && self.has(q, LINE) {
            q = self.after(q);
        }
        q
    }

    fn begin(&mut self, kind: T, len: usize, next: S) {
        self.emit(kind, self.p, self.p + len);
        self.push(next);
    }

    fn keyword(&mut self, lit: &str, kind: T, next: Option<S>) -> bool {
        if !self.word(lit) {
            return false;
        }
        self.emit(kind, self.p, self.p + lit.len());
        if let Some(next) = next {
            self.push(next);
        }
        true
    }

    /// The actor menu keywords count only when spaces and the start of a name follow, as
    /// Pele's menu reads them.
    fn menu(&mut self, lit: &str, kind: T) -> bool {
        if !self.ci(self.p, lit) {
            return false;
        }
        let mut q = self.p + lit.len();
        let mut c = self.code(q);
        if c != 32 && c != 9 {
            return false;
        }
        while c == 32 || c == 9 {
            q += 1;
            c = self.code(q);
        }
        if q >= self.n || !self.has(q, MENU) {
            return false;
        }
        self.emit(kind, self.p, self.p + lit.len());
        true
    }

    /// Whether an arrow, or the `()` of a central connection, starts at q, as Pele's
    /// arrowAhead tells.
    fn arrow_ahead(&self, q: usize) -> bool {
        let c = self.code(q);
        if !(0..128).contains(&c) {
            return false;
        }
        let d = self.code(q + 1);
        if c == 45 {
            if matches!(d, 120 | 88 | 41 | 92 | 47 | 45) {
                return true;
            }
            let e = self.code(q + 2);
            return d == 124 && (e == 92 || e == 47);
        }
        if c == 47 || c == 92 {
            return (d == 124 || d == c) && self.code(q + 2) == 45;
        }
        c == 40 && d == 41
    }

    fn actor(&mut self) {
        let p = self.p;
        let mut q = self.after(p);
        while q < self.n && self.has(q, ACTOR_START) {
            q = self.after(q);
        }
        while q < self.n && !self.arrow_ahead(q) {
            let mut r = q;
            while self.code(r) == 45 {
                r += 1;
            }
            if r >= self.n || !self.has(r, ACTOR_REST) {
                break;
            }
            r = self.after(r);
            while r < self.n && self.has(r, ACTOR_REST) {
                r = self.after(r);
            }
            q = r;
        }
        self.emit_text(T::ACTOR, p, q, trim(&self.src[p..q]));
    }

    fn number(&mut self) -> bool {
        let p = self.p;
        let mut q = p;
        while is_digit(self.code(q)) {
            q += 1;
        }
        let whole = q > p;
        if self.code(q) == 46 && is_digit(self.code(q + 1)) {
            let r = if is_digit(self.code(q + 2)) {
                q + 3
            } else {
                q + 2
            };
            let after = self.code(r);
            if after == 32 || after == 10 {
                self.emit(T::NUM, p, r);
                return true;
            }
        }
        let after = self.code(q);
        if whole && (after == 32 || after == 10) {
            self.emit(T::NUM, p, q);
            return true;
        }
        false
    }

    fn dash(&mut self) {
        let p = self.p;
        let d = self.code(p + 1);
        let e = self.code(p + 2);
        if d == 62 {
            if e == 62 {
                self.emit(T::SOLID_ARROW, p, p + 3);
            } else {
                self.emit(T::SOLID_OPEN_ARROW, p, p + 2);
            }
        } else if d == 120 || d == 88 {
            self.emit(T::SOLID_CROSS, p, p + 2);
        } else if d == 41 {
            self.emit(T::SOLID_POINT, p, p + 2);
        } else if d == 124 && e == 92 {
            self.emit(T::SOLID_ARROW_TOP, p, p + 3);
        } else if d == 124 && e == 47 {
            self.emit(T::SOLID_ARROW_BOTTOM, p, p + 3);
        } else if d == 92 && e == 92 {
            self.emit(T::STICK_ARROW_TOP, p, p + 3);
        } else if d == 47 && e == 47 {
            self.emit(T::STICK_ARROW_BOTTOM, p, p + 3);
        } else if d == 45 {
            let f = self.code(p + 3);
            if e == 62 {
                if f == 62 {
                    self.emit(T::DOTTED_ARROW, p, p + 4);
                } else {
                    self.emit(T::DOTTED_OPEN_ARROW, p, p + 3);
                }
            } else if e == 120 || e == 88 {
                self.emit(T::DOTTED_CROSS, p, p + 3);
            } else if e == 41 {
                self.emit(T::DOTTED_POINT, p, p + 3);
            } else if e == 124 && f == 92 {
                self.emit(T::SOLID_ARROW_TOP_DOTTED, p, p + 4);
            } else if e == 124 && f == 47 {
                self.emit(T::SOLID_ARROW_BOTTOM_DOTTED, p, p + 4);
            } else if e == 92 && f == 92 {
                self.emit(T::STICK_ARROW_TOP_DOTTED, p, p + 4);
            } else if e == 47 && f == 47 {
                self.emit(T::STICK_ARROW_BOTTOM_DOTTED, p, p + 4);
            } else {
                self.emit(T::MINUS, p, p + 1);
            }
        } else {
            self.emit(T::MINUS, p, p + 1);
        }
    }

    fn reverse(&mut self, c: i32) -> bool {
        let p = self.p;
        let d = self.code(p + 1);
        if (d != 124 && d != c) || self.code(p + 2) != 45 {
            return false;
        }
        let dotted = self.code(p + 3) == 45;
        let top = c == 47;
        let kind = if d == 124 {
            match (dotted, top) {
                (true, true) => T::SOLID_ARROW_TOP_REVERSE_DOTTED,
                (true, false) => T::SOLID_ARROW_BOTTOM_REVERSE_DOTTED,
                (false, true) => T::SOLID_ARROW_TOP_REVERSE,
                (false, false) => T::SOLID_ARROW_BOTTOM_REVERSE,
            }
        } else {
            match (dotted, top) {
                (true, true) => T::STICK_ARROW_TOP_REVERSE_DOTTED,
                (true, false) => T::STICK_ARROW_BOTTOM_REVERSE_DOTTED,
                (false, true) => T::STICK_ARROW_TOP_REVERSE,
                (false, false) => T::STICK_ARROW_BOTTOM_REVERSE,
            }
        };
        self.emit(kind, p, p + if dotted { 4 } else { 3 });
        true
    }

    fn title(&mut self) -> bool {
        if !self.ci(self.p, "title") {
            return false;
        }
        let mut q = self.p + 5;
        let legacy = self.code(q) == 58;
        if legacy {
            q += 1;
        }
        let mut e = q;
        while e < self.n && self.code(e) != 10 && is_space(self.code(e)) {
            e = self.after(e);
        }
        if e == self.n || self.code(e) == 10 {
            self.p = e;
            return true;
        }
        let sep = self.code(q);
        let next = self.after(q);
        if !is_space(sep) || sep == 13 || !self.has(next, LINE) {
            return false;
        }
        let kind = if legacy { T::legacy_title } else { T::title };
        let end = self.line_run(next);
        self.emit(kind, self.p, end);
        true
    }

    fn acc(&mut self) -> bool {
        let descr = self.ci(self.p, "accdescr");
        if !descr && !self.ci(self.p, "acctitle") {
            return false;
        }
        let q = self.skip_ws(self.p + 8);
        let c = self.code(q);
        if c == 58 {
            let end = self.skip_ws(q + 1);
            let kind = if descr { T::acc_descr } else { T::acc_title };
            self.emit(kind, self.p, end);
            self.push(if descr { S::acc_descr } else { S::acc_title });
            return true;
        }
        if descr && c == 123 {
            self.p = self.skip_ws(q + 1);
            self.push(S::acc_descr_multiline);
            return true;
        }
        false
    }

    fn keywords(&mut self, c: i32) -> bool {
        match c | 32 {
            97 => {
                self.keyword("actor", T::participant_actor, Some(S::ID))
                    || self.keyword("alt", T::alt, Some(S::LINE))
                    || self.keyword("and", T::and, Some(S::LINE))
                    || self.keyword("activate", T::activate, Some(S::ID))
                    || self.acc()
                    || self.keyword("autonumber", T::autonumber, None)
            }
            98 => {
                self.keyword("box", T::r#box, Some(S::LINE))
                    || self.keyword("break", T::r#break, Some(S::LINE))
            }
            99 => {
                self.keyword("create", T::create, None)
                    || self.keyword("critical", T::critical, Some(S::LINE))
            }
            100 => {
                self.keyword("destroy", T::destroy, Some(S::ID))
                    || self.menu("details", T::details)
                    || self.keyword("deactivate", T::deactivate, Some(S::ID))
            }
            101 => {
                self.keyword("else", T::r#else, Some(S::LINE)) || self.keyword("end", T::end, None)
            }
            108 => {
                self.keyword("loop", T::r#loop, Some(S::LINE))
                    || self.keyword("left of", T::left_of, None)
                    || self.menu("links", T::links)
                    || self.menu("link", T::link)
            }
            110 => self.keyword("note", T::note, None),
            111 => {
                self.keyword("opt", T::opt, Some(S::LINE))
                    || self.keyword("option", T::option, Some(S::LINE))
                    || self.keyword("over", T::over, None)
                    || self.keyword("off", T::off, None)
            }
            112 => {
                self.keyword("participant", T::participant, Some(S::ID))
                    || self.keyword("par", T::par, Some(S::LINE))
                    || self.keyword("par_over", T::par_over, Some(S::LINE))
                    || self.menu("properties", T::properties)
            }
            114 => {
                self.keyword("rect", T::rect, Some(S::LINE))
                    || self.keyword("right of", T::right_of, None)
            }
            115 => self.keyword("sequencediagram", T::SD, None),
            116 => self.title(),
            _ => false,
        }
    }

    fn initial(&mut self) {
        let p = self.p;
        let c = self.code(p);
        if c == 10 {
            let mut q = p + 1;
            while self.code(q) == 10 {
                q += 1;
            }
            self.emit(T::NEWLINE, p, q);
            return;
        }
        if is_space(c) {
            self.p = self.skip_ws(self.after(p));
            return;
        }
        if c == 35 || (c == 37 && self.code(p + 1) != 123) {
            self.p = self.eol(p);
            return;
        }
        if (is_digit(c) || c == 46) && self.number() {
            return;
        }
        if (0..128).contains(&c) && (97..=122).contains(&(c | 32)) && self.keywords(c) {
            return;
        }
        let rest = &self.src[p..];
        if c == 44 {
            self.emit(T::COMMA, p, p + 1);
        } else if c == 59 {
            self.emit(T::NEWLINE, p, p + 1);
        } else if self.has(p, ACTOR_START) {
            self.actor();
        } else if c == 45 {
            self.dash();
        } else if c == 60 && rest.starts_with("<<->>") {
            self.emit(T::BIDIRECTIONAL_SOLID_ARROW, p, p + 5);
        } else if c == 60 && rest.starts_with("<<-->>") {
            self.emit(T::BIDIRECTIONAL_DOTTED_ARROW, p, p + 6);
        } else if (c == 47 || c == 92) && self.reverse(c) {
        } else if c == 58 {
            let end = self.line_run(p + 1);
            self.emit(T::TXT, p, end);
        } else if c == 43 {
            self.emit(T::PLUS, p, p + 1);
        } else if c == 40 && self.code(p + 1) == 41 {
            self.emit(T::CENTRAL, p, p + 2);
        } else {
            self.emit(T::INVALID, p, p + 1);
        }
    }

    /// A participant name, up to an alias, a config object, or the end of the statement, as
    /// Pele's id reads it.
    fn id(&mut self) -> bool {
        if self.skip_same_line() {
            return true;
        }
        let p = self.p;
        let c = self.code(p);
        if c == 35 {
            self.p = self.eol(p);
            return true;
        }
        if c == 64 && self.code(p + 1) == 123 {
            self.begin(T::CONFIG_START, 2, S::CONFIG);
            return true;
        }
        let mut a = p;
        while a < self.n && self.has(a, ID) {
            a = self.after(a);
        }
        if a > p {
            let w = self.skip_ws(a);
            if self.code(w) == 64 && self.code(w + 1) == 123 {
                self.emit(T::ACTOR, p, a);
                return true;
            }
            if w > a && self.ci(w, "as") && is_space(self.code(w + 2)) {
                self.emit(T::ACTOR, p, a);
                self.push(S::ALIAS);
                return true;
            }
        }
        let mut b = a;
        while b < self.n && self.has(b, ID_WIDE) {
            b = self.after(b);
        }
        if b > p {
            let end = self.code(b);
            // Before any other delimiter the name must be followed by a comment, so it ends at
            // the last one. A # is never a byte of a character outside ASCII, so the bytes
            // can be stepped back over one at a time.
            let mut cut = b;
            if b < self.n && end != 10 && end != 59 {
                cut -= 1;
                while cut > p && self.src.as_bytes()[cut] != b'#' {
                    cut -= 1;
                }
            }
            if cut > p {
                self.emit_text(T::ACTOR, p, cut, trim(&self.src[p..cut]));
                self.pop();
                return true;
            }
        }
        if self.code(b) == 60 {
            let e = self.eol(p);
            self.emit(T::INVALID, p, e);
            self.pop();
            return true;
        }
        if c != 10 {
            let e = self.eol(p);
            self.emit_text(T::INVALID, p, e, trim(&self.src[p..e]));
            self.pop();
            return true;
        }
        false
    }

    fn step(&mut self) -> bool {
        match self.state {
            S::INITIAL => {
                self.initial();
                true
            }
            S::ID => self.id(),
            S::ALIAS => {
                if self.skip_same_line() {
                    return true;
                }
                if self.code(self.p) == 35 {
                    self.p = self.eol(self.p);
                    return true;
                }
                self.pop();
                self.pop();
                if self.word("as") {
                    self.begin(T::AS, 2, S::LINE);
                } else {
                    self.emit(T::NEWLINE, self.p, self.p);
                }
                true
            }
            S::LINE => {
                if self.skip_same_line() {
                    return true;
                }
                if self.code(self.p) == 35 {
                    self.p = self.eol(self.p);
                    return true;
                }
                let end = self.line_run(self.p);
                self.emit(T::restOfLine, self.p, end);
                self.pop();
                true
            }
            S::CONFIG => {
                let p = self.p;
                if self.code(p) != 125 {
                    let q = self.src[p..].find('}').map_or(self.n, |i| p + i);
                    self.emit(T::CONFIG_CONTENT, p, q);
                } else {
                    let w = self.skip_ws(p + 1);
                    let alias = w > p + 1 && self.ci(w, "as") && is_space(self.code(w + 2));
                    self.emit(T::CONFIG_END, p, p + 1);
                    self.pop();
                    if alias {
                        self.push(S::ALIAS);
                    } else {
                        self.pop();
                    }
                }
                true
            }
            S::acc_title | S::acc_descr => {
                let kind = if self.state == S::acc_title {
                    T::acc_title_value
                } else {
                    T::acc_descr_value
                };
                let e = self.eol(self.p);
                self.emit(kind, self.p, e);
                self.pop();
                true
            }
            S::acc_descr_multiline => {
                let p = self.p;
                if self.code(p) == 125 {
                    self.p += 1;
                    self.pop();
                } else {
                    let q = self.src[p..].find('}').map_or(self.n, |i| p + i);
                    self.emit(T::acc_descr_multiline_value, p, q);
                }
                true
            }
        }
    }
}

/// Reads src into tokens, as Pele's tokenize does. The last is END, or ERROR where no rule
/// matches the text.
pub(crate) fn tokenize(src: &str) -> Vec<Tok<'_>> {
    let n = src.len();
    let mut l = Lexer {
        src,
        n,
        toks: Vec::new(),
        stack: Vec::new(),
        state: S::INITIAL,
        p: 0,
        next_nl: None,
    };
    while l.p < n {
        if !l.step() {
            l.toks.push(Tok {
                kind: T::ERROR,
                text: "",
                start: l.p,
            });
            return l.toks;
        }
    }
    // Mermaid's lexer tries its rules once more on the empty remainder before reporting the
    // end.
    match l.state {
        S::INITIAL | S::ALIAS => l.emit(T::NEWLINE, n, n),
        S::LINE => l.emit(T::restOfLine, n, n),
        S::acc_title => l.emit(T::acc_title_value, n, n),
        S::acc_descr => l.emit(T::acc_descr_value, n, n),
        S::acc_descr_multiline => l.emit(T::acc_descr_multiline_value, n, n),
        S::ID | S::CONFIG => {}
    }
    l.toks.push(Tok {
        kind: T::END,
        text: "",
        start: n,
    });
    l.toks
}
