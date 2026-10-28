//! The grid a C4 diagram's shapes and boundaries are laid out on, as Pele's layout.ts lays it
//! out, as Mermaid does: in each boundary its shapes in rows, then the boundaries inside it.

use std::collections::HashMap;

use super::db::{C4Db, C4Shape, ElementRef};
use super::route::{Box, HEAD, LABEL_PAD, LANE, NARROW};
use crate::js::chars::{to_lower, trim};
use crate::js::math::{max, min};
use crate::scene::MARKER_SIZE;
use crate::scene::text::{Face, Label};
use crate::text::{Setting, layout_label};

const SHAPE_W: f64 = 216.0;
const SHAPE_MIN_H: f64 = 64.0;
const PAD_X: f64 = 14.0;
const PAD_Y: f64 = 12.0;
// Room between two columns for the label of a line across them, clear of an arrowhead on
// either side, with an arrowhead's length to spare for a word a little wider than the label
// room.
const GAP_X: f64 = NARROW + LABEL_PAD + 3.0 * MARKER_SIZE;
const GAP_Y: f64 = 88.0;
// Room for a relation to run along the inside of a boundary.
const GROUP_PAD: f64 = LANE + 8.0;
const GROUP_GAP: f64 = 48.0;
/// Where a boundary's heading starts, from its top-left corner.
pub(crate) const HEAD_LEFT: f64 = 16.0;
pub(crate) const HEAD_TOP: f64 = 10.0;

/// The shared shape a shape keyword Mermaid accepts in $shape, $sprite or $tags names, as
/// Pele's FORMS has it.
fn form_named(name: &str) -> Option<&'static str> {
    Some(match name {
        "person" => "person",
        "box" | "rounded" | "component" => "rect",
        "cylinder" | "database" | "db" => "cyl",
        "queue" | "pipe" => "h-cyl",
        _ => return None,
    })
}

/// A shape laid out, Pele's Node. Its labels are its type, name, technology and
/// description.
pub(crate) struct Node {
    pub(crate) b: Box,
    /// The shape's place among the diagram's shapes.
    pub(crate) shape: usize,
    pub(crate) labels: [Label; 4],
    pub(crate) text_h: f64,
}

/// A boundary laid out, Pele's Group. Its labels are its name, type and description, and
/// none for the top level. Its kids are boundaries, by their place among the diagram's.
pub(crate) struct Group {
    pub(crate) b: Box,
    pub(crate) kids: Vec<usize>,
    pub(crate) nodes: Vec<Node>,
    pub(crate) labels: Vec<Label>,
    pub(crate) head: f64,
    pub(crate) head_w: f64,
    pub(crate) head_x: f64,
}

/// The diagram laid out, Pele's Layout: a group for each boundary, in the diagram's order,
/// the boundaries that are drawn, parents before children, the first the unnamed top level,
/// and the box of each shape and boundary.
pub(crate) struct Layout {
    pub(crate) groups: Vec<Group>,
    pub(crate) order: Vec<usize>,
    pub(crate) boxes: HashMap<ElementRef, Box>,
}

/// The shared shape a shape is drawn as, as Pele's formOf picks it.
fn form_of(shape: &C4Shape) -> &'static str {
    let named = |s: &Option<String>| form_named(&to_lower(s.as_deref().unwrap_or("")));
    if let Some(form) = named(&shape.el.shape).or_else(|| named(&shape.el.sprite)) {
        return form;
    }
    if let Some(tags) = shape.el.tags.as_deref().filter(|t| !t.is_empty()) {
        for tag in tags.split(',') {
            if let Some(form) = form_named(&to_lower(trim(tag))) {
                return form;
            }
        }
    }
    let kind = shape.kind.as_str();
    if kind.ends_with("person") {
        "person"
    } else if kind.ends_with("_db") {
        "cyl"
    } else if kind.ends_with("_queue") {
        "h-cyl"
    } else {
        "rect"
    }
}

/// A text in brackets, or nothing for none, as Pele writes a technology or a type.
pub(crate) fn bracketed(s: Option<&str>) -> String {
    match s {
        Some(s) if !s.is_empty() => format!("[{s}]"),
        _ => String::new(),
    }
}

