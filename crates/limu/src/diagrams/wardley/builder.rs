//! The builder of Wardley maps, as Pele's builder.ts has it.

use std::collections::HashMap;

use crate::js::map::Ordered;

/// A thing placed on the map, Pele's WardleyNode: an anchor, a component, or a component of a
/// pipeline. Its x is its evolution and its y its visibility, as percentages. What Pele leaves
/// undefined is none.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WardleyNode {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) x: f64,
    pub(crate) y: f64,
    /// "anchor", "component" or "pipeline-component".
    pub(crate) class_name: Option<&'static str>,
    pub(crate) label_offset_x: Option<f64>,
    pub(crate) label_offset_y: Option<f64>,
    pub(crate) in_pipeline: bool,
    pub(crate) is_pipeline_parent: bool,
    pub(crate) inertia: Option<bool>,
    /// "build", "buy", "outsource" or "market".
    pub(crate) source_strategy: Option<String>,
}

/// What a node added to the map sets, as the object Pele hands to addNode holds it. An
/// anchor's says nothing of inertia nor strategy, and leaves them as they were, where a
/// component's sets both, if only to undefined.
pub(crate) struct NodeSet {
    pub(crate) node: WardleyNode,
    pub(crate) sets_inertia: bool,
}

/// A line from one node to another, Pele's WardleyLink. Its flow is "forward", "backward" or
/// "bidirectional".
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WardleyLink {
    pub(crate) source: String,
    pub(crate) target: String,
    pub(crate) dashed: bool,
    pub(crate) label: Option<String>,
    pub(crate) flow: Option<&'static str>,
}

/// Where a component is evolving to, Pele's WardleyTrend.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WardleyTrend {
    pub(crate) node_id: String,
    pub(crate) target_x: f64,
    pub(crate) target_y: f64,
}

/// A component and the components that make it up, Pele's WardleyPipeline.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WardleyPipeline {
    pub(crate) node_id: String,
    pub(crate) component_ids: Vec<String>,
}

/// A numbered mark at places on the map, Pele's WardleyAnnotation. Each place is an x and a
/// y, as percentages.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WardleyAnnotation {
    pub(crate) number: f64,
    pub(crate) coordinates: Vec<(f64, f64)>,
    pub(crate) text: String,
}

/// Text placed on the map, Pele's WardleyNote.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WardleyNote {
    pub(crate) text: String,
    pub(crate) x: f64,
    pub(crate) y: f64,
}

/// An accelerator or a deaccelerator, Pele's WardleyForce.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WardleyForce {
    pub(crate) name: String,
    pub(crate) x: f64,
    pub(crate) y: f64,
}

/// The labels and stages of the axes, Pele's WardleyAxesConfig. Each boundary is the right
/// edge of a stage, from 0 to 1. What Pele leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct WardleyAxesConfig {
    pub(crate) x_label: Option<String>,
    pub(crate) y_label: Option<String>,
    pub(crate) stages: Option<Vec<String>>,
    pub(crate) stage_boundaries: Option<Vec<f64>>,
}

/// Pele's WardleyBuildResult. The annotations box is an x and a y, and the size a width and
/// a height.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct WardleyBuildResult {
    pub(crate) nodes: Vec<WardleyNode>,
    pub(crate) links: Vec<WardleyLink>,
    pub(crate) trends: Vec<WardleyTrend>,
    pub(crate) pipelines: Vec<WardleyPipeline>,
    pub(crate) annotations: Vec<WardleyAnnotation>,
    pub(crate) notes: Vec<WardleyNote>,
    pub(crate) accelerators: Vec<WardleyForce>,
    pub(crate) deaccelerators: Vec<WardleyForce>,
    pub(crate) annotations_box: Option<(f64, f64)>,
    pub(crate) axes: WardleyAxesConfig,
    pub(crate) size: Option<(f64, f64)>,
}

/// Pele's WardleyBuilder. Its maps keep the order keys were first added in, as JavaScript's
/// do.
#[derive(Default)]
pub(crate) struct WardleyBuilder {
    nodes: Ordered<WardleyNode>,
    labels: Option<HashMap<String, String>>,
    links: Vec<WardleyLink>,
    trends: Ordered<WardleyTrend>,
    pipelines: Ordered<WardleyPipeline>,
    annotations: Vec<WardleyAnnotation>,
    notes: Vec<WardleyNote>,
    accelerators: Vec<WardleyForce>,
    deaccelerators: Vec<WardleyForce>,
    annotations_box: Option<(f64, f64)>,
    axes: WardleyAxesConfig,
    size: Option<(f64, f64)>,
}

