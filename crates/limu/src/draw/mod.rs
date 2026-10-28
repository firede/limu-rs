//! What the diagrams laid out in boxes and edges share in drawing them, as Pele's src/svg has
//! it: the shapes of nodes, and how edges meet them.

mod shapes;

pub(crate) use shapes::*;

#[cfg(test)]
mod tests;
