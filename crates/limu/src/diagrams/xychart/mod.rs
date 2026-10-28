//! XY charts, as Pele's src/diagrams/xychart reads and draws them: a lexer and a parser that
//! agree with Mermaid's xychart.jison, the XyChartDb they fill, and a layout of the axes, the
//! bars and lines of each series, and a legend.
//!
//! Values are numbers as JavaScript's are, f64, as Number reads them from the text.

mod db;
mod lexer;
mod parser;
mod render;
mod ticks;
mod tokens;

#[cfg(test)]
mod tests;

pub(crate) use db::XyChartDb;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses an XY chart's text, as Pele's xychart.parse does. title is the one its front
/// matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<XyChartDb, Error> {
    let mut db = XyChartDb {
        title: title.map(str::to_string),
        ..XyChartDb::default()
    };
    parser::parse_xy_chart(source, &mut db)?;
    Ok(db)
}
