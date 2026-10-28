//! Use case diagrams, as Pele's src/diagrams/usecase reads and draws them: a lexer and a
//! parser that agree with Mermaid's Chevrotain grammar, the model they resolve, and the graph
//! that is drawn of it.

mod db;
mod graph;
mod json;
mod lexer;
mod ordered;
mod parser;
mod render;
mod tokens;
mod types;

#[cfg(test)]
mod tests;

pub(crate) use render::layout;
pub(crate) use types::UsecaseModel;

use crate::errors::Error;

/// Parses a use case diagram's text, as Pele's usecase.parse does. title is the one its front
/// matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<UsecaseModel, Error> {
    parser::parse_usecase(source, title)
}
