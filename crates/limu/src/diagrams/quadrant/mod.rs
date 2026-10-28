//! Quadrant charts, as Pele's src/diagrams/quadrant reads and draws them: a lexer and a parser
//! that agree with Mermaid's quadrant.jison, the QuadrantDb they fill, and a layout of the
//! quadrants, the axes and the points.

mod db;
mod lexer;
mod parser;
mod render;
mod tokens;

#[cfg(test)]
mod tests;

pub(crate) use db::QuadrantDb;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses a quadrant chart's text, as Pele's quadrantChart.parse does. title is the one its
/// front matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<QuadrantDb, Error> {
    let mut db = QuadrantDb {
        title: title.map(str::to_string),
        ..QuadrantDb::default()
    };
    parser::parse_quadrant(source, &mut db)?;
    Ok(db)
}
