//! Requirement diagrams, as Pele's src/diagrams/requirement reads them: a lexer and a parser
//! that agree with Mermaid's requirementDiagram.jison, the RequirementDb they fill, the graph
//! that is drawn of it, and its layout.

mod db;
mod graph;
mod lexer;
mod parser;
mod render;
mod tokens;

#[cfg(test)]
mod tests;

pub(crate) use db::RequirementDb;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses a requirement diagram's text, as Pele's requirement.parse does. title is the one
/// its front matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<RequirementDb, Error> {
    let mut db = RequirementDb::default();
    if let Some(title) = title.filter(|t| !t.is_empty()) {
        db.title = Some(title.to_string());
    }
    parser::parse_requirement(source, &mut db)?;
    Ok(db)
}
