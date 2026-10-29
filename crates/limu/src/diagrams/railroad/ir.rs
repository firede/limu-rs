//! `railroad-beta`, Mermaid's own notation, one constructor call a node, as Pele's ir.ts
//! reads it.

use std::sync::OnceLock;

use super::common::{
    ACC_DESCR, ACC_TITLE, Grammar, Rule, add, between, choice, comment, directive, enclosed,
    identifier, literal, opening, optional, parse_grammar, quoted, repetition, sequence, title,
    to_rules, unescape, whitespace, word_title, yaml,
};
use super::db::{RailroadNode, RailroadRule};
use crate::common::tokens::{Reader, Table, TokenType};
use crate::errors::Error;

// The kinds of token, by index in IR_TOKENS, as Pele's T.
pub(super) const T_NONTERMINAL: usize = 1;
pub(super) const T_ZERO_OR_MORE: usize = 2;
pub(super) const T_ONE_OR_MORE: usize = 3;
pub(super) const T_SEQUENCE: usize = 4;
pub(super) const T_OPTIONAL: usize = 5;
pub(super) const T_TERMINAL: usize = 6;
pub(super) const T_SPECIAL: usize = 7;
pub(super) const T_CHOICE: usize = 8;
pub(super) const T_EQUALS: usize = 9;
pub(super) const T_SEMICOLON: usize = 10;
pub(super) const T_OPEN: usize = 11;
pub(super) const T_COMMA: usize = 12;
pub(super) const T_CLOSE: usize = 13;
pub(super) const T_TITLE: usize = 14;
pub(super) const T_ID: usize = 17;
pub(super) const T_STRING: usize = 18;

/// A word of the notation, which a name that starts with it and goes on is not.
const fn word(name: &'static str) -> TokenType {
    literal(name, &[T_ID])
}

/// Pele's IR_TOKENS.
pub(crate) const IR_TOKENS: &[TokenType] = &[
    opening("railroad-beta", &[T_ID]),
    word("nonterminal"),
    word("zeroOrMore"),
    word("oneOrMore"),
    word("sequence"),
    word("optional"),
    word("terminal"),
    word("special"),
    word("choice"),
    literal("=", &[]),
    literal(";", &[]),
    literal("(", &[]),
    literal(",", &[]),
    literal(")", &[]),
    title(word_title),
    ACC_TITLE,
    ACC_DESCR,
    identifier("RR_ID"),
    quoted("RR_STRING"),
    whitespace("RR_WHITESPACE"),
    yaml("RR_YAML"),
    directive("RR_DIRECTIVE"),
    comment("RR_SINGLE_LINE_COMMENT"),
    between(
        "RR_BLOCK_COMMENT",
        "/",
        |s, at| enclosed(s, at, "/*", "*/"),
        |s, at| s[at..].starts_with("/*"),
    ),
];

/// IR_TOKENS made ready to tokenize with.
pub(crate) fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| Table::new(IR_TOKENS))
}

/// A part of the syntax tree, Pele's IrExpression. A wrapper's element is none until the
/// parser has read it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum IrExpression {
    Sequence(Vec<usize>),
    Choice(Vec<usize>),
    Optional(Option<usize>),
    OneOrMore(Option<usize>),
    ZeroOrMore(Option<usize>),
    Terminal(String),
    NonTerminal(String),
    Special(String),
}

impl IrExpression {
    /// The parts of the expression, as Pele's parts gives them.
    pub(crate) fn parts(&self) -> Vec<usize> {
        match self {
            IrExpression::Sequence(kids) | IrExpression::Choice(kids) => kids.clone(),
            IrExpression::Optional(e)
            | IrExpression::OneOrMore(e)
            | IrExpression::ZeroOrMore(e) => e.iter().copied().collect(),
            _ => Vec::new(),
        }
    }
}

/// A railroad-beta syntax tree, Pele's IrAst.
pub(crate) type IrAst = Grammar<IrExpression>;