/// Lays out a shape's labels and sizes it about them, as Pele's measure does.
fn measure(db: &C4Db, at: usize, set: &Setting, size: f64, small: f64, width: f64) -> Node {
    let shape = &db.shapes[at];
    let form = form_of(shape);
    let cap_w = if form == "h-cyl" { 36.0 } else { 0.0 };
    let cap_h = match form {
        "person" => HEAD,
        "cyl" => 24.0,
        _ => 0.0,
    };
    let inner = width - 2.0 * PAD_X - cap_w;
    let el = &shape.el;
    let kind = format!("«{}»", shape.kind.replace('_', " "));
    let labels = [
        layout_label(&kind, false, set, small - 1.0, inner, Face::REGULAR),
        layout_label(&el.label, false, set, size, inner, Face::BOLD),
        layout_label(
            &bracketed(el.techn.as_deref()),
            false,
            set,
            small,
            inner,
            Face::REGULAR,
        ),
        layout_label(
            el.descr.as_deref().unwrap_or(""),
            false,
            set,
            small,
            inner,
            Face::REGULAR,
        ),
    ];
    let mut tw = 0.0;
    let mut text_h = if labels[3].height > 0.0 { 4.0 } else { 0.0 };
    for label in &labels {
        tw = max(tw, label.width);
        text_h += label.height;
    }
    Node {
        b: Box {
            x: 0.0,
            y: 0.0,
            w: max(width, tw + 2.0 * PAD_X + cap_w),
            h: max(SHAPE_MIN_H, text_h + 2.0 * PAD_Y + cap_h),
            form,
            id: at,
        },
        shape: at,
        labels,
        text_h,
    }
}

/// Places boxes left to right in rows of per_row, from (0, 0), and gives the width and
/// height the rows take, as Pele's rows does. Shapes are stretched to the height of their
/// row, and boundaries keep their own. per_row is a count as parseInt reads it, which may be
/// past any integer.
fn rows(boxes: &mut [Box], per_row: f64, gap_x: f64, gap_y: f64, stretch: bool) -> (f64, f64) {
    let mut width = 0.0;
    let mut y = 0.0;
    let count = boxes.len() as f64;
    let mut from = 0.0;
    while from < count {
        let to = min(count, from + per_row) as usize;
        let row = &mut boxes[from as usize..to];
        let mut x = 0.0;
        let mut row_h = 0.0;
        for b in row.iter() {
            row_h = max(row_h, b.h);
        }
        for b in row.iter_mut() {
            b.x = x;
            b.y = y;
            if stretch {
                b.h = row_h;
            }
            x += b.w + gap_x;
        }
        width = max(width, x - gap_x);
        y += row_h + gap_y;
        from += per_row;
    }
    (width, if boxes.is_empty() { 0.0 } else { y - gap_y })
}

