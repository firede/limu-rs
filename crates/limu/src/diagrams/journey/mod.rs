//! User journeys, as Pele's src/diagrams/journey reads and draws them: a lexer and a parser
//! that make the decisions Mermaid's Jison parser makes, the db they fill, and a layout of
//! the tasks as cards with their scores as faces on a chart under them, or as cards down.

mod db;
mod lexer;
mod parser;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use db::JourneyDb;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses a user journey's text, as the parse of Pele's journey does. title is the one its
/// front matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<JourneyDb, Error> {
    let mut db = JourneyDb::new(title);
    parser::parse_journey(source, &mut db)?;
    Ok(db)
}
