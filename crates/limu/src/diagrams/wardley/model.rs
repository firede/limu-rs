//! The model of a Wardley map, as Pele's model.ts makes it of the tree.

use super::builder::{
    NodeSet, WardleyAnnotation, WardleyAxesConfig, WardleyBuilder, WardleyForce, WardleyLink,
    WardleyNode, WardleyNote, WardleyPipeline, WardleyTrend,
};
use super::parser::{LabelAst, WardleyAst};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::{drop_indents, trim, trim_start};
use crate::js::number::to_string;

/// A parsed Wardley map, Pele's WardleyModel: what the builder gives, with the titles. What
/// Pele leaves undefined is none. The annotations box is an x and a y, and the size a width
/// and a height.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WardleyModel {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
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

fn semantic(message: String) -> Error {
    Error {
        diagram: Some(DiagramType::Wardley),
        ..Error::new(message, ErrorCode::Semantic)
    }
}

/// A place as a percentage, as Pele's toPercent reads it: values up to 1 are fractions, and
/// larger ones are percentages already.
fn to_percent(value: f64, context: &str) -> Result<f64, Error> {
    let normalized = if value <= 1.0 { value * 100.0 } else { value };
    if normalized < 0.0 || normalized > 100.0 {
        return Err(semantic(format!(
            "{context} must be between 0-1 (decimal) or 0-100 (percentage). Received: {}",
            to_string(value)
        )));
    }
    Ok(normalized)
}

/// A place given by visibility and evolution, as Pele's toCoordinates reads it: x is the
/// evolution, read first, and y the visibility.
fn to_coordinates(visibility: f64, evolution: f64, context: &str) -> Result<(f64, f64), Error> {
    let x = to_percent(evolution, &format!("{context} evolution"))?;
    let y = to_percent(visibility, &format!("{context} visibility"))?;
    Ok((x, y))
}

/// The flow a port on a link gives, as Pele's flowFromPort reads it.
fn flow_from_port(port: Option<&str>) -> Option<&'static str> {
    match port? {
        "+<>" => Some("bidirectional"),
        "+<" => Some("backward"),
        "+>" => Some("forward"),
        _ => None,
    }
}

/// The flow and the label an arrow gives, as Pele's flowFromArrow reads them: only an arrow
/// that starts with a plus has either, and its label is what is between the quotes after the
/// plus. The flow is read from the whole arrow, its label too.
fn flow_from_arrow(arrow: Option<&str>) -> (Option<&'static str>, Option<String>) {
    let Some(arrow) = arrow.filter(|a| a.starts_with('+')) else {
        return (None, None);
    };
    let end = arrow.get(2..).and_then(|rest| rest.find('\''));
    let label = match end {
        Some(end) if arrow.as_bytes().get(1) == Some(&b'\'') => Some(arrow[2..2 + end].to_string()),
        _ => None,
    };
    let flow = if arrow.contains("<>") {
        Some("bidirectional")
    } else if arrow.contains('<') {
        Some("backward")
    } else if arrow.contains('>') {
        Some("forward")
    } else {
        None
    };
    (flow, label)
}

/// A label's offsets, as Pele's offsets gives them. A minus sign before a zero makes it -0,
/// as in JavaScript.
fn offsets(label: Option<&LabelAst>) -> (Option<f64>, Option<f64>) {
    let Some(l) = label else {
        return (None, None);
    };
    let sign = |neg: bool| if neg { -1.0 } else { 1.0 };
    (
        Some(sign(l.neg_x) * l.offset_x),
        Some(sign(l.neg_y) * l.offset_y),
    )
}

/// A node of a place and a class, with nothing else set.
fn node(id: String, label: String, x: f64, y: f64, class_name: &'static str) -> WardleyNode {
    WardleyNode {
        id,
        label,
        x,
        y,
        class_name: Some(class_name),
        label_offset_x: None,
        label_offset_y: None,
        in_pipeline: false,
        is_pipeline_parent: false,
        inertia: None,
        source_strategy: None,
    }
}

