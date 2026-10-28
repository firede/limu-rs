//! The shapes of an agent flow, as Pele's shapes.ts has them: the names its authors give the
//! shapes that mean something here, the flowchart shapes it no longer draws, and those it
//! draws.

use super::diagnostics::DiagnosticId;
use super::graph::GraphNode;

/// The names authors give the shapes that carry meaning in an agent flow, and the shapes they
/// stand for, Pele's SHAPE_ALIASES.
const SHAPE_ALIASES: [(&str, &str); 7] = [
    ("task", "roundedRect"),
    ("tool", "subroutine"),
    ("input", "lean-right"),
    ("decision", "diamond"),
    ("refdoc", "lin-doc"),
    ("action", "hexagon"),
    ("round", "rect"),
];

/// The flowchart shapes agent flows drew once and no longer do, Pele's REMOVED_SHAPES.
const REMOVED_SHAPES: [&str; 27] = [
    "doc",
    "stadium",
    "terminal",
    "circle",
    "trapezoid",
    "inv_trapezoid",
    "inv-trapezoid",
    "doublecircle",
    "double-circle",
    "typeDeclaration",
    "procs",
    "lean_left",
    "lean-left",
    "in-out",
    "cylinder",
    "ellipse",
    "odd",
    "tag-rect",
    "tagged-rectangle",
    "delay",
    "half-rounded-rectangle",
    "lin-rect",
    "lined-rectangle",
    "win-pane",
    "window-pane",
    "curv-trap",
    "curved-trapezoid",
];

/// The shapes an agent flow draws, Pele's ALLOWED_SHAPES.
const ALLOWED_SHAPES: [&str; 13] = [
    "roundedRect",
    "subroutine",
    "subprocess",
    "subproc",
    "framed-rectangle",
    "lean-right",
    "diamond",
    "lin-doc",
    "lined-document",
    "hexagon",
    "hex",
    "connector",
    "collapsedGroup",
];

/// The shape a node is drawn as where an agent flow does not draw its own, Pele's
/// DEFAULT_SHAPE.
const DEFAULT_SHAPE: &str = "roundedRect";

/// The shape a name stands for, as Pele's resolveShapeAlias gives it: an alias's shape, and
/// any other name as it is.
pub(crate) fn resolve_shape_alias(shape: Option<&str>) -> Option<&str> {
    match shape {
        Some(s) if !s.is_empty() => Some(
            SHAPE_ALIASES
                .iter()
                .find(|(alias, _)| *alias == s)
                .map_or(s, |(_, shape)| shape),
        ),
        _ => shape,
    }
}

/// Whether a node's metadata may name a shape, as Pele's isValidShape says: a flowchart knows
/// it. Mermaid looks the name up with `in` on a plain object, so `constructor` passes too.
pub(crate) fn is_valid_shape(name: &str) -> bool {
    crate::diagrams::flowchart::is_valid_shape(name) || name == "constructor"
}

/// A shape found that is not drawn, as Pele's normaliseNodeShapes reports it to its
/// ShapeSink: the id of the finding, whether it is an error rather than a warning, its
/// message, and the node it is of.
pub(crate) struct ShapeFinding {
    pub(crate) id: DiagnosticId,
    pub(crate) error: bool,
    pub(crate) message: String,
    pub(crate) node_id: String,
}

/// Reduces the shape of every node to one an agent flow draws, as Pele's normaliseNodeShapes
/// does, and gives what it finds of the shapes that were removed and those not supported.
pub(crate) fn normalise_node_shapes(nodes: &mut [GraphNode]) -> Vec<ShapeFinding> {
    let mut found = Vec::new();
    for node in nodes {
        if node.is_group {
            continue;
        }
        if matches!(node.shape.as_str(), "" | "squareRect" | "rect") {
            node.shape = DEFAULT_SHAPE.into();
        }
        let (id, error, message) = if REMOVED_SHAPES.contains(&node.shape.as_str()) {
            let message = format!(
                "shape \"{}\" was removed in v0.8.1, using \"{DEFAULT_SHAPE}\"",
                node.shape
            );
            (DiagnosticId::ShapeRemoved, true, message)
        } else if !ALLOWED_SHAPES.contains(&node.shape.as_str()) {
            let message = format!(
                "shape \"{}\" is not supported, using \"{DEFAULT_SHAPE}\"",
                node.shape
            );
            (DiagnosticId::ShapeUnsupported, false, message)
        } else {
            continue;
        };
        found.push(ShapeFinding {
            id,
            error,
            message,
            node_id: node.id.clone(),
        });
        node.shape = DEFAULT_SHAPE.into();
    }
    found
}
