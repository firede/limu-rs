//! A mindmap laid out as a scene, as Pele's renderMindmap in render.ts draws it.

use super::db::{MindmapDb, MindmapNode};
use crate::common::{
    class_names, has_icon, icon_svg, setting, title_label, title_room, turn_to_fit,
};
use crate::draw::{draw_shape, shape_inset, shape_size};
use crate::js::math::{max, min, round};
use crate::layout::Options;
use crate::scene::style::{Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{
    Align, Edge, Item, Kind, Painting, Part, Path, Placed, Point, Rect, Scene, Shape,
};
use crate::text::layout_label;
use crate::value::{Map, Value};

/// The roles of a mindmap's items: the title from front matter, the root, every other node,
/// and the link from a node to each of its children.
pub(crate) const ROLE_TITLE: &str = "mindmap.title";
pub(crate) const ROLE_ROOT: &str = "mindmap.root";
pub(crate) const ROLE_NODE: &str = "mindmap.node";
pub(crate) const ROLE_EDGE: &str = "mindmap.edge";

/// The shapes of Mermaid's node types, by index, as Pele's SHAPES has them. A node without
/// delimiters has none: it is text on a line, except the root.
const SHAPES: [&str; 7] = ["", "rounded", "rect", "circle", "cloud", "bang", "hex"];

/// The root's shape when its text gives it none, Pele's ROOT_SHAPE.
pub(crate) const ROOT_SHAPE: &str = "stadium";

// The spacing of a map, as Pele's render.ts has it.
const GAP_X: f64 = 36.0;
const GAP_Y: f64 = 8.0;
/// Between two subtrees, of which at least one has children of its own.
const GROUP_GAP: f64 = 18.0;
const BRANCH_GAP_X: f64 = 52.0;
const BRANCH_GAP_Y: f64 = 28.0;
const TEXT_PAD: f64 = 6.0;
const PILL_PAD: f64 = 12.0;
pub(crate) const TINT: f64 = 0.22;
const ICON_GAP: f64 = 6.0;
// Running down, each level is set in by INDENT, and links leave a node STEM from its left
// edge.
const INDENT: f64 = 28.0;
const STEM: f64 = 12.0;
const BEND: f64 = 8.0;
const ROW_GAP: f64 = 10.0;

/// A shape w by h drawn about the origin and moved to cx, cy, as Pele draws a node's shape in
/// the node's own frame.
pub(crate) fn outline(shape: &str, cx: f64, cy: f64, w: f64, h: f64) -> Vec<Part> {
    let mut frame = Item::new(Kind::Node, ROLE_NODE);
    frame.parts = draw_shape(shape, 0.0, 0.0, w, h);
    frame.move_by(cx, cy);
    frame.parts
}

/// Lays a mindmap out as a scene, as Pele's renderMindmap draws it: the first-level branches
/// spread to the right of the root and to its left, split where the two sides are closest in
/// height, each subtree in a band of its own. A map wider than the width the host has is laid
/// out as an outline instead when that is narrower: the root at the top, and each level set
/// in under its parent.
///
/// Each first-level branch takes a series color, that of its section, which its links are
/// stroked in and its nodes outlined in and filled with at 22%. A node with no shape of its
/// own is plain text where its branch ends, and text in a pill, a stadium, where the branch
/// goes on from it. The root is in the theme's surface and border. Every item of a branch has
/// the tag "section-" and its section, as Pele's group about the branch has that class. A
/// node's icon is a picture beside its text, where the host can draw it. One it cannot draw
/// takes no room.
pub(crate) fn layout(m: &MindmapDb, config: &Map, o: &Options) -> Scene {
    turn_to_fit(o, |down| draw(m, config, o, down))
}

/// The subtrees of a list of nodes stacked top to bottom, from and to indexes in it, as
/// Pele's stack stacks them: each one's offset among them, in rel, the height they take
/// together, and the height at which their parent's anchor belongs.
struct Bands<'a> {
    nodes: &'a [MindmapNode],
    extent: Vec<f64>,
    rel: Vec<f64>,
    own: Vec<f64>,
    kids: Vec<f64>,
    anchor: &'a [f64],
}

impl Bands<'_> {
    /// Stacks the subtrees, and gives the height they take and where their parent's anchor
    /// belongs.
    fn stack(&mut self, list: &[usize], from: usize, to: usize, gap: f64) -> (f64, f64) {
        let mut y = 0.0;
        let mut last = 0.0;
        for k in from..to {
            let c = list[k];
            self.rel[c] = y;
            let grouped = !self.nodes[c].children.is_empty()
                || (k + 1 < to && !self.nodes[list[k + 1]].children.is_empty());
            last = if grouped { max(gap, GROUP_GAP) } else { gap };
            y += self.extent[c] + last;
        }
        let (a, b) = (list[from], list[to - 1]);
        let (rel, own, anchor) = (&self.rel, &self.own, self.anchor);
        let middle = (rel[a] + own[a] + anchor[a] + rel[b] + own[b] + anchor[b]) / 2.0;
        (y - last, middle)
    }
}

