//! The scene limu lays a diagram out as, its style, and the drawing every backend shares.
//!
//! A scene is a diagram laid out: a list of items, each a thing of the diagram, where it is
//! and what it is, in the order they are drawn, but not how they look. A backend draws a
//! scene, settling its colors from a style's look. This crate is what a backend needs and
//! nothing more, so that a backend depends on it alone. The `limu` crate parses diagrams
//! and lays them out as scenes.
