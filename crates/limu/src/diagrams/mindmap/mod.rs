//! Mindmaps, as Pele's src/diagrams/mindmap reads and draws them: an indented outline read
//! with the grammar mindmaps share with kanban boards, made into a tree of nodes, and laid out
//! with the first-level branches spread to both sides of the root, or, in a narrow host, as an
//! outline running down.

mod db;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use db::MindmapDb;
pub(crate) use render::layout;

use crate::common::outline::parse_outline;
use crate::errors::Error;

/// A mindmap's tokens, as Pele's tokenize in parser.ts reads them.
#[cfg(test)]
fn tokenize(src: &str) -> crate::common::outline::Tokens {
    crate::common::outline::tokenize(src, false)
}

/// Parses a mindmap's text into a DB, as Pele's parseMindmap does.
fn parse_mindmap(src: &str, db: &mut MindmapDb) -> Result<(), Error> {
    parse_outline(src, db, false)
}

/// Parses a mindmap's text, as Pele's mindmap.parse does. title is the one its front matter
/// gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<MindmapDb, Error> {
    let mut db = MindmapDb::default();
    db.title = title.map(str::to_string);
    parse_mindmap(source, &mut db)?;
    Ok(db)
}
