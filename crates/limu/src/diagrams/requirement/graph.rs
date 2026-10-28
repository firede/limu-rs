//! What is laid out of a requirement diagram, as Pele's graph.ts builds it from the DB.

use super::db::{CONTAINS, Element, Requirement, RequirementDb};

/// A requirement or an element to lay out, Pele's GraphNode. Exactly one of requirement and
/// element is set.
#[derive(Clone, Debug)]
pub(crate) struct GraphNode<'a> {
    pub(crate) id: &'a str,
    pub(crate) requirement: Option<&'a Requirement>,
    pub(crate) element: Option<&'a Element>,
    pub(crate) css_styles: &'a [String],
    pub(crate) css_classes: String,
}

/// A relationship to lay out, Pele's GraphEdge. Its start and end are none where they name no
/// requirement or element.
#[derive(Clone, Debug)]
pub(crate) struct GraphEdge<'a> {
    pub(crate) id: String,
    pub(crate) start: Option<&'a str>,
    pub(crate) end: Option<&'a str>,
    pub(crate) kind: &'a str,
    pub(crate) contains: bool,
}

/// What is laid out, Pele's RequirementGraph.
#[derive(Clone, Debug)]
pub(crate) struct RequirementGraph<'a> {
    pub(crate) nodes: Vec<GraphNode<'a>>,
    pub(crate) edges: Vec<GraphEdge<'a>>,
    pub(crate) direction: &'a str,
}

/// The nodes and edges to lay out, as Pele's buildRequirementGraph makes them: requirements
/// first, then elements, as Mermaid orders them.
pub(crate) fn build_requirement_graph(db: &RequirementDb) -> RequirementGraph<'_> {
    let mut nodes = Vec::new();
    for r in db.requirements.values() {
        nodes.push(GraphNode {
            id: &r.name,
            requirement: Some(r),
            element: None,
            css_styles: &r.css_styles,
            css_classes: r.classes.join(" "),
        });
    }
    for e in db.elements.values() {
        nodes.push(GraphNode {
            id: &e.name,
            requirement: None,
            element: Some(e),
            css_styles: &e.css_styles,
            css_classes: e.classes.join(" "),
        });
    }
    let has = |id: &str| db.requirements.contains(id) || db.elements.contains(id);
    let edges = db
        .relations
        .iter()
        .enumerate()
        .map(|(index, r)| GraphEdge {
            id: format!("{}-{}-{index}", r.src, r.dst),
            start: has(&r.src).then_some(r.src.as_str()),
            end: has(&r.dst).then_some(r.dst.as_str()),
            kind: &r.kind,
            contains: r.kind == CONTAINS,
        })
        .collect();
    RequirementGraph {
        nodes,
        edges,
        direction: &db.direction,
    }
}
