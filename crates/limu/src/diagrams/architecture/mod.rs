//! Architecture diagrams, as Pele's src/diagrams/architecture reads and draws them: a tokenizer
//! and a parser that read what Mermaid's Langium grammar reads, the db that checks what the
//! text declares, a placement of the services, junctions and groups on a grid of cells as the
//! edges' sides ask, and a layout that sizes the grid's rows and columns and draws the edges
//! in orthogonal routes between the sides they name.

mod db;
mod icons;
mod layout;
mod parser;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use db::ArchitectureDb;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses an architecture diagram's text, as Pele's architecture.parse does. title is the one
/// its front matter gives, which a title the text gives takes the place of.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<ArchitectureDb, Error> {
    let mut db = ArchitectureDb::default();
    db.title = title.filter(|t| !t.is_empty()).map(str::to_string);
    db::populate(parser::parse_architecture(source)?, &mut db)?;
    Ok(db)
}
