//! Swimlane diagrams, as Pele's src/diagrams/swimlane reads and draws them. A swimlane
//! diagram is a flowchart whose top-level subgraphs are lanes. It has the flowchart's syntax
//! and model, and is laid out with lane_layout, each lane drawn as a band the length of the
//! diagram with its title in a band at its start.
//!
//! A lane has the role swimlane.lane. Everything else is drawn by the flowchart's layout and
//! keeps the flowchart's roles, so a host that styles flowcharts styles a swimlane diagram's
//! nodes and edges the same way.

use super::flowchart::{FlowDb, FlowGraph, GraphNode, Variant, layout_variant};
use crate::css::{Author, Resolved};
use crate::graphs::compound::{CEdge, CNode, CompoundResult, Dir};
use crate::graphs::lanes::{LANE_PAD, lane_layout};
use crate::graphs::layered::LayeredOptions;
use crate::layout::Options;
use crate::scene::style::{Paint, Style};
use crate::scene::text::Label;
use crate::scene::{Item, Kind, Painting, Part, Placed, Point, Rect, Scene};
use crate::value::Map;

#[cfg(test)]
mod tests;

/// The role of a lane.
pub(crate) const ROLE_LANE: &str = "swimlane.lane";

/// The id of the lane that holds the nodes outside every lane.
const DEFAULT_LANE: &str = "__swimlane_default__";

/// Parses a swimlane diagram's text, as Pele's swimlane does, with the flowchart's parser.
pub(crate) use super::flowchart::parse;

/// Lays out a parsed swimlane diagram as a scene, as Pele's swimlane renders it: with the
/// flowchart's renderFlowchart and the variant that makes its top-level groups lanes. It
/// reads the "swimlane" section of the configuration, or the "flowchart" section where there
/// is no "swimlane" section.
pub(crate) fn layout(db: &FlowDb, config: &Map, o: &Options) -> Scene {
    layout_variant(db, config, o, Some(&VARIANT))
}

const VARIANT: Variant = Variant {
    diagram: "swimlane",
    prepare,
    layout: lay_lanes,
    draw_group: Some(draw_lane),
    filled_box: Some(lane_band),
};

/// Lays the lanes out, with less room about what each holds where they are laid out tightly,
/// as Pele's variant does.
fn lay_lanes(
    nodes: &mut [CNode],
    edges: &mut [CEdge],
    dir: Dir,
    opt: &LayeredOptions,
    tight: bool,
) -> CompoundResult {
    lane_layout(nodes, edges, dir, opt, if tight { 12.0 } else { LANE_PAD })
}

/// Puts the nodes outside every lane in one unnamed lane, as Mermaid does, and drops the
/// direction of groups, as Pele's prepare does. Lanes all run the same way, so a direction
/// inside a subgraph has no effect.
fn prepare(graph: &mut FlowGraph) {
    let mut loose = false;
    for node in &mut graph.nodes {
        if node.is_group {
            node.dir = None;
        } else if node.parent_id.is_none() {
            loose = true;
        }
    }
    if !loose {
        return;
    }
    for node in &mut graph.nodes {
        if !node.is_group && node.parent_id.is_none() {
            node.parent_id = Some(DEFAULT_LANE.to_string());
        }
    }
    graph.nodes.push(GraphNode {
        id: DEFAULT_LANE.to_string(),
        shape: "rect".to_string(),
        is_group: true,
        ..GraphNode::default()
    });
}

/// Draws a lane, as Pele's drawLane does: an outline the length of the diagram, with its
/// title in a band at the start. Where the lanes run across, the band is at their left and
/// the title is turned to read upwards.
fn draw_lane(
    c: &CNode,
    label: &Label,
    style: &Resolved,
    author: &Author,
    class: Vec<String>,
    id: &str,
    dir: Dir,
) -> Item {
    let left = c.x - c.w / 2.0;
    let top = c.y - c.h / 2.0;
    let sideways = matches!(dir, Dir::LR | Dir::RL);
    let outline = Rect::new(left, top, c.w, c.h);
    let (band, at) = if sideways {
        (
            Rect::new(left, top, c.pad_top, c.h),
            Point::new(left + c.pad_top / 2.0, c.y),
        )
    } else {
        (
            Rect::new(left, top, c.w, c.pad_top),
            Point::new(c.x, top + c.pad_top / 2.0),
        )
    };
    let mut it = Item::new(Kind::Group, ROLE_LANE);
    it.id = Some(id.to_string());
    it.class = class;
    it.bounds = outline;
    it.parts = vec![
        Part::rect(outline, false, Painting::StrokeOnly),
        Part::rect(band, false, Painting::FillAndStroke),
    ];
    // The title lies on the band, and takes the color that stands out on a fill its author
    // sets.
    it.style = Style {
        fill: Some(Paint::SURFACE_ALT),
        stroke: Some(Paint::BORDER),
        text: Some(Paint::MUTED),
        on_fill: style.inks_text(style),
        ..Style::default()
    };
    it.author = author.style.clone();
    if !label.is_empty() {
        let mut placed = Placed::new(label.clone(), at);
        if sideways {
            placed.rotate = -90.0;
        }
        it.label = Some(placed);
    }
    it
}

/// The band of a lane that takes the fill its author sets, as left, top, right and bottom, as
/// Pele's laneBand gives it.
fn lane_band(c: &CNode, dir: Dir) -> [f64; 4] {
    let left = c.x - c.w / 2.0;
    let top = c.y - c.h / 2.0;
    if matches!(dir, Dir::LR | Dir::RL) {
        [left, top, left + c.pad_top, top + c.h]
    } else {
        [left, top, left + c.w, top + c.pad_top]
    }
}
