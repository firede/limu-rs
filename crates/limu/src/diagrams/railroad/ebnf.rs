//! `railroad-ebnf-beta`, EBNF in the W3C and ISO 14977 styles, as Pele's ebnf.ts reads it.

use std::sync::OnceLock;

use super::common::{
    ACC_DESCR, ACC_TITLE, Grammar, Rule, add, between, choice, comment, directive, enclosed,
    identifier, inner, literal, opening, optional, parse_grammar, quoted, repetition, sequence,
    title, to_rules, unescape, whitespace, word_title, yaml,
};
use super::db::{RailroadNode, RailroadRule};
use crate::common::tokens::{Pattern, Reader, Table, TokenType, token};
use crate::errors::Error;
use crate::js::chars::{is_space, trim, unit_at};

// The kinds of token, by index in EBNF_TOKENS, as Pele's T.
pub(super) const T_DEFINES: usize = 1;
pub(super) const T_EQUALS: usize = 2;
pub(super) const T_SEMICOLON: usize = 3;
pub(super) const T_BAR: usize = 4;
pub(super) const T_COMMA: usize = 5;
pub(super) const T_GROUP: usize = 6;
pub(super) const T_OPTION: usize = 8;
pub(super) const T_REPEAT: usize = 10;
pub(super) const T_QUESTION: usize = 12;
pub(super) const T_STAR: usize = 13;
pub(super) const T_PLUS: usize = 14;
pub(super) const T_MINUS: usize = 15;
pub(super) const T_TITLE: usize = 16;
pub(super) const T_ID: usize = 19;
pub(super) const T_STRING: usize = 20;
pub(super) const T_SPECIAL: usize = 21;
pub(super) const T_YAML: usize = 23;
pub(super) const T_ISO_COMMENT: usize = 27;

/// A special sequence, as Pele's specialSequence reads it: a question mark, text with
/// something other than blanks in it and no `?` or `;`, and a question mark. Mermaid's
/// pattern looks ahead with two nested runs, which rescans, and this reads the text once.
fn special_sequence(src: &str, at: usize) -> Option<usize> {
    if src.as_bytes()[at] != b'?' {
        return None;
    }
    let mut solid = false;
    let mut i = at + 1;
    while i < src.len() {
        let (c, size) = unit_at(src, i);
        if c == 63 {
            return solid.then_some(i + 1);
        }
        if c == 59 {
            return None;
        }
        solid = solid || !is_space(c);
        i += size;
    }
    None
}

/// Pele's EBNF_TOKENS.
pub(crate) const EBNF_TOKENS: &[TokenType] = &[
    opening("railroad-ebnf-beta", &[T_ID]),
    literal("::=", &[]),
    literal("=", &[]),
    literal(";", &[]),
    literal("|", &[]),
    literal(",", &[]),
    literal("(", &[T_ISO_COMMENT]),
    literal(")", &[]),
    literal("[", &[]),
    literal("]", &[]),
    literal("{", &[]),
    literal("}", &[]),
    literal("?", &[T_SPECIAL]),
    literal("*", &[]),
    literal("+", &[]),
    literal("-", &[T_YAML]),
    title(word_title),
    ACC_TITLE,
    ACC_DESCR,
    identifier("EBNF_ID"),
    quoted("EBNF_STRING"),
    TokenType {
        first: Some("?"),
        ..token("EBNF_SPECIAL_SEQUENCE", Pattern::Match(special_sequence))
    },
    whitespace("EBNF_WHITESPACE"),
    yaml("EBNF_YAML"),
    directive("EBNF_DIRECTIVE"),
    comment("EBNF_SINGLE_LINE_COMMENT"),
    between(
        "EBNF_BLOCK_COMMENT",
        "/",
        |s, at| enclosed(s, at, "/*", "*/"),
        |s, at| s[at..].starts_with("/*"),
    ),
    between(
        "EBNF_ISO_COMMENT",
        "(",
        |s, at| enclosed(s, at, "(*", "*)"),
        |s, at| s[at..].starts_with("(*"),
    ),
];

/// EBNF_TOKENS made ready to tokenize with.
pub(crate) fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| Table::new(EBNF_TOKENS))
}

/// An operator after a primary, Pele's EbnfPostfix.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EbnfPostfix {
    Optional(String),
    ZeroOrMore(String),
    OneOrMore(String),
    /// What the primary is not, the part at an index.
    Exception(usize),
}

