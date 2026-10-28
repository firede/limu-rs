//! What a sankey diagram's parser fills, as Pele's SankeyDb in db.ts has it.

use std::collections::HashMap;

/// A node, Pele's SankeyNode: its id as written, with the entities of the text encoded as
/// Pele's preprocessing leaves them, and its place among the nodes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SankeyNode {
    pub(crate) id: String,
    pub(crate) index: usize,
}

/// A band of flow from one node to another, Pele's SankeyLink, with its nodes by their place
/// among the nodes. value is what parseFloat reads of the third field, which is always
/// finite, as a row whose value is not is an error.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SankeyLink {
    pub(crate) source: usize,
    pub(crate) target: usize,
    pub(crate) value: f64,
}

/// A sankey diagram as its parser fills it, Pele's SankeyDb. Its grammar has no accessible
/// title or description, so Pele's accTitle and accDescr are always undefined and are left
/// out.
#[derive(Clone, Debug, Default)]
pub(crate) struct SankeyDb {
    pub(crate) nodes: Vec<SankeyNode>,
    pub(crate) links: Vec<SankeyLink>,
    pub(crate) title: Option<String>,
    lookup: HashMap<String, usize>,
}

impl SankeyDb {
    /// The node of an id, added after the others the first time the id is seen, as Pele's
    /// findOrCreateNode does.
    pub(crate) fn find_or_create_node(&mut self, id: &str) -> usize {
        if let Some(&i) = self.lookup.get(id) {
            return i;
        }
        let i = self.nodes.len();
        self.lookup.insert(id.to_string(), i);
        self.nodes.push(SankeyNode {
            id: id.to_string(),
            index: i,
        });
        i
    }

    pub(crate) fn add_link(&mut self, source: usize, target: usize, value: f64) {
        self.links.push(SankeyLink {
            source,
            target,
            value,
        });
    }
}
