//! Block diagrams, as Pele's src/diagrams/block reads and draws them: blocks set out in the
//! columns of their composites, arrow blocks, and edges between blocks.

mod arrows;
mod db;
mod layout;
mod lexer;
mod parser;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use db::BlockDb;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses a block diagram's text, as the parse of Pele's block does. title is the one its
/// front matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<BlockDb, Error> {
    let mut db = BlockDb::default();
    if let Some(title) = title.filter(|t| !t.is_empty()) {
        db.title = Some(title.to_string());
    }
    parser::parse_block(source, &mut db)?;
    Ok(db)
}