/// A part of the syntax tree, Pele's EbnfChoice, EbnfSequence, EbnfTerm and EbnfPrimary.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EbnfPart {
    Choice(Vec<usize>),
    Sequence(Vec<usize>),
    Term {
        base: usize,
        postfixes: Vec<EbnfPostfix>,
    },
    Terminal(String),
    NonTerminal(String),
    Special(String),
    Group(usize),
    Optional(usize),
    Repetition(usize),
}

impl EbnfPart {
    /// The parts of the part, as Pele's parts gives them: a term's base and then what each
    /// of its exceptions excepts.
    pub(crate) fn parts(&self) -> Vec<usize> {
        match self {
            EbnfPart::Choice(kids) | EbnfPart::Sequence(kids) => kids.clone(),
            EbnfPart::Term { base, postfixes } => std::iter::once(*base)
                .chain(postfixes.iter().filter_map(|p| match p {
                    EbnfPostfix::Exception(e) => Some(*e),
                    _ => None,
                }))
                .collect(),
            EbnfPart::Group(e) | EbnfPart::Optional(e) | EbnfPart::Repetition(e) => vec![*e],
            _ => Vec::new(),
        }
    }
}

/// An EBNF syntax tree, Pele's EbnfAst.
pub(crate) type EbnfAst = Grammar<EbnfPart>;

/// One open bracket, or the rule itself at the bottom of the stack, as Pele's Frame: its
/// choice, the sequence and the term being read, the bracket's node, its closing token, and
/// whether it is the right side of an exception.
struct Frame {
    choice: usize,
    sequence: usize,
    term: Option<usize>,
    node: Option<usize>,
    close: Option<usize>,
    except: bool,
}

/// A frame of a choice, with the first of its sequences, as Pele's frame makes it.
fn frame(
    parts: &mut Vec<EbnfPart>,
    node: Option<usize>,
    choice: usize,
    close: Option<usize>,
    except: bool,
) -> Frame {
    let sequence = push(parts, EbnfPart::Sequence(Vec::new()));
    if let EbnfPart::Choice(alternatives) = &mut parts[choice] {
        alternatives.push(sequence);
    }
    Frame {
        choice,
        sequence,
        term: None,
        node,
        close,
        except,
    }
}

fn push(parts: &mut Vec<EbnfPart>, part: EbnfPart) -> usize {
    parts.push(part);
    parts.len() - 1
}

/// Reads a rule's definition, as Pele's definition does, with the brackets still open on a
/// stack rather than in recursion.
fn definition(r: &mut Reader, parts: &mut Vec<EbnfPart>) -> Result<usize, Error> {
    let mut stack: Vec<Frame> = Vec::new();
    let root = push(parts, EbnfPart::Choice(Vec::new()));
    let mut top = frame(parts, None, root, None, false);
    let mut except = false;
    loop {
        let kind = r.kind();
        let mut primary = match kind {
            T_STRING => push(parts, EbnfPart::Terminal(unescape(r.take()))),
            T_ID => {
                let name = r.take().to_string();
                push(parts, EbnfPart::NonTerminal(name))
            }
            T_SPECIAL => {
                let text = trim(inner(r.take())).to_string();
                push(parts, EbnfPart::Special(text))
            }
            T_GROUP | T_OPTION | T_REPEAT => {
                r.i += 1;
                let element = push(parts, EbnfPart::Choice(Vec::new()));
                let node = push(
                    parts,
                    match kind {
                        T_GROUP => EbnfPart::Group(element),
                        T_OPTION => EbnfPart::Optional(element),
                        _ => EbnfPart::Repetition(element),
                    },
                );
                let open = frame(parts, Some(node), element, Some(kind + 1), except);
                stack.push(std::mem::replace(&mut top, open));
                except = false;
                continue;
            }
            _ => return Err(r.fail("a terminal, a name, or an opening bracket")),
        };
        loop {
            let term = if except {
                except = false;
                let term = top.term.expect("a term before an exception");
                if let EbnfPart::Term { postfixes, .. } = &mut parts[term] {
                    postfixes.push(EbnfPostfix::Exception(primary));
                }
                term
            } else {
                let term = push(
                    parts,
                    EbnfPart::Term {
                        base: primary,
                        postfixes: Vec::new(),
                    },
                );
                top.term = Some(term);
                if let EbnfPart::Sequence(elements) = &mut parts[top.sequence] {
                    elements.push(term);
                }
                term
            };
            loop {
                let postfix = match r.kind() {
                    T_QUESTION => EbnfPostfix::Optional(r.take().to_string()),
                    T_STAR => EbnfPostfix::ZeroOrMore(r.take().to_string()),
                    T_PLUS => EbnfPostfix::OneOrMore(r.take().to_string()),
                    _ => break,
                };
                if let EbnfPart::Term { postfixes, .. } = &mut parts[term] {
                    postfixes.push(postfix);
                }
            }
            let k = r.kind();
            if Some(k) == top.close {
                r.i += 1;
                primary = top.node.expect("a bracket's node");
                except = top.except;
                top = stack.pop().expect("an open bracket");
                continue;
            }
            if k == T_MINUS {
                r.i += 1;
                except = true;
            } else if k == T_COMMA {
                r.i += 1;
            } else if k == T_BAR {
                r.i += 1;
                top.sequence = push(parts, EbnfPart::Sequence(Vec::new()));
                if let EbnfPart::Choice(alternatives) = &mut parts[top.choice] {
                    alternatives.push(top.sequence);
                }
            } else if !matches!(
                k,
                T_STRING | T_ID | T_SPECIAL | T_GROUP | T_OPTION | T_REPEAT
            ) {
                let Some(close) = top.close else {
                    return Ok(top.choice);
                };
                let expected = format!("token of type '{}'", EBNF_TOKENS[close].name);
                return Err(r.fail(&expected));
            }
            break;
        }
    }
}

