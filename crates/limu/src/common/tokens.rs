//! The tokenizer of the diagram types Mermaid defines with Langium grammars, as Pele's
//! diagrams/common/tokens.ts has it. Token types are tried in order and the first that matches
//! wins, as in Chevrotain, but for the longer alternatives a type names. The regular
//! expressions of Pele's patterns are written out by hand, with the backtracking they would
//! do. Positions are bytes.

use crate::detect::DiagramType;
use crate::errors::{Error, syntax_error};
use crate::js::chars::{is_line_end, is_space, space_run, trim, unit, unit_at, unit_before};

/// The kind of the token that ends the text, Pele's -1.
pub(crate) const END: usize = usize::MAX;

/// A matcher that a scanner makes, as Pele's Scanner is: the end of the match at a byte of
/// the text, given the kind of the last token that is not hidden, or none.
pub(crate) type Scan = Box<dyn FnMut(&str, usize, Option<usize>) -> Option<usize>>;

/// How a token type matches, Pele's pattern, match or scanner.
#[derive(Clone, Copy)]
pub(crate) enum Pattern {
    /// A literal string.
    Literal(&'static str),
    /// A word, as Pele's keyword makes it, ended only by white space, a comment or the end of
    /// the text.
    Keyword(&'static str),
    /// A regular expression, written out: the end of its match at a byte, or none.
    Match(fn(&str, usize) -> Option<usize>),
    /// Pele's match, for a type whose match depends on the token before it: the kind of the
    /// last token that is not hidden, or none at the start.
    After(fn(&str, usize, Option<usize>) -> Option<usize>),
    /// Pele's scanner, for a pattern that could rescan the text. It makes a matcher once for
    /// each text tokenized, so the matcher may remember what it has scanned.
    Scanner(fn() -> Scan),
}

/// A type of token, Pele's TokenType.
#[derive(Clone, Copy)]
pub(crate) struct TokenType {
    pub(crate) name: &'static str,
    pub(crate) pattern: Pattern,
    pub(crate) hidden: bool,
    /// Whether the start of a construct that needs a closer is at a byte. Where it is but the
    /// whole pattern does not match, no closer is in the rest of the text, and the type is not
    /// tried again.
    pub(crate) opener: Option<fn(&str, usize) -> bool>,
    /// Every ASCII character a match of a regular expression can start with, where that is
    /// known. The type is then tried only at those characters and at characters outside
    /// ASCII.
    pub(crate) first: Option<&'static str>,
    /// Chevrotain's LONGER_ALT, by index in the table. Where this type matches, these are
    /// tried at the same place, and the first that matches more text becomes the token.
    pub(crate) longer: &'static [usize],
}

/// A type of token of a name and a pattern, with nothing else set.
pub(crate) const fn token(name: &'static str, pattern: Pattern) -> TokenType {
    TokenType {
        name,
        pattern,
        hidden: false,
        opener: None,
        first: None,
        longer: &[],
    }
}

/// Pele's keyword: the word, ended only by white space, a comment or the end of the text, as
/// (?:(?=%%)|(?!\S)) ends it.
pub(crate) const fn keyword(word: &'static str) -> TokenType {
    token(word, Pattern::Keyword(word))
}

/// The tokens of a text, Pele's Tokens: the kind of each, by index in the table, and the
/// bytes it starts and ends at, with a last of kind END at the end of the text.
pub(crate) struct Tokens<'a> {
    pub(crate) kinds: Vec<usize>,
    pub(crate) starts: Vec<usize>,
    pub(crate) ends: Vec<usize>,
    pub(crate) src: &'a str,
}

/// A table of token types made ready to tokenize with, as Pele's prepare makes it: the types,
/// and those that can start with each ASCII character, in matching order.
pub(crate) struct Table {
    pub(crate) types: &'static [TokenType],
    starting: Vec<Vec<usize>>,
    every: Vec<usize>,
}

impl Table {
    /// Makes a table of token types ready to tokenize with, as Pele's prepare does.
    pub(crate) fn new(types: &'static [TokenType]) -> Table {
        let every: Vec<usize> = (0..types.len()).collect();
        let starting = (0..128u8)
            .map(|c| {
                every
                    .iter()
                    .copied()
                    .filter(|&k| {
                        let t = &types[k];
                        match t.pattern {
                            Pattern::Literal(s) | Pattern::Keyword(s) => s.as_bytes()[0] == c,
                            _ => t.first.is_none_or(|f| f.as_bytes().contains(&c)),
                        }
                    })
                    .collect()
            })
            .collect();
        Table {
            types,
            starting,
            every,
        }
    }
}

/// Splits src into tokens with the types of a table, as Pele's tokenize does. Text no type
/// matches is a lexical error of the diagram type.
pub(crate) fn tokenize<'a>(
    src: &'a str,
    table: &Table,
    diagram: DiagramType,
) -> Result<Tokens<'a>, Error> {
    let types = table.types;
    let mut out = Tokens {
        kinds: Vec::new(),
        starts: Vec::new(),
        ends: Vec::new(),
        src,
    };
    let mut dead = vec![false; types.len()];
    let mut scanners: Vec<Option<Scan>> = (0..types.len()).map(|_| None).collect();
    let mut matches = |k: usize, p: usize, kinds: &[usize]| -> Option<usize> {
        if dead[k] {
            return None;
        }
        let t = &types[k];
        let previous = kinds.last().copied();
        let end = match t.pattern {
            Pattern::After(m) => return m(src, p, previous),
            Pattern::Scanner(make) => {
                return scanners[k].get_or_insert_with(make)(src, p, previous);
            }
            Pattern::Literal(s) => return src[p..].starts_with(s).then_some(p + s.len()),
            Pattern::Keyword(word) => keyword_at(src, p, word),
            Pattern::Match(m) => m(src, p),
        };
        if end.is_none() && t.opener.is_some_and(|o| o(src, p)) {
            dead[k] = true;
        }
        end
    };
    let n = src.len();
    let mut p = 0;
    'scan: while p < n {
        let c = src.as_bytes()[p];
        let candidates = if c < 128 {
            &table.starting[usize::from(c)]
        } else {
            &table.every
        };
        for &k in candidates {
            let Some(mut end) = matches(k, p, &out.kinds).filter(|&e| e > p) else {
                continue;
            };
            let mut kind = k;
            for &alt in types[k].longer {
                if let Some(alt_end) = matches(alt, p, &out.kinds)
                    && alt_end > end
                {
                    (kind, end) = (alt, alt_end);
                    break;
                }
            }
            if !types[kind].hidden {
                out.kinds.push(kind);
                out.starts.push(p);
                out.ends.push(end);
            }
            p = end;
            continue 'scan;
        }
        return Err(syntax_error(diagram, src, p, "", true));
    }
    out.kinds.push(END);
    out.starts.push(n);
    out.ends.push(n);
    Ok(out)
}

