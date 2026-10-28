//! The nodes and edges of a flowchart that are drawn, as Pele's buildFlowGraph resolves them
//! from what the parser read: what each group holds, the subgraphs that are collapsed, the
//! styles of classes, and what edges have by default.

use std::collections::{HashMap, HashSet};

use super::db::{FlowDb, FlowNode, LabelType};
use crate::js::chars::trim;
use crate::value::{Map, Value};

/// A node or a group to draw, Pele's GraphNode. What Pele leaves undefined is none.
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
    pub(crate) icon: Option<String>,
    pub(crate) pos: Option<String>,
    pub(crate) img: Option<String>,
    pub(crate) asset_width: Option<f64>,
    pub(crate) asset_height: Option<f64>,
    pub(crate) constraint: Option<String>,
    /// Where the group is among the subgraphs, each before what it holds, for its color.
    pub(crate) color_index: Option<usize>,
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
}

/// What a flowchart draws, Pele's FlowGraph.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FlowGraph {
    pub(crate) nodes: Vec<GraphNode>,
    pub(crate) edges: Vec<GraphEdge>,
}

/// The shape a node is drawn as, as Pele's shapeOf names it.
fn shape_of(vertex: &FlowNode) -> String {
    let set = |s: &Option<String>| s.as_deref().is_some_and(|s| !s.is_empty());
    if set(&vertex.img) {
        return "imageSquare".into();
    }
    if set(&vertex.icon) {
        return match vertex.form.as_deref() {
            Some("circle") => "iconCircle",
            Some("square") => "iconSquare",
            Some("rounded") => "iconRounded",
            _ => "icon",
        }
        .into();
    }
    match vertex.kind.as_deref() {
        None | Some("square") => "squareRect".into(),
        Some("round") => "roundedRect".into(),
        Some(kind) => kind.into(),
    }
}

/// The markers at the start and the end of an edge of a type, as Pele's arrows gives them.
fn arrows(kind: Option<&str>) -> (String, String) {
    match kind {
        Some(k @ ("arrow_point" | "arrow_circle" | "arrow_cross")) => ("none".into(), k.into()),
        Some(k @ ("double_arrow_point" | "double_arrow_circle" | "double_arrow_cross")) => {
            let single = k.replacen("double_", "", 1);
            (single.clone(), single)
        }
        _ => ("none".into(), "arrow_point".into()),
    }
}

/// Whether a subgraph's metadata collapses it.
fn collapsed(metadata: Option<&Map>) -> bool {
    metadata.and_then(|m| m.get("view")) == Some(&Value::String("collapsed".into()))
}

/// The outermost collapsed subgraph around the subgraph of an id, as Pele's
/// outermostCollapsed finds it, resolved once per chain of parents: outermost keeps what each
/// subgraph on the way resolves to.
fn outermost_collapsed<'s>(
    id: &'s str,
    outermost: &mut HashMap<&'s str, Option<&'s str>>,
    parent: &HashMap<&'s str, &'s str>,
    is_collapsed: impl Fn(&str) -> bool,
) -> Option<&'s str> {
    let mut chain: Vec<&str> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut current = Some(id);
    while let Some(c) = current {
        if outermost.contains_key(c) || seen.contains(c) {
            break;
        }
        seen.insert(c);
        chain.push(c);
        current = parent.get(c).copied();
    }
    let mut result = current.and_then(|c| outermost.get(c).copied().flatten());
    for &c in chain.iter().rev() {
        if result.is_none() && is_collapsed(c) {
            result = Some(c);
        }
        outermost.insert(c, result);
    }
    result
}