impl WardleyBuilder {
    /// Adds a node, or sets one again, as Pele's addNode does: a node added again keeps its
    /// class and label offsets unless the new one sets them, and whether it is in a pipeline.
    pub(crate) fn add_node(&mut self, set: NodeSet) {
        let node = set.node;
        let mut next = match self.nodes.get(&node.id) {
            Some(existing) => existing.clone(),
            None => WardleyNode {
                id: node.id.clone(),
                label: node.label.clone(),
                x: node.x,
                y: node.y,
                class_name: None,
                label_offset_x: None,
                label_offset_y: None,
                in_pipeline: false,
                is_pipeline_parent: false,
                inertia: None,
                source_strategy: None,
            },
        };
        next.label = node.label;
        next.x = node.x;
        next.y = node.y;
        next.class_name = node.class_name.or(next.class_name);
        next.label_offset_x = node.label_offset_x.or(next.label_offset_x);
        next.label_offset_y = node.label_offset_y.or(next.label_offset_y);
        if set.sets_inertia {
            next.inertia = node.inertia;
            next.source_strategy = node.source_strategy;
        }
        self.nodes.set(&node.id, next);
        self.labels = None;
    }

    pub(crate) fn add_link(&mut self, link: WardleyLink) {
        self.links.push(link);
    }

    pub(crate) fn add_trend(&mut self, trend: WardleyTrend) {
        let id = trend.node_id.clone();
        self.trends.set(&id, trend);
    }

    pub(crate) fn start_pipeline(&mut self, node_id: &str) {
        let pipeline = WardleyPipeline {
            node_id: node_id.to_string(),
            component_ids: Vec::new(),
        };
        self.pipelines.set(node_id, pipeline);
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.is_pipeline_parent = true;
        }
    }

    pub(crate) fn add_pipeline_component(&mut self, pipeline_node_id: &str, component_id: &str) {
        if let Some(p) = self.pipelines.get_mut(pipeline_node_id) {
            p.component_ids.push(component_id.to_string());
        }
        if let Some(node) = self.nodes.get_mut(component_id) {
            node.in_pipeline = true;
        }
    }

    pub(crate) fn add_annotation(&mut self, annotation: WardleyAnnotation) {
        self.annotations.push(annotation);
    }

    pub(crate) fn add_note(&mut self, note: WardleyNote) {
        self.notes.push(note);
    }

    pub(crate) fn add_accelerator(&mut self, accelerator: WardleyForce) {
        self.accelerators.push(accelerator);
    }

    pub(crate) fn add_deaccelerator(&mut self, deaccelerator: WardleyForce) {
        self.deaccelerators.push(deaccelerator);
    }

    pub(crate) fn set_annotations_box(&mut self, x: f64, y: f64) {
        self.annotations_box = Some((x, y));
    }

    /// Sets the stages of the axes, as Pele's setAxes does with the only part populate gives
    /// it.
    pub(crate) fn set_axes(&mut self, stages: Vec<String>, stage_boundaries: Vec<f64>) {
        self.axes.stages = Some(stages);
        self.axes.stage_boundaries = Some(stage_boundaries);
    }

    pub(crate) fn set_size(&mut self, width: f64, height: f64) {
        self.size = Some((width, height));
    }

    pub(crate) fn get_node(&self, id: &str) -> Option<&WardleyNode> {
        self.nodes.get(id)
    }

    /// A name as a node's id, as Pele's resolveNodeId finds it: the name, if a node has it as
    /// its id, or else the id of the first node with it as its label. Pipeline components have
    /// ids like "Parent_Child" and are linked by label.
    pub(crate) fn resolve_node_id(&mut self, name: &str) -> String {
        if self.nodes.contains(name) {
            return name.to_string();
        }
        let nodes = &self.nodes;
        let labels = self.labels.get_or_insert_with(|| {
            let mut labels = HashMap::new();
            for node in nodes.values() {
                labels
                    .entry(node.label.clone())
                    .or_insert_with(|| node.id.clone());
            }
            labels
        });
        labels
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    }

    /// The map the builder holds, as Pele's build gives it. Every node is added with a place,
    /// so none is without one, as Pele's build checks.
    pub(crate) fn build(self) -> WardleyBuildResult {
        WardleyBuildResult {
            nodes: self.nodes.values().cloned().collect(),
            links: self.links,
            trends: self.trends.values().cloned().collect(),
            pipelines: self.pipelines.values().cloned().collect(),
            annotations: self.annotations,
            notes: self.notes,
            accelerators: self.accelerators,
            deaccelerators: self.deaccelerators,
            annotations_box: self.annotations_box,
            axes: self.axes,
            size: self.size,
        }
    }
}
