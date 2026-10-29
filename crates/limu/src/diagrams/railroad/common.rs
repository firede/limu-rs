//! What the four railroad grammars share, as Pele's common.ts has it: the title and
//! accessibility tokens, which unlike the ones of Mermaid's common grammar take no blanks
//! before them, the hidden tokens, the statements before the first rule, string values, and
//! the walk from a syntax tree to the rule model.

use super::db::{RailroadNode, RailroadRule};
use crate::common::tokens::{
    ACC_DESCR as COMMON_ACC_DESCR, ACC_TITLE as COMMON_ACC_TITLE, DIRECTIVE, END, Pattern, Reader,
    SINGLE_LINE_COMMENT, STRING, Table, TokenType, YAML, acc_descr_named, acc_descr_value,
    acc_title_value, keyword, match_title, title_value, token, tokenize,
};
use crate::detect::DiagramType;
use crate::errors::Error;
use crate::js::chars::{is_word, unit};

/// The letters a name starts with, Pele's LETTERS.
pub(crate) const LETTERS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Pele's title(rest), /title(?:[\t ][^\n\r]*?(?=%%)|[\t ][^\n\r]*|(?!rest))/y, of the
/// matcher a grammar writes with match_title and its rest, which matches a name's suffix so
/// that `title` does not split a longer name.
pub(crate) const fn title(matcher: fn(&str, usize) -> Option<usize>) -> TokenType {
    TokenType {
        first: Some("t"),
        ..token("TITLE", Pattern::Match(matcher))
    }
}

/// TITLE where a name goes on with [\w-], as three of the grammars write it.
pub(crate) fn word_title(s: &str, at: usize) -> Option<usize> {
    match_title(s, at, Some(|s, i| is_word(unit(s, i)) || unit(s, i) == 45))
}

/// Pele's ACC_TITLE, /accTitle[\t ]*:(?:[^\n\r]*?(?=%%)|[^\n\r]*)/y. The common one tried
/// only at an a takes no blanks before it.
pub(crate) const ACC_TITLE: TokenType = TokenType {
    first: Some("a"),
    ..COMMON_ACC_TITLE
};

/// Pele's ACC_DESCR, /accDescr(?:[\t ]*:([^\n\r]*?(?=%%)|[^\n\r]*)|\s*{([^}]*)})/y, with its
/// scanner. The braced form scans to the next closing brace, and once none is found, none
/// exists further on either, so later braced descriptions fail without scanning again.
pub(crate) const ACC_DESCR: TokenType = TokenType {
    first: Some("a"),
    ..acc_descr_named(COMMON_ACC_DESCR.name)
};

/// Pele's literal: a token of a string, with the longer alternatives given.
pub(crate) const fn literal(name: &'static str, longer: &'static [usize]) -> TokenType {
    TokenType {
        longer,
        ..token(name, Pattern::Literal(name))
    }
}

/// The diagram's keyword, Pele's opening. A name that starts with it and goes on is a name,
/// the type at index name.
pub(crate) const fn opening(word: &'static str, name: &'static [usize]) -> TokenType {
    TokenType {
        longer: name,
        ..keyword(word)
    }
}

/// A name, Pele's identifier, /[A-Z_a-z][\w-]*/y.
pub(crate) const fn identifier(name: &'static str) -> TokenType {
    TokenType {
        first: Some("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_"),
        ..token(name, Pattern::Match(identifier_end))
    }
}

fn identifier_end(s: &str, at: usize) -> Option<usize> {
    let b = s.as_bytes();
    if !(b[at].is_ascii_alphabetic() || b[at] == b'_') {
        return None;
    }
    let mut i = at + 1;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'-') {
        i += 1;
    }
    Some(i)
}

/// A quoted string, Pele's quoted, /"([^"\\]|\\.)*"|'([^'\\]|\\.)*'/y, as STRING matches it.
pub(crate) const fn quoted(name: &'static str) -> TokenType {
    TokenType { name, ..STRING }
}

// The hidden tokens of Pele's hiddenTokens, in the order every railroad grammar lists them:
// white space, front matter, a directive and a `%%` comment.

