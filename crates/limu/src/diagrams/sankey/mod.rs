//! Sankey diagrams, as Pele's src/diagrams/sankey reads and draws them: a lexer and a parser
//! of the CSV that Mermaid's Jison grammar reads, the model they fill, and a layout that
//! places the nodes in columns by their flow and draws the bands between them, with a label
//! beside each node.
//!
//! Offsets into the text are bytes. The columns of errors are UTF-16 code units, as Pele's
//! are.

mod db;
mod layout;
mod lexer;
mod parser;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use db::SankeyDb;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses a sankey diagram's text, as the parse of Pele's sankey diagram does. title is the
/// one its front matter gives. The text is prepared as Pele's prepareTextForParsing prepares
/// it before it is parsed, and an error is placed in the text before it was prepared.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<SankeyDb, Error> {
    let mut db = SankeyDb::default();
    db.title = title.filter(|t| !t.is_empty()).map(str::to_string);
    let src = parser::prepare_text_for_parsing(source, None);
    parser::parse_sankey(&src, &mut db, source)?;
    Ok(db)
}
