//! A class diagram laid out as a scene, as Pele's renderClass in render.ts draws it.
//!
//! Pele also sets the annotations and the texts at the ends of relations as math where they
//! hold `$$` and the host typesets it. limu-rs does not, as its labels do not, so they are
//! always text, and the room Pele leaves for the height of math between ranks is none.

use std::collections::HashMap;

use super::db::{ClassDb, ClassNode};
use super::graph::{GraphEdge, GraphNode, build_class_graph};
use super::members::ClassMember;
use super::{class_config, hierarchical};
use crate::common::{
    class_names, clickable, line_of, parse_generic_types, routing_curve, runs_across, setting,
    tighten, trim_default, turn_to_fit, with_title,
};
use crate::css::{declarations, resolve_style};
use crate::draw::{
    Crossings, Curve, EdgePath, Node, STRUCK_HALO, Tip, cluster_title_x, end_side, label_patch,
    label_patch_box, mark_crossings, marker_trim, own_reach, route_path, struck_depth, struck_tint,
    title_crossed,
};
use crate::graphs::components::wrap_components;
use crate::graphs::compound::{CEdge, CNode, Dir, cnode, compound_layout, direction};
use crate::graphs::layered::LayeredOptions;
use crate::js::chars::trim;
use crate::js::math::{by, max, min, or, round};
use crate::layout::{Monospace, Options};
use crate::scene::style::{Cap, Join, Paint, Style, Token};
use crate::scene::text::{Face, Label};
use crate::scene::{
    Align, Edge, Item, Kind, MarkerType, Painting, Part, Path, Placed, Point, Rect, Scene, Shape,
};
use crate::text::{Setting, decode_entities, layout_label};
use crate::url::link_url;
use crate::value::{Map, Value};

/// The roles of the items of a class diagram.
pub(crate) const ROLE_ANNOTATION: &str = "class.annotation";
pub(crate) const ROLE_ATTRIBUTE: &str = "class.attribute";
pub(crate) const ROLE_CARDINALITY: &str = "class.cardinality";
pub(crate) const ROLE_CLASS: &str = "class.class";
pub(crate) const ROLE_DIVIDER: &str = "class.divider";
pub(crate) const ROLE_EDGE_LABEL: &str = "class.edge-label";
pub(crate) const ROLE_INTERFACE: &str = "class.interface";
pub(crate) const ROLE_METHOD: &str = "class.method";
pub(crate) const ROLE_NAMESPACE: &str = "class.namespace";
pub(crate) const ROLE_NAMESPACE_TITLE: &str = "class.namespace-title";
pub(crate) const ROLE_NOTE: &str = "class.note";
pub(crate) const ROLE_NOTE_EDGE: &str = "class.note-edge";
pub(crate) const ROLE_RELATION: &str = "class.relation";
pub(crate) const ROLE_TITLE: &str = "class.title";

// The spacing of a class diagram, as Pele's render.ts has it.
const NODE_SEP: f64 = 40.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 56.0;
const GROUP_PAD: f64 = 20.0;
const LOOP: f64 = 26.0;
/// Between a class's side and its text.
const PAD_X: f64 = 12.0;
/// Above and below a class's name, and about a note's text.
const TITLE_PAD: f64 = 8.0;
/// Above and below the members of a compartment.
const SECTION_PAD: f64 = 6.0;
/// The height of a compartment with no members.
const EMPTY_SECTION: f64 = 10.0;
/// The least width of a class.
const MIN_WIDTH: f64 = 64.0;
/// The size of a note's folded corner.
const FOLD: f64 = 8.0;

/// A number of the class section of a configuration, where it is one above 0, at most 2000,
/// as Pele's numberOption reads it.
fn number_option(section: Option<&Map>, key: &str, fallback: f64) -> f64 {
    match section.and_then(|s| s.get(key)) {
        Some(&Value::Number(v)) if v > 0.0 => min(v, 2000.0),
        _ => fallback,
    }
}

/// A member of a class, or an annotation, as it is drawn, Pele's Line.
struct Line {
    text: String,
    width: f64,
    classifier: &'static str,
}

/// A node as it is drawn, Pele's View, with what its author sets.
struct View<'g> {
    node: &'g GraphNode,
    /// The style the author gives a class.
    author: Style,
    label: Label,
    w: f64,
    h: f64,
    annotations: Vec<Line>,
    attributes: Vec<Line>,
    methods: Vec<Line>,
    title_h: f64,
    attributes_h: f64,
    font_size: f64,
    /// Whether the members are set in the monospace face.
    mono: bool,
    /// Whether Pele's inkFill picks the color of the class's texts on the fill its author
    /// gives it.
    inks: bool,
}

