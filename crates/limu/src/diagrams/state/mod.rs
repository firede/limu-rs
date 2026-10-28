//! State diagrams, as Pele's src/diagrams/state reads them: a lexer and a parser that agree
//! with Mermaid's stateDiagram.jison, the StateDb they fill, the graph it resolves into, and
//! the layout of that graph as a scene.

mod db;
mod graph;
mod lexer;
mod parser;
mod render;
mod tokens;

#[cfg(test)]
mod tests;

pub(crate) use db::StateDb;
pub(crate) use render::layout;

use crate::errors::Error;
use parser::parse_state;

/// Parses a state diagram's text, as Pele's state.parse does. title is the one its front
/// matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<StateDb, Error> {
    let mut db = StateDb::default();
    if let Some(title) = title.filter(|t| !t.is_empty()) {
        db.title = Some(title.to_string());
    }
    parse_state(source, &mut db)?;
    Ok(db)
}
