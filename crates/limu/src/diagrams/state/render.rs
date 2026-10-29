//! A state diagram laid out as a scene, as Pele's renderState in render.ts draws it.

use std::cmp::Ordering;
use std::collections::HashMap;

use super::db::StateDb;
use super::graph::{StateEdge, StateNode};
use crate::common::{class_names, routing_curve, runs_across, setting, turn_to_fit, with_title};
use crate::css::{Resolved, declarations, resolve_style};
use crate::draw::{
    Crossings, Curve, EdgePath, Fill, Node, SideReach, Tip, casing, end_side, label_patch,
    label_patch_box, marker_trim, on_fills, route_path,
};
use crate::graphs::compound::{CEdge, CNode, Dir, cnode, compound_layout, direction};
use crate::graphs::layered::LayeredOptions;
use crate::js::math::{by, hypot, max, min, or, or1, round, sign};
use crate::layout::Options;
use crate::scene::style::{Cap, Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{
    Edge, Item, Kind, MarkerType, Painting, Part, Path, Placed, Point, Rect, Scene, Shape,
};
use crate::text::{decode_entities, layout_label};
use crate::url::{link_url, safe_url};
use crate::value::{Map, Value};

/// The roles of the items of a state diagram.
pub(crate) const ROLE_CASING: &str = "state.casing";
pub(crate) const ROLE_CHOICE: &str = "state.choice";
pub(crate) const ROLE_COMPOSITE: &str = "state.composite";
pub(crate) const ROLE_DESCRIPTION: &str = "state.description";
pub(crate) const ROLE_DIVIDER: &str = "state.divider";
pub(crate) const ROLE_EDGE_LABEL: &str = "state.edge-label";
pub(crate) const ROLE_END: &str = "state.end";
pub(crate) const ROLE_FORK: &str = "state.fork";
pub(crate) const ROLE_JOIN: &str = "state.join";
pub(crate) const ROLE_NOTE: &str = "state.note";
pub(crate) const ROLE_NOTE_EDGE: &str = "state.note-edge";
pub(crate) const ROLE_START: &str = "state.start";
pub(crate) const ROLE_STATE: &str = "state.state";
pub(crate) const ROLE_TITLE: &str = "state.title";
pub(crate) const ROLE_TRANSITION: &str = "state.transition";

// The spacing of a state diagram, as Pele's render.ts has it.
const NODE_SEP: f64 = 40.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 44.0;
const GROUP_PAD: f64 = 16.0;
const REGION_PAD: f64 = 10.0;
const PAD_X: f64 = 16.0;
pub(crate) const PAD_Y: f64 = 10.0;
const LOOP: f64 = 26.0;
pub(crate) const FOLD: f64 = 8.0;
const NOTE_WRAP: f64 = 320.0;
const CORNER: f64 = 10.0;
const BAR: f64 = 8.0;

/// What a node is drawn as, Pele's K.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum K {
    State,
    Described,
    Start,
    End,
    Choice,
    Fork,
    Join,
    Note,
    Composite,
    Region,
    Hidden,
}

/// What a node of a shape that is no group is drawn as, Pele's KINDS.
fn kind_of(shape: Option<&str>) -> K {
    match shape {
        Some("rectWithTitle") => K::Described,
        Some("stateStart") => K::Start,
        Some("stateEnd") => K::End,
        Some("choice") => K::Choice,
        Some("fork") => K::Fork,
        Some("join") => K::Join,
        Some("note") => K::Note,
        Some("divider") => K::Hidden,
        _ => K::State,
    }
}

/// The role of what a kind is drawn as, which Pele gives its classes.
fn role_of(kind: K) -> &'static str {
    match kind {
        K::Start => ROLE_START,
        K::End => ROLE_END,
        K::Choice => ROLE_CHOICE,
        K::Fork => ROLE_FORK,
        K::Join => ROLE_JOIN,
        K::Note => ROLE_NOTE,
        K::Composite => ROLE_COMPOSITE,
        _ => ROLE_STATE,
    }
}

/// A node as it is drawn, Pele's View, with the declarations its style is read from. node is
/// none for the unseen helpers the layout needs.
struct View<'g> {
    node: Option<&'g StateNode>,
    kind: K,
    decls: Vec<String>,
    style: Option<Resolved>,
    label: Label,
    lines: Vec<Label>,
    w: f64,
    h: f64,
    title_h: f64,
}

fn vertical(dir: Option<Dir>) -> bool {
    matches!(dir, Some(Dir::TB | Dir::BT))
}

/// A number of the state diagram's configuration, where it is one above 0, at most 2000, as
/// Pele's numberOption reads it.
fn number_option(config: &Map, key: &str, fallback: f64) -> f64 {
    match config
        .get("state")
        .and_then(Value::as_object)
        .and_then(|s| s.get(key))
    {
        Some(&Value::Number(v)) if v > 0.0 => min(v, 2000.0),
        _ => fallback,
    }
}

/// The text without the quotes Mermaid keeps about a click statement's strings, as Pele's
/// unquote takes them off.
fn unquote(text: &str) -> &str {
    text.trim_matches('"')
}

fn clamp(value: f64, lo: f64, hi: f64) -> f64 {
    if value < lo {
        lo
    } else if value > hi {
        hi
    } else {
        value
    }
}

