//! Pie charts, as Pele's src/diagrams/pie reads and draws them: a tokenizer and a parser that
//! read what Mermaid's Langium grammar reads, the model of their tree, and a layout of the
//! slices, their percentages and a legend.
//!
//! Values are numbers as JavaScript's are, f64, as a value may be written with more digits
//! than any f64 holds.

mod model;
mod parser;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use model::PieModel;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses a pie chart's text, as Pele's pie.parse does. title is the one its front matter
/// gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<PieModel, Error> {
    model::build_pie(parser::parse_pie(source)?, title)
}
