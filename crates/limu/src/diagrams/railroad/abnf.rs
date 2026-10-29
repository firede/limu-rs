//! `railroad-abnf-beta`, ABNF as in RFC 5234 with each rule closed by a semicolon, as Pele's
//! abnf.ts reads it.

use std::sync::OnceLock;

use super::common::{
    ACC_DESCR, ACC_TITLE, Grammar, LETTERS, Rule, add, choice, comment, directive, inner, literal,
    opening, optional, parse_grammar, repetition, sequence, title, to_rules, whitespace, yaml,
};
use super::db::{RailroadNode, RailroadRule};
use crate::common::tokens::{Pattern, Reader, Table, TokenType, match_title, token};
use crate::errors::Error;
use crate::js::number::parse_int;

// The kinds of token, by index in ABNF_TOKENS, as Pele's T.
pub(super) const T_EQUALS: usize = 1;
pub(super) const T_SEMICOLON: usize = 2;
pub(super) const T_SLASH: usize = 3;
pub(super) const T_GROUP: usize = 4;
pub(super) const T_OPTION: usize = 6;
pub(super) const T_TITLE: usize = 8;
pub(super) const T_NAME: usize = 11;
pub(super) const T_STRING: usize = 12;
pub(super) const T_NUMBER: usize = 13;
pub(super) const T_REPEAT: usize = 14;
pub(super) const T_EXACT: usize = 15;
pub(super) const T_COMMENT: usize = 20;

const DIGITS: &str = "0123456789";

/// The end of a run of bytes from i that are in a set.
fn run(s: &str, mut i: usize, within: impl Fn(u8) -> bool) -> usize {
    let b = s.as_bytes();
    while i < b.len() && within(b[i]) {
        i += 1;
    }
    i
}

/// TITLE where a name goes on with [A-Za-z0-9-].
fn abnf_title(s: &str, at: usize) -> Option<usize> {
    match_title(
        s,
        at,
        Some(|s, i| {
            s.as_bytes()
                .get(i)
                .is_some_and(|&c| c.is_ascii_alphanumeric() || c == b'-')
        }),
    )
}

/// Pele's ABNF_RULENAME, /[A-Za-z][A-Za-z0-9-]*/y.
fn rule_name(s: &str, at: usize) -> Option<usize> {
    if !s.as_bytes()[at].is_ascii_alphabetic() {
        return None;
    }
    Some(run(s, at + 1, |c| c.is_ascii_alphanumeric() || c == b'-'))
}

/// Pele's ABNF_STRING, /"[^"]*"/y.
fn string(s: &str, at: usize) -> Option<usize> {
    if s.as_bytes()[at] != b'"' {
        return None;
    }
    s[at + 1..].find('"').map(|k| at + 1 + k + 1)
}

/// Pele's ABNF_NUMVAL, /%[xXdDbB][0-9A-Fa-f]+(?:-[0-9A-Fa-f]+|\.[0-9A-Fa-f]+)*/y. A range or a
/// dot not followed by a digit is left for the next token.
fn number(s: &str, at: usize) -> Option<usize> {
    let b = s.as_bytes();
    if b[at] != b'%' || !b.get(at + 1).is_some_and(|c| b"xXdDbB".contains(c)) {
        return None;
    }
    let hex = |i: usize| run(s, i, |c| c.is_ascii_hexdigit());
    let mut end = hex(at + 2);
    if end == at + 2 {
        return None;
    }
    while matches!(b.get(end), Some(b'-' | b'.')) && hex(end + 1) > end + 1 {
        end = hex(end + 1);
    }
    Some(end)
}

/// Pele's ABNF_REPEAT, /[0-9]*\*[0-9]*/y.
fn repeat(s: &str, at: usize) -> Option<usize> {
    let star = run(s, at, |c| c.is_ascii_digit());
    if s.as_bytes().get(star) != Some(&b'*') {
        return None;
    }
    Some(run(s, star + 1, |c| c.is_ascii_digit()))
}

/// Pele's ABNF_EXACT_REPEAT, /[0-9]+/y.
fn exact(s: &str, at: usize) -> Option<usize> {
    Some(run(s, at, |c| c.is_ascii_digit())).filter(|&end| end > at)
}

