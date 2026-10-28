//! What a use case diagram draws, as Pele's graph.ts resolves it from the model: each actor,
//! use case, note, JSON table and boundary as a node, and each relationship and note
//! connector as an edge.

use super::ordered::Ordered;
use super::types::{
    LINE_SOLID, LabelType, Relationship, SOLID_ARROW, UsecaseJsonNode, UsecaseModel,
};
use crate::js::chars::trim;
use crate::value::Value;

/// A row of a JSON table, Pele's JsonRow. Its key is blank on the second and later values of
/// an array, and its accessible key is the key again, for every row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JsonRow {
    pub(crate) key: String,
    pub(crate) accessible_key: String,
    pub(crate) value: String,
}

/// A node of the graph, Pele's UsecaseGraphNode. Its shape is one of Mermaid's shape names.
/// What Pele leaves undefined is none.
#[derive(Clone, Debug)]
pub(crate) struct UsecaseGraphNode {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) label_type: LabelType,
    pub(crate) shape: &'static str,
    pub(crate) is_group: bool,
    pub(crate) padding: f64,
    pub(crate) css_classes: String,
    pub(crate) css_styles: Vec<String>,
    pub(crate) css_compiled_styles: Vec<String>,
    pub(crate) actor_type: Option<&'static str>,
    pub(crate) business: Option<bool>,
    pub(crate) icon: Option<String>,
    pub(crate) stereotype: Option<String>,
    pub(crate) parent_id: Option<String>,
    pub(crate) note_target: Option<String>,
    pub(crate) json_rows: Option<Vec<JsonRow>>,
    pub(crate) boundary_type: Option<&'static str>,
}

/// An edge of the graph, Pele's UsecaseGraphEdge. An internal edge is the dotted line from a
/// note to what it is about.
#[derive(Clone, Debug)]
pub(crate) struct UsecaseGraphEdge {
    pub(crate) id: String,
    pub(crate) start: String,
    pub(crate) end: String,
    pub(crate) relationship_type: &'static str,
    pub(crate) pattern: &'static str,
    pub(crate) arrow_type_start: &'static str,
    pub(crate) arrow_type_end: &'static str,
    pub(crate) label: Option<String>,
    pub(crate) label_type: Option<LabelType>,
    pub(crate) classes: String,
    pub(crate) style: Vec<String>,
    pub(crate) css_compiled_styles: Vec<String>,
    pub(crate) animate: bool,
    pub(crate) animation: Option<&'static str>,
    pub(crate) minlen: f64,
    pub(crate) explicit_id: bool,
    pub(crate) internal: bool,
}

/// The graph, Pele's UsecaseGraph.
#[derive(Clone, Debug)]
pub(crate) struct UsecaseGraph {
    pub(crate) nodes: Vec<UsecaseGraphNode>,
    pub(crate) edges: Vec<UsecaseGraphEdge>,
}

/// The shape of an actor of a type, Pele's ACTOR_SHAPES.
fn actor_shape(kind: &str) -> &'static str {
    match kind {
        "hollow" => "usecaseActorHollow",
        "awesome" => "usecaseActorAwesome",
        "icon" => "usecaseActorIcon",
        _ => "usecaseActor",
    }
}

/// The start and end markers of each arrow type, in the order of the arrow types, Pele's
/// MARKERS.
const MARKERS: [[&str; 2]; 7] = [
    ["none", "arrow_point"],
    ["arrow_point", "none"],
    ["none", "none"],
    ["none", "arrow_circle"],
    ["none", "arrow_cross"],
    ["arrow_circle", "none"],
    ["arrow_cross", "none"],
];

/// A JSON value as a row writes it, as Pele's scalar does.
fn scalar(v: Option<&Value>) -> String {
    match v {
        None => "undefined".to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) => "null".to_string(),
        Some(v) => v.to_js_string(),
    }
}

