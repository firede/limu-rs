//! An agent flow laid out as a scene, as Pele's renderAgentflow in render.ts draws it.
//!
//! The roles of its items are "agentflow.title", "agentflow.flow" for a flow's box with its
//! title, and a node by its kind: "agentflow.tool", "agentflow.task", "agentflow.decision",
//! "agentflow.input", "agentflow.refdoc", "agentflow.connector" or "agentflow.action". A
//! collapsed flow is "agentflow.collapsed", with "agentflow.collapsed-rule" and
//! "agentflow.collapsed-dots" for the dashed rule and the dots drawn under its label. An edge
//! is "agentflow.sequence", "agentflow.reference" for a dotted one, or "agentflow.failure"
//! for one that ends in a cross. "agentflow.casing" lies under an edge that crosses a fill
//! its author sets, and "agentflow.edge-label" is an edge's label.

use std::collections::HashMap;

use super::color_slots::{KIND_COUNT, kind_slot};
use super::db::{AgentflowDb, LabelType};
use super::graph::{GraphEdge, GraphNode};
use crate::common::{
    class_names, clickable, routing_curve, runs_across, setting, title_label, trim_default,
    turn_to_fit,
};
use crate::css::{Resolved, declarations, resolve_style};
use crate::diagrams::flowchart::canonical_shape;
use crate::draw::{
    Crossings, Curve, EdgePath, Fill, Node, PAD_X, PAD_Y, casing, cluster_title_x, draw_shape,
    inset_route, label_patch, label_patch_box, loop_path, mark_crossings, marker_trim, on_fills,
    own_reach, route_path, shape_filled, shape_size,
};
use crate::graphs::compound::{
    CEdge, CNode, Dir, cnode, compound_layout, direction, inherit_crossed_directions, shift_layout,
};
use crate::graphs::layered::LayeredOptions;
use crate::js::math::{max, min, round};
use crate::layout::{Options, Routing};
use crate::scene::style::{Cap, Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{
    Edge, Item, Kind, MarkerType, Painting, Part, Path, Placed, Point, Rect, Scene, Shape,
};
use crate::text::{decode_entities, layout_label};
use crate::url::{internal_link, link_url};
use crate::value::{Map, Value};

/// The roles of the items of an agent flow, but for its nodes and edges, whose roles are
/// their kinds.
pub(crate) const ROLE_CASING: &str = "agentflow.casing";
pub(crate) const ROLE_COLLAPSED: &str = "agentflow.collapsed";
pub(crate) const ROLE_COLLAPSED_DOTS: &str = "agentflow.collapsed-dots";
pub(crate) const ROLE_COLLAPSED_RULE: &str = "agentflow.collapsed-rule";
pub(crate) const ROLE_EDGE_LABEL: &str = "agentflow.edge-label";
pub(crate) const ROLE_FLOW: &str = "agentflow.flow";
pub(crate) const ROLE_TITLE: &str = "agentflow.title";

// The spacing of an agent flow, as Pele's render.ts has it.
const NODE_SEP: f64 = 40.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 48.0;
const GROUP_PAD: f64 = 20.0;
const LOOP: f64 = 26.0;
const DOTS: f64 = 14.0;

/// The shape a collapsed flow is drawn with: a box with a dashed rule and three dots under
/// its label, Pele's COLLAPSED.
const COLLAPSED: &str = "collapsed";

/// The side a flow runs toward, with sides numbered clockwise from the top, Pele's FLOW_END.
fn flow_end(dir: Dir) -> usize {
    match dir {
        Dir::TB => 2,
        Dir::RL => 3,
        Dir::BT => 0,
        Dir::LR => 1,
    }
}

/// A number of an agent flow's configuration, where it is one above 0, at most 2000, as
/// Pele's numberOption reads it.
fn number_option(config: &Map, key: &str, fallback: f64) -> f64 {
    match config
        .get("agentflow")
        .and_then(Value::as_object)
        .and_then(|s| s.get(key))
    {
        Some(&Value::Number(v)) if v > 0.0 => min(v, 2000.0),
        _ => fallback,
    }
}

/// A palette color as an outline with a light wash of the same color inside, as Pele's tint
/// draws it. The wash is a fill opacity, which Pele writes only where the author declares no
/// fill, so that a fill the author sets is opaque.
fn tint(slot: usize, wash: f64, style: &Resolved) -> Style {
    Style {
        fill: Some(Paint::series(slot)),
        stroke: Some(Paint::series(slot)),
        fill_opacity: if style.fill_declared() {
            None
        } else {
            Some(wash)
        },
        ..Style::default()
    }
}

/// The palette slot of a flow, collapsed or not: its own, or the one past the kinds, as
/// Pele's node.colorIndex ?? KIND_SLOT.size gives it.
fn slot(n: &GraphNode) -> usize {
    n.color_index.unwrap_or(KIND_COUNT)
}

/// A node as it is drawn, Pele's NodeView, with the declarations its style is read from.
struct View<'g> {
    node: &'g GraphNode,
    decls: Vec<String>,
    style: Resolved,
    label: Label,
    shape: &'static str,
    w: f64,
    h: f64,
    dy: f64,
}