/// White space, /[\t \r\n]+/y, which is hidden.
pub(crate) const fn whitespace(name: &'static str) -> TokenType {
    TokenType {
        first: Some("\t \r\n"),
        hidden: true,
        ..token(name, Pattern::Match(whitespace_end))
    }
}

fn whitespace_end(s: &str, at: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = at;
    while i < b.len() && matches!(b[i], b'\t' | b' ' | b'\r' | b'\n') {
        i += 1;
    }
    (i > at).then_some(i)
}

/// Front matter, as YAML matches it, which is hidden.
pub(crate) const fn yaml(name: &'static str) -> TokenType {
    TokenType { name, ..YAML }
}

/// A directive, as DIRECTIVE matches it, which is hidden.
pub(crate) const fn directive(name: &'static str) -> TokenType {
    TokenType { name, ..DIRECTIVE }
}

/// A `%%` comment, as SINGLE_LINE_COMMENT matches it, which is hidden.
pub(crate) const fn comment(name: &'static str) -> TokenType {
    TokenType {
        name,
        ..SINGLE_LINE_COMMENT
    }
}

/// A comment between open and close, as /\/\*[\s\S]*?\*\//y matches one between /* and */,
/// which is hidden.
pub(crate) const fn between(
    name: &'static str,
    first: &'static str,
    matcher: fn(&str, usize) -> Option<usize>,
    opener: fn(&str, usize) -> bool,
) -> TokenType {
    TokenType {
        first: Some(first),
        hidden: true,
        opener: Some(opener),
        ..token(name, Pattern::Match(matcher))
    }
}

/// The end of the text from at that opens with open and closes with close, the first close
/// after the opening.
pub(crate) fn enclosed(s: &str, at: usize, open: &str, close: &str) -> Option<usize> {
    if !s[at..].starts_with(open) {
        return None;
    }
    let from = at + open.len();
    s[from..].find(close).map(|k| from + k + close.len())
}

/// The text of a quoted string, as Mermaid's railroad value converters read it and Pele's
/// unescape does: the quotes taken off, and a backslash with the character after it made
/// that character, or the line feed, carriage return or tab \n, \r or \t name. A backslash
/// just before the closing quote stands for itself.
pub(crate) fn unescape(text: &str) -> String {
    let inner = inner(text);
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
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some(c) => out.push(c),
            None => out.push('\\'),
        }
    }
    out
}

/// text.slice(1, -1): the text without its first character and its last, which are quotes.
pub(crate) fn inner(text: &str) -> &str {
    if text.len() < 2 {
        return "";
    }
    &text[1..text.len() - 1]
}

/// What a text says before its first rule, Pele's Parsed: its title and its accessible title
/// and description, where it gives them.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Parsed {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
}

/// A rule of a syntax tree: its name, and the part of the tree's parts that defines it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Rule {
    pub(crate) name: String,
    pub(crate) definition: usize,
}

/// A syntax tree of one notation, Pele's Grammar: what the text says before its first rule,
/// and its rules, of parts of type P kept in parts and found there by their indices.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Grammar<P> {
    pub(crate) parsed: Parsed,
    pub(crate) rules: Vec<Rule>,
    pub(crate) parts: Vec<P>,
}

impl<P> Grammar<P> {
    /// What the text says before its first rule.
    pub(crate) fn parsed(self) -> Parsed {
        self.parsed
    }
}

