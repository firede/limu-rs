//! Info diagrams, as Pele's src/diagrams/info reads and draws them: a tokenizer and a parser
//! that read what Mermaid's Langium grammar reads, the model of their tree, and a layout that
//! draws the renderer's name and version under the title, where there is one.

mod model;
mod parser;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use model::InfoModel;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses an info diagram's text, as Pele's info.parse does. title is the one its front
/// matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<InfoModel, Error> {
    Ok(model::build_info(parser::parse_info(source)?, title))
}
