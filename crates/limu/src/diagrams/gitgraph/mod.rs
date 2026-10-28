//! Git graphs, as Pele's src/diagrams/git reads and draws them: a tokenizer and a parser that
//! read what Mermaid's Langium grammar reads, the replay of the statements into a model of
//! commits and branches, and a layout of the commits along the lanes of their branches.

mod db;
mod parser;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use db::GitModel;
pub(crate) use render::layout;

use crate::errors::Error;
use crate::value::Map;

/// Parses a git graph's text, as Pele's gitGraph.parse does. title is the one its front
/// matter gives.
pub(crate) fn parse(source: &str, config: &Map, title: Option<&str>) -> Result<GitModel, Error> {
    db::build_git(parser::parse_git(source)?, config, title)
}
