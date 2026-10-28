//! An Ishikawa diagram laid out as a scene, as Pele's renderIshikawa in render.ts draws it.

use super::db::{IshikawaDb, IshikawaNode};
use crate::common::{break_label, setting, title_room, turn_to_fit};
use crate::js::math::{hypot, max, min, round};
use crate::layout::Options;
use crate::scene::style::{Join, Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{
    Edge, Item, Kind, MARKER_SIZE, Marker, MarkerType, Painting, Part, Path, Placed, Point, Rect,
    Scene, Shape,
};
use crate::text::{Setting, layout_label};
use crate::value::Map;

/// The roles of the items of an Ishikawa diagram. Across, in the order drawn: its title; the
/// spine, with the arrowhead at its end; and for each category, its bones, the bone that
/// meets the spine and those of the causes off it, one path, with the arrowhead where it
/// meets the spine, its category, the box with its name at the far end of its bone, and a
/// cause for the text of each of its causes; then the head, the effect at the head of the
/// spine. Down, the title, a link for the line from each node to its parent, and then the
/// effect, the categories and the causes, in the order of the outline.
pub(crate) const ROLE_TITLE: &str = "ishikawa.title";
pub(crate) const ROLE_SPINE: &str = "ishikawa.spine";
pub(crate) const ROLE_BONES: &str = "ishikawa.bones";
pub(crate) const ROLE_CATEGORY: &str = "ishikawa.category";
pub(crate) const ROLE_CAUSE: &str = "ishikawa.cause";
pub(crate) const ROLE_HEAD: &str = "ishikawa.head";
pub(crate) const ROLE_LINK: &str = "ishikawa.link";

// The measures of Pele's render.ts. SLANT is how far a bone moves toward the tail for each
// unit it rises from the spine.
const SLANT: f64 = 0.5;
const FIRST_ROW: f64 = 14.0;
const ROW_GAP: f64 = 6.0;
const BONE_END: f64 = 8.0;
const TWIG_STEP: f64 = 22.0;
const STUB: f64 = 18.0;
const TEXT_GAP: f64 = 5.0;
const PAIR_GAP: f64 = 26.0;
const HEAD_GAP: f64 = 34.0;
const TAIL: f64 = 28.0;
const BOX_PAD_X: f64 = 10.0;
const BOX_PAD_Y: f64 = 5.0;

/// The length of a bone for each unit it rises, Pele's LENGTH.
fn length() -> f64 {
    hypot(SLANT, 1.0)
}

/// A cause below a category, Pele's Bone. Bones at even depths run level, from the slanted
/// bone above them toward the tail. Bones at odd depths slant away from the spine, from the
/// level bone above them.
struct Bone {
    label: Label,
    depth: usize,
    parent: Option<usize>,
    /// Its place among its siblings, counted from the spine outward.
    slot: usize,
    kids: usize,
    /// How far its row is from the spine.
    rise: f64,
    /// Where it meets its parent, measured level from the category's bone: 0 on it, negative
    /// toward the tail.
    at: f64,
    /// Where its text ends.
    text_end: f64,
}

/// A child of the effect, Pele's Category.
struct Category<'a> {
    node: &'a IshikawaNode,
    label: Label,
    box_w: f64,
    box_h: f64,
    bones: Vec<Bone>,
    /// The length of its bone, as distance from the spine.
    rise: f64,
    /// How far its causes reach toward the tail, measured level from its bone.
    reach: f64,
}

/// A step of arrange's walk, Pele's Frame.
struct Frame {
    node: usize,
    depth: usize,
    parent: Option<usize>,
    slot: usize,
    /// Set when the bone was made and only its row is still to come.
    bone: Option<usize>,
}