/// The string argument of a constructor of a leaf, as Pele's argument reads it.
fn argument(r: &mut Reader) -> Result<String, Error> {
    r.i += 1;
    r.expect(T_OPEN)?;
    let value = unescape(r.expect(T_STRING)?);
    r.expect(T_CLOSE)?;
    Ok(value)
}

/// Reads an expression, as Pele's expression does, with the constructors still open on a
/// stack rather than in recursion.
fn expression(r: &mut Reader, parts: &mut Vec<IrExpression>) -> Result<usize, Error> {
    let mut open: Vec<usize> = Vec::new();
    loop {
        let value = match r.kind() {
            T_TERMINAL => IrExpression::Terminal(argument(r)?),
            T_NONTERMINAL => IrExpression::NonTerminal(argument(r)?),
            T_SPECIAL => IrExpression::Special(argument(r)?),
            T_SEQUENCE => IrExpression::Sequence(Vec::new()),
            T_CHOICE => IrExpression::Choice(Vec::new()),
            T_OPTIONAL => IrExpression::Optional(None),
            T_ONE_OR_MORE => IrExpression::OneOrMore(None),
            T_ZERO_OR_MORE => IrExpression::ZeroOrMore(None),
            _ => return Err(r.fail("an expression")),
        };
        let leaf = matches!(
            value,
            IrExpression::Terminal(_) | IrExpression::NonTerminal(_) | IrExpression::Special(_)
        );
        parts.push(value);
        let mut value = parts.len() - 1;
        if !leaf {
            r.i += 1;
            r.expect(T_OPEN)?;
            open.push(value);
            continue;
        }
        loop {
            let Some(&parent) = open.last() else {
                return Ok(value);
            };
            match &mut parts[parent] {
                IrExpression::Optional(e)
                | IrExpression::OneOrMore(e)
                | IrExpression::ZeroOrMore(e) => {
                    *e = Some(value);
                }
                IrExpression::Sequence(list) | IrExpression::Choice(list) => {
                    list.push(value);
                    if r.accept(T_COMMA) {
                        break;
                    }
                }
                _ => unreachable!("a leaf is never open"),
            }
            r.expect(T_CLOSE)?;
            open.pop();
            value = parent;
        }
    }
}

/// Parses a railroad-beta text into its syntax tree, as Pele's parseIr does.
pub(crate) fn parse_ir(src: &str) -> Result<IrAst, Error> {
    parse_grammar(src, table(), T_TITLE, unescape, |r, parts| {
        let name = r.expect(T_ID)?.to_string();
        r.expect(T_EQUALS)?;
        let definition = expression(r, parts)?;
        r.expect(T_SEMICOLON)?;
        Ok(Rule { name, definition })
    })
}

/// The model's node of an expression, of the nodes its parts were built into, as Pele's node
/// builds it.
fn node(nodes: &mut Vec<RailroadNode>, e: &IrExpression, kids: Vec<usize>) -> usize {
    match e {
        IrExpression::Terminal(v) => add(nodes, RailroadNode::Terminal(v.clone())),
        IrExpression::NonTerminal(v) => add(nodes, RailroadNode::Nonterminal(v.clone())),
        IrExpression::Special(v) => add(nodes, RailroadNode::Special(v.clone())),
        IrExpression::Sequence(_) => sequence(nodes, kids),
        IrExpression::Choice(_) => choice(nodes, kids),
        IrExpression::Optional(_) => optional(nodes, kids[0]),
        IrExpression::OneOrMore(_) => repetition(nodes, kids[0], 1.0, f64::INFINITY),
        IrExpression::ZeroOrMore(_) => repetition(nodes, kids[0], 0.0, f64::INFINITY),
    }
}

/// The rules of a railroad-beta syntax tree in the common model, Pele's irRules.
pub(crate) fn ir_rules(ast: &IrAst, nodes: &mut Vec<RailroadNode>) -> Vec<RailroadRule> {
    to_rules(ast, nodes, IrExpression::parts, node)
}