/// Parses the keyword, then any titles and accessibility statements, then rules to the end
/// of the text, as Pele's parseGrammar does. Every table starts with its keyword and has
/// TITLE, ACC_TITLE and ACC_DESCR in a row from meta. A quoted title is read with unquote.
pub(crate) fn parse_grammar<P>(
    src: &str,
    table: &Table,
    meta: usize,
    unquote: fn(&str) -> String,
    mut rule: impl FnMut(&mut Reader, &mut Vec<P>) -> Result<Rule, Error>,
) -> Result<Grammar<P>, Error> {
    let tokens = tokenize(src, table, DiagramType::Railroad)?;
    let mut r = Reader::new(tokens, table, DiagramType::Railroad);
    let mut ast = Grammar {
        parsed: Parsed::default(),
        rules: Vec::new(),
        parts: Vec::new(),
    };
    r.expect(0)?;
    loop {
        let kind = r.kind();
        if kind == meta {
            let title = title_value(r.take());
            let quoted = (title.starts_with('"') && title.ends_with('"'))
                || (title.starts_with('\'') && title.ends_with('\''));
            ast.parsed.title = Some(if quoted { unquote(&title) } else { title });
        } else if kind == meta + 1 {
            ast.parsed.acc_title = Some(acc_title_value(r.take()));
        } else if kind == meta + 2 {
            ast.parsed.acc_descr = Some(acc_descr_value(r.take()));
        } else {
            break;
        }
    }
    while r.kind() != END {
        let rule = rule(&mut r, &mut ast.parts)?;
        ast.rules.push(rule);
    }
    Ok(ast)
}

/// Adds a node to the model's nodes and gives its index.
pub(crate) fn add(nodes: &mut Vec<RailroadNode>, node: RailroadNode) -> usize {
    nodes.push(node);
    nodes.len() - 1
}

/// An element that may be left out, Pele's optional.
pub(crate) fn optional(nodes: &mut Vec<RailroadNode>, element: usize) -> usize {
    add(nodes, RailroadNode::Optional(element))
}

/// A repetition of an element, at least min times and at most max, Pele's repetition.
pub(crate) fn repetition(
    nodes: &mut Vec<RailroadNode>,
    element: usize,
    min: f64,
    max: f64,
) -> usize {
    add(nodes, RailroadNode::Repetition { element, min, max })
}

/// A sequence, as Pele's sequence makes it: a sequence of one is that one, in every notation.
pub(crate) fn sequence(nodes: &mut Vec<RailroadNode>, elements: Vec<usize>) -> usize {
    if elements.len() == 1 {
        return elements[0];
    }
    add(nodes, RailroadNode::Sequence(elements))
}

/// A choice, as Pele's choice makes it: a choice of one is that one.
pub(crate) fn choice(nodes: &mut Vec<RailroadNode>, alternatives: Vec<usize>) -> usize {
    if alternatives.len() == 1 {
        return alternatives[0];
    }
    add(nodes, RailroadNode::Choice(alternatives))
}

/// Turns each rule of a syntax tree into the common model, as Pele's toRules does, given the
/// parts of a part of the tree and how one node is built from them. The nodes are added to
/// nodes.
pub(crate) fn to_rules<P>(
    ast: &Grammar<P>,
    nodes: &mut Vec<RailroadNode>,
    parts: impl Fn(&P) -> Vec<usize>,
    build: impl Fn(&mut Vec<RailroadNode>, &P, Vec<usize>) -> usize,
) -> Vec<RailroadRule> {
    ast.rules
        .iter()
        .map(|rule| RailroadRule {
            name: rule.name.clone(),
            definition: fold(
                rule.definition,
                |i| parts(&ast.parts[i]),
                |i, kids| build(nodes, &ast.parts[i], kids),
            ),
        })
        .collect()
}

/// Rebuilds a tree bottom-up without recursion, as Pele's fold does, so that its depth is
/// bounded by memory, not by the stack: each node, by its index, is built from what its
/// children were built into.
pub(crate) fn fold<R>(
    root: usize,
    children: impl Fn(usize) -> Vec<usize>,
    mut build: impl FnMut(usize, Vec<R>) -> R,
) -> R {
    let mut nodes = vec![root];
    let mut lists = vec![children(root)];
    let mut done: Vec<Vec<R>> = vec![Vec::new()];
    loop {
        let top = nodes.len() - 1;
        if done[top].len() < lists[top].len() {
            let child = lists[top][done[top].len()];
            nodes.push(child);
            lists.push(children(child));
            done.push(Vec::new());
            continue;
        }
        let results = done.pop().expect("a frame");
        let value = build(nodes[top], results);
        if top == 0 {
            return value;
        }
        nodes.pop();
        lists.pop();
        done[top - 1].push(value);
    }
}