/// The end of the run of tabs and spaces from byte i, [\t ]*.
fn blanks(s: &str, mut i: usize) -> usize {
    let b = s.as_bytes();
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    i
}

/// Whether JavaScript's \s matches at byte i of s.
fn space_at(s: &str, i: usize) -> bool {
    is_space(unit(s, i))
}

/// Matches [^\n\r]*?(?=%%)|[^\n\r]* from byte i: the text up to the first %% on the line, or
/// to the end of the line.
fn line_rest(s: &str, i: usize) -> usize {
    let b = s.as_bytes();
    (i..b.len())
        .find(|&k| b[k] == b'\n' || b[k] == b'\r' || b[k..].starts_with(b"%%"))
        .unwrap_or(b.len())
}

/// Matches \r?\n at byte i.
pub(crate) fn line_break(s: &str, i: usize) -> Option<usize> {
    let b = &s.as_bytes()[i..];
    if b.starts_with(b"\r\n") {
        Some(i + 2)
    } else if b.starts_with(b"\n") {
        Some(i + 1)
    } else {
        None
    }
}

/// Matches (?:\r?\n|(?!\S)) at byte i: a line break, or nothing before white space or the end
/// of the text.
pub(crate) fn closed(s: &str, i: usize) -> Option<usize> {
    line_break(s, i).or((i == s.len() || space_at(s, i)).then_some(i))
}

