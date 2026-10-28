//! Venn diagrams, as Pele's src/diagrams/venn reads and draws them: a lexer and a parser that
//! make the decisions Mermaid's Jison grammar makes, the db they fill, and a layout that
//! places a circle for each set, with areas in proportion to the sizes of the sets and each
//! pair as far apart as its overlap asks, and writes each region's label and texts where the
//! region has the most room.

mod db;
mod layout;
mod lexer;
mod parser;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use db::VennDb;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses a venn diagram's text, as the parse of Pele's venn does. title is the one its front
/// matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<VennDb, Error> {
    let mut db = VennDb::new(title);
    parser::parse_venn(source, &mut db)?;
    Ok(db)
}
