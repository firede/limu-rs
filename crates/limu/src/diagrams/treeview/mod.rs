//! Tree views, as Pele's src/diagrams/treeview reads and draws them: a preprocessor that reads
//! a tree drawn with box-drawing characters as one indented, a tokenizer and a parser that
//! read what Mermaid's Langium grammar reads, a database that nests the entries by their
//! indentation, and a layout of a row to an entry.

mod boxdrawing;
mod icons;
mod model;
mod parser;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use model::TreeViewModel;
pub(crate) use render::layout;

use crate::errors::Error;

/// Parses a tree view's text, as the parse of Pele's treeView does. title is the one its
/// front matter gives.
pub(crate) fn parse(source: &str, title: Option<&str>) -> Result<TreeViewModel, Error> {
    let ast = model::read_tree_view(source)?;
    Ok(model::build_tree_view(&ast, title))
}
