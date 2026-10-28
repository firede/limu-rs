//! Ishikawa (fishbone) diagrams, as Pele's src/diagrams/ishikawa reads and draws them: a
//! lexer and a parser that make the decisions Mermaid's Jison parser makes, the db they
//! fill, which reads the causes as a tree by their indentation, and a layout that draws the
//! effect at the head of a spine and the causes as bones off it, or, in a narrow host, as an
//! outline that runs down.

mod db;
mod lexer;
mod parser;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use db::IshikawaDb;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses an Ishikawa diagram's text, as the parse of Pele's ishikawa does. title is the one
/// its front matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<IshikawaDb, Error> {
    let mut db = IshikawaDb::new(title);
    parser::parse_ishikawa(source, &mut db)?;
    Ok(db)
}
