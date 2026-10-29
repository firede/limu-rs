//! Event modeling diagrams, as Pele's src/diagrams/eventmodeling reads and draws them: a
//! tokenizer and a parser that read what Mermaid's Langium grammar reads, a model of frames
//! in lanes and the relations between them, and a layout across, or down when the host is
//! narrow.

mod model;
mod parser;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use model::EventModelingModel;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses an event modeling diagram's text, as the parse of Pele's eventmodeling does. title
/// is the one its front matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<EventModelingModel, Error> {
    let ast = parser::parse_event_model(source)?;
    model::build_event_model(&ast, source, title)
}