impl<'g> View<'g> {
    fn new(node: &'g GraphNode, label: Label, w: f64, h: f64, font_size: f64) -> View<'g> {
        View {
            node,
            author: Style::default(),
            label,
            w,
            h,
            annotations: Vec::new(),
            attributes: Vec::new(),
            methods: Vec::new(),
            title_h: 0.0,
            attributes_h: 0.0,
            font_size,
            mono: false,
            inks: false,
        }
    }
}

/// Where an edge meets the side of a class, Pele's End: the edge, by its place, and the
/// point of its route.
#[derive(Clone, Copy)]
struct End {
    edge: usize,
    at: usize,
    toward: f64,
    order: usize,
    room: f64,
    side: usize,
    /// Whether the text of this end goes on the right of, or below, its line.
    after: bool,
}

/// The text at an end of a relation, as it is placed.
struct Card {
    text: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    /// The node of the class it is beside.
    parent: usize,
}

/// The face a member is set in: italic where it is abstract, and monospace where mono says,
/// as Pele's memberLine measures it.
fn member_face(classifier: &str, mono: bool) -> Face {
    let mut face = Face::REGULAR;
    if classifier == "*" {
        face |= Face::ITALIC;
    }
    if mono {
        face |= Face::MONO;
    }
    face
}

/// A member as it is drawn, at a size, as Pele's memberLine gives it.
fn member_line(m: &ClassMember, set: &Setting, size: f64, mono: bool) -> Line {
    let text = trim(&decode_entities(&m.display_text())).to_string();
    let width = set.width(&text, size, member_face(m.classifier, mono));
    Line {
        text,
        width,
        classifier: m.classifier,
    }
}

/// The declarations an author sets for a class: those of the classDefs of its classes, and
/// then its own. Pele's classStyle keeps the last declaration of each property that has an
/// effect and resolves those, which comes to the same as resolving them all.
fn class_decls(db: &ClassDb, cls: &ClassNode) -> Vec<String> {
    let mut decls = Vec::new();
    for name in cls.css_classes.split(' ') {
        if let Some(def) = db.style_classes.get(name) {
            for s in &def.styles {
                decls.extend(s.split(',').map(str::to_string));
            }
        }
    }
    decls.extend(cls.styles.iter().cloned());
    decls
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

/// The marker of an end of a relation, for the marker Pele's classMarker draws there.
fn marker_type(name: &str) -> Option<MarkerType> {
    if name == "none" {
        return None;
    }
    MarkerType::from_name(&format!("class.{name}"))
}

/// Lays out a parsed class diagram as a scene, as Pele's renderClass draws it: across and
/// loosely, and, where that does not fit the host's width, tightly, or down instead of
/// across, whichever is narrower.
pub(crate) fn layout(db: &ClassDb, config: &Map, o: &Options) -> Scene {
    let fitted = |down: bool| tighten(o, |tight| draw(db, config, o, down, tight));
    if runs_across(Some(&db.direction)) {
        turn_to_fit(o, fitted)
    } else {
        fitted(false)
    }
}

/// Lays the diagram out once, as Pele's draw does. turned lays out a diagram that runs
/// across as one that runs down, and tight gives up the room that keeps a namespace's name
/// clear of an edge coming in beside it.
fn draw(db: &ClassDb, config: &Map, o: &Options, turned: bool, tight: bool) -> Scene {
    let conf = class_config(config);
    let graph = build_class_graph(db, hierarchical(config));
    let hide_empty = conf.and_then(|c| c.get("hideEmptyMembersBox")) == Some(&Value::Bool(true));
    let monospaced = o.monospace.contains(Monospace::MEMBERS);
    let size = o.font_size;
    let small = round(size * 0.875);
    let tiny = round(size * 0.8);
    let tiny_height = round(tiny * 1.4);
    let set = setting(o);
    let pad = o.padding;
    let card_width = |raw: &str| {
        if raw.is_empty() {
            0.0
        } else {
            set.width(&decode_entities(raw), tiny, Face::REGULAR)
        }
    };
    let root_dir = if turned {
        Dir::TB
    } else {
        direction(Some(&db.direction))
    };
    let nodes = &graph.nodes;

    let mut children = vec![0; nodes.len()];
    for node in nodes {
        if node.parent >= 0 {
            children[node.parent as usize] += 1;
        }
    }

    // Namespaces that an edge may cross into are made wide enough for the title to sit
    // beside it.
    let mut crossed = vec![false; nodes.len()];
    for edge in &graph.edges {
        if edge.source < 0 || edge.target < 0 {
            continue;
        }
        let a = nodes[edge.source as usize].parent;
        let b = nodes[edge.target as usize].parent;
        if a == b {
            continue;
        }
        for start in [a, b] {
            let mut g = start;
            while g >= 0 && !crossed[g as usize] {
                crossed[g as usize] = true;
                g = nodes[g as usize].parent;
            }
        }
    }

    // A class as it is drawn: its name, its annotations and its members, and the box they
    // fit in, as the classView of Pele's draw measures it.
    let class_view = |i: usize, cls: &ClassNode| {
        let node = &nodes[i];
        let decls = class_decls(db, cls);
        let style = resolve_style(&decls);
        // A font family the author gives the class keeps its members out of the monospace
        // face.
        let mono = monospaced && !style.font_family_declared();
        let font_size = style.font_size.unwrap_or(size);
        let member_size = round(font_size * 0.875);
        let line_height = round(member_size * 1.5);
        // The generic type is escaped so that `<T>` is not read as a tag.
        let generic = if cls.kind.is_empty() {
            String::new()
        } else {
            let t = parse_generic_types(&cls.kind);
            format!("&lt;{}&gt;", t.replace('<', "&lt;").replace('>', "&gt;"))
        };
        let mut face = Face::BOLD;
        if style.italic {
            face |= Face::ITALIC;
        }
        let label = layout_label(
            &format!("{}{generic}", cls.label),
            false,
            &set,
            font_size,
            4000.0,
            face,
        );
        let annotations: Vec<Line> = cls
            .annotations
            .iter()
            .map(|a| {
                let text = format!("«{}»", trim(&decode_entities(a)));
                let width = set.width(&text, tiny, Face::REGULAR);
                Line {
                    text,
                    width,
                    classifier: "",
                }
            })
            .collect();
        let lines = |list: &[ClassMember]| -> Vec<Line> {
            list.iter()
                .map(|m| member_line(m, &set, member_size, mono))
                .collect()
        };
        let (attributes, methods) = (lines(&cls.members), lines(&cls.methods));
        let mut inner = label.width;
        let mut annotations_h = 0.0;
        for a in &annotations {
            inner = max(inner, a.width);
            annotations_h += tiny_height;
        }
        for l in attributes.iter().chain(&methods) {
            inner = max(inner, l.width);
        }
        let sections = !(hide_empty && attributes.is_empty() && methods.is_empty());
        let section = |count: usize| {
            if !sections {
                0.0
            } else if count > 0 {
                2.0 * SECTION_PAD + count as f64 * line_height
            } else {
                EMPTY_SECTION
            }
        };
        let title_h = 2.0 * TITLE_PAD + annotations_h + max(label.height, round(font_size * 1.5));
        let attributes_h = section(attributes.len());
        let h = title_h + attributes_h + section(methods.len());
        View {
            node,
            author: declarations(&decls).style,
            w: max(inner + 2.0 * PAD_X, MIN_WIDTH),
            h,
            label,
            annotations,
            attributes,
            methods,
            title_h,
            attributes_h,
            font_size,
            mono,
            inks: style.inks_text(&style),
        }
    };

    let mut views: Vec<View> = Vec::with_capacity(nodes.len());
    let mut cnodes: Vec<CNode> = Vec::with_capacity(nodes.len());
    for (i, node) in nodes.iter().enumerate() {
        let (view, mut c) = if node.is_group {
            let label = layout_label(&node.label, false, &set, size, 4000.0, Face::REGULAR);
            if children[i] > 0 {
                let mut c = cnode(0.0, 0.0, -1);
                c.is_group = true;
                c.pad_x = GROUP_PAD;
                c.pad_top = if label.height > 0.0 {
                    label.height + 16.0
                } else {
                    GROUP_PAD
                };
                c.pad_bottom = GROUP_PAD;
                let room = if crossed[i] && !tight {
                    min(label.width + 24.0, 240.0)
                } else {
                    0.0
                };
                c.min_w = label.width + 2.0 * GROUP_PAD - 12.0 + room;
                (View::new(node, label, 0.0, 0.0, size), c)
            } else {
                let w = max(label.width + 2.0 * GROUP_PAD, 80.0);
                let h = label.height + 2.0 * GROUP_PAD;
                (View::new(node, label, w, h, size), cnode(w, h, -1))
            }
        } else if let Some(at) = node.class {
            let view = class_view(i, &db.classes[at]);
            let c = cnode(view.w, view.h, -1);
            (view, c)
        } else if node.shape == "note" {
            let label = layout_label(&node.label, true, &set, small, 260.0, Face::REGULAR);
            let w = max(label.width + 2.0 * PAD_X, 40.0);
            let h = label.height + 2.0 * TITLE_PAD;
            (View::new(node, label, w, h, small), cnode(w, h, -1))
        } else {
            let label = layout_label(&node.label, false, &set, small, 200.0, Face::REGULAR);
            let w = max(label.width, 12.0) + 4.0;
            let h = max(label.height, 12.0);
            (View::new(node, label, w, h, small), cnode(w, h, -1))
        };
        c.parent = node.parent;
        c.seq = i as f64;
        views.push(view);
        cnodes.push(c);
    }

    let mut cedges: Vec<CEdge> = Vec::new();
    let mut drawn: Vec<&GraphEdge> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    let mut start_room: Vec<f64> = Vec::new();
    let mut end_room: Vec<f64> = Vec::new();
    // Edges leave one side of a class and arrive at the other, so a class with many of them
    // is widened until they, and the text at their ends, fit side by side.
    let mut crowd = vec![0.0; views.len() * 4];
    let mut meet = |item: usize, slot: usize, room: f64| {
        crowd[item * 4 + slot] += 1.0;
        if room > crowd[item * 4 + slot + 1] {
            crowd[item * 4 + slot + 1] = room;
        }
    };
    for edge in &graph.edges {
        if edge.source < 0 || edge.target < 0 {
            continue;
        }
        let (src, dst) = (edge.source as usize, edge.target as usize);
        let label = layout_label(
            edge.label.as_deref().unwrap_or(""),
            true,
            &set,
            small,
            200.0,
            Face::REGULAR,
        );
        let start_w = card_width(&edge.start_label_right);
        let end_w = card_width(&edge.end_label_left);
        if src == dst {
            let room = if label.width > 0.0 {
                label.width + 12.0
            } else {
                0.0
            };
            cnodes[src].w += 2.0 * (LOOP + room);
        } else {
            meet(src, 0, start_w);
            meet(dst, 2, end_w);
        }
        start_room.push(start_w);
        end_room.push(end_w);
        drawn.push(edge);
        cedges.push(CEdge {
            src,
            dst,
            minlen: 1.0,
            label_w: if label.width > 0.0 {
                label.width + 12.0
            } else {
                0.0
            },
            label_h: if label.height > 0.0 {
                label.height + 4.0
            } else {
                0.0
            },
            route: Vec::new(),
            label_x: 0.0,
            label_y: 0.0,
        });
        edge_labels.push(label);
    }

    let down = matches!(root_dir, Dir::TB | Dir::BT);
    if down {
        for i in 0..views.len() {
            if views[i].node.class.is_none() {
                continue;
            }
            let mut need: f64 = 0.0;
            for slot in [0, 2] {
                let count = crowd[i * 4 + slot];
                let room = crowd[i * 4 + slot + 1];
                if count > 1.0 {
                    let per = if room > 0.0 { room + 24.0 } else { 14.0 };
                    need = max(need, (count - 1.0) * per + 24.0);
                }
            }
            need = min(need, 480.0);
            if need > views[i].w {
                cnodes[i].w += need - views[i].w;
                views[i].w = need;
            }
        }
    }

    let lettered = (0..drawn.len()).any(|i| start_room[i] > 0.0 || end_room[i] > 0.0);
    // The host may ask for edges at right angles, or straight, as it may of a flowchart.
    let curve = routing_curve(o.routing);
    let opt = LayeredOptions {
        node_sep: number_option(conf, "nodeSpacing", NODE_SEP),
        edge_sep: EDGE_SEP,
        rank_sep: number_option(
            conf,
            "rankSpacing",
            if lettered { RANK_SEP + 16.0 } else { RANK_SEP },
        ),
        port_sep: 20.0,
        tracks: curve == Some("step"),
        straight: curve == Some("linear"),
        apart: true,
        ..LayeredOptions::default()
    };
    let result = compound_layout(&mut cnodes, &mut cedges, root_dir, &opt);

    // Every edge reaches the middle of a side. Where several share a side they are spread
    // along it, in the order of where they come from. An end with text runs straight for the
    // length of the text, which sits beside it on the side the line then bends toward, away
    // from the neighbors that bend the other way. Neighbors are set apart far enough for the
    // text between them.
    let mut sides: HashMap<usize, Vec<End>> = HashMap::new();
    let mut side_keys: Vec<usize> = Vec::new();
    let mut starts: Vec<Option<End>> = vec![None; cedges.len()];
    let mut ends: Vec<Option<End>> = vec![None; cedges.len()];
    // Records where an end of an edge meets the side of a class, as the attach of Pele's
    // draw does, and gives the end, or none where it meets no side.
    let mut attach = |cedges: &mut [CEdge], edge: usize, at: usize, item: usize, room: f64| {
        let c = &cnodes[item];
        let route = &mut cedges[edge].route;
        let side = end_side(route, at, &node_of(c))?;
        let slack = (c.w - views[item].w) / 2.0;
        if side == 1 {
            route[at] -= slack;
        } else if side == 3 {
            route[at] += slack;
        }
        let across = side & 1;
        let toward = route[if at == 0 { 3 } else { at - 3 } + across];
        let bend = toward - route[at + across];
        let end = End {
            edge,
            at,
            toward,
            order: edge,
            room,
            side,
            after: if across == 1 { bend > 0.0 } else { bend >= 0.0 },
        };
        let key = item * 4 + side;
        if !sides.contains_key(&key) {
            side_keys.push(key);
        }
        sides.entry(key).or_default().push(end);
        Some(end)
    };
    for i in 0..cedges.len() {
        let ce = &cedges[i];
        if ce.src != ce.dst && ce.route.len() >= 6 {
            let (src, dst, last) = (ce.src, ce.dst, ce.route.len() - 3);
            starts[i] = attach(&mut cedges, i, 0, src, start_room[i]);
            ends[i] = attach(&mut cedges, i, last, dst, end_room[i]);
        }
    }
    // Whether a point lies on the border of one of the namespaces around a class, as the
    // onBorder of Pele's draw says.
    let on_border = |item: usize, x: f64, y: f64| {
        let (mut g, mut hops) = (cnodes[item].parent, 0);
        while g >= 0 && hops < 10000 {
            let c = &cnodes[g as usize];
            let dx = (x - c.x).abs();
            let dy = (y - c.y).abs();
            if (dy - c.h / 2.0).abs() < 0.5 && dx <= c.w / 2.0 + 0.5
                || (dx - c.w / 2.0).abs() < 0.5 && dy <= c.h / 2.0 + 0.5
            {
                return true;
            }
            g = c.parent;
            hops += 1;
        }
        false
    };
    let mut gaps: Vec<f64> = Vec::new();
    for key in &side_keys {
        let list = sides.get_mut(key).expect("a side recorded");
        if list.len() < 2 {
            continue;
        }
        let item = key >> 2;
        let across = key & 1;
        let length = if across == 1 {
            views[item].h
        } else {
            views[item].w
        };
        list.sort_by(|a, b| by(or(a.toward - b.toward, a.order as f64 - b.order as f64)));
        let mut total = 0.0;
        gaps.clear();
        for j in 0..list.len() - 1 {
            let (a, b) = (&list[j], &list[j + 1]);
            let mut gap = if across == 1 { 20.0 } else { 24.0 };
            if a.room > 0.0 && a.after {
                gap += if across == 1 {
                    tiny_height + 6.0
                } else {
                    a.room
                };
            }
            if b.room > 0.0 && !b.after {
                gap += if across == 1 {
                    tiny_height + 6.0
                } else {
                    b.room
                };
            }
            gaps.push(gap);
            total += gap;
        }
        let scale = min(1.0, (length - 24.0) / total);
        if scale <= 0.0 {
            continue;
        }
        let mut at = if across == 1 {
            cnodes[item].y
        } else {
            cnodes[item].x
        } - total * scale / 2.0;
        for (j, e) in list.iter().enumerate() {
            let route = &mut cedges[e.edge].route;
            let end = e.at;
            // An edge that crosses the border of the class's namespace close by runs
            // straight to it: the point where it crosses moves into line with its end.
            let step: isize = if end == 0 { 3 } else { -3 };
            let first = end as isize + step;
            let mut border = None;
            let mut k = first;
            while k >= 0
                && (k as usize) < route.len()
                && (route[k as usize + across] - route[first as usize + across]).abs() < 0.5
            {
                if on_border(item, route[k as usize], route[k as usize + 1]) {
                    border = Some(k);
                    break;
                }
                k += step;
            }
            if let Some(border) = border
                && (route[first as usize + across] - at).abs() <= 24.0
            {
                let mut k = first;
                while k != border + step {
                    route[k as usize + across] = at;
                    k += step;
                }
            }
            route[end + across] = at;
            if let Some(gap) = gaps.get(j) {
                at += gap * scale;
            }
        }
    }

    // Makes an end with text run straight from the class for the length of the text, with a
    // point added to its route, as the straighten of Pele's draw does.
    let straighten = |route: &mut Vec<f64>, end: Option<End>, room: f64, first: bool| {
        let Some(end) = end else {
            return;
        };
        if room <= 0.0 {
            return;
        }
        let across = end.side & 1;
        let at = if first { 0 } else { route.len() - 3 };
        let along = at + 1 - across;
        let from = if first { 3 } else { at - 3 };
        let reach = (route[from + 1 - across] - route[along]).abs();
        let length = min(
            if across == 1 {
                room + 10.0
            } else {
                tiny_height + 6.0
            },
            reach * 0.35,
        );
        if length < 4.0 {
            return;
        }
        let mut point = [route[at], route[at + 1], route[at + 2]];
        let sign = if end.side == 0 || end.side == 3 {
            -1.0
        } else {
            1.0
        };
        point[1 - across] += sign * length;
        let to = if first { 3 } else { at };
        route.splice(to..to, point);
    };
    for i in 0..cedges.len() {
        straighten(&mut cedges[i].route, starts[i], start_room[i], true);
        straighten(&mut cedges[i].route, ends[i], end_room[i], false);
    }

    let (mut min_x, mut min_y) = (0.0, 0.0);
    let (mut max_x, mut max_y) = (result.width, result.height);
    let mut cards: Vec<Card> = Vec::new();
    // Places the text at an end of a relation beside its line, and widens the bounds to take
    // it, as the card of Pele's draw does.
    let mut card = |parent: usize, raw: &str, width: f64, x: f64, y: f64, side: usize, after| {
        if raw.is_empty() {
            return;
        }
        let h = tiny_height;
        let (mut cx, mut cy) = (x, y);
        if side & 1 == 1 {
            cx += if side == 3 { -1.0 } else { 1.0 } * (5.0 + width / 2.0);
            cy += if after { 1.0 } else { -1.0 } * (7.0 + h / 2.0);
        } else {
            cx += if after { 1.0 } else { -1.0 } * (9.0 + width / 2.0);
            cy += if side == 0 { -1.0 } else { 1.0 } * (3.0 + h / 2.0);
        }
        cards.push(Card {
            text: decode_entities(raw),
            x: cx,
            y: cy,
            w: width,
            h,
            parent,
        });
        min_x = min(min_x, cx - width / 2.0);
        max_x = max(max_x, cx + width / 2.0);
        min_y = min(min_y, cy - h / 2.0);
        max_y = max(max_y, cy + h / 2.0);
    };
    let mut loop_seen: HashMap<usize, usize> = HashMap::new();
    let mut loop_at = vec![0; cedges.len()];
    for i in 0..cedges.len() {
        let edge = drawn[i];
        let ce = &mut cedges[i];
        if ce.src == ce.dst {
            let k = *loop_seen.get(&ce.src).unwrap_or(&0);
            loop_seen.insert(ce.src, k + 1);
            loop_at[i] = k;
            let c = &cnodes[ce.src];
            let view = &views[ce.src];
            let x = c.x + view.w / 2.0;
            let spread = min(view.h / 2.0 - 4.0, 8.0 + k as f64 * 6.0);
            card(
                ce.src,
                &edge.start_label_right,
                start_room[i],
                x,
                c.y - spread,
                1,
                false,
            );
            card(
                ce.dst,
                &edge.end_label_left,
                end_room[i],
                x,
                c.y + spread,
                1,
                true,
            );
            ce.label_x = c.x + view.w / 2.0 + LOOP + k as f64 * 8.0 + 4.0 + ce.label_w / 2.0;
            ce.label_y = c.y;
        } else if ce.route.len() >= 6 {
            let last = ce.route.len() - 3;
            let (from_side, from_after) = starts[i].map_or((2, true), |s| (s.side, s.after));
            let (to_side, to_after) = ends[i].map_or((0, true), |e| (e.side, e.after));
            let (sx, sy) = (ce.route[0], ce.route[1]);
            let (ex, ey) = (ce.route[last], ce.route[last + 1]);
            card(
                ce.src,
                &edge.start_label_right,
                start_room[i],
                sx,
                sy,
                from_side,
                from_after,
            );
            card(
                ce.dst,
                &edge.end_label_left,
                end_room[i],
                ex,
                ey,
                to_side,
                to_after,
            );
        }
    }

    // A diagram wider than the host has its parts that nothing joins wrapped into rows, the
    // texts at the ends of relations with the classes they are beside, as Pele's draw wraps
    // them.
    if o.max_width > 0.0 && max_x - min_x + 2.0 * pad > o.max_width {
        let mut all = cnodes.clone();
        for item in &cards {
            let mut n = cnode(item.w, item.h, item.parent as i32);
            (n.x, n.y) = (item.x, item.y);
            all.push(n);
        }
        let width = max(1.0, o.max_width - 2.0 * pad);
        let gap = number_option(conf, "nodeSpacing", NODE_SEP);
        if let Some(wrapped) = wrap_components(&mut all, &mut cedges, width, gap) {
            for (item, n) in cards.iter_mut().zip(&all[cnodes.len()..]) {
                (item.x, item.y) = (n.x, n.y);
            }
            all.truncate(cnodes.len());
            cnodes = all;
            (min_x, min_y) = (0.0, 0.0);
            (max_x, max_y) = (wrapped.width, wrapped.height);
        }
    }

    let ox = pad - min_x;
    let oy = pad - min_y;
    let mut sc = Scene {
        diagram: "class".into(),
        width: (max_x - min_x + 2.0 * pad).ceil(),
        height: (max_y - min_y + 2.0 * pad).ceil(),
        font_size: size,
        title: db.acc_title.clone(),
        desc: db.acc_descr.clone(),
        items: Vec::new(),
    };

    let mut crossings = Crossings::default();
    if nodes.first().is_some_and(|n| n.is_group) {
        for e in &cedges {
            mark_crossings(&mut crossings, &e.route);
        }
    }

    let (mut clusters, mut struck, mut node_items) = (Vec::new(), Vec::new(), Vec::new());
    for (i, view) in views.iter().enumerate() {
        let c = &cnodes[i];
        let n = view.node;
        let (x, y) = (c.x + ox, c.y + oy);
        let (w, h) = if c.is_group {
            (c.w, c.h)
        } else {
            (view.w, view.h)
        };
        if n.is_group {
            let title_x = if c.is_group {
                cluster_title_x(&node_of(c), view.label.width, &crossings, GROUP_PAD)
            } else {
                c.x
            };
            let title_y = y - h / 2.0 + 8.0 + view.label.height / 2.0;
            let bounds = Rect::new(x - w / 2.0, y - h / 2.0, w, h);
            let mut cluster = Item::new(Kind::Group, ROLE_NAMESPACE);
            cluster.id = Some(n.id.clone());
            cluster.bounds = bounds;
            cluster.parts = vec![Part::rect(bounds, true, Painting::FillAndStroke)];
            cluster.style = Style {
                fill: Some(Paint::SURFACE_ALT),
                fill_opacity: Some(0.5),
                stroke: Some(Paint::BORDER),
                text: Some(Paint::MUTED),
                ..Style::default()
            };
            if !view.label.is_empty() {
                let at = Point::new(title_x + ox, title_y);
                if c.is_group && title_crossed(&node_of(c), view.label.width, title_x, &crossings) {
                    let parent = |k: usize| usize::try_from(nodes[k].parent).ok();
                    let depth = struck_depth(parent, i);
                    let mut it = Item::new(Kind::Text, ROLE_NAMESPACE_TITLE);
                    it.id = Some(n.id.clone());
                    it.label = Some(Placed::new(view.label.clone(), at));
                    it.style = Style {
                        text: Some(Paint::MUTED),
                        halo: Some(
                            Paint::SURFACE_ALT
                                .alpha(struck_tint(depth) / 100.0)
                                .over(Token::Bg),
                        ),
                        halo_width: Some(STRUCK_HALO),
                        ..Style::default()
                    };
                    struck.push(it);
                } else {
                    cluster.label = Some(Placed::new(view.label.clone(), at));
                }
            }
            clusters.push(cluster);
            continue;
        }
        let bounds = Rect::new(x - w / 2.0, y - h / 2.0, w, h);
        if let Some(at) = n.class {
            node_items.extend(class_items(
                view,
                &db.classes[at],
                x,
                y,
                tiny,
                tiny_height,
                o,
            ));
        } else if n.shape == "note" {
            let (r, t) = (w / 2.0, -h / 2.0);
            let mut outline = Path::new();
            outline
                .move_to(x + -r, y + t)
                .line_to(x + (r - FOLD), y + t)
                .line_to(x + r, y + (t + FOLD))
                .line_to(x + r, y + -t)
                .line_to(x + -r, y + -t)
                .close();
            let mut corner = Path::new();
            corner
                .move_to(x + (r - FOLD), y + t)
                .line_to(x + (r - FOLD), y + (t + FOLD))
                .line_to(x + r, y + (t + FOLD));
            let mut it = Item::new(Kind::Node, ROLE_NOTE);
            it.shape = Some(Shape::new("note"));
            it.id = Some(n.id.clone());
            it.bounds = bounds;
            it.parts = vec![
                Part::path(outline, Painting::FillAndStroke),
                Part::path(corner, Painting::StrokeOnly),
            ];
            it.label = Some(Placed::new(view.label.clone(), Point::new(x, y)));
            // Pele turns the corners of both of a note's paths round.
            it.style = Style {
                fill: Some(Paint::SURFACE_ALT),
                stroke: Some(Paint::BORDER),
                join: Some(Join::Round),
                ..Style::default()
            };
            node_items.push(it);
        } else {
            let mut it = Item::new(Kind::Node, ROLE_INTERFACE);
            it.shape = Some(Shape::new("interface"));
            it.id = Some(n.id.clone());
            it.bounds = bounds;
            it.label = Some(Placed::new(view.label.clone(), Point::new(x, y)));
            node_items.push(it);
        }
    }

    let (mut edges, mut labels) = (Vec::new(), Vec::new());
    for (i, ce) in cedges.iter_mut().enumerate() {
        let edge = drawn[i];
        let label = &edge_labels[i];
        let start = marker_type(edge.arrow_type_start);
        let end = marker_type(edge.arrow_type_end);
        let (start_trim, end_trim) = (marker_trim(start), marker_trim(end));
        let path = if ce.src == ce.dst {
            let c = &cnodes[ce.src];
            let view = &views[ce.src];
            let k = loop_at[i] as f64;
            let x = c.x + ox + view.w / 2.0;
            let y = c.y + oy;
            let spread = min(view.h / 2.0 - 4.0, 8.0 + k * 6.0);
            let reach = LOOP + k * 8.0;
            let mut p = Path::new();
            p.move_to(x + start_trim, y - spread).cube_to(
                x + reach + start_trim,
                y - spread,
                x + reach + end_trim,
                y + spread,
                x + end_trim,
                y + spread,
            );
            let tip = |y: f64| Tip {
                at: Point::new(x, y),
                dx: -1.0,
                dy: 0.0,
            };
            EdgePath {
                path: p,
                start: tip(y - spread),
                end: tip(y + spread),
            }
        } else {
            for point in ce.route.chunks_mut(3) {
                point[0] += ox;
                point[1] += oy;
            }
            route_path(
                &ce.route,
                Curve::of(curve.unwrap_or("")),
                start_trim,
                end_trim,
                own_reach(start_trim),
                own_reach(end_trim),
            )
        };
        // Pele draws the edges in a group that ends their lines round.
        let mut own = Style {
            stroke: Some(Paint::LINE),
            fill: Some(Paint::NONE),
            cap: Some(Cap::Round),
            ..Style::default()
        };
        let role = match edge.pattern {
            "dashed" => {
                own.dash = Some(vec![6.0, 4.0]);
                ROLE_RELATION
            }
            "dotted" => {
                own.dash = Some(vec![2.0, 4.0]);
                ROLE_NOTE_EDGE
            }
            _ => ROLE_RELATION,
        };
        let mut route: Vec<Point> = ce
            .route
            .as_chunks::<3>()
            .0
            .iter()
            .map(|&[x, y, _]| Point::new(x, y))
            .collect();
        if route.is_empty() {
            // A self-loop, which the layout gives no route: the corners its curve is drawn
            // about.
            route = path.path.corners();
        }
        let mut it = Item::new(
            Kind::Edge(Edge {
                route,
                ends: path.ends(start, end),
            }),
            role,
        );
        it.id = Some(edge.id.clone());
        it.bounds = path.path.bounds();
        it.style = own;
        it.parts = vec![Part::path(path.path, Painting::StrokeOnly)];
        edges.push(it);
        if label.width > 0.0 {
            let (x, y) = (ce.label_x + ox, ce.label_y + oy);
            let mut it = Item::new(Kind::Text, ROLE_EDGE_LABEL);
            it.id = Some(edge.id.clone());
            it.bounds = label_patch_box(label.width, label.height, x, y);
            it.parts = vec![label_patch(label.width, label.height, x, y)];
            it.label = Some(Placed::new(label.clone(), Point::new(x, y)));
            it.style = Style {
                fill: Some(Paint::BG),
                stroke: Some(Paint::NONE),
                ..Style::default()
            };
            labels.push(it);
        }
    }

    let card_items = cards.iter().map(|c| {
        let mut it = Item::new(Kind::Text, ROLE_CARDINALITY);
        let label = line_of(&c.text, c.w, tiny, tiny_height, Face::REGULAR);
        it.label = Some(Placed::new(label, Point::new(c.x + ox, c.y + oy)));
        it.style = Style {
            text: Some(Paint::MUTED),
            ..Style::default()
        };
        it
    });

    sc.items.extend(clusters);
    sc.items.extend(edges);
    sc.items.extend(struck);
    sc.items.extend(labels);
    sc.items.extend(card_items);
    sc.items.extend(node_items);
    with_title(&mut sc, db.title.as_deref(), ROLE_TITLE, &set, size, pad);
    sc
}

/// The items of a class centered on x, y, as Pele's classSvg draws it: the box, with the
/// class's name as its label, the lines under the name and the attributes, and then each
/// annotation and member as a text of its own.
fn class_items(
    view: &View,
    cls: &ClassNode,
    x: f64,
    y: f64,
    tiny: f64,
    tiny_height: f64,
    o: &Options,
) -> Vec<Item> {
    let (w, h) = (view.w, view.h);
    let (left, top) = (-w / 2.0, -h / 2.0);
    let member_size = round(view.font_size * 0.875);
    let line_height = round(member_size * 1.5);
    let bounds = Rect::new(x + left, y + top, w, h);
    let first = top + view.title_h;
    let second = first + view.attributes_h;
    // The texts are set on the class's fill, in the color that stands out on a fill the
    // author gives it, as Pele's inkFill picks it: the name on the box's own fill, and the
    // annotations and members on the fill under them.
    let ground = if view.inks { view.author.fill } else { None };

    let (class, tags) = clickable(class_names(trim_default(&cls.css_classes)));
    let mut at = top + TITLE_PAD;
    let mut annotations = Vec::new();
    for a in &view.annotations {
        let mut it = Item::new(Kind::Text, ROLE_ANNOTATION);
        it.id = Some(cls.id.clone());
        let label = line_of(&a.text, a.width, tiny, tiny_height, Face::REGULAR);
        it.label = Some(Placed::new(
            label,
            Point::new(x, y + (at + tiny_height / 2.0)),
        ));
        it.style = Style {
            text: Some(Paint::MUTED),
            ground,
            ..Style::default()
        };
        annotations.push(it);
        at += tiny_height;
    }
    let mut it = Item::new(Kind::Node, ROLE_CLASS);
    it.shape = Some(Shape::new("classBox"));
    it.id = Some(cls.id.clone());
    (it.class, it.tags) = (class, tags);
    it.bounds = bounds;
    it.parts = vec![Part::rect(bounds, true, Painting::FillAndStroke)];
    it.label = Some(Placed::new(
        view.label.clone(),
        Point::new(x, y + (at + first - TITLE_PAD) / 2.0),
    ));
    it.style = Style {
        fill: Some(Paint::SURFACE),
        stroke: Some(Paint::BORDER),
        on_fill: view.inks,
        ..Style::default()
    };
    it.author = view.author.clone();
    it.tooltip = cls
        .tooltip
        .as_deref()
        .filter(|t| !t.is_empty())
        .map(decode_entities);
    if let Some(link) = cls.link.as_deref().filter(|l| !l.is_empty()) {
        it.link = link_url(link, &o.schemes());
    }
    let mut items = vec![it];
    // The lines under the name and the attributes are an item of their own, as Pele gives
    // them only the stroke the author sets, and none of the rest of the class's style.
    if h > view.title_h {
        let mut divider = Path::new();
        divider
            .move_to(x + left, y + first)
            .line_to(x + -left, y + first)
            .move_to(x + left, y + second)
            .line_to(x + -left, y + second);
        let mut it = Item::new(Kind::Mark, ROLE_DIVIDER);
        it.id = Some(cls.id.clone());
        it.bounds = divider.bounds();
        it.parts = vec![Part::path(divider, Painting::StrokeOnly)];
        it.style = Style {
            stroke: Some(Paint::BORDER),
            ..Style::default()
        };
        it.author = Style {
            stroke: view.author.stroke,
            ..Style::default()
        };
        items.push(it);
    }
    items.extend(annotations);
    // The members of a compartment, one text each, from the top of the compartment down, as
    // the lines of Pele's classSvg draws them. A static member is underlined.
    let mut lines = |list: &[Line], role: &'static str, from: f64| {
        let mut middle = from + SECTION_PAD + line_height / 2.0;
        for line in list {
            let mut face = member_face(line.classifier, view.mono);
            if line.classifier == "$" {
                face |= Face::UNDERLINE;
            }
            let label = line_of(&line.text, line.width, member_size, line_height, face);
            let mut it = Item::new(Kind::Text, role);
            it.id = Some(cls.id.clone());
            let mut placed = Placed::new(label, Point::new(x + (left + PAD_X), y + middle));
            placed.align = Align::Start;
            it.label = Some(placed);
            it.style = Style {
                ground,
                ..Style::default()
            };
            it.author = Style {
                text: view.author.text,
                ..Style::default()
            };
            items.push(it);
            middle += line_height;
        }
    };
    lines(&view.attributes, ROLE_ATTRIBUTE, first);
    lines(&view.methods, ROLE_METHOD, second);
    items
}