/// Matches Pele's keyword(word) at byte at: the word, then a comment, white space or the end
/// of the text.
fn keyword_at(s: &str, at: usize, word: &str) -> Option<usize> {
    if !s[at..].starts_with(word) {
        return None;
    }
    let end = at + word.len();
    (s[end..].starts_with("%%") || end == s.len() || space_at(s, end)).then_some(end)
}

/// Pele's ACC_DESCR, /[\t ]*accDescr(?:[\t ]*:([^\n\r]*?(?=%%)|[^\n\r]*)|\s*{([^}]*)})/y.
pub(crate) const ACC_DESCR: TokenType = TokenType {
    first: Some("\t a"),
    ..token("ACC_DESCR", Pattern::Match(acc_descr))
};

/// The end of ACC_DESCR's match at byte at, for a grammar that reads it under its own rules.
pub(crate) fn acc_descr(s: &str, at: usize) -> Option<usize> {
    let mut i = blanks(s, at);
    if !s[i..].starts_with("accDescr") {
        return None;
    }
    i += "accDescr".len();
    let j = blanks(s, i);
    if s.as_bytes().get(j) == Some(&b':') {
        return Some(line_rest(s, j + 1));
    }
    i = space_run(s, i);
    if s.as_bytes().get(i) != Some(&b'{') {
        return None;
    }
    s[i + 1..].find('}').map(|end| i + 1 + end + 1)
}

/// Pele's accDescr(name), in common/accDescr.ts: ACC_DESCR under the name given, for grammars
/// where lexing goes on after an `accDescr {` that never closes, because accDescr is also a
/// name there. It matches what ACC_DESCR does, but once the braced form finds no closing
/// brace there is none further on, so later braced descriptions fail without scanning the
/// rest of the text again.
pub(crate) const fn acc_descr_named(name: &'static str) -> TokenType {
    TokenType {
        name,
        pattern: Pattern::Scanner(acc_descr_scanner),
        ..ACC_DESCR
    }
}

/// The scanner of Pele's accDescr, which matches
/// /[\t ]*accDescr(?:[\t ]*:(?:[^\n\r]*?(?=%%)|[^\n\r]*)|\s*({))/y and then, for the braced
/// form, looks for the closing brace unless it has found none before.
fn acc_descr_scanner() -> Scan {
    let mut unclosed = false;
    Box::new(move |s, at, _| {
        let mut i = blanks(s, at);
        if !s[i..].starts_with("accDescr") {
            return None;
        }
        i += "accDescr".len();
        let j = blanks(s, i);
        if s.as_bytes().get(j) == Some(&b':') {
            return Some(line_rest(s, j + 1));
        }
        i = space_run(s, i);
        if s.as_bytes().get(i) != Some(&b'{') || unclosed {
            return None;
        }
        let close = s[i + 1..].find('}');
        unclosed = close.is_none();
        close.map(|end| i + 1 + end + 1)
    })
}

/// Pele's ACC_TITLE, /[\t ]*accTitle[\t ]*:(?:[^\n\r]*?(?=%%)|[^\n\r]*)/y.
pub(crate) const ACC_TITLE: TokenType = TokenType {
    first: Some("\t a"),
    ..token("ACC_TITLE", Pattern::Match(acc_title))
};

fn acc_title(s: &str, at: usize) -> Option<usize> {
    let mut i = blanks(s, at);
    if !s[i..].starts_with("accTitle") {
        return None;
    }
    i = blanks(s, i + "accTitle".len());
    if s.as_bytes().get(i) != Some(&b':') {
        return None;
    }
    Some(line_rest(s, i + 1))
}

/// Pele's TITLE, /[\t ]*title(?:[\t ][^\n\r]*?(?=%%)|[\t ][^\n\r]*|)/y. The word alone matches
/// too, so `title1` starts with a title.
pub(crate) const TITLE: TokenType = TokenType {
    first: Some("\t t"),
    ..token("TITLE", Pattern::Match(|s, at| match_title(s, at, None)))
};

