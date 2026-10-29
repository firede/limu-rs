//! `railroad-peg-beta`, parsing expression grammars, as Pele's peg.ts reads them.

use std::sync::OnceLock;

use super::common::{
    ACC_DESCR, ACC_TITLE, Grammar, Rule, add, choice, comment, directive, identifier, literal,
    opening, optional, parse_grammar, quoted, repetition, sequence, title, to_rules, unescape,
    whitespace, word_title, yaml,
};
use super::db::{RailroadNode, RailroadRule};
use crate::common::tokens::{Pattern, Reader, Table, TokenType, token};
use crate::errors::Error;

// The kinds of token, by index in PEG_TOKENS, as Pele's T.
pub(super) const T_ARROW: usize = 1;
pub(super) const T_SEMICOLON: usize = 2;
pub(super) const T_SLASH: usize = 3;
pub(super) const T_AND: usize = 4;
pub(super) const T_NOT: usize = 5;
pub(super) const T_QUESTION: usize = 6;
pub(super) const T_STAR: usize = 7;
pub(super) const T_PLUS: usize = 8;
pub(super) const T_GROUP: usize = 9;
pub(super) const T_GROUP_END: usize = 10;
pub(super) const T_DOT: usize = 11;
pub(super) const T_TITLE: usize = 12;
pub(super) const T_ID: usize = 15;
pub(super) const T_STRING: usize = 16;

/// Pele's PEG_TOKENS.
pub(crate) const PEG_TOKENS: &[TokenType] = &[
    opening("railroad-peg-beta", &[T_ID]),
    literal("<-", &[]),
    literal(";", &[]),
    literal("/", &[]),
    literal("&", &[]),
    literal("!", &[]),
    literal("?", &[]),
    literal("*", &[]),
    literal("+", &[]),
    literal("(", &[]),
    literal(")", &[]),
    literal(".", &[]),
    title(word_title),
    ACC_TITLE,
    ACC_DESCR,
    identifier("PEG_ID"),
    quoted("PEG_STRING"),
    whitespace("PEG_WHITESPACE"),
    yaml("PEG_YAML"),
    directive("PEG_DIRECTIVE"),
    comment("PEG_SINGLE_LINE_COMMENT"),
    TokenType {
        first: Some("#"),
        hidden: true,
        ..token(
            "PEG_LINE_COMMENT",
            Pattern::Match(|s, at| {
                (s.as_bytes()[at] == b'#').then(|| {
                    let b = s.as_bytes();
                    (at + 1..b.len())
                        .find(|&k| b[k] == b'\n' || b[k] == b'\r')
                        .unwrap_or(b.len())
                })
            }),
        )
    },
];

/// PEG_TOKENS made ready to tokenize with.
pub(crate) fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| Table::new(PEG_TOKENS))
}

/// A part of the syntax tree, Pele's PegChoice, PegSequence, PegPrefix, PegSuffix and
/// PegPrimary.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PegPart {
    Choice(Vec<usize>),
    Sequence(Vec<usize>),
    Prefix {
        operator: Option<String>,
        suffix: usize,
    },
    Suffix {
        primary: usize,
        operator: Option<String>,
    },
    Literal(String),
    Identifier(String),
    Any(String),
    Group(usize),
}

impl PegPart {
    /// The parts of the part, as Pele's parts gives them.
    pub(crate) fn parts(&self) -> Vec<usize> {
        match self {
            PegPart::Choice(kids) | PegPart::Sequence(kids) => kids.clone(),
            PegPart::Prefix { suffix: e, .. }
            | PegPart::Suffix { primary: e, .. }
            | PegPart::Group(e) => vec![*e],
            _ => Vec::new(),
        }
    }
}

/// A PEG syntax tree, Pele's PegAst.
pub(crate) type PegAst = Grammar<PegPart>;

/// One open group, or the rule itself at the bottom of the stack, as Pele's Frame: its
/// choice, the sequence being read, the group's node, and the predicate written before it.
struct Frame {
    choice: usize,
    sequence: usize,
    node: Option<usize>,
    operator: Option<String>,
}

fn push(parts: &mut Vec<PegPart>, part: PegPart) -> usize {
    parts.push(part);
    parts.len() - 1
}

/// A frame of a choice, with the first of its sequences, as Pele's frame makes it.
fn frame(
    parts: &mut Vec<PegPart>,
    node: Option<usize>,
    choice: usize,
    operator: Option<String>,
) -> Frame {
    let sequence = push(parts, PegPart::Sequence(Vec::new()));
    if let PegPart::Choice(alternatives) = &mut parts[choice] {
        alternatives.push(sequence);
    }
    Frame {
        choice,
        sequence,
        node,
        operator,
    }
}

