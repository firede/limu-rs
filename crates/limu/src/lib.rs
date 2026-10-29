//! limu reads Mermaid diagrams and lays them out, for programs that draw them themselves: in
//! a window, in a terminal, as SVG.
//!
//! A diagram goes through three stages. Parsing reads the diagram's text into a model of it.
//! Layout places the model's parts, measuring text with a measurer the host gives, into a
//! scene, which says what each part is and where it is, but not how it looks. A backend draws
//! the scene, settling its colors from a style's look. Style is not layout, so a scene is
//! drawn in any theme without being laid out again.
//!
//! Much of limu is ported from Pele, the Mermaid renderer of the Obsidian team, and its tests
//! compare what limu does with what Pele does, number for number.

// Where only some types of diagram are built, the parts the others alone use go unused.
// Unused code is looked for where every type is built.
#![cfg_attr(
    not(all(
        feature = "agentflow",
        feature = "architecture",
        feature = "block",
        feature = "c4",
        feature = "class",
        feature = "cynefin",
        feature = "er",
        feature = "eventmodeling",
        feature = "flowchart",
        feature = "gantt",
        feature = "gitgraph",
        feature = "info",
        feature = "ishikawa",
        feature = "journey",
        feature = "kanban",
        feature = "mindmap",
        feature = "packet",
        feature = "pie",
        feature = "quadrant",
        feature = "radar",
        feature = "railroad",
        feature = "requirement",
        feature = "sankey",
        feature = "sequence",
        feature = "state",
        feature = "swimlane",
        feature = "timeline",
        feature = "treemap",
        feature = "treeview",
        feature = "usecase",
        feature = "venn",
        feature = "wardley",
        feature = "xychart",
    )),
    allow(dead_code, unused_imports)
)]

pub use limu_scene as scene;

mod common;
mod css;
mod detect;
mod diagram;
mod diagrams;
mod draw;
mod errors;
mod graphs;
mod js;
pub mod layout;
mod prepare;
mod preprocess;
pub mod text;
mod url;
mod value;
mod yaml;

#[cfg(test)]
mod testing;

pub use detect::DiagramType;
pub use diagram::{Diagram, layout, parse, supports, types};
pub use errors::{Error, ErrorCode};
pub use prepare::{DEFAULT_LIMIT, DEFAULT_MAX_EDGES, ParseOptions};
pub use value::{Map, Value};