/// TITLE at byte at, or Pele's titleBefore(rest) where rest is given: TITLE with its last
/// alternative made (?!rest), so that the word alone matches only where rest does not match
/// after it, and `title` does not split a longer name. A diagram that needs titleBefore
/// writes its TITLE with this.
pub(crate) fn match_title(
    s: &str,
    at: usize,
    rest: Option<fn(&str, usize) -> bool>,
) -> Option<usize> {
    let mut i = blanks(s, at);
    if !s[i..].starts_with("title") {
        return None;
    }
    i += "title".len();
    if matches!(s.as_bytes().get(i), Some(b' ' | b'\t')) {
        return Some(line_rest(s, i + 1));
    }
    if rest.is_some_and(|r| r(s, i)) {
        return None;
    }
    Some(i)
}

/// Pele's STRING, /"([^"\\]|\\.)*"|'([^'\\]|\\.)*'/y. Each character has one way to be taken,
/// so the first quote that ends it is its end. A backslash before a line terminator, which .
/// does not take, ends no string.
pub(crate) const STRING: TokenType = TokenType {
    first: Some("\"'"),
    ..token("STRING", Pattern::Match(string))
};

fn string(s: &str, at: usize) -> Option<usize> {
    let b = s.as_bytes();
    let q = b[at];
    if q != b'"' && q != b'\'' {
        return None;
    }
    let mut i = at + 1;
    while i < b.len() {
        if b[i] == q {
            return Some(i + 1);
        }
        if b[i] == b'\\' {
            let (c, size) = unit_at(s, i + 1);
            if c < 0 || is_line_end(c) {
                return None;
            }
            i += 1 + size;
        } else {
            i += 1;
        }
    }
    None
}

/// The INT that Pele's packet, git and Wardley diagrams each define alike,
/// /0|[1-9][0-9]*(?!\.)/y: a lone 0, or digits not followed by a dot, which give back the last
/// digit rather than fail where they are.
pub(crate) fn int(s: &str, at: usize) -> Option<usize> {
    let b = s.as_bytes();
    match b[at] {
        b'0' => return Some(at + 1),
        b'1'..=b'9' => {}
        _ => return None,
    }
    let mut end = at + 1;
    while end < b.len() && b[end].is_ascii_digit() {
        end += 1;
    }
    if b.get(end) == Some(&b'.') {
        return (end - 1 > at).then_some(end - 1);
    }
    Some(end)
}

/// Pele's NEWLINE, /\r?\n/y.
pub(crate) const NEWLINE: TokenType = TokenType {
    first: Some("\r\n"),
    ..token("NEWLINE", Pattern::Match(line_break))
};

/// Pele's WHITESPACE, /[\t ]+/y, which is hidden.
pub(crate) const WHITESPACE: TokenType = TokenType {
    first: Some("\t "),
    hidden: true,
    ..token(
        "WHITESPACE",
        Pattern::Match(|s, at| Some(blanks(s, at)).filter(|&end| end > at)),
    )
};

/// Matches /---[\t ]*\r?\n/y at byte at.
fn yaml_opener(s: &str, at: usize) -> Option<usize> {
    if !s[at..].starts_with("---") {
        return None;
    }
    line_break(s, blanks(s, at + 3))
}

/// Pele's YAML, /---[\t ]*\r?\n(?:[\S\s]*?\r?\n)?---(?:\r?\n|(?!\S))/y, front matter, which is
/// hidden. The group is tried first, so the closing dashes are the first after a line break,
/// past any that follow the opener at once.
pub(crate) const YAML: TokenType = TokenType {
    first: Some("-"),
    hidden: true,
    opener: Some(|s, at| yaml_opener(s, at).is_some()),
    ..token("YAML", Pattern::Match(yaml))
};

fn yaml(s: &str, at: usize) -> Option<usize> {
    let q = yaml_opener(s, at)?;
    for k in q..s.len() {
        if let Some(j) = line_break(s, k)
            && s[j..].starts_with("---")
            && let Some(end) = closed(s, j + 3)
        {
            return Some(end);
        }
    }
    if s[q..].starts_with("---") {
        return closed(s, q + 3);
    }
    None
}

/// Matches /[\t ]*%%{/y at byte at.
fn directive_opener(s: &str, at: usize) -> Option<usize> {
    let i = blanks(s, at);
    s[i..].starts_with("%%{").then_some(i + 3)
}

