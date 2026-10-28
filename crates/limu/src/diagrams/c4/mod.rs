//! C4 diagrams, as Pele's src/diagrams/c4 reads and draws them: a lexer and a parser that
//! read what Mermaid's c4Diagram.jison reads, the model they build, the grid of shapes and
//! boundaries Mermaid lays them out in, the routes of the relations between them, and the
//! scene drawn of it all.

mod db;
mod layout;
mod lexer;
mod parser;
mod render;
mod route;

#[cfg(test)]
mod tests;

pub(crate) use db::C4Db;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses a C4 diagram's text, as the parse of Pele's C4 diagram does. title is the one its
/// front matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<C4Db, Error> {
    let mut db = C4Db::default();
    if let Some(title) = title.filter(|t| !t.is_empty()) {
        db.title = Some(title.to_string());
    }
    parser::parse_c4(source, &mut db)?;
    Ok(db)
}
