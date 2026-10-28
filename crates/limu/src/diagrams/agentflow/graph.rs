//! The nodes and edges of an agent flow that are drawn, as Pele's buildAgentGraph resolves
//! them from what the parser read: what each flow holds, the flows that are collapsed, the
//! shapes an agent flow draws, the styles of classes, and the palette slots.

use std::collections::{HashMap, HashSet};

use super::color_slots::assign_color_slots;
use super::db::{AgentflowDb, LabelType};
use super::diagnostics::DiagnosticId;
use super::shapes::{normalise_node_shapes, resolve_shape_alias};
use crate::js::chars::trim;
use crate::value::{Map, Value};

/// A node or a flow to draw, Pele's GraphNode. What Pele leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct GraphNode {
    pub(crate) id: String,
    pub(crate) label: Option<String>,
    pub(crate) label_type: LabelType,
    pub(crate) shape: String,
    pub(crate) is_group: bool,
    pub(crate) parent_id: Option<String>,
    pub(crate) css_styles: Vec<String>,
    pub(crate) css_compiled_styles: Vec<String>,
    pub(crate) css_classes: String,
    pub(crate) dir: Option<String>,
    pub(crate) link: Option<String>,
    pub(crate) link_target: Option<String>,
    pub(crate) tooltip: Option<String>,
    /// The palette slot of a container.
    pub(crate) color_index: Option<usize>,
    /// What a node is, which picks its palette slot.
    pub(crate) kind: Option<&'static str>,
    pub(crate) metadata: Option<Map>,
}

/// An edge to draw, Pele's GraphEdge.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct GraphEdge {
    pub(crate) id: String,
    pub(crate) is_user_defined_id: bool,
    pub(crate) start: String,
    pub(crate) end: String,
    pub(crate) kind: String,
    pub(crate) label: String,
    pub(crate) label_type: LabelType,
    pub(crate) thickness: Option<String>,
    pub(crate) pattern: Option<String>,
    pub(crate) minlen: Option<f64>,
    pub(crate) arrow_type_start: String,
    pub(crate) arrow_type_end: String,
    pub(crate) css_compiled_styles: Vec<String>,
    pub(crate) style: Vec<String>,
    pub(crate) classes: String,
    pub(crate) animate: Option<Value>,
    pub(crate) animation: Option<Value>,
    /// The edge's curve: its own, the default linkStyle gives, or the configuration's.
    pub(crate) curve: Option<Value>,
    pub(crate) metadata: Option<Map>,
}

/// What an agent flow draws, Pele's AgentGraph.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct AgentGraph {
    pub(crate) nodes: Vec<GraphNode>,
    pub(crate) edges: Vec<GraphEdge>,
}

/// Whether a flow's metadata collapses it.
fn collapsed(metadata: Option<&Map>) -> bool {
    metadata.and_then(|m| m.get("view")) == Some(&Value::String("collapsed".into()))
}

/// Resolves a parsed agent flow into the nodes and edges that are drawn, as Pele's
/// buildAgentGraph does, and reports on the model the shapes and the nesting it finds wrong.
/// default_curve is the configuration's curve.
pub(crate) fn build_agent_graph(
    db: &mut AgentflowDb,
    palette_length: usize,
    default_curve: Option<&str>,
) -> AgentGraph {
    let (graph, findings) = resolve(db, palette_length, default_curve);
    for (id, error, message, node_id) in findings {
        if error {
            db.emit_error(id, message, &node_id);
        } else {
            db.emit_warning(id, message, &node_id);
        }
    }
    graph
}

/// What a finding about the graph is: its id, whether it is an error, its message, and the
/// node it is of.
type Finding = (DiagnosticId, bool, String, String);