/// Pele's DIRECTIVE, /[\t ]*%%{[\S\s]*?}%%(?:\r?\n|(?!\S))/y, which is hidden.
pub(crate) const DIRECTIVE: TokenType = TokenType {
    first: Some("\t %"),
    hidden: true,
    opener: Some(|s, at| directive_opener(s, at).is_some()),
    ..token("DIRECTIVE", Pattern::Match(directive))
};

fn directive(s: &str, at: usize) -> Option<usize> {
    let q = directive_opener(s, at)?;
    (q..s.len())
        .filter(|&k| s.as_bytes()[k..].starts_with(b"}%%"))
        .find_map(|k| closed(s, k + 3))
}

/// Pele's SINGLE_LINE_COMMENT, /[\t ]*%%[^\n\r]*/y, which is hidden.
pub(crate) const SINGLE_LINE_COMMENT: TokenType = TokenType {
    first: Some("\t %"),
    hidden: true,
    ..token("SINGLE_LINE_COMMENT", Pattern::Match(comment))
};

fn comment(s: &str, at: usize) -> Option<usize> {
    let i = blanks(s, at);
    if !s[i..].starts_with("%%") {
        return None;
    }
    let b = s.as_bytes();
    let mut end = i + 2;
    while end < b.len() && b[end] != b'\n' && b[end] != b'\r' {
        end += 1;
    }
    Some(end)
}

/// Pele's single, text.trim().replace(/[\t ]{2,}/gm, ' '): the text trimmed, with each run of
/// two or more tabs and spaces made one space.
fn single(text: &str) -> String {
    squeeze(trim(text), |b| b == b' ' || b == b'\t', " ")
}

/// The title of a TITLE token, as Pele's titleValue reads it with /title([\t ][^\n\r]*|)/:
/// what follows the word, which the token holds to one line.
pub(crate) fn title_value(text: &str) -> String {
    let i = text.find("title").expect("a title");
    single(&text[i + "title".len()..])
}

/// The title of an ACC_TITLE token, as Pele's accTitleValue reads it with
/// /accTitle[\t ]*:([^\n\r]*)/.
pub(crate) fn acc_title_value(text: &str) -> String {
    let i = text.find("accTitle").expect("an accessible title");
    let i = blanks(text, i + "accTitle".len());
    single(&text[i + 1..])
}

/// The description of an ACC_DESCR token, as Pele's accDescrValue reads it with
/// /accDescr(?:[\t ]*:([^\n\r]*)|\s*{([^}]*)})/: a line made single, or a block with its lines'
/// leading and trailing white space taken off, runs of tabs and spaces made one space, and
/// runs of line breaks one line feed.
pub(crate) fn acc_descr_value(text: &str) -> String {
    let i = text.find("accDescr").expect("an accessible description") + "accDescr".len();
    let j = blanks(text, i);
    if text.as_bytes().get(j) == Some(&b':') {
        return single(&text[j + 1..]);
    }
    let open = i + text[i..].find('{').expect("a brace");
    let body = &text[open + 1..text.len() - 1];
    let body = drop_trailing(&drop_leading(body));
    let body = squeeze(&body, |b| b == b' ' || b == b'\t', " ");
    squeeze(&body, |b| b == b'\n' || b == b'\r', "\n")
}

/// Whether ^ matches at byte i of s in a multiline regular expression: at its start, or after
/// a line terminator.
fn line_start(s: &str, i: usize) -> bool {
    i == 0 || is_line_end(unit_before(s, i))
}

/// s.replace(/^\s*/gm, ""): the white space at the start of each line, with the line breaks in
/// it, taken off.
fn drop_leading(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if line_start(s, i) {
            let j = space_run(s, i);
            if j > i {
                i = j;
                continue;
            }
        }
        let c = s[i..].chars().next().expect("a character");
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// s.replace(/\s+$/gm, ""): a run of white space is taken off up to the last line terminator
/// in it after its first character, or whole at the end of the text.
fn drop_trailing(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let j = space_run(s, i);
        if j == i {
            let c = s[i..].chars().next().expect("a character");
            out.push(c);
            i += c.len_utf8();
            continue;
        }
        if j == s.len() {
            break;
        }
        let mut last = None;
        let mut k = i + unit_at(s, i).1;
        while k < j {
            let (c, size) = unit_at(s, k);
            if is_line_end(c) {
                last = Some(k);
            }
            k += size;
        }
        if let Some(last) = last {
            i = last;
        }
        out.push_str(&s[i..j]);
        i = j;
    }
    out
}

