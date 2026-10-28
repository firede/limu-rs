//! What the diagrams laid out in boxes and edges share in drawing them, as Pele's src/svg has
//! it: the shapes of nodes, the paths of edges and how they meet the nodes, and where
//! clusters' titles sit.

mod cluster;
mod edges;
mod shapes;

pub(crate) use cluster::*;
pub(crate) use edges::*;
pub(crate) use shapes::*;

#[cfg(test)]
mod tests;