/// Lays the diagram out on Mermaid's grid, as Pele's layoutC4 does: in each boundary, its
/// shapes in rows of c4ShapeInRow, then the boundaries inside it in rows of
/// c4BoundaryInRow, each sized around its own content. With room, it is laid out to fit
/// that width, with gap between its columns.
pub(crate) fn layout_c4(
    db: &C4Db,
    set: &Setting,
    size: f64,
    small: f64,
    room: Option<f64>,
    gap: f64,
) -> Layout {
    let group_pad = if room.is_none() { GROUP_PAD } else { 12.0 };
    let first_boundary = db.shapes.len();
    let mut groups: Vec<Group> = Vec::with_capacity(db.boundaries.len());
    for (at, boundary) in db.boundaries.iter().enumerate() {
        let label_room = room.map_or(4000.0, |room| {
            max(80.0, room - 2.0 * HEAD_LEFT - 2.0 * GROUP_PAD * 2.0)
        });
        let el = &boundary.el;
        let labels = if el.parent.is_some() {
            vec![
                layout_label(&el.label, false, set, size, label_room, Face::BOLD),
                layout_label(
                    &bracketed(el.r#type.as_deref()),
                    false,
                    set,
                    small,
                    label_room,
                    Face::REGULAR,
                ),
                layout_label(
                    el.descr.as_deref().unwrap_or(""),
                    false,
                    set,
                    small,
                    label_room,
                    Face::REGULAR,
                ),
            ]
        } else {
            Vec::new()
        };
        let mut head = 0.0;
        let mut head_w = 0.0;
        for label in &labels {
            head += label.height;
            head_w = max(head_w, label.width);
        }
        groups.push(Group {
            b: Box {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
                form: "rect",
                id: first_boundary + at,
            },
            kids: Vec::new(),
            nodes: Vec::new(),
            labels,
            head: if head > 0.0 {
                head + HEAD_TOP + 6.0 + LANE
            } else {
                group_pad
            },
            head_w,
            head_x: HEAD_LEFT,
        });
    }
    for (at, boundary) in db.boundaries.iter().enumerate() {
        if let Some(parent) = boundary.el.parent {
            groups[parent].kids.push(at);
        }
    }
    // A boundary that ends up inside itself is not reachable from the top, as in Mermaid.
    let mut order = vec![0];
    let mut depths: HashMap<usize, f64> = HashMap::from([(0, 0.0)]);
    let mut i = 0;
    while i < order.len() {
        let at = order[i];
        for k in 0..groups[at].kids.len() {
            let kid = groups[at].kids[k];
            depths.insert(kid, depths[&at] + 1.0);
            order.push(kid);
        }
        i += 1;
    }
    for (at, shape) in db.shapes.iter().enumerate() {
        if let Some(parent) = shape.el.parent {
            let depth = depths.get(&parent).copied().unwrap_or(0.0);
            let node_width = room.map_or(SHAPE_W, |room| {
                max(100.0, min(SHAPE_W, room - 2.0 * depth * group_pad - 32.0))
            });
            let node = measure(db, at, set, size, small, node_width);
            groups[parent].nodes.push(node);
        }
    }

    let per_row = max(1.0, db.shape_in_row);
    let groups_per_row = max(1.0, db.boundary_in_row);
    let mut kids_top: HashMap<usize, f64> = HashMap::new();
    for &at in order.iter().rev() {
        let gap_x = if room.is_none() { GAP_X } else { gap };
        // How many boxes go in a row: limit, or with room as many of the widest box as fit
        // it, as the count of Pele's layoutC4 gives it.
        let count = |boxes: &[Box], limit: f64, gap: f64| {
            let Some(room) = room else {
                return limit;
            };
            let mut widest = 1.0;
            for b in boxes {
                widest = max(widest, b.w);
            }
            max(
                1.0,
                min(
                    limit,
                    ((room + gap - 2.0 * group_pad) / (widest + gap)).floor(),
                ),
            )
        };
        let mut nodes: Vec<Box> = groups[at].nodes.iter().map(|n| n.b).collect();
        let mut kids: Vec<Box> = groups[at].kids.iter().map(|&k| groups[k].b).collect();
        let node_count = count(&nodes, per_row, gap_x);
        let (grid_w, grid_h) = rows(&mut nodes, node_count, gap_x, GAP_Y, true);
        let kid_count = count(&kids, groups_per_row, GROUP_GAP);
        let (kids_w, kids_h) = rows(&mut kids, kid_count, GROUP_GAP, GROUP_GAP, false);
        for (n, b) in groups[at].nodes.iter_mut().zip(nodes) {
            n.b = b;
        }
        for (k, b) in groups[at].kids.clone().into_iter().zip(kids) {
            groups[k].b = b;
        }
        let between = if grid_h > 0.0 && kids_h > 0.0 {
            GROUP_GAP
        } else {
            0.0
        };
        let frame = if at == 0 { 0.0 } else { group_pad };
        kids_top.insert(at, grid_h + between);
        let g = &mut groups[at];
        g.b.w = max(
            max(grid_w, kids_w),
            g.head_w + 2.0 * HEAD_LEFT - 2.0 * frame,
        ) + 2.0 * frame;
        let top = if frame != 0.0 { g.head } else { 0.0 };
        g.b.h = top + grid_h + between + kids_h + frame;
    }

    let mut boxes = HashMap::new();
    for &at in &order {
        let g = &mut groups[at];
        let (left, top) = if at == 0 {
            (g.b.x, g.b.y)
        } else {
            (g.b.x + group_pad, g.b.y + g.head)
        };
        for n in &mut g.nodes {
            n.b.x += left;
            n.b.y += top;
            boxes.insert(ElementRef::Shape(n.shape), n.b);
        }
        let below = top + kids_top[&at];
        for k in groups[at].kids.clone() {
            groups[k].b.x += left;
            groups[k].b.y += below;
        }
        if at != 0 {
            boxes.insert(ElementRef::Boundary(at), groups[at].b);
        }
    }
    Layout {
        groups,
        order,
        boxes,
    }
}