/// Replaces each run of two or more bytes in a set with with.
fn squeeze(s: &str, within: impl Fn(u8) -> bool, with: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        let mut j = i;
        while j < b.len() && within(b[j]) {
            j += 1;
        }
        if j - i >= 2 {
            out.push_str(with);
            i = j;
        } else if j > i {
            out.push_str(&s[i..j]);
            i = j;
        } else {
            let c = s[i..].chars().next().expect("a character");
            out.push(c);
            i += c.len_utf8();
        }
    }
    out
}

/// The text of a STRING token, as Pele's stringValue reads it: the quotes taken off, and each
/// backslash with the character after it made that character, or the control character \b,
/// \f, \n, \r, \t, \v or \0 names.
pub(crate) fn string_value(text: &str) -> String {
    let inner = &text[1..text.len() - 1];
    if !text.contains('\\') {
        return inner.to_string();
    }
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        // A backslash last of all escapes the closing quote, as Pele reads it.
        let next = chars
            .next()
            .unwrap_or(text.as_bytes()[text.len() - 1] as char);
        out.push(match next {
            'b' => '\u{8}',
            'f' => '\u{c}',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'v' => '\u{b}',
            '0' => '\0',
            c => c,
        });
    }
    out
}

/// Reads tokens for a hand-written parser, as Pele's Reader does, with Chevrotain's messages
/// on failure.
pub(crate) struct Reader<'a> {
    pub(crate) i: usize,
    pub(crate) tokens: Tokens<'a>,
    pub(crate) types: &'static [TokenType],
    pub(crate) diagram: DiagramType,
}

impl<'a> Reader<'a> {
    /// A reader of tokens of the types of a table, at the first.
    pub(crate) fn new(tokens: Tokens<'a>, table: &Table, diagram: DiagramType) -> Reader<'a> {
        Reader {
            i: 0,
            tokens,
            types: table.types,
            diagram,
        }
    }

    /// The kind of the token being read.
    pub(crate) fn kind(&self) -> usize {
        self.tokens.kinds[self.i]
    }

    /// The text of token i.
    pub(crate) fn text(&self, i: usize) -> &'a str {
        let t = &self.tokens;
        &t.src[t.starts[i]..t.ends[i]]
    }

    /// Reads the token and gives its text.
    pub(crate) fn take(&mut self) -> &'a str {
        self.i += 1;
        self.text(self.i - 1)
    }

    /// Reads the token if it is of a kind, and says whether it was.
    pub(crate) fn accept(&mut self, kind: usize) -> bool {
        if self.kind() != kind {
            return false;
        }
        self.i += 1;
        true
    }

    /// Reads the token, which must be of a kind, and gives its text.
    pub(crate) fn expect(&mut self, kind: usize) -> Result<&'a str, Error> {
        if self.kind() != kind {
            let expected = format!("token of type '{}'", self.types[kind].name);
            return Err(self.fail(&expected));
        }
        Ok(self.take())
    }

    /// The syntax error at the token, of what was expected and what was found, as Pele's
    /// fail makes it. At the end of the text, the error is put after the last token, before
    /// the line breaks and hidden comments that end the text.
    pub(crate) fn fail(&self, expected: &str) -> Error {
        let found = if self.kind() == END {
            "end of input".to_string()
        } else {
            format!("`{}`", self.text(self.i))
        };
        let t = &self.tokens;
        let mut at = t.starts[self.i];
        if self.kind() == END {
            let mut k = self.i;
            while k > 0 && trim(self.text(k - 1)).is_empty() {
                k -= 1;
            }
            if k > 0 {
                at = t.ends[k - 1];
            }
        }
        let detail = format!("Expecting {expected} but found {found}.");
        syntax_error(self.diagram, t.src, at, &detail, false)
    }
}