/// Reads a rule's definition, as Pele's definition does, with the groups still open on a
/// stack rather than in recursion.
fn definition(r: &mut Reader, parts: &mut Vec<PegPart>) -> Result<usize, Error> {
    let mut stack: Vec<Frame> = Vec::new();
    let root = push(parts, PegPart::Choice(Vec::new()));
    let mut top = frame(parts, None, root, None);
    loop {
        let mut operator = matches!(r.kind(), T_AND | T_NOT).then(|| r.take().to_string());
        let mut primary = match r.kind() {
            T_STRING => push(parts, PegPart::Literal(unescape(r.take()))),
            T_ID => {
                let name = r.take().to_string();
                push(parts, PegPart::Identifier(name))
            }
            T_DOT => {
                let dot = r.take().to_string();
                push(parts, PegPart::Any(dot))
            }
            T_GROUP => {
                r.i += 1;
                let element = push(parts, PegPart::Choice(Vec::new()));
                let node = push(parts, PegPart::Group(element));
                let open = frame(parts, Some(node), element, operator);
                stack.push(std::mem::replace(&mut top, open));
                continue;
            }
            _ => return Err(r.fail("a literal, a name, a dot, or an opening parenthesis")),
        };
        loop {
            let suffix_operator =
                matches!(r.kind(), T_QUESTION | T_STAR | T_PLUS).then(|| r.take().to_string());
            let suffix = push(
                parts,
                PegPart::Suffix {
                    primary,
                    operator: suffix_operator,
                },
            );
            let prefix = push(
                parts,
                PegPart::Prefix {
                    operator: operator.take(),
                    suffix,
                },
            );
            if let PegPart::Sequence(elements) = &mut parts[top.sequence] {
                elements.push(prefix);
            }
            let k = r.kind();
            if k == T_GROUP_END && !stack.is_empty() {
                r.i += 1;
                primary = top.node.expect("a group's node");
                operator = top.operator.take();
                top = stack.pop().expect("an open group");
                continue;
            }
            if k == T_SLASH {
                r.i += 1;
                top.sequence = push(parts, PegPart::Sequence(Vec::new()));
                if let PegPart::Choice(alternatives) = &mut parts[top.choice] {
                    alternatives.push(top.sequence);
                }
            } else if !matches!(k, T_AND | T_NOT | T_STRING | T_ID | T_DOT | T_GROUP) {
                if stack.is_empty() {
                    return Ok(top.choice);
                }
                return Err(r.fail("token of type ')'"));
            }
            break;
        }
    }
}

/// Parses a railroad-peg-beta text into its syntax tree, as Pele's parsePeg does.
pub(crate) fn parse_peg(src: &str) -> Result<PegAst, Error> {
    parse_grammar(src, table(), T_TITLE, unescape, |r, parts| {
        let name = r.expect(T_ID)?.to_string();
        r.expect(T_ARROW)?;
        let definition = definition(r, parts)?;
        r.expect(T_SEMICOLON)?;
        Ok(Rule { name, definition })
    })
}

/// What a predicate shows of the expression it looks ahead for, as Pele's label writes it.
fn label(node: &RailroadNode) -> String {
    match node {
        RailroadNode::Terminal(v) => format!("\"{v}\""),
        RailroadNode::Nonterminal(v) | RailroadNode::Special(v) => v.clone(),
        _ => "(...)".to_string(),
    }
}

/// The model's node of a part, of the nodes its parts were built into, as Pele's node builds
/// it. A predicate is special text, as a railroad has no track for it.
fn node(nodes: &mut Vec<RailroadNode>, part: &PegPart, kids: Vec<usize>) -> usize {
    let kid = kids.first().copied();
    match part {
        PegPart::Choice(_) => choice(nodes, kids),
        PegPart::Sequence(_) => sequence(nodes, kids),
        PegPart::Prefix { operator, .. } => {
            let kid = kid.expect("a suffix");
            match operator.as_deref().filter(|o| !o.is_empty()) {
                Some(op) => {
                    let text = format!("{op}{}", label(&nodes[kid]));
                    add(nodes, RailroadNode::Special(text))
                }
                None => kid,
            }
        }
        PegPart::Suffix { operator, .. } => {
            let kid = kid.expect("a primary");
            match operator.as_deref().filter(|o| !o.is_empty()) {
                None => kid,
                Some("?") => optional(nodes, kid),
                Some(op) => {
                    let min = if op == "+" { 1.0 } else { 0.0 };
                    repetition(nodes, kid, min, f64::INFINITY)
                }
            }
        }
        PegPart::Literal(v) => add(nodes, RailroadNode::Terminal(v.clone())),
        PegPart::Identifier(v) => add(nodes, RailroadNode::Nonterminal(v.clone())),
        PegPart::Any(v) => add(nodes, RailroadNode::Special(v.clone())),
        PegPart::Group(_) => kid.expect("a choice"),
    }
}

/// The rules of a PEG syntax tree in the common model, Pele's pegRules.
pub(crate) fn peg_rules(ast: &PegAst, nodes: &mut Vec<RailroadNode>) -> Vec<RailroadRule> {
    to_rules(ast, nodes, PegPart::parts, node)
}