/// Pele's ABNF_TOKENS.
pub(crate) const ABNF_TOKENS: &[TokenType] = &[
    opening("railroad-abnf-beta", &[T_NAME]),
    literal("=", &[]),
    // A semicolon closes a rule only at the end of its line. With anything after it, it
    // starts a comment.
    literal(";", &[T_COMMENT]),
    literal("/", &[]),
    literal("(", &[]),
    literal(")", &[]),
    literal("[", &[]),
    literal("]", &[]),
    title(abnf_title),
    ACC_TITLE,
    ACC_DESCR,
    TokenType {
        first: Some(LETTERS),
        ..token("ABNF_RULENAME", Pattern::Match(rule_name))
    },
    TokenType {
        first: Some("\""),
        ..token("ABNF_STRING", Pattern::Match(string))
    },
    TokenType {
        first: Some("%"),
        ..token("ABNF_NUMVAL", Pattern::Match(number))
    },
    TokenType {
        first: Some("0123456789*"),
        ..token("ABNF_REPEAT", Pattern::Match(repeat))
    },
    TokenType {
        first: Some(DIGITS),
        ..token("ABNF_EXACT_REPEAT", Pattern::Match(exact))
    },
    whitespace("ABNF_WHITESPACE"),
    yaml("ABNF_YAML"),
    directive("ABNF_DIRECTIVE"),
    comment("ABNF_SINGLE_LINE_COMMENT"),
    TokenType {
        first: Some(";"),
        hidden: true,
        ..token(
            "ABNF_COMMENT",
            Pattern::Match(|s, at| (s.as_bytes()[at] == b';').then(|| line_end(s, at + 1))),
        )
    },
];

/// The end of the line from byte i, [^\n\r]*.
fn line_end(s: &str, i: usize) -> usize {
    run(s, i, |c| c != b'\n' && c != b'\r')
}

/// ABNF_TOKENS made ready to tokenize with.
pub(crate) fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| Table::new(ABNF_TOKENS))
}

/// A part of the syntax tree, Pele's AbnfAlternation, AbnfConcatenation, AbnfElement and
/// AbnfPrimary.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AbnfPart {
    Alternation(Vec<usize>),
    Concatenation(Vec<usize>),
    Element {
        repeat: Option<String>,
        primary: usize,
    },
    StringLiteral(String),
    NumVal(String),
    RuleName(String),
    Group(usize),
    OptionalGroup(usize),
}

impl AbnfPart {
    /// The parts of the part, as Pele's parts gives them.
    pub(crate) fn parts(&self) -> Vec<usize> {
        match self {
            AbnfPart::Alternation(kids) | AbnfPart::Concatenation(kids) => kids.clone(),
            AbnfPart::Element { primary, .. } => vec![*primary],
            AbnfPart::Group(e) | AbnfPart::OptionalGroup(e) => vec![*e],
            _ => Vec::new(),
        }
    }
}

/// An ABNF syntax tree, Pele's AbnfAst.
pub(crate) type AbnfAst = Grammar<AbnfPart>;

/// One open bracket, or the rule itself at the bottom of the stack, as Pele's Frame: its
/// alternation, the concatenation being read, the bracket's node, its closing token, and the
/// repeat written before it.
struct Frame {
    alternation: usize,
    concatenation: usize,
    node: Option<usize>,
    close: Option<usize>,
    repeat: Option<String>,
}

fn push(parts: &mut Vec<AbnfPart>, part: AbnfPart) -> usize {
    parts.push(part);
    parts.len() - 1
}

/// A frame of an alternation, with the first of its concatenations, as Pele's frame makes
/// it.
fn frame(
    parts: &mut Vec<AbnfPart>,
    node: Option<usize>,
    alternation: usize,
    close: Option<usize>,
    repeat: Option<String>,
) -> Frame {
    let concatenation = push(parts, AbnfPart::Concatenation(Vec::new()));
    if let AbnfPart::Alternation(alternatives) = &mut parts[alternation] {
        alternatives.push(concatenation);
    }
    Frame {
        alternation,
        concatenation,
        node,
        close,
        repeat,
    }
}

