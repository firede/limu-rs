//! The scene limu lays a diagram out as, its style, and the drawing every backend shares.
//!
//! A scene is a diagram laid out: a list of items, each a thing of the diagram, where it is
//! and what it is, in the order they are drawn, but not how they look. A backend draws a
//! scene, settling its colors from a style's look. This crate is what a backend needs and
//! nothing more, so that a backend depends on it alone. The `limu` crate parses diagrams
//! and lays them out as scenes.

mod flatten;
mod geom;
mod marker;
mod path;
mod pathdata;
mod scene;
pub mod style;
pub mod text;

pub use flatten::{Polyline, dash};
pub use geom::{Point, Rect};
pub use marker::{MARKER_SIZE, Marker, MarkerType};
pub use path::{Op, Path, Step};
pub use pathdata::PathError;
pub use scene::{
    Align, Edge, Grid, Item, Kind, Outline, Painting, Part, Picture, PictureSource, Placed, Role,
    Scene, Shape,
};
