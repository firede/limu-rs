//! The names of a flowchart's shapes, as Pele's shapes.ts has them.

/// Mermaid's documented shape names: the short name first, then its aliases.
const SHAPES: &[&[&str]] = &[
    &["rect", "proc", "process", "rectangle", "squareRect"],
    &["rounded", "event", "roundedRect"],
    &["stadium", "terminal", "pill"],
    &[
        "fr-rect",
        "subprocess",
        "subproc",
        "framed-rectangle",
        "subroutine",
    ],
    &["cyl", "db", "database", "cylinder"],
    &["datastore", "data-store"],
    &["folder", "directory"],
    &["bucket"],
    &["console"],
    &["browser"],
    &["person"],
    &["circle", "circ"],
    &["bang"],
    &["cloud"],
    &["diam", "decision", "diamond", "question"],
    &["hex", "hexagon", "prepare"],
    &["lean-r", "lean-right", "in-out", "lean_right"],
    &["lean-l", "lean-left", "out-in", "lean_left"],
    &["trap-b", "priority", "trapezoid-bottom", "trapezoid"],
    &[
        "trap-t",
        "manual",
        "trapezoid-top",
        "inv-trapezoid",
        "inv_trapezoid",
    ],
    &["dbl-circ", "double-circle", "doublecircle"],
    &["text"],
    &["notch-rect", "card", "notched-rectangle"],
    &[
        "lin-rect",
        "lined-rectangle",
        "lined-process",
        "lin-proc",
        "shaded-process",
    ],
    &["sm-circ", "start", "small-circle", "stateStart"],
    &["fr-circ", "stop", "framed-circle", "stateEnd"],
    &["fork", "join", "forkJoin"],
    &["hourglass", "collate"],
    &["brace", "comment", "brace-l"],
    &["brace-r"],
    &["braces"],
    &["bolt", "com-link", "lightning-bolt"],
    &["doc", "document"],
    &["delay", "half-rounded-rectangle"],
    &["h-cyl", "das", "horizontal-cylinder"],
    &["lin-cyl", "disk", "lined-cylinder"],
    &["curv-trap", "curved-trapezoid", "display"],
    &[
        "div-rect",
        "div-proc",
        "divided-rectangle",
        "divided-process",
    ],
    &["tri", "extract", "triangle"],
    &["win-pane", "internal-storage", "window-pane"],
    &["f-circ", "junction", "filled-circle"],
    &["notch-pent", "loop-limit", "notched-pentagon"],
    &["flip-tri", "manual-file", "flipped-triangle"],
    &["sl-rect", "manual-input", "sloped-rectangle"],
    &["docs", "documents", "st-doc", "stacked-document"],
    &["st-rect", "procs", "processes", "stacked-rectangle"],
    &["bow-rect", "stored-data", "bow-tie-rectangle"],
    &["cross-circ", "summary", "crossed-circle"],
    &["tag-doc", "tagged-document"],
    &["tag-rect", "tagged-rectangle", "tag-proc", "tagged-process"],
    &["flag", "paper-tape"],
    &["odd", "rect_left_inv_arrow"],
    &["lin-doc", "lined-document"],
];

/// Shapes Mermaid accepts by name without documenting them.
const INTERNAL: &[&str] = &[
    "state",
    "choice",
    "note",
    "composite",
    "rectWithTitle",
    "labelRect",
    "block_arrow",
    "collapsedGroup",
    "iconSquare",
    "iconCircle",
    "icon",
    "iconRounded",
    "imageSquare",
    "anchor",
    "kanbanItem",
    "mindmapCircle",
    "defaultMindmapNode",
    "classBox",
    "erBox",
    "requirementBox",
    "usecaseActor",
    "usecaseActorHollow",
    "usecaseActorAwesome",
    "usecaseActorIcon",
    "usecaseBusiness",
    "usecaseEllipse",
    "usecaseJsonTable",
];

/// The shapes the bracket syntax makes, by their documented names, Pele's SYNTAX.
const SYNTAX: &[(&str, &str)] = &[
    ("square", "rect"),
    ("round", "rounded"),
    ("ellipse", "ellipse"),
    ("hexagon", "hex"),
    ("diamond", "diam"),
];

/// The documented name of a shape's name, or the name of an undocumented one, as Pele's
/// canonical map has it.
fn canonical(name: &str) -> Option<&'static str> {
    SHAPES
        .iter()
        .find(|names| names.contains(&name))
        .map(|names| names[0])
        .or_else(|| INTERNAL.iter().find(|&&n| n == name).copied())
}

/// Whether Mermaid knows a shape of the name, as Pele's isValidShape says.
pub(crate) fn is_valid_shape(name: &str) -> bool {
    canonical(name).is_some()
}

/// The shape a node of a shape's name is drawn as, as Pele's canonicalShape gives it.
pub(crate) fn canonical_shape(name: Option<&str>) -> &'static str {
    let Some(name) = name else { return "rect" };
    SYNTAX
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, s)| *s)
        .or_else(|| canonical(name))
        .unwrap_or("rect")
}