/// A node for a group, as Pele's group makes one.
fn group(dir: Dir, pad_x: f64, pad_top: f64, pad_bottom: f64) -> CNode {
    let mut c = cnode(0.0, 0.0, -1);
    c.is_group = true;
    c.dir = Some(dir);
    c.pad_x = pad_x;
    c.pad_top = pad_top;
    c.pad_bottom = pad_bottom;
    c
}

/// A node's classes, as Pele writes them: without the ones Mermaid gives every state,
/// cluster and note, which Pele takes off with
/// /(?:^| )statediagram-(?:state|cluster-alt|cluster|note)(?= |$)/g. That takes off each of
/// them that stands between spaces, and the space before it, and classNames, which reads what
/// is left, splits it at spaces, so taking off the names that a split at spaces gives is the
/// same.
fn state_classes(raw: &str) -> Vec<String> {
    let kept: Vec<&str> = raw
        .split(' ')
        .filter(|name| {
            !matches!(
                *name,
                "statediagram-state"
                    | "statediagram-cluster-alt"
                    | "statediagram-cluster"
                    | "statediagram-note"
            )
        })
        .collect();
    class_names(&kept.join(" "))
}

/// The box of a laid-out node that the drawing takes.
fn node_of(c: &CNode) -> Node {
    Node {
        x: c.x,
        y: c.y,
        w: c.w,
        h: c.h,
        group: c.is_group,
    }
}

/// A side of a node, as Pele's sideReach takes it: -1 for none.
fn side(s: Option<usize>) -> i32 {
    s.map_or(-1, |s| s as i32)
}

/// Lays out a parsed state diagram as a scene, as Pele's renderState draws it: in its
/// direction, and, for one that runs across and does not fit the host's width, down instead
/// where that is narrower.
pub(crate) fn layout(db: &StateDb, config: &Map, o: &Options) -> Scene {
    if !runs_across(Some(db.direction())) {
        return draw(db, config, o, false);
    }
    turn_to_fit(o, |down| draw(db, config, o, down))
}

