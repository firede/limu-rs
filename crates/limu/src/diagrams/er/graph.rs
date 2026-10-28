//! What is drawn of an entity relationship diagram, as Pele's buildErGraph in graph.ts
//! resolves it: subgraphs as groups, entities as nodes with their classes' styles, and
//! relationships as edges with a marker at each end.

use std::collections::{HashMap, HashSet};

use super::db::{ErAttribute, ErDb};
use crate::js::chars::trim;

/// A node of the graph, Pele's ErGraphNode: an entity or a subgraph.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ErGraphNode {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) alias: String,
    pub(crate) attributes: Vec<ErAttribute>,
    pub(crate) is_group: bool,
    pub(crate) parent_id: Option<String>,
    pub(crate) css_styles: Vec<String>,
    pub(crate) css_compiled_styles: Vec<String>,
    pub(crate) css_classes: String,
    pub(crate) dir: Option<&'static str>,
}

/// An edge of the graph, Pele's ErGraphEdge. Its arrow types are Mermaid's cardinalities in
/// lower case, and its pattern "solid" or "dashed".
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ErGraphEdge {
    pub(crate) id: String,
    pub(crate) start: String,
    pub(crate) end: String,
    pub(crate) label: String,
    pub(crate) arrow_type_start: String,
    pub(crate) arrow_type_end: String,
    pub(crate) pattern: &'static str,
}

/// The graph of a diagram, Pele's ErGraph.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ErGraph {
    pub(crate) nodes: Vec<ErGraphNode>,
    pub(crate) edges: Vec<ErGraphEdge>,
}

/// Resolves a parsed diagram into what is drawn, as Pele's buildErGraph does. Every node's
/// label type is Pele's "markdown".
pub(crate) fn build_er_graph(db: &ErDb) -> ErGraph {
    let compiled = |class_names: &mut dyn Iterator<Item = &str>| {
        let mut out = Vec::new();
        for name in class_names {
            let Some(def) = db.class(name) else {
                continue;
            };
            out.extend(def.styles.iter().map(|s| trim(s).to_string()));
            out.extend(def.text_styles.iter().map(|s| trim(s).to_string()));
        }
        out
    };

    let mut parent_of: HashMap<&str, &str> = HashMap::new();
    let mut subgraph_ids: HashSet<&str> = HashSet::new();
    for s in db.subgraphs.iter().rev() {
        subgraph_ids.insert(&s.id);
        for id in &s.nodes {
            parent_of.insert(id, &s.id);
        }
    }

    let mut nodes = Vec::new();
    for s in db.subgraphs.iter().rev() {
        nodes.push(ErGraphNode {
            id: s.id.clone(),
            label: s.title.clone(),
            alias: String::new(),
            attributes: Vec::new(),
            is_group: true,
            parent_id: parent_of.get(s.id.as_str()).map(|p| p.to_string()),
            css_styles: s.css_styles.clone(),
            css_compiled_styles: compiled(&mut s.classes.iter().map(String::as_str)),
            css_classes: s.classes.join(" "),
            dir: s.dir,
        });
    }

    // A name that is also a subgraph's id stands for the subgraph. Mermaid drops such an
    // entity and leaves relationships written before the subgraph pointing at nothing. Pele
    // has them follow it.
    let mut moved: HashMap<&str, &str> = HashMap::new();
    for entity in &db.entities {
        let name = entity.label.as_str();
        if subgraph_ids.contains(name) {
            moved.insert(&entity.id, name);
            continue;
        }
        nodes.push(ErGraphNode {
            id: entity.id.clone(),
            label: entity.label.clone(),
            alias: entity.alias.clone(),
            attributes: entity.attributes.clone(),
            is_group: false,
            parent_id: parent_of.get(name).map(|p| p.to_string()),
            css_styles: entity.css_styles.clone(),
            css_compiled_styles: compiled(&mut entity.css_classes.split(' ')),
            css_classes: entity.css_classes.clone(),
            dir: None,
        });
    }

    let edges = db
        .relationships
        .iter()
        .enumerate()
        .map(|(index, r)| {
            let follow = |id: &str| moved.get(id).map_or(id, |m| m).to_string();
            ErGraphEdge {
                id: format!("id_{}_{}_{index}", r.entity_a, r.entity_b),
                start: follow(&r.entity_a),
                end: follow(&r.entity_b),
                label: r.role_a.clone(),
                arrow_type_start: r.rel_spec.card_b.to_ascii_lowercase(),
                arrow_type_end: r.rel_spec.card_a.to_ascii_lowercase(),
                pattern: if r.rel_spec.rel_type == "IDENTIFYING" {
                    "solid"
                } else {
                    "dashed"
                },
            }
        })
        .collect();

    ErGraph { nodes, edges }
}
