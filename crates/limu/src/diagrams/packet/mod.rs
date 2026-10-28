//! Packet diagrams, as Pele's src/diagrams/packet reads and draws them: a tokenizer and a
//! parser that read what Mermaid's Langium grammar reads, the model that splits the fields
//! into rows, and a layout of the rows.
//!
//! Bits are numbers as JavaScript's are, f64, as a field may be written wider than any
//! integer holds.

mod model;
mod parser;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use model::PacketModel;
pub(crate) use render::layout;

use crate::errors::Error;
use crate::value::Map;

/// Parses a packet diagram's text, as Pele's packet.parse does. title is the one its front
/// matter gives.
pub(crate) fn parse(source: &str, config: &Map, title: Option<&str>) -> Result<PacketModel, Error> {
    model::build_packet(parser::parse_packet(source)?, config, title)
}
