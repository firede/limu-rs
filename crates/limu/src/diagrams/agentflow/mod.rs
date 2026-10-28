//! Agent flows, as Pele's src/diagrams/agentflow reads them: a lexer and a parser that agree
//! with Mermaid's agentflow.jison, the AgentflowDb they fill, and the graph that is drawn of
//! it. An agent flow is a flowchart of an agent's work, and takes its shapes from a
//! flowchart's.

mod color_slots;
mod db;
mod diagnostics;
mod graph;
mod lexer;
mod parser;
mod render;
mod shapes;
mod tokens;

#[cfg(test)]
mod tests;

pub(crate) use db::AgentflowDb;
pub(crate) use render::layout;

use db::AgentflowDbOptions;
use graph::build_agent_graph;
use parser::parse_agentflow;

use crate::errors::Error;
use crate::value::{Map, Value};

/// The number of series colors, the palette Mermaid's rules of slots are applied to, Pele's
/// PALETTE.
const PALETTE: usize = 8;

/// The curve a configuration gives the edges, where it is a string, as Pele's agentGraph
/// reads it from the flowchart section.
fn default_curve(config: &Map) -> Option<&str> {
    config
        .get("flowchart")
        .and_then(Value::as_object)
        .and_then(|f| f.get("curve"))
        .and_then(Value::as_str)
}

/// Parses an agent flow's text, as Pele's agentflow.parse does, and builds the graph that is
/// drawn of it, which records what is found wrong with its shapes and its nesting. title is
/// the one its front matter gives, and line_offset the lines of front matter before source.
pub(crate) fn parse(
    source: &str,
    config: &Map,
    title: Option<&str>,
    line_offset: usize,
    max_edges: Option<usize>,
) -> Result<AgentflowDb, Error> {
    // Mermaid reads this setting from the flowchart section for agent flows too.
    let inherit_dir = config
        .get("flowchart")
        .and_then(Value::as_object)
        .and_then(|f| f.get("inheritDir"))
        == Some(&Value::Bool(true));
    let mut db = AgentflowDb::new(AgentflowDbOptions {
        max_edges,
        inherit_dir,
        direction: config
            .get("direction")
            .and_then(Value::as_str)
            .map(str::to_string),
        line_offset,
        source: Some(source.to_string()),
    });
    if let Some(title) = title.filter(|t| !t.is_empty()) {
        db.title = Some(title.to_string());
    }
    parse_agentflow(source, &mut db)?;
    let graph = build_agent_graph(&mut db, PALETTE, default_curve(config));
    db.graph = Some(graph);
    Ok(db)
}
