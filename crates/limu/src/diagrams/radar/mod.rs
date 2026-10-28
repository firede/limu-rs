//! Radar charts, as Pele's src/diagrams/radar reads and draws them: a tokenizer and a parser
//! that read what Mermaid's Langium grammar reads, the model of their tree, which puts each
//! curve's values in the order of the axes, and a layout of a graticule, the axes and their
//! labels, a curve for each series, and a legend.
//!
//! Values are numbers as JavaScript's are, f64, as a value may be written with more digits
//! than any f64 holds.

mod model;
mod parser;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use model::RadarModel;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses a radar chart's text, as the parse of Pele's radar diagram does. title is the one
/// its front matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<RadarModel, Error> {
    model::build_radar(parser::parse_radar(source)?, title)
}
