//! Class diagrams, as Pele's src/diagrams/class reads them: a lexer and a parser that agree
//! with Mermaid's classDiagram.jison, the ClassDb they fill, the members of its classes, and
//! the graph that is drawn of it.

mod db;
mod graph;
mod lexer;
mod members;
mod parser;
mod render;
mod tokens;

#[cfg(test)]
mod tests;

pub(crate) use db::ClassDb;
pub(crate) use render::layout;

use parser::parse_class_diagram;

use crate::errors::Error;
use crate::value::{Map, Value};

/// The class section of a configuration, or none where it is no object.
fn class_config(config: &Map) -> Option<&Map> {
    config.get("class").and_then(Value::as_object)
}

/// Whether a class diagram's configuration nests its namespaces, as Pele's render reads it
/// for buildClassGraph: unless class.hierarchicalNamespaces is false.
fn hierarchical(config: &Map) -> bool {
    class_config(config).and_then(|c| c.get("hierarchicalNamespaces")) != Some(&Value::Bool(false))
}

/// Parses a class diagram's text, as the parse of Pele's classDiagram does. title is the one
/// its front matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<ClassDb, Error> {
    let mut db = ClassDb::default();
    if let Some(title) = title.filter(|t| !t.is_empty()) {
        db.title = Some(title.to_string());
    }
    parse_class_diagram(source, &mut db)?;
    Ok(db)
}
