//! The nodes and edges a class diagram is drawn as, as Pele's buildClassGraph in graph.ts
//! resolves them from its ClassDb.

use std::collections::HashMap;

use super::db::{ClassDb, LOLLIPOP, RelationEnd};

/// A node that is drawn, Pele's GraphNode.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GraphNode {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) is_group: bool,
    /// "rect", "classBox", "note" or "interface".
    pub(crate) shape: &'static str,
    pub(crate) parent_id: Option<String>,
    /// The index of the namespace node this one is drawn in, or -1.
    pub(crate) parent: i32,
    /// The class it draws, by its place in the DB's classes.
    pub(crate) class: Option<usize>,
    /// Its place among the classes, in the order they were declared. Namespaces and notes
    /// have none.
    pub(crate) color_index: Option<usize>,
}

/// An edge that is drawn, Pele's GraphEdge.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GraphEdge {
    pub(crate) id: String,
    pub(crate) start: String,
    pub(crate) end: String,
    /// Indexes into the nodes, or -1. Ids alone are ambiguous, as a class may be named
    /// `note0` or `interface0`.
    pub(crate) source: i32,
    pub(crate) target: i32,
    pub(crate) label: Option<String>,
    pub(crate) arrow_type_start: &'static str,
    pub(crate) arrow_type_end: &'static str,
    pub(crate) start_label_right: String,
    pub(crate) end_label_left: String,
    /// "solid", "dashed" or "dotted".
    pub(crate) pattern: &'static str,
    pub(crate) classes: &'static str,
}

/// What a class diagram is drawn as, Pele's ClassGraph.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ClassGraph {
    pub(crate) nodes: Vec<GraphNode>,
    pub(crate) edges: Vec<GraphEdge>,
}

const MARKERS: [&str; 5] = [
    "aggregation",
    "extension",
    "composition",
    "dependency",
    "lollipop",
];

/// The marker of an end, as Pele's marker names it.
fn marker(end: RelationEnd) -> &'static str {
    end.and_then(|t| MARKERS.get(usize::from(t)).copied())
        .unwrap_or("none")
}

/// Resolves a parsed class diagram into the nodes and edges that are drawn, as Pele's
/// buildClassGraph does. With hierarchical off, only namespaces the text declares are kept,
/// and each is labeled with its full name.
pub(crate) fn build_class_graph(model: &ClassDb, hierarchical: bool) -> ClassGraph {
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut edges: Vec<GraphEdge> = Vec::new();
    let mut namespace_at: HashMap<&str, usize> = HashMap::new();
    let mut class_at: HashMap<&str, usize> = HashMap::new();
    let mut interface_at: HashMap<&str, usize> = HashMap::new();

    // The namespace a namespace is drawn in, where they are not nested: the nearest the text
    // declares, of itself and its ancestors.
    let mut declared: HashMap<String, Option<String>> = HashMap::new();
    let mut declared_ancestor = |id: Option<&str>| -> Option<String> {
        let id = id?;
        if hierarchical {
            return Some(id.to_string());
        }
        let mut chain: Vec<String> = Vec::new();
        let mut current = Some(id.to_string());
        let mut found = None;
        while let Some(c) = current.filter(|c| !c.is_empty()) {
            if let Some(f) = declared.get(&c) {
                found = f.clone();
                break;
            }
            let Some(ns) = model.namespaces.get(&c) else {
                break;
            };
            if ns.explicit {
                found = Some(c);
                break;
            }
            current = ns.parent.clone();
            chain.push(c);
        }
        for visited in chain {
            declared.insert(visited, found.clone());
        }
        found
    };
    let group = |namespace_at: &HashMap<&str, usize>, id: Option<&str>| -> i32 {
        id.and_then(|id| namespace_at.get(id))
            .map_or(-1, |&i| i as i32)
    };

    for ns in model.namespaces.values() {
        if !hierarchical && !ns.explicit {
            continue;
        }
        namespace_at.insert(&ns.id, nodes.len());
        nodes.push(GraphNode {
            id: ns.id.clone(),
            label: if hierarchical {
                ns.label.clone()
            } else {
                ns.id.clone()
            },
            is_group: true,
            shape: "rect",
            parent_id: if hierarchical {
                ns.parent.clone()
            } else {
                None
            },
            parent: -1,
            class: None,
            color_index: None,
        });
    }
    for node in &mut nodes {
        node.parent = group(&namespace_at, node.parent_id.as_deref());
    }

    for (color_index, class) in model.classes.values().enumerate() {
        let parent_id = declared_ancestor(class.parent.as_deref());
        class_at.insert(&class.id, nodes.len());
        nodes.push(GraphNode {
            id: class.id.clone(),
            label: class.label.clone(),
            is_group: false,
            shape: "classBox",
            parent: group(&namespace_at, parent_id.as_deref()),
            parent_id,
            class: Some(color_index),
            color_index: Some(color_index),
        });
    }

    for note in model.notes.values() {
        let parent_id = declared_ancestor(note.parent.as_deref());
        let target = note.class.as_deref().and_then(|c| class_at.get(c).copied());
        if let Some(target) = target {
            edges.push(GraphEdge {
                id: format!("edgeNote{}", note.index),
                start: note.id.clone(),
                end: nodes[target].id.clone(),
                source: nodes.len() as i32,
                target: target as i32,
                label: None,
                arrow_type_start: "none",
                arrow_type_end: "none",
                start_label_right: String::new(),
                end_label_left: String::new(),
                pattern: "dotted",
                classes: "relation",
            });
        }
        nodes.push(GraphNode {
            id: note.id.clone(),
            label: note.text.clone(),
            is_group: false,
            shape: "note",
            parent: group(&namespace_at, parent_id.as_deref()),
            parent_id,
            class: None,
            color_index: None,
        });
    }

    // Mermaid leaves every lollipop interface outside the namespaces. Pele keeps it beside
    // its class.
    for item in &model.interfaces {
        let owner = class_at.get(item.class_id.as_str()).copied();
        interface_at.insert(&item.id, nodes.len());
        nodes.push(GraphNode {
            id: item.id.clone(),
            label: item.label.clone(),
            is_group: false,
            shape: "interface",
            parent_id: None,
            parent: owner.map_or(-1, |o| nodes[o].parent),
            class: None,
            color_index: None,
        });
    }

    for (count, relation) in model.relations.iter().enumerate() {
        let (type1, type2) = (relation.type1, relation.type2);
        // addRelation swaps in an interface for the lollipop end of a relation with no other
        // marker.
        let start_interface = type1 == Some(LOLLIPOP) && type2.is_none();
        let end_interface = !start_interface && type2 == Some(LOLLIPOP) && type1.is_none();
        let at = |interface: bool, id: &str| {
            let map = if interface { &interface_at } else { &class_at };
            map.get(id).map_or(-1, |&i| i as i32)
        };
        let title = |t: &str| {
            if t == "none" {
                String::new()
            } else {
                t.to_string()
            }
        };
        edges.push(GraphEdge {
            id: format!("id_{}_{}_{}", relation.id1, relation.id2, count + 1),
            start: relation.id1.clone(),
            end: relation.id2.clone(),
            source: at(start_interface, &relation.id1),
            target: at(end_interface, &relation.id2),
            label: relation.title.clone(),
            arrow_type_start: marker(type1),
            arrow_type_end: marker(type2),
            start_label_right: title(&relation.relation_title1),
            end_label_left: title(&relation.relation_title2),
            pattern: if relation.line_type == 1 {
                "dashed"
            } else {
                "solid"
            },
            classes: "relation",
        });
    }

    ClassGraph { nodes, edges }
}