/// Parses a railroad-ebnf-beta text into its syntax tree, as Pele's parseEbnf does.
pub(crate) fn parse_ebnf(src: &str) -> Result<EbnfAst, Error> {
    parse_grammar(src, table(), T_TITLE, unescape, |r, parts| {
        let name = r.expect(T_ID)?.to_string();
        if !r.accept(T_EQUALS) {
            r.expect(T_DEFINES)?;
        }
        let definition = definition(r, parts)?;
        r.expect(T_SEMICOLON)?;
        Ok(Rule { name, definition })
    })
}

/// The model's node of a part, of the nodes its parts were built into, as Pele's node builds
/// it.
fn node(nodes: &mut Vec<RailroadNode>, part: &EbnfPart, kids: Vec<usize>) -> usize {
    match part {
        EbnfPart::Choice(_) => choice(nodes, kids),
        EbnfPart::Sequence(_) => sequence(nodes, kids),
        EbnfPart::Term { postfixes, .. } => {
            // Each postfix operator applies to everything before it in the term.
            let mut result = kids[0];
            let mut next = 1;
            for postfix in postfixes {
                result = match postfix {
                    EbnfPostfix::Optional(_) => optional(nodes, result),
                    EbnfPostfix::Exception(_) => {
                        let minus = add(nodes, RailroadNode::Terminal("-".into()));
                        next += 1;
                        add(
                            nodes,
                            RailroadNode::Sequence(vec![result, minus, kids[next - 1]]),
                        )
                    }
                    EbnfPostfix::OneOrMore(_) => repetition(nodes, result, 1.0, f64::INFINITY),
                    EbnfPostfix::ZeroOrMore(_) => repetition(nodes, result, 0.0, f64::INFINITY),
                };
            }
            result
        }
        EbnfPart::Terminal(v) => add(nodes, RailroadNode::Terminal(v.clone())),
        EbnfPart::NonTerminal(v) => add(nodes, RailroadNode::Nonterminal(v.clone())),
        EbnfPart::Special(v) => add(nodes, RailroadNode::Special(v.clone())),
        EbnfPart::Group(_) => kids[0],
        EbnfPart::Optional(_) => optional(nodes, kids[0]),
        EbnfPart::Repetition(_) => repetition(nodes, kids[0], 0.0, f64::INFINITY),
    }
}

/// The rules of an EBNF syntax tree in the common model, Pele's ebnfRules.
pub(crate) fn ebnf_rules(ast: &EbnfAst, nodes: &mut Vec<RailroadNode>) -> Vec<RailroadRule> {
    to_rules(ast, nodes, EbnfPart::parts, node)
}
