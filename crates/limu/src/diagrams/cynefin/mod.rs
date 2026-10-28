//! Cynefin diagrams, as Pele's src/diagrams/cynefin reads and draws them: a tokenizer and a
//! parser that read what Mermaid's Langium grammar reads, the model of their tree, and a
//! layout of the five domains, with the wavy boundaries between them and the transitions from
//! one to another.

mod boundaries;
mod model;
mod parser;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use model::CynefinModel;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses a Cynefin diagram's text, as Pele's cynefin.parse does. title is the one its front
/// matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<CynefinModel, Error> {
    Ok(model::build_cynefin(parser::parse_cynefin(source)?, title))
}