/// The rows of a JSON table, one for each leaf of its object, depth first in the order the
/// keys were written, as Pele's flattenJsonRows makes them. Nesting is kept on a stack of
/// its own, so depth is limited by memory and not by the call stack.
pub(crate) fn flatten_json_rows(json: &UsecaseJsonNode) -> Vec<JsonRow> {
    let mut rows = Vec::new();
    let root = Value::Object(json.value.clone());
    let mut stack: Vec<(Option<&Value>, String, String)> =
        vec![(Some(&root), String::new(), String::new())];
    let row = |key: &str, accessible: &str, value: String| JsonRow {
        key: key.to_string(),
        accessible_key: accessible.to_string(),
        value,
    };
    while let Some((current, path, pointer)) = stack.pop() {
        let mut children: Vec<(Option<&Value>, String, String)> = Vec::new();
        match current {
            Some(Value::Array(items)) => {
                let leaves = items
                    .iter()
                    .all(|v| !matches!(v, Value::Array(_) | Value::Object(_)));
                if items.is_empty() {
                    rows.push(row(&path, &path, "[]".to_string()));
                } else if leaves {
                    for (i, item) in items.iter().enumerate() {
                        let key = if i == 0 { path.as_str() } else { "" };
                        rows.push(row(key, &path, scalar(Some(item))));
                    }
                } else {
                    for (i, item) in items.iter().enumerate() {
                        children.push((
                            Some(item),
                            format!("{path}[{i}]"),
                            format!("{pointer}/{i}"),
                        ));
                    }
                }
            }
            Some(Value::Object(object)) => {
                let written = json
                    .property_order
                    .iter()
                    .find(|(p, _)| *p == pointer)
                    .map(|(_, keys)| keys.clone());
                let keys = written.unwrap_or_else(|| object.keys().map(str::to_string).collect());
                if keys.is_empty() {
                    rows.push(row(&path, &path, "{}".to_string()));
                }
                for key in keys {
                    let child = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    let escaped = key.replace('~', "~0").replace('/', "~1");
                    children.push((object.get(&key), child, format!("{pointer}/{escaped}")));
                }
            }
            v => rows.push(row(&path, &path, scalar(v))),
        }
        stack.extend(children.into_iter().rev());
    }
    rows
}