/// Reads a rule's definition, as Pele's alternation does, with the brackets still open on a
/// stack rather than in recursion.
fn alternation(r: &mut Reader, parts: &mut Vec<AbnfPart>) -> Result<usize, Error> {
    let mut stack: Vec<Frame> = Vec::new();
    let root = push(parts, AbnfPart::Alternation(Vec::new()));
    let mut top = frame(parts, None, root, None, None);
    loop {
        let mut repeat = matches!(r.kind(), T_REPEAT | T_EXACT).then(|| r.take().to_string());
        let kind = r.kind();
        let mut primary = match kind {
            T_STRING => {
                let value = inner(r.take()).to_string();
                push(parts, AbnfPart::StringLiteral(value))
            }
            T_NUMBER => {
                let value = r.take().to_string();
                push(parts, AbnfPart::NumVal(value))
            }
            T_NAME => {
                let name = r.take().to_string();
                push(parts, AbnfPart::RuleName(name))
            }
            T_GROUP | T_OPTION => {
                r.i += 1;
                let element = push(parts, AbnfPart::Alternation(Vec::new()));
                let node = push(
                    parts,
                    match kind {
                        T_GROUP => AbnfPart::Group(element),
                        _ => AbnfPart::OptionalGroup(element),
                    },
                );
                let open = frame(parts, Some(node), element, Some(kind + 1), repeat);
                stack.push(std::mem::replace(&mut top, open));
                continue;
            }
            _ => return Err(r.fail("a string, a number, a name, or an opening bracket")),
        };
        loop {
            let element = push(
                parts,
                AbnfPart::Element {
                    repeat: repeat.take(),
                    primary,
                },
            );
            if let AbnfPart::Concatenation(elements) = &mut parts[top.concatenation] {
                elements.push(element);
            }
            let k = r.kind();
            if Some(k) == top.close {
                r.i += 1;
                primary = top.node.expect("a bracket's node");
                repeat = top.repeat.take();
                top = stack.pop().expect("an open bracket");
                continue;
            }
            if k == T_SLASH {
                r.i += 1;
                top.concatenation = push(parts, AbnfPart::Concatenation(Vec::new()));
                if let AbnfPart::Alternation(alternatives) = &mut parts[top.alternation] {
                    alternatives.push(top.concatenation);
                }
            } else if !matches!(
                k,
                T_STRING | T_NUMBER | T_NAME | T_GROUP | T_OPTION | T_REPEAT | T_EXACT
            ) {
                let Some(close) = top.close else {
                    return Ok(top.alternation);
                };
                let expected = format!("token of type '{}'", ABNF_TOKENS[close].name);
                return Err(r.fail(&expected));
            }
            break;
        }
    }
}

/// Parses a railroad-abnf-beta text into its syntax tree, as Pele's parseAbnf does. A quoted
/// title is the text between its quotes.
pub(crate) fn parse_abnf(src: &str) -> Result<AbnfAst, Error> {
    parse_grammar(
        src,
        table(),
        T_TITLE,
        |text| inner(text).to_string(),
        |r, parts| {
            let name = r.expect(T_NAME)?.to_string();
            r.expect(T_EQUALS)?;
            let definition = alternation(r, parts)?;
            r.expect(T_SEMICOLON)?;
            Ok(Rule { name, definition })
        },
    )
}

/// The model's node of a part, of the nodes its parts were built into, as Pele's node builds
/// it. An element repeated from 0 to 1 times is optional.
fn node(nodes: &mut Vec<RailroadNode>, part: &AbnfPart, kids: Vec<usize>) -> usize {
    match part {
        AbnfPart::Alternation(_) => choice(nodes, kids),
        AbnfPart::Concatenation(_) => sequence(nodes, kids),
        AbnfPart::Element { repeat, .. } => {
            let Some(repeat) = repeat.as_deref().filter(|r| !r.is_empty()) else {
                return kids[0];
            };
            let star = repeat.find('*');
            let exact = star.is_none();
            let min = if star.is_some_and(|s| s > 0) || exact {
                parse_int(repeat, 10)
            } else {
                0.0
            };
            let max = match star {
                None => min,
                Some(s) if s < repeat.len() - 1 => parse_int(&repeat[s + 1..], 10),
                Some(_) => f64::INFINITY,
            };
            if min == 0.0 && max == 1.0 {
                optional(nodes, kids[0])
            } else {
                repetition(nodes, kids[0], min, max)
            }
        }
        AbnfPart::StringLiteral(v) | AbnfPart::NumVal(v) => {
            add(nodes, RailroadNode::Terminal(v.clone()))
        }
        AbnfPart::RuleName(v) => add(nodes, RailroadNode::Nonterminal(v.clone())),
        AbnfPart::Group(_) => kids[0],
        AbnfPart::OptionalGroup(_) => optional(nodes, kids[0]),
    }
}

/// The rules of an ABNF syntax tree in the common model, Pele's abnfRules.
pub(crate) fn abnf_rules(ast: &AbnfAst, nodes: &mut Vec<RailroadNode>) -> Vec<RailroadRule> {
    to_rules(ast, nodes, AbnfPart::parts, node)
}