/// Gives every cause of a category a row of its own, so that no two texts can meet, and
/// measures how far the whole of it reaches, as Pele's arrange does. A level bone takes the
/// row nearest the spine and its twigs the rows beyond. A slanted twig takes the row beyond
/// everything that hangs off it. The outline may be nested as deep as its indentation goes,
/// so the walk keeps a stack of its own.
fn arrange<'a>(
    db: &'a IshikawaDb,
    node: &'a IshikawaNode,
    up: bool,
    set: &Setting,
    size: f64,
    small: f64,
) -> Category<'a> {
    let label = layout_label(&node.text, false, set, size, 180.0, Face::REGULAR);
    let line_height = round(small * 1.5);
    let mut bones: Vec<Bone> = Vec::new();
    let mut rows: Vec<usize> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    // Above the spine the first cause goes farthest out, so that the causes read from the top
    // down.
    let push = |stack: &mut Vec<Frame>, children: &[usize], depth: usize, parent| {
        let n = children.len();
        for slot in (0..n).rev() {
            let node = children[if up { n - 1 - slot } else { slot }];
            stack.push(Frame {
                node,
                depth,
                parent,
                slot,
                bone: None,
            });
        }
    };
    push(&mut stack, &node.children, 2, None);
    while let Some(mut frame) = stack.pop() {
        if let Some(bone) = frame.bone {
            rows.push(bone);
            continue;
        }
        let index = bones.len();
        let n = &db.nodes[frame.node];
        bones.push(Bone {
            label: layout_label(&n.text, false, set, small, 200.0, Face::REGULAR),
            depth: frame.depth,
            parent: frame.parent,
            slot: frame.slot,
            kids: n.children.len(),
            rise: 0.0,
            at: 0.0,
            text_end: 0.0,
        });
        let depth = frame.depth;
        if depth % 2 == 0 {
            rows.push(index);
        } else {
            frame.bone = Some(index);
            stack.push(frame);
        }
        push(&mut stack, &n.children, depth + 1, Some(index));
    }

    let mut rise = FIRST_ROW;
    for &index in &rows {
        let height = max(bones[index].label.height, line_height) + ROW_GAP;
        bones[index].rise = rise + height / 2.0;
        rise += height;
    }
    let box_w = label.width + 2.0 * BOX_PAD_X;
    let mut reach = box_w / 2.0;
    for i in 0..bones.len() {
        if bones[i].depth.is_multiple_of(2) {
            let at = bones[i].parent.map_or(0.0, |p| bones[p].at);
            let b = &mut bones[i];
            b.at = at;
            let run = if b.kids > 0 {
                (b.kids + 1) as f64 * TWIG_STEP
            } else {
                STUB
            };
            b.text_end = b.at - run - TEXT_GAP;
        } else {
            // Twigs stand along their level bone, the one nearest the spine nearest the tail.
            let parent = &bones[bones[i].parent.expect("a twig has a level bone")];
            let at = parent.at - (parent.kids - bones[i].slot) as f64 * TWIG_STEP;
            let b = &mut bones[i];
            b.at = at;
            b.text_end = b.at - TEXT_GAP;
        }
        reach = max(reach, bones[i].label.width - bones[i].text_end);
    }
    Category {
        node,
        box_h: label.height + 2.0 * BOX_PAD_Y,
        label,
        box_w,
        bones,
        rise: rise + BONE_END,
        reach,
    }
}

/// Lays an Ishikawa diagram out as a scene, as Pele's renderIshikawa does: across, the
/// effect at the head of a spine and the categories off it by turns above and below, or,
/// when the host is narrow, down, as an outline.
pub(crate) fn layout(db: &IshikawaDb, _config: &Map, o: &Options) -> Scene {
    turn_to_fit(o, |down| draw(db, o, down))
}

/// The style of a line Pele strokes in the color of lines.
fn stroked() -> Style {
    Style {
        fill: Some(Paint::NONE),
        stroke: Some(Paint::LINE),
        ..Style::default()
    }
}

/// A text item of a title, drawn bold at the top of the diagram.
fn title_item(title: Label, width: f64, pad: f64) -> Item {
    let mut it = Item::new(Kind::Text, ROLE_TITLE);
    let at = Point::new(width / 2.0, pad + title.height / 2.0);
    it.label = Some(Placed::new(title, at));
    it.style.text = Some(Paint::FG);
    it
}

