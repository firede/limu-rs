//! The model of a railroad diagram, as Pele's types.ts and db.ts have it: its rules in one
//! model whatever the notation they were written in.

use std::collections::HashMap;

use super::abnf::{abnf_rules, parse_abnf};
use super::common::Parsed;
use super::ebnf::{ebnf_rules, parse_ebnf};
use super::ir::{ir_rules, parse_ir};
use super::peg::{parse_peg, peg_rules};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::{drop_indents, has_prefix_fold, space_run, trim_start};

/// A part of a rule, Pele's RailroadNode. Its parts are the nodes at their indices in the
/// model's nodes, so that a tree of any depth is kept and dropped without recursion.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RailroadNode {
    Terminal(String),
    Nonterminal(String),
    Special(String),
    Sequence(Vec<usize>),
    Choice(Vec<usize>),
    Optional(usize),
    /// max is infinite where the count has no upper bound.
    Repetition {
        element: usize,
        min: f64,
        max: f64,
    },
}

impl RailroadNode {
    /// The node's parts, as Pele's parts gives them.
    pub(crate) fn parts(&self) -> Vec<usize> {
        match self {
            RailroadNode::Sequence(kids) | RailroadNode::Choice(kids) => kids.clone(),
            RailroadNode::Optional(element) | RailroadNode::Repetition { element, .. } => {
                vec![*element]
            }
            _ => Vec::new(),
        }
    }
}

/// A rule, Pele's RailroadRule: its name and the node of its definition.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RailroadRule {
    pub(crate) name: String,
    pub(crate) definition: usize,
}

/// Which of Mermaid's four grammars the text was written in, Pele's Notation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Notation {
    Railroad,
    Ebnf,
    Abnf,
    Peg,
}

impl Notation {
    /// The notation's name, as Pele writes it.
    #[cfg(test)]
    pub(crate) fn name(self) -> &'static str {
        match self {
            Notation::Railroad => "railroad",
            Notation::Ebnf => "ebnf",
            Notation::Abnf => "abnf",
            Notation::Peg => "peg",
        }
    }
}

/// A railroad diagram's model, Pele's RailroadDb.
#[derive(Clone, Debug)]
pub(crate) struct RailroadDb {
    pub(crate) notation: Notation,
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    /// Every rule in source order. A name defined twice appears twice.
    pub(crate) rules: Vec<RailroadRule>,
    /// The nodes the rules are made of.
    pub(crate) nodes: Vec<RailroadNode>,
    by_name: HashMap<String, usize>,
}

impl RailroadDb {
    fn new() -> RailroadDb {
        RailroadDb {
            notation: Notation::Railroad,
            title: None,
            acc_title: None,
            acc_descr: None,
            rules: Vec::new(),
            nodes: Vec::new(),
            by_name: HashMap::new(),
        }
    }

    fn set_title(&mut self, text: &str) {
        self.title = Some(text.to_string());
    }

    fn set_acc_title(&mut self, text: &str) {
        self.acc_title = Some(trim_start(text).to_string());
    }

    fn set_acc_description(&mut self, text: &str) {
        self.acc_descr = Some(drop_indents(text));
    }

    /// Adds a rule. A name defined again is drawn again, and looking the name up gives the
    /// later definition.
    fn add_rule(&mut self, rule: RailroadRule) {
        self.by_name.insert(rule.name.clone(), self.rules.len());
        self.rules.push(rule);
    }

    /// The rule of a name, the last defined where it is defined more than once.
    #[cfg(test)]
    pub(crate) fn get_rule(&self, name: &str) -> Option<&RailroadRule> {
        self.by_name.get(name).map(|&i| &self.rules[i])
    }
}

/// The notation a diagram's keyword names, or none where it has no keyword, as Pele's
/// notationOf reads it with /^\s*railroad-(ebnf-|abnf-|peg-)?beta/i. Mermaid has a detector
/// for each keyword, each matching without regard to case.
pub(crate) fn notation_of(source: &str) -> Option<Notation> {
    let rest = &source[space_run(source, 0)..];
    if !has_prefix_fold(rest, "railroad-") {
        return None;
    }
    let rest = &rest["railroad-".len()..];
    for (infix, notation) in [
        ("ebnf-", Notation::Ebnf),
        ("abnf-", Notation::Abnf),
        ("peg-", Notation::Peg),
    ] {
        if has_prefix_fold(rest, infix) && has_prefix_fold(&rest[infix.len()..], "beta") {
            return Some(notation);
        }
    }
    has_prefix_fold(rest, "beta").then_some(Notation::Railroad)
}

/// Parses the text in one notation and gives its statements and rules, as Pele's grammar
/// does. The rules' nodes are added to nodes.
fn grammar(
    source: &str,
    notation: Notation,
    nodes: &mut Vec<RailroadNode>,
) -> Result<(Parsed, Vec<RailroadRule>), Error> {
    Ok(match notation {
        Notation::Ebnf => {
            let ast = parse_ebnf(source)?;
            let rules = ebnf_rules(&ast, nodes);
            (ast.parsed(), rules)
        }
        Notation::Abnf => {
            let ast = parse_abnf(source)?;
            let rules = abnf_rules(&ast, nodes);
            (ast.parsed(), rules)
        }
        Notation::Peg => {
            let ast = parse_peg(source)?;
            let rules = peg_rules(&ast, nodes);
            (ast.parsed(), rules)
        }
        Notation::Railroad => {
            let ast = parse_ir(source)?;
            let rules = ir_rules(&ast, nodes);
            (ast.parsed(), rules)
        }
    })
}

/// Parses the text in one notation and loads the result into the model, as Mermaid's four
/// parsers do and Pele's populate does.
fn populate(db: &mut RailroadDb, source: &str, notation: Notation) -> Result<(), Error> {
    let (ast, rules) = grammar(source, notation, &mut db.nodes)?;
    db.notation = notation;
    if let Some(d) = ast.acc_descr.as_deref().filter(|d| !d.is_empty()) {
        db.set_acc_description(d);
    }
    if let Some(t) = ast.acc_title.as_deref().filter(|t| !t.is_empty()) {
        db.set_acc_title(t);
    }
    if let Some(t) = ast.title.as_deref().filter(|t| !t.is_empty()) {
        db.set_title(t);
    }
    for rule in rules {
        db.add_rule(rule);
    }
    Ok(())
}

/// Parses a railroad diagram, as Pele's parseRailroad does. title is the one its front
/// matter gives.
pub(crate) fn parse_railroad(source: &str, title: Option<&str>) -> Result<RailroadDb, Error> {
    let Some(notation) = notation_of(source) else {
        let mut e = Error::new("No railroad diagram keyword found.", ErrorCode::Syntax);
        e.diagram = Some(DiagramType::Railroad);
        return Err(e);
    };
    let mut db = RailroadDb::new();
    if let Some(t) = title.filter(|t| !t.is_empty()) {
        db.title = Some(t.to_string());
    }
    populate(&mut db, source, notation)?;
    Ok(db)
}