/// Fills the builder from the tree, as Pele's populate does, which follows Mermaid's
/// populateDb: each kind of statement in turn, in Mermaid's order.
fn populate(ast: &WardleyAst, db: &mut WardleyBuilder) -> Result<(), Error> {
    if let Some((width, height)) = ast.size {
        db.set_size(width, height);
    }

    if let Some(evolution) = &ast.evolution {
        let stages = evolution
            .iter()
            .map(|stage| match &stage.second_name {
                Some(second) => format!("{} / {}", trim(&stage.name), trim(second)),
                None => trim(&stage.name).to_string(),
            })
            .collect();
        let boundaries = evolution
            .iter()
            .filter_map(|stage| stage.boundary)
            .collect();
        db.set_axes(stages, boundaries);
    }

    for anchor in &ast.anchors {
        let context = format!("Anchor \"{}\"", anchor.name);
        let (x, y) = to_coordinates(anchor.visibility, anchor.evolution, &context)?;
        db.add_node(NodeSet {
            node: node(anchor.name.clone(), anchor.name.clone(), x, y, "anchor"),
            sets_inertia: false,
        });
    }

    for component in &ast.components {
        let context = format!("Component \"{}\"", component.name);
        let (x, y) = to_coordinates(component.visibility, component.evolution, &context)?;
        let (label_offset_x, label_offset_y) = offsets(component.label.as_ref());
        db.add_node(NodeSet {
            node: WardleyNode {
                label_offset_x,
                label_offset_y,
                inertia: Some(component.inertia),
                source_strategy: component.decorator.clone(),
                ..node(
                    component.name.clone(),
                    component.name.clone(),
                    x,
                    y,
                    "component",
                )
            },
            sets_inertia: true,
        });
    }

    for note in &ast.notes {
        let context = format!("Note \"{}\"", note.text);
        let (x, y) = to_coordinates(note.visibility, note.evolution, &context)?;
        db.add_note(WardleyNote {
            text: note.text.clone(),
            x,
            y,
        });
    }

    for pipeline in &ast.pipelines {
        let Some(parent) = db.get_node(&pipeline.parent) else {
            return Err(semantic(format!(
                "Pipeline \"{}\" must reference an existing component with coordinates.",
                pipeline.parent
            )));
        };
        let y = parent.y;
        db.start_pipeline(&pipeline.parent);
        for component in &pipeline.components {
            let id = format!("{}_{}", pipeline.parent, component.name);
            let (label_offset_x, label_offset_y) = offsets(component.label.as_ref());
            let context = format!("Pipeline component \"{}\" evolution", component.name);
            let x = to_percent(component.evolution, &context)?;
            db.add_node(NodeSet {
                node: WardleyNode {
                    label_offset_x,
                    label_offset_y,
                    ..node(
                        id.clone(),
                        component.name.clone(),
                        x,
                        y,
                        "pipeline-component",
                    )
                },
                sets_inertia: true,
            });
            db.add_pipeline_component(&pipeline.parent, &id);
        }
    }

    for link in &ast.links {
        let arrow = link.arrow.as_deref();
        let dashed = arrow.is_some_and(|a| a.contains("-.->") || a.contains(".-."));
        let (arrow_flow, arrow_label) = flow_from_arrow(arrow);
        let flow = flow_from_port(link.from_port.as_deref())
            .or(flow_from_port(link.to_port.as_deref()))
            .or(arrow_flow);
        let source = db.resolve_node_id(&link.from);
        let target = db.resolve_node_id(&link.to);
        db.add_link(WardleyLink {
            source,
            target,
            dashed,
            label: arrow_label.or_else(|| link.link_label.clone()),
            flow,
        });
    }

    for evolve in &ast.evolves {
        if let Some(node) = db.get_node(&evolve.component) {
            let target_y = node.y;
            let context = format!("Evolve target for \"{}\"", evolve.component);
            let target_x = to_percent(evolve.target, &context)?;
            db.add_trend(WardleyTrend {
                node_id: evolve.component.clone(),
                target_x,
                target_y,
            });
        }
    }

    if let Some(&(x, y)) = ast.annotations.first() {
        let (x, y) = to_coordinates(x, y, "Annotations box")?;
        db.set_annotations_box(x, y);
    }

    for annotation in &ast.annotation {
        let context = format!("Annotation {}", to_string(annotation.number));
        let (x, y) = to_coordinates(annotation.x, annotation.y, &context)?;
        db.add_annotation(WardleyAnnotation {
            number: annotation.number,
            coordinates: vec![(x, y)],
            text: annotation.text.clone(),
        });
    }

    for accelerator in &ast.accelerators {
        let context = format!("Accelerator \"{}\"", accelerator.name);
        let (x, y) = to_coordinates(accelerator.x, accelerator.y, &context)?;
        db.add_accelerator(WardleyForce {
            name: accelerator.name.clone(),
            x,
            y,
        });
    }

    for deaccelerator in &ast.deaccelerators {
        let context = format!("Deaccelerator \"{}\"", deaccelerator.name);
        let (x, y) = to_coordinates(deaccelerator.x, deaccelerator.y, &context)?;
        db.add_deaccelerator(WardleyForce {
            name: deaccelerator.name.clone(),
            x,
            y,
        });
    }
    Ok(())
}

/// Makes the model of a map's tree, as Pele's buildWardley does. title is the one its front
/// matter gives, which the text's own takes the place of.
pub(crate) fn build_wardley(ast: WardleyAst, title: Option<&str>) -> Result<WardleyModel, Error> {
    let mut db = WardleyBuilder::default();
    populate(&ast, &mut db)?;
    let b = db.build();
    Ok(WardleyModel {
        title: ast
            .title
            .filter(|t| !t.is_empty())
            .or_else(|| title.map(str::to_string)),
        acc_title: ast
            .acc_title
            .filter(|t| !t.is_empty())
            .map(|t| trim_start(&t).to_string()),
        acc_descr: ast
            .acc_descr
            .filter(|d| !d.is_empty())
            .map(|d| drop_indents(&d)),
        nodes: b.nodes,
        links: b.links,
        trends: b.trends,
        pipelines: b.pipelines,
        annotations: b.annotations,
        notes: b.notes,
        accelerators: b.accelerators,
        deaccelerators: b.deaccelerators,
        annotations_box: b.annotations_box,
        axes: b.axes,
        size: b.size,
    })
}
