//! Entity relationship diagrams, as Pele's src/diagrams/er reads them: a lexer and a parser
//! that agree with Mermaid's erDiagram.jison, the ErDb they fill, the graph that is drawn of
//! it, and its layout. Its markers are the scene's, as Pele's markers.ts draws them.

mod db;
mod graph;
mod lexer;
mod parser;
mod render;
mod tokens;

#[cfg(test)]
mod tests;

pub(crate) use db::ErDb;
pub(crate) use render::layout;

use parser::parse_er;

use crate::errors::Error;

/// Parses an entity relationship diagram's text, as Pele's er.parse does. title is the one
/// its front matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<ErDb, Error> {
    let mut db = ErDb::default();
    if let Some(title) = title.filter(|t| !t.is_empty()) {
        db.title = Some(title.to_string());
    }
    parse_er(source, &mut db)?;
    Ok(db)
}