/// The graph of a model, and what is found wrong with it in the order Pele reports it.
fn resolve(
    db: &AgentflowDb,
    palette_length: usize,
    default_curve: Option<&str>,
) -> (AgentGraph, Vec<Finding>) {
    let mut findings: Vec<Finding> = Vec::new();
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut edges: Vec<GraphEdge> = Vec::new();
    let subgraphs = &db.subgraphs;

    let compiled = |class_names: &mut dyn Iterator<Item = &str>| -> Vec<String> {
        let mut out = Vec::new();
        for name in class_names {
            let Some(def) = db.classes.get(name) else {
                continue;
            };
            out.extend(def.styles.iter().map(|s| trim(s).to_string()));
            out.extend(def.text_styles.iter().map(|s| trim(s).to_string()));
        }
        out
    };

    // Everything inside a collapsed flow is hidden, and stands for its outermost collapsed
    // flow. Mermaid walks the whole inside of each collapsed flow, in the order the flows
    // closed, and the last walk to reach a thing wins. A thing has one container, so a later
    // walk can only enter an earlier one through its root: it stops there, and the root's
    // owner is followed afterwards.
    let mut hidden: Vec<&str> = Vec::new();
    let mut is_hidden: HashSet<&str> = HashSet::new();
    let mut owner: HashMap<&str, &str> = HashMap::new();
    let mut walked: HashSet<&str> = HashSet::new();
    for sg in subgraphs {
        if !collapsed(sg.metadata.as_ref()) || is_hidden.contains(sg.id.as_str()) {
            continue;
        }
        let mut pending = vec![sg.id.as_str()];
        while let Some(id) = pending.pop() {
            let Some(inside) = db.subgraph(id) else {
                continue;
            };
            for child in &inside.nodes {
                let child = child.as_str();
                if child == sg.id || is_hidden.contains(child) {
                    continue;
                }
                hidden.push(child);
                is_hidden.insert(child);
                owner.insert(child, &sg.id);
                if !walked.contains(child) {
                    pending.push(child);
                }
            }
        }
        walked.insert(&sg.id);
    }
    let mut collapsed_into: HashMap<&str, &str> = HashMap::new();
    for &id in &hidden {
        let mut path = vec![id];
        let mut top = owner[id];
        loop {
            let known = collapsed_into.get(top).copied();
            let Some(up) = known.or_else(|| owner.get(top).copied()) else {
                break;
            };
            if known.is_none() {
                path.push(top);
            }
            top = up;
            if known.is_some() {
                break;
            }
        }
        for p in path {
            collapsed_into.insert(p, top);
        }
    }

    let position: HashMap<&str, usize> = subgraphs
        .iter()
        .enumerate()
        .map(|(i, sg)| (sg.id.as_str(), i))
        .collect();

    // Two flows can each name the other. The nesting that would close the loop is dropped.
    let mut parent_of: HashMap<&str, &str> = HashMap::new();
    let closes_loop = |parent_of: &HashMap<&str, &str>, child: &str, parent: &str, at: usize| {
        // Only a flow handled earlier in this pass can already be above the parent.
        match position.get(child) {
            Some(&child_at) if child_at >= at => {}
            _ => return false,
        }
        let mut current = Some(parent);
        let mut hops = 0;
        while let Some(c) = current
            && hops <= subgraphs.len()
        {
            if c == child {
                return true;
            }
            current = parent_of.get(c).copied();
            hops += 1;
        }
        false
    };
    for (i, sg) in subgraphs.iter().enumerate().rev() {
        if is_hidden.contains(sg.id.as_str()) {
            continue;
        }
        for id in &sg.nodes {
            if closes_loop(&parent_of, id, &sg.id, i) {
                let message = format!(
                    "Container \"{}\" cannot contain \"{id}\" because \"{id}\" already contains it. The nesting that would close the loop is dropped.",
                    sg.id
                );
                findings.push((
                    DiagnosticId::ContainmentViolation,
                    false,
                    message,
                    id.clone(),
                ));
                continue;
            }
            parent_of.insert(id, &sg.id);
        }
    }
    let parent = |id: &str| parent_of.get(id).map(|p| p.to_string());

    let mut by_id: HashMap<&str, usize> = HashMap::new();
    for sg in subgraphs.iter().rev() {
        if is_hidden.contains(sg.id.as_str()) {
            continue;
        }
        let collapsed = collapsed(sg.metadata.as_ref());
        let metadata = if collapsed {
            let mut m = sg.metadata.clone().unwrap_or_default();
            m.insert("containerType", Value::String("flow".into()));
            Some(m)
        } else {
            sg.metadata.clone()
        };
        by_id.insert(&sg.id, nodes.len());
        nodes.push(GraphNode {
            id: sg.id.clone(),
            label: Some(sg.title.clone()),
            label_type: sg.label_type,
            parent_id: parent(&sg.id),
            css_styles: Vec::new(),
            css_compiled_styles: compiled(&mut sg.classes.iter().map(String::as_str)),
            css_classes: sg.classes.join(" "),
            shape: if collapsed {
                "collapsedGroup"
            } else {
                "flowGroup"
            }
            .into(),
            dir: sg.dir.clone(),
            is_group: !collapsed,
            metadata,
            ..GraphNode::default()
        });
    }

    for vertex in db.nodes.values() {
        if is_hidden.contains(vertex.id.as_str()) {
            continue;
        }
        if let Some(&at) = by_id.get(vertex.id.as_str()) {
            let existing = &mut nodes[at];
            existing.css_styles = vertex.styles.clone();
            existing.css_compiled_styles = compiled(&mut vertex.classes.iter().map(String::as_str));
            existing.css_classes = vertex.classes.join(" ");
            continue;
        }
        let shape = match resolve_shape_alias(vertex.kind.as_deref()) {
            None | Some("square") => "squareRect",
            Some(shape) => shape,
        };
        let classes = ["default", "node"]
            .into_iter()
            .chain(vertex.classes.iter().map(String::as_str));
        by_id.insert(&vertex.id, nodes.len());
        nodes.push(GraphNode {
            id: vertex.id.clone(),
            label: vertex.text.clone(),
            label_type: vertex.label_type,
            parent_id: parent(&vertex.id),
            css_styles: vertex.styles.clone(),
            css_compiled_styles: compiled(&mut { classes }),
            css_classes: "default ".to_string() + &vertex.classes.join(" "),
            dir: vertex.dir.clone(),
            link: vertex.link.clone(),
            link_target: vertex.link_target.clone(),
            tooltip: db.tooltips.get(&vertex.id).map(|(_, t)| t.clone()),
            metadata: vertex.metadata.clone(),
            is_group: false,
            shape: shape.into(),
            ..GraphNode::default()
        });
    }

    for (index, raw) in db.edges.iter().enumerate() {
        let start = collapsed_into
            .get(raw.start.as_str())
            .copied()
            .unwrap_or(&raw.start);
        let end = collapsed_into
            .get(raw.end.as_str())
            .copied()
            .unwrap_or(&raw.end);
        // An edge between two things inside one collapsed flow has nothing left to connect.
        if start == end && raw.start != raw.end {
            continue;
        }
        let mut styles = db.default_edge_style.clone().unwrap_or_default();
        if let Some(style) = &raw.style {
            styles.extend(style.iter().cloned());
        }
        let arrow = match raw.kind.as_deref() {
            Some("arrow_open") => "none",
            Some(k @ ("arrow_cross" | "arrow_point")) => k,
            _ => "arrow_point",
        };
        // raw.interpolate || db.defaultInterpolate || defaultCurve
        let curve = match &raw.interpolate {
            Some(v) if v.truthy() => Some(v.clone()),
            _ => match db.default_interpolate.as_deref() {
                Some(d) if !d.is_empty() => Some(Value::String(d.into())),
                _ => default_curve.map(|c| Value::String(c.into())),
            },
        };
        edges.push(GraphEdge {
            id: if raw.id.is_empty() {
                format!("L_{start}_{end}_{index}")
            } else {
                raw.id.clone()
            },
            is_user_defined_id: raw.is_user_defined_id,
            start: start.to_string(),
            end: end.to_string(),
            kind: raw.kind.clone().unwrap_or_else(|| "normal".into()),
            label: raw.text.clone(),
            label_type: raw.label_type,
            thickness: raw.stroke.clone(),
            pattern: raw.stroke.clone(),
            minlen: raw.length,
            classes: "edge-thickness-normal edge-pattern-solid flowchart-link".into(),
            arrow_type_start: "none".into(),
            arrow_type_end: arrow.into(),
            css_compiled_styles: compiled(&mut raw.classes.iter().map(String::as_str)),
            style: styles,
            animate: raw.animate.clone(),
            animation: raw.animation.clone(),
            curve,
            metadata: raw.metadata.clone(),
        });
    }

    for f in normalise_node_shapes(&mut nodes) {
        findings.push((f.id, f.error, f.message, f.node_id));
    }

    // Containers are numbered in the order they were written: a walk of the nesting, parents
    // first. The subgraphs themselves are in the order the blocks closed, with a nested flow
    // before its parent.
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    for sg in subgraphs {
        if let Some(&parent) = parent_of.get(sg.id.as_str()) {
            children.entry(parent).or_default().push(&sg.id);
        }
    }
    let mut order: HashMap<String, usize> = HashMap::new();
    for sg in subgraphs {
        if parent_of.contains_key(sg.id.as_str()) {
            continue;
        }
        let mut pending = vec![sg.id.as_str()];
        while let Some(id) = pending.pop() {
            if order.contains_key(id) {
                continue;
            }
            order.insert(id.to_string(), order.len());
            if let Some(kids) = children.get(id) {
                pending.extend(kids.iter().rev());
            }
        }
    }

    assign_color_slots(
        &mut nodes,
        |id| {
            if db.connector(id).is_some() {
                return Some("connector");
            }
            db.nodes.get(id).map(|v| db.vertex_kind(v))
        },
        &order,
        palette_length,
    );

    (AgentGraph { nodes, edges }, findings)
}
