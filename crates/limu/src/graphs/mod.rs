//! The graph layouts the diagrams share, as Pele's src/layout has them: the layered layout of
//! one flat graph, the compound layout of a graph whose nodes are nested in groups, the lane
//! layout of swimlanes, and the wrapping of a graph's disconnected parts into rows.

// The ports go over arrays by index, as Pele's code does, so that the two read side by side.
#![allow(clippy::needless_range_loop)]

pub(crate) mod components;
pub(crate) mod compound;
pub(crate) mod lanes;
pub(crate) mod layered;

#[cfg(test)]
mod tests;
