//! Sequence diagrams, as Pele's src/diagrams/sequence reads them: a lexer and a parser that
//! agree with Mermaid's sequenceDiagram.jison, the SeqDb the statements they read are applied
//! to, and the layout of its participants, messages, notes and blocks as a scene.

mod actors;
mod colors;
mod db;
mod lexer;
mod parser;
mod render;
mod tokens;

#[cfg(test)]
mod tests;

pub(crate) use db::SeqDb;
pub(crate) use render::layout;

use crate::errors::Error;
use crate::value::{Map, Value};
use parser::parse_sequence;

/// Parses a sequence diagram's text, as Pele's sequence.parse does: the configuration's wrap,
/// where it is true or false, and sequence.wrap say whether text wraps where its statement
/// does not say. title is the one its front matter gives.
pub(crate) fn parse(source: &str, config: &Map, title: Option<&str>) -> Result<SeqDb, Error> {
    let seq = config.get("sequence").and_then(Value::as_object);
    let wrap = config.get("wrap").and_then(Value::as_bool);
    let sequence_wrap = seq.and_then(|s| s.get("wrap")) == Some(&Value::Bool(true));
    let mut db = SeqDb::new(wrap, sequence_wrap);
    if let Some(title) = title.filter(|t| !t.is_empty()) {
        db.set_diagram_title(title);
    }
    parse_sequence(source, &mut db)?;
    Ok(db)
}