/// Resolves a parsed flowchart into the nodes and edges that are drawn, as Pele's
/// buildFlowGraph does. default_curve is the configuration's curve.
pub(crate) fn build_flow_graph(model: &FlowDb, default_curve: Option<&str>) -> FlowGraph {
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut edges: Vec<GraphEdge> = Vec::new();
    let subgraphs = &model.subgraphs;

    let compiled = |class_names: &mut dyn Iterator<Item = &str>| -> Vec<String> {
        let mut out = Vec::new();
        for name in class_names {
            let Some(def) = model.classes.get(name) else {
                continue;
            };
            out.extend(def.styles.iter().map(|s| trim(s).to_string()));
            out.extend(def.text_styles.iter().map(|s| trim(s).to_string()));
        }
        out
    };

    let mut subgraph_parent: HashMap<&str, &str> = HashMap::new();
    for sg in subgraphs {
        for child in &sg.nodes {
            if model.subgraph(child).is_some() {
                subgraph_parent.insert(child, &sg.id);
            }
        }
    }

    let mut declaration_index: HashMap<&str, usize> = HashMap::new();
    let mut children_of: HashMap<&str, Vec<&str>> = HashMap::new();
    for sg in subgraphs {
        if let Some(&parent) = subgraph_parent.get(sg.id.as_str()) {
            children_of.entry(parent).or_default().push(&sg.id);
        }
    }
    let mut next_index = 0;
    for sg in subgraphs {
        if subgraph_parent.contains_key(sg.id.as_str()) {
            continue;
        }
        let mut pending = vec![sg.id.as_str()];
        while let Some(id) = pending.pop() {
            if declaration_index.contains_key(id) {
                continue;
            }
            declaration_index.insert(id, next_index);
            next_index += 1;
            if let Some(kids) = children_of.get(id) {
                pending.extend(kids.iter().rev());
            }
        }
    }

    let is_collapsed = |id: &str| collapsed(model.subgraph(id).and_then(|s| s.metadata.as_ref()));
    let any_collapsed = subgraphs.iter().any(|sg| collapsed(sg.metadata.as_ref()));
    let mut outermost: HashMap<&str, Option<&str>> = HashMap::new();
    let mut hidden: HashSet<String> = HashSet::new();
    let mut collapsed_into: HashMap<String, String> = HashMap::new();
    if any_collapsed {
        for sg in subgraphs {
            let Some(ancestor) =
                outermost_collapsed(&sg.id, &mut outermost, &subgraph_parent, is_collapsed)
            else {
                continue;
            };
            if sg.id != ancestor {
                hidden.insert(sg.id.clone());
                collapsed_into.insert(sg.id.clone(), ancestor.to_string());
            }
            for child in &sg.nodes {
                if child == ancestor {
                    continue;
                }
                hidden.insert(child.clone());
                collapsed_into.insert(child.clone(), ancestor.to_string());
            }
        }
    }

    let mut parent_of: HashMap<&str, &str> = HashMap::new();
    let mut groups: HashSet<&str> = HashSet::new();
    for sg in subgraphs.iter().rev() {
        if hidden.contains(&sg.id) {
            continue;
        }
        if !sg.nodes.is_empty() {
            groups.insert(&sg.id);
        }
        for id in &sg.nodes {
            parent_of.insert(id, &sg.id);
        }
    }
    let parent = |id: &str| parent_of.get(id).map(|p| p.to_string());

    let mut by_id: HashMap<&str, usize> = HashMap::new();
    for sg in subgraphs.iter().rev() {
        if hidden.contains(&sg.id) {
            continue;
        }
        let collapsed = collapsed(sg.metadata.as_ref());
        by_id.insert(&sg.id, nodes.len());
        nodes.push(GraphNode {
            id: sg.id.clone(),
            label: Some(sg.title.clone()),
            label_type: sg.label_type,
            parent_id: parent(&sg.id),
            css_styles: Vec::new(),
            css_compiled_styles: compiled(&mut sg.classes.iter().map(String::as_str)),
            css_classes: sg.classes.join(" "),
            shape: if collapsed { "collapsedGroup" } else { "rect" }.into(),
            dir: sg.dir.clone(),
            is_group: !collapsed,
            color_index: declaration_index.get(sg.id.as_str()).copied(),
            metadata: if collapsed { None } else { sg.metadata.clone() },
            ..GraphNode::default()
        });
    }

    for vertex in model.nodes.values() {
        if hidden.contains(&vertex.id) {
            continue;
        }
        if let Some(&at) = by_id.get(vertex.id.as_str()) {
            let existing = &mut nodes[at];
            existing.css_styles = vertex.styles.clone();
            existing.css_compiled_styles = compiled(&mut vertex.classes.iter().map(String::as_str));
            existing.css_classes = vertex.classes.join(" ");
            continue;
        }
        let is_group = groups.contains(vertex.id.as_str());
        let classes = ["default", "node"]
            .into_iter()
            .chain(vertex.classes.iter().map(String::as_str));
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
            tooltip: model.tooltips.get(&vertex.id).map(|(_, t)| t.clone()),
            icon: vertex.icon.clone(),
            pos: vertex.pos.clone(),
            img: vertex.img.clone(),
            asset_width: vertex.asset_width,
            asset_height: vertex.asset_height,
            constraint: vertex.constraint.clone(),
            is_group,
            shape: if is_group {
                "rect".into()
            } else {
                shape_of(vertex)
            },
            ..GraphNode::default()
        });
    }

    for (index, raw) in model.edges.iter().enumerate() {
        let start = collapsed_into.get(&raw.start).unwrap_or(&raw.start);
        let end = collapsed_into.get(&raw.end).unwrap_or(&raw.end);
        if start == end
            && (collapsed_into.contains_key(&raw.start) || collapsed_into.contains_key(&raw.end))
        {
            continue;
        }

        let (arrow_start, arrow_end) = arrows(raw.kind.as_deref());
        let mut styles = model.default_edge_style.clone().unwrap_or_default();
        if let Some(style) = &raw.style {
            styles.extend(style.iter().cloned());
        }
        let invisible = raw.stroke.as_deref() == Some("invisible");
        let plain = invisible || raw.kind.as_deref() == Some("arrow_open");
        // raw.interpolate || model.defaultInterpolate || defaultCurve
        let curve = match &raw.interpolate {
            Some(v) if v.truthy() => Some(v.clone()),
            _ => match model.default_interpolate.as_deref() {
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
            start: start.clone(),
            end: end.clone(),
            kind: raw.kind.clone().unwrap_or_else(|| "normal".into()),
            label: raw.text.clone(),
            label_type: raw.label_type,
            thickness: raw.stroke.clone(),
            pattern: raw.stroke.clone(),
            minlen: raw.length,
            classes: if invisible {
                ""
            } else {
                "edge-thickness-normal edge-pattern-solid flowchart-link"
            }
            .into(),
            arrow_type_start: if plain { "none".into() } else { arrow_start },
            arrow_type_end: if plain { "none".into() } else { arrow_end },
            css_compiled_styles: compiled(&mut raw.classes.iter().map(String::as_str)),
            style: styles,
            animate: raw.animate.clone(),
            animation: raw.animation.clone(),
            curve,
        });
    }

    FlowGraph { nodes, edges }
}
