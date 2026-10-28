//! Kanban boards, as Pele's src/diagrams/kanban reads and draws them: an outline of columns,
//! each with its cards, and the cards' metadata of tickets, assignees and priorities.

mod db;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use db::KanbanDb;
pub(crate) use render::layout;

use crate::common::outline::parse_outline;
use crate::errors::Error;

/// A kanban board's tokens, as Pele's tokenize in parser.ts reads them.
#[cfg(test)]
fn tokenize(src: &str) -> crate::common::outline::Tokens {
    crate::common::outline::tokenize(src, true)
}

/// Parses a kanban board's text into a DB, as Pele's parseKanban does.
fn parse_kanban(src: &str, db: &mut KanbanDb) -> Result<(), Error> {
    parse_outline(src, db, true)
}

/// Parses a kanban board's text, as Pele's kanban.parse does. title is the one its front
/// matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<KanbanDb, Error> {
    let mut db = KanbanDb::default();
    db.title = title.map(str::to_string);
    parse_kanban(source, &mut db)?;
    Ok(db)
}