/// Lays the diagram out across, as Pele's draw does, or down.
fn draw(db: &IshikawaDb, o: &Options, down: bool) -> Scene {
    if down {
        return vertical_causes(db, o);
    }
    let size = o.font_size;
    let small = round(size * 0.875);
    let set = setting(o);
    let pad = o.pad();
    let title_text = db.title.as_deref().unwrap_or("");
    let title = layout_label(title_text, false, &set, size, 4000.0, Face::BOLD);
    let title_height = if title.height > 0.0 {
        title.height + 12.0
    } else {
        0.0
    };
    let arrow = MARKER_SIZE;
    let length = length();

    let mut body: Vec<Item> = Vec::new();
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (0.0, 0.0, 0.0, 0.0);
    // The head is drawn last, once it is known where the body goes.
    let mut head = None;
    if let Some(root) = db.root() {
        let label = layout_label(&root.text, false, &set, size, 200.0, Face::BOLD);
        let head_h = max(label.height + 24.0, 48.0);
        let head_w = label.width + 24.0 + head_h / 2.0;
        let mut categories: Vec<Category> = root
            .children
            .iter()
            .enumerate()
            .map(|(i, &c)| arrange(db, &db.nodes[c], i % 2 == 0, &set, size, small))
            .collect();
        // Even a category without causes has a bone long enough to carry its name clear of
        // the head.
        for c in &mut categories {
            c.rise = max(c.rise, head_h / 2.0 + 14.0);
        }

        // Categories meet the spine in pairs, one above and one below, each pair as far
        // toward the tail from the one before as the wider of the two needs.
        let mut xs: Vec<f64> = Vec::new();
        let mut x = -HEAD_GAP;
        for i in (0..categories.len()).step_by(2) {
            if i > 0 {
                let mut room: f64 = 0.0;
                for side in 0..2 {
                    let before = &categories[i - 2 + side];
                    let next = categories.get(i + side).map_or(0.0, |c| c.box_w / 2.0);
                    room = max(room, before.reach + next);
                }
                x -= room + PAIR_GAP;
            }
            xs.extend([x, x]);
        }
        let tail = if categories.is_empty() {
            -HEAD_GAP
        } else {
            x - TAIL
        };

        max_x = head_w;
        min_x = tail;
        min_y = -head_h / 2.0;
        max_y = head_h / 2.0;
        let mut spine = Path::new();
        spine.move_to(tail, 0.0).line_to(-(arrow - 1.0), 0.0);
        let mut it = Item::new(
            Kind::Edge(Edge {
                route: vec![Point::new(tail, 0.0), Point::new(-(arrow - 1.0), 0.0)],
                ends: [
                    None,
                    Some(Marker {
                        kind: MarkerType::ArrowPoint,
                        at: Point::new(0.0, 0.0),
                        dx: 1.0,
                        dy: 0.0,
                    }),
                ],
            }),
            ROLE_SPINE,
        );
        it.bounds = spine.bounds();
        it.parts = vec![Part::path(spine, Painting::StrokeOnly)];
        it.style = stroked();
        body.push(it);

        for (i, c) in categories.iter().enumerate() {
            let sign = if i % 2 == 0 { -1.0 } else { 1.0 };
            let ax = xs[i];
            let px = |at: f64, rise: f64| ax + at - rise * SLANT;
            let end_x = px(0.0, c.rise);
            let end_y = sign * c.rise;
            // The bone stops short of the spine to leave its arrowhead a clean point.
            let trim = arrow - 1.0;
            let (from_x, from_y) = (ax - (SLANT / length) * trim, (sign * trim) / length);
            let mut d = Path::new();
            d.move_to(from_x, from_y).line_to(end_x, end_y);
            let mut causes = Vec::new();
            for b in &c.bones {
                let y = sign * b.rise;
                if b.depth % 2 == 0 {
                    d.move_to(px(b.at, b.rise), y)
                        .line_to(px(b.text_end + TEXT_GAP, b.rise), y);
                } else {
                    let from = c.bones[b.parent.expect("a twig has a level bone")].rise;
                    d.move_to(px(b.at, from), sign * from)
                        .line_to(px(b.at, b.rise), y);
                }
                let right = px(b.text_end, b.rise);
                let l = &b.label;
                let mut it = Item::new(Kind::Text, ROLE_CAUSE);
                it.bounds = Rect::new(right - l.width, y - l.height / 2.0, l.width, l.height);
                let at = Point::new(right - l.width / 2.0, y);
                it.label = Some(Placed::new(l.clone(), at));
                it.style.text = Some(if b.depth > 2 { Paint::MUTED } else { Paint::FG });
                min_x = min(min_x, right - l.width);
                causes.push(it);
            }
            let box_y = end_y + sign * (c.box_h / 2.0);
            min_x = min(min_x, end_x - c.box_w / 2.0);
            max_x = max(max_x, end_x + c.box_w / 2.0);
            min_y = min(min_y, box_y - c.box_h / 2.0);
            max_y = max(max_y, box_y + c.box_h / 2.0);

            let mut it = Item::new(
                Kind::Edge(Edge {
                    route: vec![Point::new(from_x, from_y), Point::new(end_x, end_y)],
                    ends: [
                        Some(Marker {
                            kind: MarkerType::ArrowPoint,
                            at: Point::new(ax, 0.0),
                            dx: SLANT / length,
                            dy: -sign / length,
                        }),
                        None,
                    ],
                }),
                ROLE_BONES,
            );
            it.id = Some(c.node.text.clone());
            it.bounds = d.bounds();
            it.parts = vec![Part::path(d, Painting::StrokeOnly)];
            it.style = stroked();
            body.push(it);

            let r = Rect::new(
                end_x - c.box_w / 2.0,
                box_y - c.box_h / 2.0,
                c.box_w,
                c.box_h,
            );
            let mut it = Item::new(Kind::Node, ROLE_CATEGORY);
            it.shape = Some(Shape::new("rect"));
            it.id = Some(c.node.text.clone());
            it.bounds = r;
            it.parts = vec![Part::rect(r, true, Painting::FillAndStroke)];
            it.label = Some(Placed::new(c.label.clone(), Point::new(end_x, box_y)));
            it.style = Style {
                fill: Some(Paint::SURFACE),
                stroke: Some(Paint::BORDER),
                text: Some(Paint::FG),
                ..Style::default()
            };
            body.push(it);
            body.extend(causes);
        }
        head = Some((root, label, head_w, head_h));
    }

    let inner = max(max_x - min_x, title.width);
    let width = (inner + 2.0 * pad).ceil();
    let height = (max_y - min_y + title_height + 2.0 * pad).ceil();
    let ox = pad + (inner - (max_x - min_x)) / 2.0 - min_x;
    let oy = pad + title_height - min_y;
    let mut items = Vec::new();
    if !title.is_empty() {
        items.push(title_item(title, width, pad));
    }
    for it in &mut body {
        it.move_by(ox, oy);
    }
    items.extend(body);
    if let Some((root, label, head_w, head_h)) = head {
        // The head's outline is drawn where it is on the page, its arc between the corners
        // as Pele writes them moved there, so that the curves it is drawn with are those of
        // that arc.
        let r = head_h / 2.0;
        let (top, bottom, left, right) = (-r + oy, r + oy, 0.0 + ox, head_w - r + ox);
        let mut outline = Path::new();
        outline
            .move_to(left, top)
            .line_to(right, top)
            .arc_to(r, r, 0.0, false, true, right, bottom)
            .line_to(left, bottom)
            .close();
        let mut it = Item::new(Kind::Node, ROLE_HEAD);
        it.shape = Some(Shape::new("head"));
        it.id = Some(root.text.clone());
        it.bounds = Rect::new(left, top, head_w, head_h);
        it.parts = vec![Part::path(outline, Painting::FillAndStroke)];
        let at = Point::new(12.0 + label.width / 2.0 + ox, 0.0 + oy);
        it.label = Some(Placed::new(label, at));
        it.style = Style {
            fill: Some(Paint::SURFACE),
            stroke: Some(Paint::BORDER),
            text: Some(Paint::FG),
            join: Some(Join::Round),
            ..Style::default()
        };
        items.push(it);
    }
    Scene {
        diagram: "ishikawa".into(),
        width,
        height,
        font_size: size,
        title: None,
        desc: None,
        items,
    }
}