/// Lays the diagram out once, as Pele's draw does. turned lays out a diagram that runs across
/// as one that runs down.
fn draw(db: &StateDb, config: &Map, o: &Options, turned: bool) -> Scene {
    let graph = &db.graph;
    // State diagrams follow the flowchart curve, as Pele's draw does, unless the host asks
    // for transitions at right angles, or straight.
    let curve = routing_curve(o.routing).or_else(|| {
        config
            .get("flowchart")
            .and_then(Value::as_object)
            .and_then(|f| f.get("curve"))
            .and_then(Value::as_str)
    });
    let size = o.font_size;
    let small = round(size * 0.875);
    let set = setting(o);
    let wrap_width = number_option(config, "wrappingWidth", 200.0);
    let pad = o.pad();
    let schemes = o.schemes();
    let root_dir = if turned {
        Dir::TB
    } else {
        direction(Some(db.direction()))
    };
    let text = |raw: &str, font_size: f64, wrap: f64, face: Face| {
        layout_label(raw, true, &set, font_size, wrap, face)
    };

    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut views: Vec<View> = Vec::new();
    let mut cnodes: Vec<CNode> = Vec::new();
    fn add<'g>(
        views: &mut Vec<View<'g>>,
        cnodes: &mut Vec<CNode>,
        node: Option<&'g StateNode>,
        kind: K,
        c: Option<CNode>,
    ) -> usize {
        views.push(View {
            node,
            kind,
            decls: Vec::new(),
            style: None,
            label: Label::default(),
            lines: Vec::new(),
            w: 0.0,
            h: 0.0,
            title_h: 0.0,
        });
        cnodes.extend(c);
        views.len() - 1
    }
    for node in &graph.nodes {
        if node.shape == Some("noteGroup") {
            continue;
        }
        let kind = if node.is_group {
            if node.shape == Some("divider") {
                K::Region
            } else {
                K::Composite
            }
        } else {
            kind_of(node.shape)
        };
        let at = add(&mut views, &mut cnodes, Some(node), kind, None);
        index.insert(&node.id, at);
    }
    let states = views.len();
    let state_nodes: Vec<&StateNode> = views.iter().filter_map(|v| v.node).collect();
    let node_at = |i: usize| state_nodes[i];

    let mut logical: Vec<i32> = (0..states)
        .map(|i| {
            let parent = node_at(i)
                .parent_id
                .as_deref()
                .and_then(|p| index.get(p).copied());
            match parent {
                Some(p) if p != i && node_at(p).is_group => p as i32,
                _ => -1,
            }
        })
        .collect();
    // Composites that are redeclared inside each other form a ring with no way in from the
    // root. One member of each ring moves to the root, which leaves a tree.
    {
        let mut kids: Vec<Vec<usize>> = vec![Vec::new(); states];
        let mut queue: Vec<usize> = Vec::new();
        let mut placed = vec![false; states];
        for i in 0..states {
            if logical[i] == -1 {
                placed[i] = true;
                queue.push(i);
            } else {
                kids[logical[i] as usize].push(i);
            }
        }
        let mut done = 0;
        let settle = |queue: &mut Vec<usize>, placed: &mut [bool], done: &mut usize| {
            while *done < queue.len() {
                for &kid in &kids[queue[*done]] {
                    if placed[kid] {
                        continue;
                    }
                    placed[kid] = true;
                    queue.push(kid);
                }
                *done += 1;
            }
        };
        settle(&mut queue, &mut placed, &mut done);
        let mut seen = vec![usize::MAX; states];
        for i in 0..states {
            if placed[i] {
                continue;
            }
            let mut ring = i;
            while seen[ring] != i {
                seen[ring] = i;
                ring = logical[ring] as usize;
            }
            logical[ring] = -1;
            placed[ring] = true;
            queue.push(ring);
            settle(&mut queue, &mut placed, &mut done);
        }
    }
    let mut owners: Vec<i32> = vec![-1; states];
    for i in 0..states {
        let owner = node_at(i)
            .note_for
            .as_deref()
            .and_then(|n| index.get(n).copied());
        let Some(owner) = owner else { continue };
        owners[i] = owner as i32;
        logical[i] = logical[owner];
    }

    // As in Mermaid, a composite that sets no direction runs top to bottom whatever surrounds
    // it.
    let inner_dir = |composite: usize| direction(node_at(composite).dir.as_deref());
    // Whether the flow around each state runs top to bottom or bottom to top.
    let across: Vec<bool> = (0..states)
        .map(|i| {
            vertical(Some(if logical[i] == -1 {
                root_dir
            } else {
                inner_dir(logical[i] as usize)
            }))
        })
        .collect();

    let mut degree = vec![0usize; states];
    let mut noted = vec![false; states];
    // When each state first takes part in a transition. Layout follows this order rather
    // than the order of declaration, so a state declared up front does not become the top of
    // the flow.
    let mut appear: Vec<f64> = vec![f64::INFINITY; states];
    let mut appeared = 0.0;
    let mut counts = vec![0usize; 2 * states];
    for edge in &graph.edges {
        let (Some(&src), Some(&dst)) =
            (index.get(edge.start.as_str()), index.get(edge.end.as_str()))
        else {
            continue;
        };
        if edge.pattern == Some("dashed") {
            noted[if views[src].kind == K::Note { dst } else { src }] = true;
            continue;
        }
        counts[2 * src] += 1;
        degree[src] = degree[src].max(counts[2 * src]);
        counts[2 * dst + 1] += 1;
        degree[dst] = degree[dst].max(counts[2 * dst + 1]);
        if appear[src] == f64::INFINITY {
            appear[src] = appeared;
            appeared += 1.0;
        }
        if appear[dst] == f64::INFINITY {
            appear[dst] = appeared;
            appeared += 1.0;
        }
    }

    for i in 0..states {
        let node = node_at(i);
        let mut decls = node.css_compiled_styles.clone();
        decls.extend(node.css_styles.iter().cloned());
        let style = resolve_style(&decls);
        let mut face = Face::default();
        if style.bold {
            face |= Face::BOLD;
        }
        if style.italic {
            face |= Face::ITALIC;
        }
        let font_size = style.font_size.unwrap_or(size);
        let view = &mut views[i];
        let mut c = None;
        match view.kind {
            K::Composite => {
                view.label = text(&node.label, font_size, 4000.0, face);
                view.title_h = if view.label.height > 0.0 {
                    view.label.height + 12.0
                } else {
                    0.0
                };
                let mut g = group(inner_dir(i), GROUP_PAD, view.title_h + GROUP_PAD, GROUP_PAD);
                g.min_w = view.label.width + 2.0 * GROUP_PAD;
                c = Some(g);
            }
            K::Region => {
                c = Some(group(inner_dir(i), REGION_PAD, REGION_PAD, REGION_PAD));
            }
            K::Start => (view.w, view.h) = (14.0, 14.0),
            K::End => (view.w, view.h) = (18.0, 18.0),
            K::Choice => (view.w, view.h) = (30.0, 30.0),
            K::Fork | K::Join => {
                let length = clamp(40.0 * degree[i] as f64 + 16.0, 72.0, 320.0);
                (view.w, view.h) = if across[i] {
                    (length, BAR)
                } else {
                    (BAR, length)
                };
            }
            K::Hidden => {}
            K::Note => {
                view.label = text(
                    &node.label,
                    small,
                    max(wrap_width, NOTE_WRAP),
                    Face::default(),
                );
                view.w = view.label.width + 24.0;
                view.h = view.label.height + 16.0;
            }
            K::State | K::Described => {
                view.label = text(&node.label, font_size, wrap_width, face);
                let mut width = view.label.width;
                let mut height = view.label.height + 2.0 * PAD_Y;
                if view.kind == K::Described {
                    height += 14.0;
                    for line in &node.description {
                        let line = if line.is_empty() { " " } else { line };
                        let label = text(line, small, wrap_width, face);
                        if label.width > width {
                            width = label.width;
                        }
                        height += label.height;
                        view.lines.push(label);
                    }
                }
                view.w = max(width + 2.0 * PAD_X, 56.0);
                view.h = height;
            }
        }
        view.decls = decls;
        view.style = Some(style);
        let mut c = c.unwrap_or_else(|| cnode(view.w, view.h, -1));
        // The transitions on one side of a state's box spread along it, clear of its corners.
        if matches!(view.kind, K::State | K::Described) {
            c.span = (if across[i] { view.w } else { view.h }) - 24.0;
        }
        cnodes.push(c);
    }

    let mut cedges: Vec<CEdge> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    let mut drawn: Vec<&StateEdge> = Vec::new();
    // The transitions each state makes to itself: how many, their widest label, and their
    // labels' height.
    let mut loop_count = vec![0usize; states];
    let mut loop_width = vec![0.0; states];
    let mut loop_height = vec![0.0; states];
    // A transition between a composite and a state inside it runs between that state and a
    // point just inside the composite, and is then drawn on to the composite's border.
    let mut anchors: HashMap<usize, usize> = HashMap::new();
    let encloses = |cnodes: &[CNode], composite: usize, item: usize| {
        if !cnodes[composite].is_group {
            return false;
        }
        let mut p = logical[item];
        while p != -1 {
            if p as usize == composite {
                return true;
            }
            p = logical[p as usize];
        }
        false
    };
    for edge in &graph.edges {
        let (Some(&src), Some(&dst)) =
            (index.get(edge.start.as_str()), index.get(edge.end.as_str()))
        else {
            continue;
        };
        let (mut src, mut dst) = (src, dst);
        let label = text(&edge.label, small, wrap_width, Face::default());
        let label_w = if label.width > 0.0 {
            label.width + 12.0
        } else {
            0.0
        };
        if src == dst {
            loop_count[src] += 1;
            loop_width[src] = max(loop_width[src], label_w);
            loop_height[src] += label.height;
        } else if encloses(&cnodes, src, dst) {
            anchors.insert(cedges.len(), src);
            appear.push(-1.0);
            let c = cnode(0.0, 0.0, src as i32);
            src = add(&mut views, &mut cnodes, None, K::Hidden, Some(c));
        } else if encloses(&cnodes, dst, src) {
            anchors.insert(cedges.len(), dst);
            appear.push(f64::INFINITY);
            let c = cnode(0.0, 0.0, dst as i32);
            dst = add(&mut views, &mut cnodes, None, K::Hidden, Some(c));
        }
        cedges.push(CEdge {
            src,
            dst,
            minlen: 1.0,
            label_w,
            label_h: if label.height > 0.0 {
                label.height + 4.0
            } else {
                0.0
            },
            route: Vec::new(),
            label_x: 0.0,
            label_y: 0.0,
        });
        drawn.push(edge);
        edge_labels.push(label);
    }

    // A state with notes, or a composite with self-transitions, sits in an unseen group of
    // its own: the notes go beside the state, across the flow, without disturbing its
    // transitions.
    for i in 0..states {
        if owners[i] != -1 {
            continue;
        }
        // Loops hang off the right, or below when the flow is horizontal, with their labels
        // beyond them.
        let mut room = 0.0;
        if loop_count[i] > 0 {
            let reach = LOOP + (loop_count[i] - 1) as f64 * 8.0;
            room = if across[i] {
                reach + loop_width[i]
            } else {
                reach * 0.75 + 4.0 + loop_height[i]
            };
            let item = &mut cnodes[i];
            if !item.is_group {
                if across[i] {
                    item.w += 2.0 * room;
                    item.h = max(item.h, loop_height[i]);
                } else {
                    item.h += 2.0 * room;
                    item.w = max(item.w, loop_width[i]);
                }
                room = 0.0;
            }
        }
        if !noted[i] && room == 0.0 {
            cnodes[i].parent = logical[i];
            continue;
        }
        let mut c = if across[i] {
            group(Dir::LR, room, 0.0, 0.0)
        } else {
            group(Dir::TB, 0.0, room, room)
        };
        c.parent = logical[i];
        appear.push(appear[i]);
        cnodes[i].parent = add(&mut views, &mut cnodes, None, K::Hidden, Some(c)) as i32;
    }
    for i in 0..states {
        if owners[i] != -1 {
            cnodes[i].parent = cnodes[owners[i] as usize].parent;
        }
    }

    {
        // Numbers the tree so that, at every level, initial states come first and the rest
        // follow in the order the transitions introduce them.
        let mut members: Vec<Vec<usize>> = vec![Vec::new(); cnodes.len()];
        let mut top: Vec<usize> = Vec::new();
        for (i, c) in cnodes.iter().enumerate() {
            if c.parent >= 0 {
                members[c.parent as usize].push(i);
            } else {
                top.push(i);
            }
        }
        let initial = |i: usize| if views[i].kind == K::Start { 0 } else { 1 };
        let by_flow = |&a: &usize, &b: &usize| {
            let flow = if appear[a] == appear[b] {
                Ordering::Equal
            } else if appear[a] < appear[b] {
                Ordering::Less
            } else {
                Ordering::Greater
            };
            initial(a).cmp(&initial(b)).then(flow).then(a.cmp(&b))
        };
        top.sort_by(by_flow);
        top.reverse();
        let mut pending = top;
        let mut seq = 0.0;
        while let Some(i) = pending.pop() {
            cnodes[i].seq = seq;
            seq += 1.0;
            let inside = &mut members[i];
            inside.sort_by(by_flow);
            pending.extend(inside.iter().rev());
        }
    }

    let opt = LayeredOptions {
        node_sep: number_option(config, "nodeSpacing", NODE_SEP),
        edge_sep: EDGE_SEP,
        rank_sep: number_option(config, "rankSpacing", RANK_SEP),
        port_sep: 20.0,
        tracks: matches!(curve, Some("step" | "stepBefore" | "stepAfter")),
        straight: curve == Some("linear"),
        head_room: Some(marker_trim(Some(MarkerType::ArrowPoint))),
        ..LayeredOptions::default()
    };
    let result = compound_layout(&mut cnodes, &mut cedges, root_dir, &opt);

    let width = (result.width + 2.0 * pad).ceil();
    let height = (result.height + 2.0 * pad).ceil();

    // Route points by rounded y, to find the edges that cross a composite's title bar.
    let mut crossings = Crossings::default();
    for e in &cedges {
        let route = &e.route;
        let mut k = 3;
        while k + 3 < route.len() {
            crossings.add(route[k], route[k + 1]);
            k += 3;
        }
    }

    let mut children: Vec<Vec<usize>> = vec![Vec::new(); cnodes.len()];
    let mut order: Vec<usize> = Vec::new();
    for (i, c) in cnodes.iter().enumerate() {
        if c.parent >= 0 {
            children[c.parent as usize].push(i);
        } else {
            order.push(i);
        }
    }
    let roots = order.clone();
    let mut q = 0;
    while q < order.len() {
        order.extend(children[order[q]].iter().copied());
        q += 1;
    }

    let (mut clusters, mut dividers, mut nodes, mut notes) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut fills: Vec<Fill> = Vec::new();
    let divide = |dividers: &mut Vec<Item>, items: &[usize], bounds: [f64; 4]| {
        divide(dividers, &views, &cnodes, pad, items, bounds);
    };
    divide(
        &mut dividers,
        &roots,
        [pad / 2.0, pad / 2.0, width - pad / 2.0, height - pad / 2.0],
    );

    for &i in &order {
        let view = &views[i];
        let (Some(node), Some(style)) = (view.node, view.style.as_ref()) else {
            continue;
        };
        if view.kind >= K::Region {
            continue;
        }
        let c = &cnodes[i];
        let x = c.x + pad;
        let y = c.y + pad;
        let (w, h) = (view.w, view.h);
        let author = declarations(&view.decls).style;
        // A label lies on its state's fill, and takes the color that stands out on a fill its
        // author sets, as Pele's inkFill gives it.
        let inks = style.inks_text(style);
        let mut it = Item::new(Kind::Node, role_of(view.kind));
        it.shape = node.shape.map(Shape::new);
        it.id = Some(node.id.clone());
        it.class = state_classes(&node.css_classes);
        it.bounds = Rect::new(x - w / 2.0, y - h / 2.0, w, h);
        it.author = author.clone();
        let mut extra = Vec::new();

        match view.kind {
            K::Composite => {
                let left = x - c.w / 2.0;
                let top = y - c.h / 2.0;
                let bounds = Rect::new(left, top, c.w, c.h);
                it.kind = Kind::Group;
                it.bounds = bounds;
                it.parts = vec![Part::rect(bounds, true, Painting::FillAndStroke)];
                if style.paints() {
                    fills.push(Fill {
                        bounds: [left, top, left + c.w, top + c.h],
                        decls: view.decls.clone(),
                    });
                }
                it.style = Style {
                    fill: Some(Paint::SURFACE_ALT),
                    // A fill its author sets is drawn at full opacity.
                    fill_opacity: if style.fill_declared() {
                        None
                    } else {
                        Some(0.5)
                    },
                    stroke: Some(Paint::BORDER),
                    text: Some(Paint::FG),
                    radius: Some(CORNER),
                    on_fill: inks,
                    ..Style::default()
                };
                if view.title_h > 0.0 {
                    let mut rule = Path::new();
                    rule.move_to(left, top + view.title_h)
                        .line_to(left + c.w, top + view.title_h);
                    it.parts.push(Part::path(rule, Painting::StrokeOnly));
                    if !view.label.is_empty() {
                        let at = Point::new(
                            title_x(c, view.label.width, &crossings) + pad,
                            top + view.title_h / 2.0,
                        );
                        it.label = Some(Placed::new(view.label.clone(), at));
                    }
                }
                divide(
                    &mut dividers,
                    &children[i],
                    [left, top + view.title_h, left + c.w, top + c.h],
                );
            }
            K::Start => {
                let mut p = Path::new();
                p.ellipse(x, y, 7.0, 7.0);
                it.parts = vec![Part::path(p, Painting::FillAndStroke)];
                it.style = Style {
                    fill: Some(Paint::LINE),
                    stroke: Some(Paint::NONE),
                    ..Style::default()
                };
            }
            K::End => {
                let (mut ring, mut dot) = (Path::new(), Path::new());
                ring.ellipse(x, y, 8.5, 8.5);
                dot.ellipse(x, y, 5.0, 5.0);
                it.parts = vec![
                    Part::path(ring, Painting::Hollow),
                    Part::path(dot, Painting::FillOnly),
                ];
                it.style = Style {
                    fill: Some(Paint::LINE),
                    stroke: Some(Paint::LINE),
                    ..Style::default()
                };
            }
            K::Choice => {
                let mut p = Path::new();
                p.polygon(&[
                    (x, y - h / 2.0),
                    (x + w / 2.0, y),
                    (x, y + h / 2.0),
                    (x - w / 2.0, y),
                ]);
                it.parts = vec![Part::path(p, Painting::FillAndStroke)];
                it.style = Style {
                    fill: Some(Paint::SURFACE),
                    stroke: Some(Paint::BORDER),
                    ..Style::default()
                };
            }
            K::Fork | K::Join => {
                it.parts = vec![Part::rect(it.bounds, true, Painting::FillAndStroke)];
                it.style = Style {
                    fill: Some(Paint::LINE),
                    stroke: Some(Paint::NONE),
                    radius: Some(2.0),
                    ..Style::default()
                };
            }
            K::Note => {
                let (r, b) = (w / 2.0, h / 2.0);
                let mut outline = Path::new();
                outline
                    .move_to(x - r, y - b)
                    .line_to(x + r - FOLD, y - b)
                    .line_to(x + r, y + FOLD - b)
                    .line_to(x + r, y + b)
                    .line_to(x - r, y + b)
                    .close();
                let mut dog_ear = Path::new();
                dog_ear
                    .move_to(x + r - FOLD, y - b)
                    .line_to(x + r - FOLD, y + FOLD - b)
                    .line_to(x + r, y + FOLD - b);
                it.parts = vec![
                    Part::path(outline, Painting::FillAndStroke),
                    Part::path(dog_ear, Painting::StrokeOnly),
                ];
                it.style = Style {
                    fill: Some(Paint::BG),
                    stroke: Some(Paint::BORDER),
                    text: Some(Paint::MUTED),
                    ..Style::default()
                };
                // A note takes no style of its author's.
                it.author = Style::default();
                it.label = Some(Placed::new(view.label.clone(), Point::new(x, y)));
            }
            _ => {
                it.parts = vec![Part::rect(it.bounds, true, Painting::FillAndStroke)];
                it.style = Style {
                    fill: Some(Paint::SURFACE),
                    stroke: Some(Paint::BORDER),
                    text: Some(Paint::FG),
                    radius: Some(min(CORNER, h / 2.0)),
                    on_fill: inks,
                    ..Style::default()
                };
                if view.kind == K::State {
                    it.label = Some(Placed::new(view.label.clone(), Point::new(x, y)));
                } else {
                    let mut at = PAD_Y - h / 2.0 + view.label.height;
                    let mut rule = Path::new();
                    rule.move_to(x - w / 2.0, y + (at + 6.0))
                        .line_to(x + w / 2.0, y + (at + 6.0));
                    it.parts.push(Part::path(rule, Painting::StrokeOnly));
                    let label_at = Point::new(x, y + (at - view.label.height / 2.0));
                    it.label = Some(Placed::new(view.label.clone(), label_at));
                    at += 14.0;
                    // The descriptions lie on the state's fill too.
                    let ground = if inks { author.fill } else { None };
                    for line in &view.lines {
                        let mut d = Item::new(Kind::Text, ROLE_DESCRIPTION);
                        d.id = Some(node.id.clone());
                        let line_at = Point::new(x, y + (at + line.height / 2.0));
                        d.label = Some(Placed::new(line.clone(), line_at));
                        d.bounds = d.label.as_ref().map_or(Rect::default(), Placed::bounds);
                        d.style = Style {
                            text: Some(Paint::FG),
                            text_ground: ground,
                            ..Style::default()
                        };
                        d.author = Style {
                            text: author.text,
                            ..Style::default()
                        };
                        extra.push(d);
                        at += line.height;
                    }
                }
            }
        }

        if let Some(link) = db.links.get(&node.id) {
            // Entity codes are resolved first, so the URL is checked in the form a browser
            // would follow.
            let tooltip = decode_entities(unquote(&link.tooltip));
            it.tooltip = Some(tooltip).filter(|t| !t.is_empty());
            it.link = link_url(&safe_url(unquote(&link.url)), &schemes);
        }
        match view.kind {
            K::Composite => clusters.push(it),
            K::Note => notes.push(it),
            _ => {
                nodes.push(it);
                nodes.extend(extra);
            }
        }
    }

    // Moves a route end from the layout box onto the drawn shape, and spreads the ends along
    // a bar.
    let attach = |route: &mut [f64], at: usize, i: usize| {
        let c = &cnodes[i];
        if c.is_group {
            return;
        }
        let view = &views[i];
        let bar = matches!(view.kind, K::Fork | K::Join);
        let other = if at == 0 { 3 } else { at - 3 };
        // 0 is the x axis and 1 the y axis. The end sits on a side that faces along axis.
        for axis in [1, 0] {
            let (extent, drawn_extent, breadth, center, cross) = if axis == 1 {
                (c.h, view.h, view.w, c.y + pad, c.x + pad)
            } else {
                (c.w, view.w, view.h, c.x + pad, c.y + pad)
            };
            let delta = route[at + axis] - center;
            if (delta.abs() - extent / 2.0).abs() >= 0.5 {
                continue;
            }
            route[at + axis] -= (sign(delta) * (extent - drawn_extent)) / 2.0;
            if bar && breadth > drawn_extent {
                route[at + 1 - axis] = clamp(
                    route[other + 1 - axis],
                    cross - breadth / 2.0 + 6.0,
                    cross + breadth / 2.0 - 6.0,
                );
            }
            return;
        }
    };

    let arrow = Some(MarkerType::ArrowPoint);
    let mut reach = SideReach::default();
    for (i, ce) in cedges.iter().enumerate() {
        if ce.src == ce.dst || ce.route.len() < 6 || drawn[i].pattern == Some("dashed") {
            continue;
        }
        let last = ce.route.len() - 3;
        let at = side(end_side(&ce.route, last, &node_of(&cnodes[ce.dst])));
        reach.add(ce.dst, at, arrow);
    }

    let (mut casings, mut edges, mut labels) = (Vec::new(), Vec::new(), Vec::new());
    let mut loop_seen = vec![0usize; states];
    let mut loop_text = vec![0.0; states];
    for i in 0..cedges.len() {
        let edge = drawn[i];
        let label = &edge_labels[i];
        let note = edge.pattern == Some("dashed");
        let end_type = if note { None } else { arrow };
        let ce = &cedges[i];
        let mut route = Vec::new();
        let path = if ce.src == ce.dst {
            let a = &cnodes[ce.src];
            let view = &views[ce.src];
            let k = loop_seen[ce.src];
            loop_seen[ce.src] += 1;
            // The loop hangs off a side the flow does not use: the right, or below when the
            // flow is horizontal.
            let side = across[ce.src];
            let (w, h) = if a.is_group {
                (a.w, a.h)
            } else {
                (view.w, view.h)
            };
            let out = (if side { w } else { h }) / 2.0;
            let spread = max(
                min((if side { h } else { w }) / 2.0 - 4.0, 8.0 + k as f64 * 6.0),
                2.0,
            );
            let reach = LOOP + k as f64 * 8.0;
            let trim = marker_trim(end_type) / or1(hypot(reach, spread));
            // Offsets are given for a loop on the right and turned a quarter for one below.
            let px = |dx: f64, dy: f64| a.x + pad + if side { out + dx } else { -dy };
            let py = |dx: f64, dy: f64| a.y + pad + if side { dy } else { out + dx };
            let mut p = Path::new();
            p.move_to(px(0.0, -spread), py(0.0, -spread)).cube_to(
                px(reach, -2.0 * spread),
                py(reach, -2.0 * spread),
                px(reach, 2.0 * spread),
                py(reach, 2.0 * spread),
                px(reach * trim, spread + spread * trim),
                py(reach * trim, spread + spread * trim),
            );
            let (mut edx, mut edy) = (
                px(-reach, -spread) - px(0.0, 0.0),
                py(-reach, -spread) - py(0.0, 0.0),
            );
            let len = or1(hypot(edx, edy));
            edx /= len;
            edy /= len;
            let path = EdgePath {
                path: p,
                start: Tip::default(),
                end: Tip {
                    at: Point::new(px(0.0, spread), py(0.0, spread)),
                    dx: edx,
                    dy: edy,
                },
            };
            // Labels stack beyond the outermost loop, in the order of their transitions.
            let furthest = LOOP + (loop_count[ce.src] - 1) as f64 * 8.0;
            let below = loop_text[ce.src] + label.height / 2.0;
            loop_text[ce.src] += label.height;
            let (label_x, label_y) = if side {
                (
                    a.x + out + furthest + 4.0 + ce.label_w / 2.0,
                    a.y - loop_height[ce.src] / 2.0 + below,
                )
            } else {
                (a.x, a.y + out + furthest * 0.75 + 4.0 + below)
            };
            (cedges[i].label_x, cedges[i].label_y) = (label_x, label_y);
            path
        } else {
            route = ce.route.clone();
            let last = route.len() - 3;
            let leaves = reach.of(ce.src, side(end_side(&route, 0, &node_of(&cnodes[ce.src]))));
            let arrives = reach.of(
                ce.dst,
                side(end_side(&route, last, &node_of(&cnodes[ce.dst]))),
            );
            for point in route.chunks_mut(3) {
                point[0] += pad;
                point[1] += pad;
            }
            attach(&mut route, 0, ce.src);
            attach(&mut route, last, ce.dst);
            if let Some(&composite) = anchors.get(&i)
                && route.len() >= 6
            {
                let g = &cnodes[composite];
                let inward = views[ce.src].kind == K::Hidden;
                let at = if inward { 0 } else { route.len() - 3 };
                let near = matches!(g.dir, Some(Dir::TB | Dir::LR)) == inward;
                let point = if vertical(g.dir) {
                    let py = if near {
                        g.y + pad + (views[composite].title_h - g.h / 2.0)
                    } else {
                        g.y + pad + g.h / 2.0
                    };
                    [route[at], py, 0.0]
                } else {
                    let px = if near {
                        g.x + pad + -g.w / 2.0
                    } else {
                        g.x + pad + g.w / 2.0
                    };
                    [px, route[at + 1], 1.0]
                };
                if inward {
                    let mut ahead = point.to_vec();
                    ahead.extend(route);
                    route = ahead;
                } else {
                    route.extend(point);
                }
            }
            route_path(
                &route,
                Curve::of(curve.unwrap_or("")),
                0.0,
                marker_trim(end_type),
                leaves,
                arrives,
            )
        };
        let ends = path.ends(None, end_type);
        let mut it = Item::new(
            Kind::Edge(Edge {
                route: Vec::new(),
                ends,
            }),
            if note {
                ROLE_NOTE_EDGE
            } else {
                ROLE_TRANSITION
            },
        );
        it.id = Some(edge.id.clone());
        it.bounds = path.path.bounds();
        // Pele draws the edges in a group with round caps.
        it.style = if note {
            Style {
                stroke: Some(Paint::BORDER),
                fill: Some(Paint::NONE),
                dash: Some(vec![3.0, 4.0]),
                cap: Some(Cap::Round),
                ..Style::default()
            }
        } else {
            Style {
                stroke: Some(Paint::LINE),
                fill: Some(Paint::NONE),
                cap: Some(Cap::Round),
                ..Style::default()
            }
        };
        // A transition inside one fill an author sets takes the color that stands out on it,
        // and one that crosses the border of one is laid on a casing, as Pele's overFills and
        // edgeCasing draw them.
        let (ground, cased) = on_fills(&path.path, &[path.end.at.x, path.end.at.y], &fills, "");
        it.style.ground = ground;
        if cased && let Some(c) = casing(ROLE_CASING, &path.path, &ends, &[], false) {
            casings.push(c);
        }
        let mut points: Vec<Point> = route
            .as_chunks::<3>()
            .0
            .iter()
            .map(|&[x, y, _]| Point::new(x, y))
            .collect();
        if points.is_empty() {
            // A self-loop, which the layout gives no route: the corners its curve is drawn
            // about.
            points = crate::items::corners(&path.path);
        }
        if let Kind::Edge(e) = &mut it.kind {
            e.route = points;
        }
        it.parts = vec![Part::path(path.path, Painting::StrokeOnly)];
        edges.push(it);
        if label.width > 0.0 {
            let ce = &cedges[i];
            let (x, y) = (ce.label_x + pad, ce.label_y + pad);
            let mut it = Item::new(Kind::Text, ROLE_EDGE_LABEL);
            it.id = Some(edge.id.clone());
            it.bounds = label_patch_box(label.width, label.height, x, y);
            it.parts = vec![label_patch(label.width, label.height, x, y)];
            it.label = Some(Placed::new(label.clone(), Point::new(x, y)));
            it.style = Style {
                fill: Some(Paint::BG),
                stroke: Some(Paint::NONE),
                text: Some(Paint::FG),
                ..Style::default()
            };
            labels.push(it);
        }
    }

    let mut sc = Scene {
        diagram: "state".into(),
        width,
        height,
        font_size: size,
        title: db.acc_title.clone(),
        desc: db.acc_descr.clone(),
        items: Vec::new(),
    };
    for layer in [clusters, dividers, casings, edges, labels, nodes, notes] {
        sc.items.extend(layer);
    }
    with_title(&mut sc, db.title.as_deref(), ROLE_TITLE, &set, size, pad);
    sc
}

