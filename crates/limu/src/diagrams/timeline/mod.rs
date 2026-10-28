//! Timelines, as Pele's src/diagrams/timeline reads and draws them: a lexer and a parser that
//! make the decisions Mermaid's Jison parser makes, the db they fill, and a layout of the
//! periods along an axis, across or down.

mod db;
mod lexer;
mod parser;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use db::TimelineDb;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses a timeline's text, as the parse of Pele's timeline does. title is the one its front
/// matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<TimelineDb, Error> {
    let mut db = TimelineDb::new(title);
    parser::parse_timeline(source, &mut db)?;
    Ok(db)
}
