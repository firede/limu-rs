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

// The parsers and layouts are ported after the parts they use.
#![allow(dead_code, unused_imports)]

pub use limu_scene as scene;

mod common;
mod css;
mod detect;
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
pub use errors::{Error, ErrorCode};
pub use prepare::{DEFAULT_LIMIT, DEFAULT_MAX_EDGES, ParseOptions};
pub use value::{Map, Value};