/// Draws the dashed lines between the regions among a composite's members, items, inside
/// bounds, its left, top, right and bottom, as Pele's divide does. Regions that sit neither
/// side by side nor in a stack each get an outline instead.
fn divide(
    dividers: &mut Vec<Item>,
    views: &[View],
    cnodes: &[CNode],
    pad: f64,
    items: &[usize],
    bounds: [f64; 4],
) {
    let [left, top, right, bottom] = bounds;
    let regions: Vec<usize> = items
        .iter()
        .copied()
        .filter(|&i| views[i].kind == K::Region)
        .collect();
    if regions.len() < 2 {
        return;
    }
    let style = Style {
        stroke: Some(Paint::BORDER),
        fill: Some(Paint::NONE),
        dash: Some(vec![4.0, 4.0]),
        radius: Some(CORNER),
        ..Style::default()
    };
    let lo = |i: usize, x: bool| {
        let c = &cnodes[i];
        if x { c.x - c.w / 2.0 } else { c.y - c.h / 2.0 }
    };
    let hi = |i: usize, x: bool| {
        let c = &cnodes[i];
        if x { c.x + c.w / 2.0 } else { c.y + c.h / 2.0 }
    };
    for x in [true, false] {
        let mut sorted = regions.clone();
        sorted.sort_by(|&a, &b| by(or(lo(a, x) - lo(b, x), a as f64 - b as f64)));
        let apart = sorted
            .windows(2)
            .all(|pair| !(hi(pair[0], x) > lo(pair[1], x) + 0.5));
        if !apart {
            continue;
        }
        for pair in sorted.windows(2) {
            let at = (hi(pair[0], x) + lo(pair[1], x)) / 2.0 + pad;
            let mut p = Path::new();
            if x {
                p.move_to(at, top).line_to(at, bottom);
            } else {
                p.move_to(left, at).line_to(right, at);
            }
            let mut it = Item::new(Kind::Mark, ROLE_DIVIDER);
            it.bounds = p.bounds();
            it.parts = vec![Part::path(p, Painting::StrokeOnly)];
            it.style = style.clone();
            dividers.push(it);
        }
        return;
    }
    for i in regions {
        let c = &cnodes[i];
        let bounds = Rect::new(c.x - c.w / 2.0 + pad, c.y - c.h / 2.0 + pad, c.w, c.h);
        let mut it = Item::new(Kind::Mark, ROLE_DIVIDER);
        it.bounds = bounds;
        it.parts = vec![Part::rect(bounds, true, Painting::StrokeOnly)];
        it.style = style.clone();
        dividers.push(it);
    }
}

/// Where a composite's title sits in its title bar, so that edges entering from above do not
/// cross it, as Pele's titleX picks it.
fn title_x(c: &CNode, width: f64, crossings: &Crossings) -> f64 {
    let Some(xs) = crossings.at(c.y - c.h / 2.0) else {
        return c.x;
    };
    if width == 0.0 {
        return c.x;
    }
    let left = c.x - c.w / 2.0;
    let half = width / 2.0;
    let (mut best, mut best_gap) = (c.x, -1.0);
    for x in [c.x, left + GROUP_PAD + half, left + c.w - GROUP_PAD - half] {
        let mut gap = f64::INFINITY;
        for &cx in xs {
            if cx > left && cx < left + c.w {
                gap = min(gap, (cx - x).abs() - half);
            }
        }
        if gap >= 6.0 {
            return x;
        }
        if gap > best_gap + 0.5 {
            (best, best_gap) = (x, gap);
        }
    }
    best
}