/// The names given, without those that are empty, separated by spaces, as Pele's classList
/// joins them.
fn class_list<'a>(names: impl IntoIterator<Item = &'a str>) -> String {
    names
        .into_iter()
        .filter(|n| !n.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The start and end markers of a relationship, as Pele's markers picks them.
fn markers(r: &Relationship) -> [&'static str; 2] {
    if r.kind == "generalization" {
        return ["none", "extension"];
    }
    if r.kind != "association" {
        return MARKERS[SOLID_ARROW];
    }
    MARKERS
        .get(r.arrow_type)
        .copied()
        .unwrap_or(MARKERS[LINE_SOLID])
}

/// Resolves a parsed use case diagram into what is drawn, as Pele's buildUsecaseGraph does,
/// which ports getData from Mermaid's use case DB.
pub(crate) fn build_usecase_graph(model: &UsecaseModel) -> UsecaseGraph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // The styles of the classes given, a later class winning a property but keeping the
    // place the property first took.
    let compiled = |names: &[String]| -> Vec<String> {
        let mut out: Ordered<String, String> = Ordered::default();
        for name in std::iter::once("default").chain(names.iter().map(String::as_str)) {
            let Some(definition) = model.class_defs.iter().find(|d| d.id == name) else {
                continue;
            };
            for raw in &definition.styles {
                let style = trim(raw);
                let property = trim(style.find(':').map_or(style, |colon| &style[..colon]));
                if !property.is_empty() {
                    out.set(property.to_string(), style.to_string());
                }
            }
        }
        out.into_values()
    };
    let node =
        |id: &str, label: &str, label_type: LabelType, shape: &'static str| UsecaseGraphNode {
            id: id.to_string(),
            label: label.to_string(),
            label_type,
            shape,
            is_group: false,
            padding: 10.0,
            css_classes: String::new(),
            css_styles: Vec::new(),
            css_compiled_styles: Vec::new(),
            actor_type: None,
            business: None,
            icon: None,
            stereotype: None,
            parent_id: None,
            note_target: None,
            json_rows: None,
            boundary_type: None,
        };
    let nonempty = |s: &Option<String>| s.clone().filter(|s| !s.is_empty());

    for actor in &model.actors {
        let mut n = node(
            &actor.id,
            &actor.label,
            actor.label_type,
            actor_shape(actor.kind),
        );
        let kind = format!("usecase-actor-{}", actor.kind);
        n.css_classes = class_list(
            [
                "default",
                "usecase-actor",
                &kind,
                if actor.business {
                    "usecase-business"
                } else {
                    ""
                },
            ]
            .into_iter()
            .chain(actor.classes.iter().map(String::as_str)),
        );
        n.css_styles = actor.styles.clone();
        n.css_compiled_styles = compiled(&actor.classes);
        n.actor_type = Some(actor.kind);
        n.business = Some(actor.business);
        n.icon = nonempty(&actor.icon);
        n.stereotype = nonempty(&actor.stereotype);
        n.parent_id = nonempty(&actor.parent_id);
        nodes.push(n);
    }
    for use_case in &model.use_cases {
        let ellipse = use_case.shape == "ellipse";
        let shape = if !ellipse {
            "rect"
        } else if use_case.business {
            "usecaseBusiness"
        } else {
            "usecaseEllipse"
        };
        let mut n = node(&use_case.id, &use_case.label, use_case.label_type, shape);
        n.padding = if ellipse { 20.0 } else { 10.0 };
        let kind = format!("usecase-{}", use_case.shape);
        n.css_classes = class_list(
            [
                "default",
                "usecase-element",
                &kind,
                if use_case.business {
                    "usecase-business"
                } else {
                    ""
                },
            ]
            .into_iter()
            .chain(use_case.classes.iter().map(String::as_str)),
        );
        n.css_styles = use_case.styles.clone();
        n.css_compiled_styles = compiled(&use_case.classes);
        n.business = Some(use_case.business);
        n.stereotype = nonempty(&use_case.stereotype);
        n.parent_id = nonempty(&use_case.parent_id);
        nodes.push(n);
    }
    for note in &model.notes {
        let mut n = node(&note.id, &note.label, note.label_type, "note");
        n.css_classes = "default usecase-note".to_string();
        n.css_compiled_styles = compiled(&[]);
        n.note_target = Some(note.target.clone());
        nodes.push(n);
    }
    for json in &model.json_nodes {
        let mut n = node(&json.id, &json.id, LabelType::Text, "usecaseJsonTable");
        n.css_classes = class_list(
            ["default", "usecase-json-table"]
                .into_iter()
                .chain(json.classes.iter().map(String::as_str)),
        );
        n.css_styles = json.styles.clone();
        n.css_compiled_styles = compiled(&json.classes);
        n.json_rows = Some(flatten_json_rows(json));
        nodes.push(n);
    }
    for boundary in &model.system_boundaries {
        let mut n = node(
            &boundary.id,
            &boundary.label,
            boundary.label_type,
            "usecaseSystemBoundary",
        );
        n.is_group = true;
        n.padding = 20.0;
        let kind = format!("system-boundary-{}", boundary.kind);
        n.css_classes = class_list(
            ["default", "system-boundary", &kind]
                .into_iter()
                .chain(boundary.classes.iter().map(String::as_str)),
        );
        n.css_styles = boundary.styles.clone();
        n.css_compiled_styles = compiled(&boundary.classes);
        n.boundary_type = Some(boundary.kind);
        nodes.push(n);
    }

    for r in &model.relationships {
        let semantic = r.kind == "include" || r.kind == "extend";
        let [arrow_type_start, arrow_type_end] = markers(r);
        let animated = r.animate || r.animation.is_some();
        let kind = format!("relationship-{}", r.kind);
        let animation = format!("edge-animation-{}", r.animation.unwrap_or("fast"));
        let classes = class_list(
            ["default", "relationship", &kind]
                .into_iter()
                .chain(r.classes.iter().map(String::as_str))
                .chain(animated.then_some(animation.as_str())),
        );
        // Include and extend are labelled with their kind, whatever label the relationship
        // carries.
        let (label, label_type) = if semantic {
            (Some(r.kind.to_string()), Some(LabelType::Text))
        } else if r.kind == "association" {
            match &r.label {
                Some((text, kind)) => ((!text.is_empty()).then(|| text.clone()), Some(*kind)),
                None => (None, None),
            }
        } else {
            (None, None)
        };
        edges.push(UsecaseGraphEdge {
            id: r.id.clone(),
            start: r.source.clone(),
            end: r.target.clone(),
            relationship_type: r.kind,
            pattern: if semantic { "dotted" } else { "solid" },
            arrow_type_start,
            arrow_type_end,
            label,
            label_type,
            classes,
            style: r.styles.clone(),
            css_compiled_styles: compiled(&r.classes),
            animate: r.animate,
            animation: r.animation,
            minlen: r.minlen,
            explicit_id: r.explicit_id,
            internal: false,
        });
    }
    for note in &model.notes {
        edges.push(UsecaseGraphEdge {
            id: format!("{}-edge", note.id),
            start: note.id.clone(),
            end: note.target.clone(),
            relationship_type: "note",
            pattern: "dotted",
            arrow_type_start: "none",
            arrow_type_end: "none",
            label: None,
            label_type: None,
            classes: "default relationship relationship-note".to_string(),
            style: Vec::new(),
            css_compiled_styles: compiled(&[]),
            animate: false,
            animation: None,
            minlen: 1.0,
            explicit_id: false,
            internal: true,
        });
    }
    UsecaseGraph { nodes, edges }
}
