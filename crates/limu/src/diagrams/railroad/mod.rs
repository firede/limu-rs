//! Railroad diagrams, as Pele's src/diagrams/railroad reads and draws them: four notations of
//! a grammar, Mermaid's own and EBNF, ABNF and PEG, each with a tokenizer and a parser that
//! read what Mermaid's Langium grammar reads, turned into one model of rules, and a layout of
//! a track for each rule.

mod abnf;
mod common;
mod db;
mod ebnf;
mod ir;
mod peg;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use db::RailroadDb;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses a railroad diagram's text, as the parse of Pele's railroad does. title is the one
/// its front matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<RailroadDb, Error> {
    db::parse_railroad(source, title)
}
