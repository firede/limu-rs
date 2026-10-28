//! Treemaps, as Pele's src/diagrams/treemap reads and draws them: a tokenizer and a parser
//! that read what Mermaid's Langium grammar reads, the model of the tree its rows nest into
//! by their indentation, a squarified layout of the tree, and a scene of its sections and
//! leaves, with their values written as a specifier of d3-format says.

mod format;
mod layout;
mod model;
mod parser;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use model::TreemapModel;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses a treemap's text, as the parse of Pele's treemap diagram does. title is the one its
/// front matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<TreemapModel, Error> {
    Ok(model::build_treemap(parser::parse_treemap(source)?, title))
}