/// Lays the map out, as Pele's draw does. down lays it out as an outline, far narrower than
/// branches spreading to both sides, and as tall as it needs to be.
fn draw(m: &MindmapDb, config: &Map, o: &Options, down: bool) -> Scene {
    let size = o.font_size;
    let set = setting(o);
    let pad = o.padding;
    let max_node_width = config
        .get("mindmap")
        .and_then(Value::as_object)
        .and_then(|c| c.get("maxNodeWidth"))
        .and_then(Value::as_f64);
    let wrap = match max_node_width {
        Some(v) if v > 0.0 => min(v, 2000.0),
        _ => 200.0,
    };
    let glyph = size * 1.25;

    let nodes = &m.nodes;
    let n = nodes.len();
    let mut parent = vec![0; n];
    let mut depth = vec![0usize; n];
    for i in 0..n {
        for &c in &nodes[i].children {
            parent[c] = i;
            depth[c] = depth[i] + 1;
        }
    }
    // An outline has the width the host gives it, less what its level is set in.
    let room = if down && o.max_width > 0.0 {
        o.max_width - 2.0 * pad
    } else {
        f64::INFINITY
    };
    let mut labels = Vec::with_capacity(n);
    let mut shapes = Vec::with_capacity(n);
    let mut w = vec![0.0; n];
    let mut h = vec![0.0; n];
    let mut dy = vec![0.0; n];
    // Where links meet the node, measured down from its top: its middle.
    let mut anchor = vec![0.0; n];
    // Whether the host has the node's icon. One it does not have takes no room.
    let mut drawn = vec![false; n];

    for (i, node) in nodes.iter().enumerate() {
        drawn[i] = node.icon.as_deref().is_some_and(|name| has_icon(o, name));
        let reserved = if drawn[i] { glyph + ICON_GAP } else { 0.0 };
        let fit = min(
            wrap,
            max(
                72.0,
                room - depth[i] as f64 * INDENT - 2.0 * TEXT_PAD - reserved,
            ),
        );
        let label = layout_label(&node.descr, true, &set, size, fit, Face::REGULAR);
        let tw = label.width
            + if drawn[i] {
                glyph + if label.width > 0.0 { ICON_GAP } else { 0.0 }
            } else {
                0.0
            };
        let th = max(label.height, round(size * 1.5));
        let mut shape = match SHAPES.get(node.kind).copied() {
            Some("") | None if i == 0 => ROOT_SHAPE,
            Some(s) => s,
            None => "",
        };
        // In an outline the root heads a column, and the trunk drops from under its left end.
        // A circle or a cloud would stand wide of that, so the root is a pill there unless it
        // is a box already.
        if down && i == 0 && shape != "rect" && shape != "rounded" {
            shape = ROOT_SHAPE;
        }
        shapes.push(shape);
        if shape.is_empty() {
            // A node with no shape of its own is plain text when it ends a branch, and text in
            // a tinted pill when the branch goes on from it, which sets the levels apart.
            let pill = !node.children.is_empty();
            w[i] = tw + 2.0 * if pill { PILL_PAD } else { TEXT_PAD };
            h[i] = th + if pill { 6.0 } else { 0.0 };
            anchor[i] = h[i] / 2.0;
        } else {
            let s = shape_size(shape, tw, th);
            (w[i], h[i], dy[i]) = (s.w, s.h, s.dy);
            anchor[i] = s.h / 2.0;
        }
        labels.push(label);
    }

    // Each subtree takes a band of height extent. rel is a subtree's offset among its
    // siblings, own the node's offset inside its band, and kids the offset of its children's
    // bands.
    let mut bands = Bands {
        nodes,
        extent: vec![0.0; n],
        rel: vec![0.0; n],
        own: vec![0.0; n],
        kids: vec![0.0; n],
        anchor: &anchor,
    };

    // Nodes are in depth-first order, so going backwards visits children before their
    // parent.
    for i in (1..n).rev() {
        let children = &nodes[i].children;
        if children.is_empty() {
            bands.extent[i] = h[i];
            continue;
        }
        let (stacked, middle) = bands.stack(children, 0, children.len(), GAP_Y);
        let top = middle - anchor[i];
        let low = min(0.0, top);
        bands.extent[i] = max(stacked, top + h[i]) - low;
        bands.own[i] = top - low;
        bands.kids[i] = -low;
    }

    let mut x = vec![0.0; n];
    let mut y = vec![0.0; n];
    let mut band = vec![0.0; n];
    let mut side = vec![0.0; n];
    // Running down: how far the trunk under each node has been drawn.
    let mut trunk = vec![0.0; n];

    if down {
        // Nodes are in depth-first order, which is the order of an outline's rows.
        let mut row = 0.0;
        for i in 0..n {
            if i > 0 && parent[i] == 0 {
                row += ROW_GAP;
            }
            x[i] = depth[i] as f64 * INDENT + w[i] / 2.0;
            y[i] = row;
            row += h[i] + ROW_GAP;
        }
    } else if n > 0 {
        // The first branches go right and the rest left, split where the two sides are
        // closest in height.
        let branches = &nodes[0].children;
        let count = branches.len();
        let mut total = 0.0;
        for &b in branches {
            total += bands.extent[b];
        }
        let mut split = count;
        let mut best = f64::INFINITY;
        let mut right = 0.0;
        for k in 1..count {
            right += bands.extent[branches[k - 1]];
            let diff = (2.0 * right - total + (2.0 * k as f64 - count as f64) * BRANCH_GAP_Y).abs();
            if diff < best {
                best = diff;
                split = k;
            }
        }
        let (mut from, mut to, mut dir) = (0, split, 1.0);
        while from < count {
            let (_, middle) = bands.stack(branches, from, to, BRANCH_GAP_Y);
            for &c in &branches[from..to] {
                band[c] = bands.rel[c] - middle;
                side[c] = dir;
            }
            (from, to, dir) = (to, count, -1.0);
        }
        y[0] = -h[0] / 2.0;
        for i in 0..n {
            if i > 0 {
                y[i] = band[i] + bands.own[i];
            }
            for &c in &nodes[i].children {
                if i > 0 {
                    side[c] = side[i];
                    band[c] = band[i] + bands.kids[i] + bands.rel[c];
                }
                let gap = if i > 0 { GAP_X } else { BRANCH_GAP_X };
                x[c] = x[i] + side[c] * (w[i] / 2.0 + gap + w[c] / 2.0);
            }
        }
    }

    let (mut min_x, mut max_x, mut min_y, mut max_y) = (0.0, 0.0, 0.0, 0.0);
    let mut items: Vec<Item> = Vec::new();
    let mut edges: Vec<Item> = Vec::new();
    let mut body: Vec<Item> = Vec::new();
    let mut color: Option<Paint> = None;
    let mut branch_class = String::new();
    // How far node i's shape lies in from its box on a side, none for a node with no shape
    // of its own.
    let inset = |shape: &str, i: usize, on: usize| {
        if shape.is_empty() {
            0.0
        } else {
            shape_inset(shape, w[i], h[i], on, 0.0)
        }
    };

    // The root is drawn last, on top of the links that leave it.
    for k in 1..=n {
        let i = k % n;
        let node = &nodes[i];
        let shape = shapes[i];
        let label = &labels[i];
        let cx = x[i];
        let top = y[i];
        min_x = min(min_x, cx - w[i] / 2.0);
        max_x = max(max_x, cx + w[i] / 2.0);
        min_y = min(min_y, top);
        max_y = max(max_y, top + h[i]);

        // Pele draws each first-level branch in a group of its own, its links first.
        if i == 0 || parent[i] == 0 {
            items.append(&mut edges);
            items.append(&mut body);
        }
        if i > 0 && parent[i] == 0 {
            let section = node.section.unwrap_or(0);
            color = Some(Paint::series(section));
            branch_class = format!("section-{section}");
        }
        if i > 0 {
            let p = parent[i];
            let mut path = Path::new();
            let route = if down {
                // The link drops from under its parent and turns in to the node's left. It
                // starts where the link to the sibling above turned in, so each stretch of a
                // shared trunk has one color. Under the root's rounded end, the trunk starts
                // where that end has come down to the bottom.
                let stem = if p == 0 {
                    min(max(STEM, h[0] / 2.0), INDENT - BEND)
                } else {
                    STEM
                };
                let x1 = x[p] - w[p] / 2.0 + min(stem, w[p] / 2.0);
                let y1 = if trunk[p] == 0.0 || f64::is_nan(trunk[p]) {
                    y[p] + h[p]
                } else {
                    trunk[p]
                };
                let left = cx - w[i] / 2.0;
                let x2 = left + inset(shape, i, 3);
                let y2 = top + anchor[i];
                let bend = max(0.0, min(min(BEND, x2 - x1), y2 - y1));
                // The next link carries on from where this one leaves the trunk, so the trunk
                // has no gap.
                trunk[p] = y2 - bend;
                path.move_to(x1, y1)
                    .line_to(x1, y2 - bend)
                    .quad_to(x1, y2, x1 + bend, y2)
                    .line_to(x2, y2);
                vec![Point::new(x1, y1), Point::new(x1, y2), Point::new(x2, y2)]
            } else {
                let dir = side[i];
                let from = if dir > 0.0 { 1 } else { 3 };
                let x1 = x[p] + dir * (w[p] / 2.0 - inset(shapes[p], p, from));
                let y1 = y[p] + anchor[p];
                let near = cx - (dir * w[i]) / 2.0;
                let x2 = near
                    + if shape.is_empty() {
                        0.0
                    } else {
                        dir * inset(shape, i, 4 - from)
                    };
                let y2 = top + anchor[i];
                let mid = (x1 + x2) / 2.0;
                path.move_to(x1, y1).cube_to(mid, y1, mid, y2, x2, y2);
                vec![Point::new(x1, y1), Point::new(x2, y2)]
            };
            let mut it = Item::new(
                Kind::Edge(Edge {
                    route,
                    ends: [None, None],
                }),
                ROLE_EDGE,
            );
            it.tags = vec![branch_class.clone()];
            it.bounds = path.bounds();
            it.parts = vec![Part::path(path, Painting::StrokeOnly)];
            it.style = Style {
                stroke: color,
                stroke_width: Some(1.5),
                ..Style::default()
            };
            edges.push(it);
        }

        let pill = shape.is_empty() && !node.children.is_empty();
        let cy = top + h[i] / 2.0;
        let mut it = Item::new(Kind::Node, if i == 0 { ROLE_ROOT } else { ROLE_NODE });
        it.id = Some(node.node_id.clone());
        it.bounds = Rect::new(cx - w[i] / 2.0, top, w[i], h[i]);
        it.class = class_names(node.class.as_deref().unwrap_or(""));
        it.style = if i == 0 {
            Style {
                fill: Some(Paint::SURFACE),
                stroke: Some(Paint::BORDER),
                text: Some(Paint::FG),
                ..Style::default()
            }
        } else {
            // Every node off the root is filled with a tint of its branch's color and
            // outlined in it, whether its shape was given in the source or is the pill of a
            // node that branches. The root is neutral.
            it.tags = vec![branch_class.clone()];
            Style {
                fill: color,
                fill_opacity: Some(TINT),
                stroke: color,
                text: Some(Paint::FG),
                ..Style::default()
            }
        };
        if !shape.is_empty() {
            it.shape = Some(Shape::new(shape));
            it.parts = outline(shape, cx, cy, w[i], h[i]);
        } else if pill {
            it.shape = Some(Shape::new(ROOT_SHAPE));
            it.parts = outline(ROOT_SHAPE, cx, cy, w[i], h[i]);
        } else {
            it.shape = Some(Shape::new("text"));
            it.style = Style {
                text: Some(Paint::FG),
                ..Style::default()
            };
        }
        let mut shift = 0.0;
        if drawn[i]
            && let Some(icon) = &node.icon
        {
            shift = (glyph + if label.width > 0.0 { ICON_GAP } else { 0.0 }) / 2.0;
            it.picture = Some(icon_svg(
                icon,
                cx + (-shift - label.width / 2.0),
                cy + (dy[i] - glyph / 2.0),
                glyph,
            ));
        }
        if !label.is_empty() {
            it.label = Some(if down && shape.is_empty() {
                // An outline's text starts at the node's left, where centered lines would
                // look ragged.
                Placed {
                    align: Align::Start,
                    ..Placed::new(
                        label.clone(),
                        Point::new(cx + (shift - label.width / 2.0), cy + dy[i]),
                    )
                }
            } else {
                Placed::new(label.clone(), Point::new(cx + shift, cy + dy[i]))
            });
        }
        if i > 0 {
            body.push(it);
        } else {
            items.push(it);
        }
    }

    let title: Label = title_label(m.title.as_deref(), &set, size, title_room(o));
    let title_height = if title.height > 0.0 {
        title.height + 16.0
    } else {
        0.0
    };
    let width = max(max_x - min_x, title.width);
    let mut sc = Scene {
        diagram: "mindmap".into(),
        width: (width + 2.0 * pad).ceil(),
        height: (max_y - min_y + title_height + 2.0 * pad).ceil(),
        font_size: size,
        title: None,
        desc: None,
        items: Vec::new(),
    };
    let ox = pad - min_x + (width - max_x + min_x) / 2.0;
    let oy = pad + title_height - min_y;
    for it in &mut items {
        it.move_by(ox, oy);
    }
    if !title.is_empty() {
        let at = Point::new(sc.width / 2.0, pad + title.height / 2.0);
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        it.label = Some(Placed::new(title, at));
        it.style = Style {
            text: Some(Paint::FG),
            ..Style::default()
        };
        sc.items.push(it);
    }
    sc.items.append(&mut items);
    sc
}