/// A node of the outline drawn down, Pele's entry of verticalCauses.
struct Entry {
    node: usize,
    depth: usize,
    parent: Option<usize>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Lays the diagram out down, as an outline, as Pele's verticalCauses does: the effect and
/// the categories framed, the causes under them indented by their depth, each with a line
/// from its parent.
fn vertical_causes(db: &IshikawaDb, o: &Options) -> Scene {
    let size = o.font_size;
    let small = round(size * 0.875);
    let set = setting(o);
    let pad = o.pad();
    // A host that gives no width is taken to give 320, as Pele's `?? 320` takes it.
    let max_width = if o.max_width.is_finite() {
        o.max_width
    } else {
        320.0
    };
    let room = max(140.0, max_width - 2.0 * pad);
    let title_text = db.title.as_deref().unwrap_or("");
    let title = layout_label(title_text, false, &set, size, title_room(o), Face::BOLD);
    let mut order: Vec<Entry> = Vec::new();
    let mut pending: Vec<(usize, usize, Option<usize>)> = Vec::new();
    if db.root().is_some() {
        pending.push((0, 0, None));
    }
    let mut depth = 0;
    while let Some((node, d, parent)) = pending.pop() {
        let index = order.len();
        order.push(Entry {
            node,
            depth: d,
            parent,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        });
        depth = depth.max(d);
        for &c in db.nodes[node].children.iter().rev() {
            pending.push((c, d + 1, Some(index)));
        }
    }
    let indent = min(20.0, max(2.0, (room - 120.0) / depth.max(1) as f64));
    let mut y = pad
        + if title.height != 0.0 {
            title.height + 12.0
        } else {
            0.0
        };
    let mut width = title.width;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for i in 0..order.len() {
        let e = &mut order[i];
        e.x = pad + e.depth as f64 * indent;
        let label_room = max(60.0, room - e.depth as f64 * indent - 20.0);
        let (label_size, face) = if e.depth < 2 {
            (size, Face::BOLD)
        } else {
            (small, Face::REGULAR)
        };
        let text = &db.nodes[e.node].text;
        let label = break_label(
            layout_label(text, false, &set, label_size, label_room, face),
            &set,
            label_room,
        );
        e.w = label.width + 20.0;
        e.h = label.height + 12.0;
        e.y = y;
        y += e.h + 12.0;
        width = max(width, e.x - pad + e.w);
        let r = Rect::new(e.x, e.y, e.w, e.h);
        let at = Point::new(e.x + e.w / 2.0, e.y + e.h / 2.0);
        let mut it = Item::new(Kind::Text, ROLE_CAUSE);
        it.id = Some(text.clone());
        it.bounds = r;
        it.label = Some(Placed::new(label, at));
        it.style.text = Some(Paint::FG);
        if e.depth < 2 {
            // The effect and the categories are framed, in boxes with corners of their own.
            let (role, rx) = if e.depth == 0 {
                (ROLE_HEAD, 12.0)
            } else {
                (ROLE_CATEGORY, 4.0)
            };
            it.kind = Kind::Node;
            it.role = role.into();
            it.shape = Some(Shape::new("rect"));
            let mut p = Path::new();
            p.round_rect(r, rx);
            it.parts = vec![Part::path(p, Painting::FillAndStroke)];
            it.style = Style {
                fill: Some(Paint::SURFACE),
                stroke: Some(Paint::BORDER),
                text: Some(Paint::FG),
                ..Style::default()
            };
        }
        nodes.push(it);
        let (ex, ey, eh) = (e.x, e.y, e.h);
        if let Some(parent) = order[i].parent {
            let parent = &order[parent];
            let gx = parent.x + indent / 2.0;
            let from = Point::new(gx, parent.y + parent.h);
            let mid = Point::new(gx, ey + eh / 2.0);
            let to = Point::new(ex, ey + eh / 2.0);
            let mut p = Path::new();
            p.move_to(from.x, from.y)
                .line_to(mid.x, mid.y)
                .line_to(to.x, to.y);
            let mut it = Item::new(
                Kind::Edge(Edge {
                    route: vec![from, mid, to],
                    ends: [None, None],
                }),
                ROLE_LINK,
            );
            it.id = Some(text.clone());
            it.bounds = p.bounds();
            it.parts = vec![Part::path(p, Painting::StrokeOnly)];
            it.style = Style {
                fill: Some(Paint::NONE),
                stroke: Some(Paint::BORDER),
                ..Style::default()
            };
            edges.push(it);
        }
    }
    let total_width = (width + 2.0 * pad).ceil();
    let height = (max(pad + title.height, y - 12.0) + pad).ceil();
    let mut items = Vec::new();
    if !title.is_empty() {
        items.push(title_item(title, total_width, pad));
    }
    items.extend(edges);
    items.extend(nodes);
    Scene {
        diagram: "ishikawa".into(),
        width: total_width,
        height,
        font_size: size,
        title: None,
        desc: None,
        items,
    }
}
