//! Flowcharts, as Pele's src/diagrams/flowchart reads them: a lexer and a parser that agree
//! with Mermaid's flow.jison, the FlowDb they fill, and the graph that is drawn of it.

mod db;
mod graph;
mod lexer;
mod parser;
mod render;
mod shapes;
mod tokens;

#[cfg(test)]
mod tests;

pub(crate) use db::FlowDb;
pub(crate) use graph::{FlowGraph, GraphNode, build_flow_graph};
pub(crate) use render::{Roles, Variant, layout, layout_variant};

use db::FlowDbOptions;
use parser::parse_flowchart;

use crate::errors::Error;
use crate::value::{Map, Value};

/// The flowchart section of a configuration, or none where it is no object.
fn flowchart_config(config: &Map) -> Option<&Map> {
    config.get("flowchart").and_then(Value::as_object)
}

/// Parses a flowchart's text, as Pele's flowchart.parse does. title is the one its front
/// matter gives.
pub(crate) fn parse(
    source: &str,
    config: &Map,
    title: Option<&str>,
    max_edges: Option<usize>,
) -> Result<FlowDb, Error> {
    let inherit_dir =
        flowchart_config(config).and_then(|f| f.get("inheritDir")) == Some(&Value::Bool(true));
    let mut db = FlowDb::new(FlowDbOptions {
        max_edges,
        inherit_dir,
    });
    if let Some(title) = title.filter(|t| !t.is_empty()) {
        db.title = Some(title.to_string());
    }
    parse_flowchart(source, &mut db)?;
    Ok(db)
}

/// The curve a flowchart's configuration gives its edges, where it is a string, as Pele's
/// renderFlowchart reads it.
pub(crate) fn default_curve(config: &Map) -> Option<&str> {
    flowchart_config(config)
        .and_then(|f| f.get("curve"))
        .and_then(Value::as_str)
}
