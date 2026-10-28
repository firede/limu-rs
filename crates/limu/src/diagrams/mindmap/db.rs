//! What a mindmap's parser fills, as Pele's MindmapDb in db.ts has it: a tree of nodes, in
//! the order the text gives them.

use crate::common::outline::{Decoration, OutlineDb};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};

/// A node of the map, Pele's MindmapNode.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MindmapNode {
    /// Its index in the map's nodes.
    pub(crate) id: usize,
    /// The id its text gave it, or its text.
    pub(crate) node_id: String,
    /// Its indentation less the root's, in UTF-16 code units.
    pub(crate) level: usize,
    pub(crate) descr: String,
    /// Its shape, one of Pele's NODE_TYPE.
    pub(crate) kind: usize,
    /// The indexes of its children.
    pub(crate) children: Vec<usize>,
    pub(crate) is_root: bool,
    /// The index of the first-level branch the node is in, wrapping after eleven, as in
    /// Mermaid. The root is in none.
    pub(crate) section: Option<usize>,
    pub(crate) class: Option<String>,
    pub(crate) icon: Option<String>,
}

/// The branches before their colors start again, Pele's SECTIONS.
const SECTIONS: usize = 11;

/// A mindmap as its parser fills it, Pele's MindmapDb.
#[derive(Clone, Debug, Default)]
pub(crate) struct MindmapDb {
    pub(crate) title: Option<String>,
    /// In source order, which is depth first. The first node is the root.
    pub(crate) nodes: Vec<MindmapNode>,
    base_level: usize,
    /// The chain of nodes that can still take children, with levels strictly rising.
    open: Vec<usize>,
}

impl OutlineDb for MindmapDb {
    /// Adds a node under the last node with a smaller indentation, wherever that is in the
    /// tree, as Pele's addNode does after Mermaid.
    fn add_node(
        &mut self,
        level: usize,
        id: &str,
        descr: &str,
        kind: usize,
        _shape_data: Option<&str>,
    ) -> Result<(), Error> {
        let is_root = self.nodes.is_empty();
        if is_root {
            self.base_level = level;
        }
        // A node indented less than the root is at a level below none, which no open node
        // is at or above, as JavaScript's negative level is.
        let level = level.checked_sub(self.base_level);

        let mut depth = self.open.len();
        while depth > 0 && level.is_none_or(|l| self.nodes[self.open[depth - 1]].level >= l) {
            depth -= 1;
        }
        let parent = depth.checked_sub(1).map(|d| self.open[d]);
        if parent.is_none() && !is_root {
            let mut e = Error::new(
                format!("There can be only one root. No parent could be found for (\"{descr}\")"),
                ErrorCode::Semantic,
            );
            e.diagram = Some(DiagramType::Mindmap);
            return Err(e);
        }
        let id_at = self.nodes.len();
        let mut node = MindmapNode {
            id: id_at,
            node_id: id.to_string(),
            level: level.unwrap_or(0),
            descr: descr.to_string(),
            kind,
            children: Vec::new(),
            is_root,
            section: None,
            class: None,
            icon: None,
        };
        if let Some(p) = parent {
            let parent = &mut self.nodes[p];
            node.section = if parent.is_root {
                Some(parent.children.len() % SECTIONS)
            } else {
                parent.section
            };
            parent.children.push(id_at);
        }
        self.open.truncate(depth);
        self.open.push(id_at);
        self.nodes.push(node);
        Ok(())
    }

    /// Gives the last node an icon or a class, as Pele's decorateNode does. Mermaid throws a
    /// TypeError when a decoration comes before any node. Here it is ignored.
    fn decorate_node(&mut self, decoration: Decoration) {
        let Some(node) = self.nodes.last_mut() else {
            return;
        };
        match decoration {
            Decoration::Icon(icon) if !icon.is_empty() => node.icon = Some(icon.to_string()),
            Decoration::Class(class) if !class.is_empty() => node.class = Some(class.to_string()),
            _ => {}
        }
    }
}