/// An edge between a node and a flow around it, which the layout has no place for.
struct Held<'g> {
    edge: &'g GraphEdge,
    label: Label,
    src: usize,
    dst: usize,
}

/// Lays out a parsed agent flow as a scene, as Pele's renderAgentflow draws it: in its own
/// direction, or, where one that runs across does not fit the host's width, down instead if
/// that is narrower.
pub(crate) fn layout(db: &AgentflowDb, config: &Map, o: &Options) -> Scene {
    if runs_across(db.direction.as_deref()) {
        turn_to_fit(o, |down| draw(db, config, o, down))
    } else {
        draw(db, config, o, false)
    }
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

/// The direction of the level node i sits in, by the nearest group about it that sets one,
/// as Pele's flowOf gives it.
fn flow_of(cnodes: &[CNode], root_dir: Dir, i: usize) -> Dir {
    let mut dir = None;
    let (mut p, mut hops) = (cnodes[i].parent, 0);
    while p >= 0 && dir.is_none() && hops < 10000 {
        dir = cnodes[p as usize].dir;
        p = cnodes[p as usize].parent;
        hops += 1;
    }
    dir.unwrap_or(root_dir)
}

/// Whether the level node i sits in runs across, as Pele's flowsSideways says.
fn flows_sideways(cnodes: &[CNode], root_dir: Dir, i: usize) -> bool {
    matches!(flow_of(cnodes, root_dir, i), Dir::LR | Dir::RL)
}

/// Adds a dot 3.5 across to a path, dx, dy from a collapsed flow's center x, y, as Pele's dot
/// draws it: two half circles from its left to its right and back.
fn dot(p: &mut Path, x: f64, y: f64, dx: f64, dy: f64) {
    let left = x + (dx - 1.75);
    p.move_to(left, y + dy)
        .arc_to(1.75, 1.75, 0.0, true, false, left + 3.5, y + dy)
        .arc_to(1.75, 1.75, 0.0, true, false, left, y + dy)
        .close();
}

/// Lays the agent flow out once, as Pele's draw does. turned lays out one that runs across as
/// one that runs down.
fn draw(db: &AgentflowDb, config: &Map, o: &Options, turned: bool) -> Scene {
    let graph = db
        .graph
        .as_ref()
        .expect("an agent flow's graph is built as it is parsed");
    let size = o.font_size;
    let edge_size = round(size * 0.875);
    let set = setting(o);
    let wrap_width = number_option(config, "wrappingWidth", 120.0);
    let min_width = number_option(config, "minNodeWidth", 120.0);
    let pad = o.pad();
    let schemes = o.schemes();
    // The routing the host asks for, which Pele has no option for, overrides the curves.
    let curve = routing_curve(o.routing);

    let mut index: HashMap<&str, usize> = HashMap::new();
    for (i, node) in graph.nodes.iter().enumerate() {
        index.insert(node.id.as_str(), i);
    }
    let mut views: Vec<View> = Vec::with_capacity(graph.nodes.len());
    let mut cnodes: Vec<CNode> = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
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
        let markdown = node.label_type == LabelType::Markdown;
        let font_size = style.font_size.unwrap_or(size);
        let raw = node.label.as_deref().unwrap_or("");
        let (shape, label, w, h, dy, mut c);
        if node.is_group {
            label = layout_label(raw, markdown, &set, font_size, 4000.0, face);
            (shape, w, h, dy) = ("group", 0.0, 0.0, 0.0);
            c = cnode(0.0, 0.0, -1);
            c.is_group = true;
            c.dir = node.dir.as_deref().map(|d| direction(Some(d)));
            c.pad_x = GROUP_PAD;
            c.pad_top = if label.height > 0.0 {
                label.height + 16.0
            } else {
                GROUP_PAD
            };
            c.pad_bottom = GROUP_PAD;
            c.min_w = label.width + 2.0 * GROUP_PAD - 12.0;
        } else if node.shape == "collapsedGroup" {
            label = layout_label(raw, markdown, &set, font_size, wrap_width, face);
            shape = COLLAPSED;
            w = max(label.width, min_width) + 2.0 * PAD_X;
            h = label.height + 2.0 * PAD_Y + DOTS;
            dy = -DOTS / 2.0;
            c = cnode(w, h, -1);
        } else {
            shape = canonical_shape(Some(&node.shape));
            label = layout_label(raw, markdown, &set, font_size, wrap_width, face);
            // A diamond grows twice as fast as its label, so it keeps the label's own width.
            let tw = if shape == "diam" {
                label.width
            } else {
                max(label.width, min_width)
            };
            let s = shape_size(shape, tw, label.height);
            (w, h, dy) = (s.w, s.h, s.dy);
            c = cnode(w, h, -1);
        }
        c.seq = views.len() as f64;
        views.push(View {
            node,
            decls,
            style,
            label,
            shape,
            w,
            h,
            dy,
        });
        cnodes.push(c);
    }
    for (i, node) in graph.nodes.iter().enumerate() {
        cnodes[i].parent = node
            .parent_id
            .as_deref()
            .map_or(-1, |p| index.get(p).map_or(-1, |&i| i as i32));
    }
    let at = |id: &str| index.get(id).map_or(-1, |&i| i as i32);
    inherit_crossed_directions(
        &mut cnodes,
        graph
            .edges
            .iter()
            .map(|e| (at(&e.start), at(&e.end)))
            .collect::<Vec<_>>(),
    );

    let root_dir = if turned {
        Dir::TB
    } else {
        direction(db.direction.as_deref())
    };

    let mut depth = vec![-1i64; cnodes.len()];
    for i in 0..cnodes.len() {
        let mut chain = Vec::new();
        let mut j = i as i32;
        while j >= 0 && depth[j as usize] < 0 && chain.len() <= cnodes.len() {
            chain.push(j as usize);
            j = cnodes[j as usize].parent;
        }
        let d = if j >= 0 { depth[j as usize] + 1 } else { 0 };
        for (d, &k) in (d..).zip(chain.iter().rev()) {
            depth[k] = d;
        }
    }
    // Whether node i is inside group, at any depth, as Pele's contains says.
    let contains = |cnodes: &[CNode], group: usize, i: usize| {
        let mut j = i as i32;
        let mut d = depth[i];
        while d > depth[group] && j >= 0 {
            j = cnodes[j as usize].parent;
            d -= 1;
        }
        j == group as i32 && i != group
    };

    let mut cedges: Vec<CEdge> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    let mut drawn: Vec<&GraphEdge> = Vec::new();
    // Edges between a node and a flow around it. The layout has no place for them, so they
    // are drawn afterwards, between the node and the flow's border.
    let mut held: Vec<Held> = Vec::new();
    for edge in &graph.edges {
        let (Some(&src), Some(&dst)) =
            (index.get(edge.start.as_str()), index.get(edge.end.as_str()))
        else {
            continue;
        };
        let markdown = edge.label_type == LabelType::Markdown;
        let label = layout_label(
            &edge.label,
            markdown,
            &set,
            edge_size,
            wrap_width,
            Face::default(),
        );
        if contains(&cnodes, src, dst) || contains(&cnodes, dst, src) {
            held.push(Held {
                edge,
                label,
                src,
                dst,
            });
            continue;
        }
        if src == dst {
            // A self-loop sits beside the node, across the flow, and the node's box grows to
            // hold it.
            if flows_sideways(&cnodes, root_dir, src) {
                cnodes[src].h += 2.0
                    * (LOOP
                        + if label.height > 0.0 {
                            label.height + 8.0
                        } else {
                            0.0
                        });
            } else {
                cnodes[src].w += 2.0
                    * (LOOP
                        + if label.width > 0.0 {
                            label.width + 12.0
                        } else {
                            0.0
                        });
            }
        }
        cedges.push(CEdge {
            src,
            dst,
            minlen: edge.minlen.unwrap_or(1.0),
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
        drawn.push(edge);
        edge_labels.push(label);
    }

    let opt = LayeredOptions {
        node_sep: number_option(config, "nodeSpacing", NODE_SEP),
        edge_sep: EDGE_SEP,
        rank_sep: number_option(config, "rankSpacing", RANK_SEP),
        port_sep: 20.0,
        tracks: curve == Some("step"),
        straight: curve == Some("linear"),
        ..LayeredOptions::default()
    };
    let result = compound_layout(&mut cnodes, &mut cedges, root_dir, &opt);

    let title = title_label(db.title.as_deref(), &set, size, 4000.0);
    let title_height = if title.height > 0.0 {
        title.height + 12.0
    } else {
        0.0
    };
    let inner = max(result.width, title.width);
    let ox = pad + (inner - result.width) / 2.0;
    let oy = pad + title_height;
    shift_layout(&mut cnodes, &mut cedges, ox, oy);
    let mut sc = Scene {
        diagram: "agentflow".into(),
        width: (inner + 2.0 * pad).ceil(),
        height: (result.height + title_height + 2.0 * pad).ceil(),
        font_size: size,
        title: db.acc_title.clone(),
        desc: db.acc_descr.clone(),
        items: Vec::new(),
    };

    // An edge between a node and a flow around it runs straight between the node and the
    // flow's border: out along the flow, or in against it. If another node is in the way it
    // takes a side instead, and failing that the far end.
    let mut held_paths: Vec<EdgePath> = Vec::with_capacity(held.len());
    let mut held_labels: Vec<f64> = Vec::with_capacity(2 * held.len());
    let mut held_routes: Vec<Vec<f64>> = Vec::with_capacity(held.len());
    let mut held_drawn: Vec<Vec<f64>> = Vec::with_capacity(held.len());
    let careful = ((held.len() * cnodes.len()) as f64) < 2e6;
    for h in &held {
        let out = contains(&cnodes, h.dst, h.src);
        let (outer, i) = if out { (h.dst, h.src) } else { (h.src, h.dst) };
        let (g, n) = (&cnodes[outer], &cnodes[i]);
        let end = flow_end(g.dir.unwrap_or_else(|| flow_of(&cnodes, root_dir, outer)));
        let first = if out { end } else { (end + 2) & 3 };
        // The straight route between the node and a side of its flow, as Pele's toward gives
        // it.
        let toward = |side: usize| -> Vec<f64> {
            let upright = side & 1 == 0;
            let sign = if side == 0 || side == 3 { -1.0 } else { 1.0 };
            let (nx, ny, bx, by, axis);
            if upright {
                (nx, ny) = (n.x, n.y + (sign * n.h) / 2.0);
                (bx, by) = (n.x, g.y + (sign * g.h) / 2.0);
                axis = 0.0;
            } else {
                (nx, ny) = (n.x + (sign * n.w) / 2.0, n.y);
                (bx, by) = (g.x + (sign * g.w) / 2.0, n.y);
                axis = 1.0;
            }
            if out {
                vec![nx, ny, axis, bx, by, axis]
            } else {
                vec![bx, by, axis, nx, ny, axis]
            }
        };
        // Whether another node that is not a flow lies across a route, as Pele's blocked
        // says.
        let blocked = |r: &[f64]| {
            cnodes.iter().enumerate().any(|(k, c)| {
                k != i
                    && !c.is_group
                    && c.x + c.w / 2.0 > min(r[0], r[3]) - 1.0
                    && c.x - c.w / 2.0 < max(r[0], r[3]) + 1.0
                    && c.y + c.h / 2.0 > min(r[1], r[4]) - 1.0
                    && c.y - c.h / 2.0 < max(r[1], r[4]) + 1.0
            })
        };
        let mut route = toward(first);
        if careful && blocked(&route) {
            for side in [(first + 1) & 3, (first + 3) & 3, (first + 2) & 3] {
                let other = toward(side);
                if !blocked(&other) {
                    route = other;
                    break;
                }
            }
        }
        held_labels.extend([(route[0] + route[3]) / 2.0, (route[1] + route[4]) / 2.0]);
        let mut drawn_route = route.clone();
        held_routes.push(route);
        let v = &views[i];
        inset_route(
            &mut drawn_route,
            if out { 0 } else { 3 },
            v.shape,
            v.w,
            v.h,
            &node_of(n),
        );
        let trim = marker_trim(MarkerType::from_name(&h.edge.arrow_type_end));
        held_paths.push(route_path(
            &drawn_route,
            Curve::Linear,
            0.0,
            trim,
            0.0,
            own_reach(trim),
        ));
        held_drawn.push(drawn_route);
    }

    let mut crossings = Crossings::default();
    if cnodes.iter().any(|c| c.is_group) {
        for e in &cedges {
            mark_crossings(&mut crossings, &e.route);
        }
        for route in &held_routes {
            mark_crossings(&mut crossings, route);
        }
    }

    let (mut titles, mut clusters, mut casings) = (Vec::new(), Vec::new(), Vec::new());
    let (mut nodes, mut edges, mut labels) = (Vec::new(), Vec::new(), Vec::new());
    // The flows whose authors set them a fill, which the edges are looked at against.
    let mut fills: Vec<Fill> = Vec::new();
    if !title.is_empty() {
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        let at = Point::new(sc.width / 2.0, pad + title.height / 2.0);
        it.label = Some(Placed::new(title, at));
        titles.push(it);
    }
    for (i, view) in views.iter().enumerate() {
        let c = &cnodes[i];
        let n = view.node;
        let (x, y) = (c.x, c.y);
        let (class, tags) = clickable(class_names(trim_default(&n.css_classes)));
        let author = declarations(&view.decls);
        if c.is_group {
            if view.style.paints() {
                fills.push(Fill {
                    bounds: [x - c.w / 2.0, y - c.h / 2.0, x + c.w / 2.0, y + c.h / 2.0],
                    decls: view.decls.clone(),
                });
            }
            let title_x = cluster_title_x(&node_of(c), view.label.width, &crossings, GROUP_PAD);
            let bounds = Rect::new(x - c.w / 2.0, y - c.h / 2.0, c.w, c.h);
            let mut flow = Item::new(Kind::Group, ROLE_FLOW);
            flow.id = Some(n.id.clone());
            (flow.class, flow.tags) = (class, tags);
            flow.bounds = bounds;
            flow.parts = vec![Part::rect(bounds, true, Painting::FillAndStroke)];
            flow.style = tint(slot(n), 0.06, &view.style);
            // The title lies on the flow's fill, and takes the color that stands out on a
            // fill its author sets, as Pele's inkFill gives it.
            flow.style.text = Some(Paint::MUTED);
            flow.style.on_fill = view.style.inks_text(&view.style);
            flow.author = author.style;
            if !view.label.is_empty() {
                let at = Point::new(title_x, y - c.h / 2.0 + 8.0 + view.label.height / 2.0);
                flow.label = Some(Placed::new(view.label.clone(), at));
            }
            clusters.push(flow);
            continue;
        }

        let (w, h) = (view.w, view.h);
        let bounds = Rect::new(x - w / 2.0, y - h / 2.0, w, h);
        let mut marks = Vec::new();
        let mut it;
        if view.shape == COLLAPSED {
            let s = slot(n);
            it = Item::new(Kind::Node, ROLE_COLLAPSED);
            it.parts = vec![Part::rect(bounds, true, Painting::FillAndStroke)];
            it.style = tint(s, 0.06, &view.style);
            // What the author sets of lines styles the rule, and fills nothing.
            let mut line = author.style.clone();
            line.fill = None;
            let rule = h / 2.0 - DOTS - 2.0;
            let mid = rule + (DOTS + 2.0) / 2.0;
            let mut p = Path::new();
            p.move_to(x + (-w / 2.0 + 10.0), y + rule)
                .line_to(x + (w / 2.0 - 10.0), y + rule);
            let mut it_rule = Item::new(Kind::Mark, ROLE_COLLAPSED_RULE);
            it_rule.id = Some(n.id.clone());
            it_rule.bounds = Rect::new(x - w / 2.0 + 10.0, y + rule, w - 20.0, 0.0);
            it_rule.parts = vec![Part::path(p, Painting::StrokeOnly)];
            it_rule.style = Style {
                stroke: Some(Paint::series(s)),
                fill: Some(Paint::NONE),
                dash: Some(vec![3.0, 3.0]),
                ..Style::default()
            };
            it_rule.author = line;
            let mut p = Path::new();
            for dx in [-9.0, 0.0, 9.0] {
                dot(&mut p, x, y, dx, mid);
            }
            let mut it_dots = Item::new(Kind::Mark, ROLE_COLLAPSED_DOTS);
            it_dots.id = Some(n.id.clone());
            it_dots.bounds = Rect::new(x - 9.0 - 1.75, y + mid - 1.75, 18.0 + 3.5, 3.5);
            it_dots.parts = vec![Part::path(p, Painting::FillOnly)];
            it_dots.style = Style {
                fill: Some(Paint::series(s)),
                stroke: Some(Paint::NONE),
                ..Style::default()
            };
            marks = vec![it_rule, it_dots];
        } else {
            let kind = n.kind.unwrap_or("task");
            let s = kind_slot(kind).unwrap_or(1);
            it = Item::new(Kind::Node, format!("agentflow.{kind}"));
            it.parts = draw_shape(view.shape, x, y, w, h);
            it.style = tint(s, 0.14, &view.style);
        }
        it.shape = Some(Shape::new(view.shape));
        it.id = Some(n.id.clone());
        (it.class, it.tags) = (class, tags);
        it.bounds = bounds;
        it.author = author.style;
        // The label lies on the node's fill, unless its shape is drawn bare.
        it.style.on_fill = (view.shape == COLLAPSED || shape_filled(view.shape))
            && view.style.inks_text(&view.style);
        if !view.label.is_empty() {
            it.label = Some(Placed::new(view.label.clone(), Point::new(x, y + view.dy)));
        }
        it.tooltip = n.tooltip.clone().filter(|t| !t.is_empty());
        if it.class.iter().any(|c| c == "internal-link")
            && let Some(label) = n.label.as_deref().filter(|l| !l.is_empty())
            && o.links
        {
            // The label is a note's name, but it ends up a link, so it is checked as an
            // address.
            let name = decode_entities(label);
            if internal_link(&name, &schemes) {
                it.link = Some(name);
            }
        }
        if let Some(link) = n.link.as_deref().filter(|l| !l.is_empty())
            && let Some(href) = link_url(link, &schemes)
        {
            it.link = Some(href);
        }
        nodes.push(it);
        nodes.extend(marks);
    }

    // Adds an edge and its label, as Pele's put draws them. route, which Pele does not have,
    // is the route the edge was laid out along.
    let mut put =
        |edge: &GraphEdge, label: &Label, path: EdgePath, route: &[f64], x: f64, y: f64| {
            let mut decls = edge.css_compiled_styles.clone();
            decls.extend(edge.style.iter().cloned());
            let (line, mut author) = if decls.is_empty() {
                (String::new(), Style::default())
            } else {
                (resolve_style(&decls).line, declarations(&decls).style)
            };
            // What styles an edge's line styles its marker, and fills nothing.
            author.fill = None;
            // Pele draws the edges in a group with round caps. What the author sets on the
            // edge's own group is laid over them.
            let mut own = Style {
                stroke: Some(Paint::LINE),
                fill: Some(Paint::NONE),
                cap: Some(Cap::Round),
                ..Style::default()
            };
            let dotted = edge.thickness.as_deref() == Some("dotted");
            let kind = if dotted {
                own.dash = Some(vec![3.0, 4.0]);
                "reference"
            } else if edge.arrow_type_end == "arrow_cross" {
                "failure"
            } else {
                "sequence"
            };
            let ends = path.ends(None, MarkerType::from_name(&edge.arrow_type_end));
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
            let noise = [
                "edge-thickness-normal",
                "edge-pattern-solid",
                "flowchart-link",
            ];
            let classes = noise
                .iter()
                .fold(edge.classes.clone(), |s, n| s.replace(n, ""));
            let mut it = Item::new(
                Kind::Edge(Edge {
                    route: points,
                    ends,
                }),
                format!("agentflow.{kind}"),
            );
            it.id = Some(edge.id.clone());
            it.class = class_names(&classes);
            it.bounds = path.path.bounds();
            it.style = own;
            it.author = author.clone();
            // An edge inside one fill its author sets takes the color that stands out on it, and
            // one that crosses the border of one is laid on a casing, as Pele's overFills and
            // edgeCasing draw them.
            let tips = [path.end.at.x, path.end.at.y];
            let (ground, cased) = on_fills(&path.path, &tips, &fills, &line);
            it.style.ground = ground;
            if cased && let Some(c) = casing(ROLE_CASING, &path.path, &ends, &decls, false) {
                casings.push(c);
            }
            it.parts = vec![Part::path(path.path, Painting::StrokeOnly)];
            edges.push(it);
            if label.width > 0.0 {
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
                it.author = Style {
                    text: author.text,
                    opacity: author.opacity,
                    ..Style::default()
                };
                labels.push(it);
            }
        };

    let mut loop_seen = vec![0; views.len()];
    for i in 0..cedges.len() {
        let edge = drawn[i];
        let trim = marker_trim(MarkerType::from_name(&edge.arrow_type_end));
        let ce = &cedges[i];
        let mut route = Vec::new();
        let path = if ce.src == ce.dst {
            let view = &views[ce.src];
            let k = loop_seen[ce.src];
            loop_seen[ce.src] += 1;
            let c = &cnodes[ce.src];
            let (path, at) = loop_path(
                Point::new(c.x, c.y),
                view.w,
                view.h,
                k,
                flows_sideways(&cnodes, root_dir, ce.src),
                LOOP,
                trim,
                ce.label_w,
                ce.label_h,
            );
            (cedges[i].label_x, cedges[i].label_y) = (at.x, at.y);
            path
        } else {
            route = ce.route.clone();
            let last = route.len() - 3;
            let (sv, dv) = (&views[ce.src], &views[ce.dst]);
            inset_route(
                &mut route,
                0,
                sv.shape,
                sv.w,
                sv.h,
                &node_of(&cnodes[ce.src]),
            );
            inset_route(
                &mut route,
                last,
                dv.shape,
                dv.w,
                dv.h,
                &node_of(&cnodes[ce.dst]),
            );
            let edge_curve = match curve {
                Some(c) if o.routing != Routing::Auto => c,
                _ => edge.curve.as_ref().and_then(Value::as_str).unwrap_or(""),
            };
            route_path(
                &route,
                Curve::of(edge_curve),
                0.0,
                trim,
                0.0,
                own_reach(trim),
            )
        };
        let ce = &cedges[i];
        put(edge, &edge_labels[i], path, &route, ce.label_x, ce.label_y);
    }
    for (i, (h, path)) in held.iter().zip(held_paths).enumerate() {
        let (x, y) = (held_labels[2 * i], held_labels[2 * i + 1]);
        put(h.edge, &h.label, path, &held_drawn[i], x, y);
    }

    for layer in [titles, clusters, casings, edges, labels, nodes] {
        sc.items.extend(layer);
    }
    sc
}
