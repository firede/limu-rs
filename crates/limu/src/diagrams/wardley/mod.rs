//! Wardley maps, as Pele's src/diagrams/wardley reads and draws them: a tokenizer and a
//! parser that read what Mermaid's Langium grammar reads, populate and the builder, which
//! turn the tree into the map's model, and a layout of the plot, its stages and axes, and on
//! it the pipelines, links, trends, nodes, annotations, notes and forces.
//!
//! Places are percentages, as Pele keeps them: a node's x is its evolution and its y its
//! visibility, each from 0 to 100.

mod builder;
mod model;
mod parser;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use model::WardleyModel;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses a Wardley map's text, as the parse of Pele's wardley diagram does. title is the one
/// its front matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<WardleyModel, Error> {
    model::build_wardley(parser::parse_wardley(source)?, title)
}
