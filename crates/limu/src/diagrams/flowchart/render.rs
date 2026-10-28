//! A flowchart laid out as a scene, as Pele's renderFlowchart in render.ts draws it.

use super::db::{FlowDb, LabelType};
use super::graph::{FlowGraph, GraphNode, build_flow_graph};
use super::shapes::canonical_shape;
use crate::common::{
    class_names, clickable, routing_curve, runs_across, setting, tighten, title_label, title_room,
    turn_to_fit,
};
use crate::css::{Author, Resolved, declarations, resolve_style};
use crate::draw::{
    Crossings, Curve, Fill, Node, STRUCK_HALO, SideReach, casing, cluster_title_x, draw_shape,
    end_side, inset_route, label_patch, label_patch_box, loop_path, mark_crossings, marker_trim,
    on_fills, route_path, shape_filled, shape_has_label, shape_size, shape_span, struck_depth,
    struck_tint, title_crossed,
};
use crate::graphs::compound::{
    CEdge, CNode, CompoundResult, Dir, cnode, compound_layout, direction,
    inherit_crossed_directions, shift_layout,
};
use crate::graphs::layered::LayeredOptions;
use crate::js::math::{max, min, round};
use crate::layout::Options;
use crate::scene::style::{Cap, Paint, Style, Token};
use crate::scene::text::{Face, Label};
use crate::scene::{
    Edge, Item, Kind, MarkerType, Painting, Part, Path, Picture, PictureSource, Placed, Point,
    Rect, Scene, Shape,
};
use crate::text::{decode_entities, layout_label};
use crate::url::{image_url, internal_link, link_url, safe_url};
use crate::value::{Map, Value};

/// The roles of the items of a flowchart.
pub(crate) const ROLE_CASING: &str = "flowchart.casing";
pub(crate) const ROLE_CLUSTER: &str = "flowchart.cluster";
pub(crate) const ROLE_CLUSTER_TITLE: &str = "flowchart.cluster-title";
pub(crate) const ROLE_EDGE: &str = "flowchart.edge";
pub(crate) const ROLE_EDGE_LABEL: &str = "flowchart.edge-label";
pub(crate) const ROLE_NODE: &str = "flowchart.node";
pub(crate) const ROLE_TITLE: &str = "flowchart.title";

// The spacing of a flowchart, as Pele's render.ts has it.
const NODE_SEP: f64 = 40.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 48.0;
const GROUP_PAD: f64 = 20.0;
const LOOP: f64 = 26.0;

/// A number of a flowchart's configuration, where it is one above 0, at most 2000, as Pele's
/// numberOption reads it.
fn number_option(section: Option<&Map>, key: &str, fallback: f64) -> f64 {
    match section.and_then(|s| s.get(key)) {
        Some(&Value::Number(v)) if v > 0.0 => min(v, 2000.0),
        _ => fallback,
    }
}

/// The size an image or an icon is given, where it is a sensible one, as Pele's assetSize
/// gives it.
fn asset_size(value: Option<f64>, fallback: f64) -> f64 {
    match value {
        Some(v) if v > 0.0 && v <= 4000.0 => v,
        _ => fallback,
    }
}

/// A type of diagram that is a flowchart laid out its own way, which may draw its top-level
/// groups itself, as Pele's FlowVariant is.
pub(crate) struct Variant {
    /// The diagram's type: the scene's, and the section of the configuration read in place
    /// of the flowchart's where it is an object.
    pub(crate) diagram: &'static str,
    /// Changes the graph before it is laid out.
    pub(crate) prepare: fn(&mut FlowGraph),
    /// Lays the graph out in place of compound_layout. tight is set where the diagram is laid
    /// out tightly to fit the host's width.
    pub(crate) layout: fn(&mut [CNode], &mut [CEdge], Dir, &LayeredOptions, bool) -> CompoundResult,
    /// Draws a top-level group in place of a cluster, where it is set.
    pub(crate) draw_group: Option<DrawGroup>,
    /// The part of a group draw_group draws that takes the fill its author sets, as left,
    /// top, right and bottom, as Pele's filledBox gives it. Without it, the fill of such a
    /// group is not looked at.
    pub(crate) filled_box: Option<fn(&CNode, Dir) -> [f64; 4]>,
}

/// What draws a top-level group of a variant, as Pele's drawGroup does: the group as it was
/// laid out, its title, its style and what its author sets, its classes, its id, and the
/// direction the diagram was laid out in.
pub(crate) type DrawGroup = fn(&CNode, &Label, &Resolved, &Author, Vec<String>, &str, Dir) -> Item;

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

/// Lays out a parsed flowchart as a scene, as Pele's renderFlowchart draws it: across and
/// loosely, and, where that does not fit the host's width, tightly, or down instead of
/// across, whichever is narrower.
pub(crate) fn layout(db: &FlowDb, config: &Map, o: &Options) -> Scene {
    layout_variant(db, config, o, None)
}

/// Lays out a parsed flowchart as a scene, as Pele's renderFlowchart draws it with a variant,
/// or as layout does where there is none.
pub(crate) fn layout_variant(
    db: &FlowDb,
    config: &Map,
    o: &Options,
    variant: Option<&Variant>,
) -> Scene {
    let fitted = |down: bool| tighten(o, |tight| draw(db, config, o, variant, down, tight));
    if runs_across(db.direction.as_deref()) {
        turn_to_fit(o, fitted)
    } else {
        fitted(false)
    }
}

/// Whether the nodes about node i run across, by the direction of the nearest group that
/// sets one, as Pele's flowsSideways says.
fn flows_sideways(cnodes: &[CNode], root_dir: Dir, i: usize) -> bool {
    let mut dir = None;
    let (mut p, mut hops) = (cnodes[i].parent, 0);
    while p >= 0 && dir.is_none() && hops < 10000 {
        dir = cnodes[p as usize].dir;
        p = cnodes[p as usize].parent;
        hops += 1;
    }
    matches!(dir.unwrap_or(root_dir), Dir::LR | Dir::RL)
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

/// Lays the flowchart out once, as Pele's draw does, with the variant where there is one.
/// turned lays out a flowchart that runs across as one that runs down, and tight sets nodes
/// closer together.
fn draw(
    db: &FlowDb,
    config: &Map,
    o: &Options,
    variant: Option<&Variant>,
    turned: bool,
    tight: bool,
) -> Scene {
    let pad = o.padding;
    let group_pad = if tight { 12.0 } else { GROUP_PAD };
    let flow = variant
        .and_then(|v| config.get(v.diagram))
        .and_then(Value::as_object)
        .or_else(|| config.get("flowchart").and_then(Value::as_object));
    // The host's routing takes the place of the curve the diagram asks for.
    let curve = routing_curve(o.routing)
        .or_else(|| flow.and_then(|f| f.get("curve")).and_then(Value::as_str));
    let mut graph = build_flow_graph(db, curve);
    if let Some(v) = variant {
        (v.prepare)(&mut graph);
    }
    let size = o.font_size;
    let edge_size = round(size * 0.875);
    let set = setting(o);
    let mut wrap_width = number_option(flow, "wrappingWidth", 200.0);
    let mut group_max = 4000.0;
    if variant.is_some() && tight {
        wrap_width = min(wrap_width, 100.0);
        group_max = max(80.0, title_room(o) / 2.0);
    }
    let schemes = o.schemes();

    let mut index = std::collections::HashMap::new();
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
        let (view, mut c) = if node.is_group {
            let label = layout_label(raw, markdown, &set, font_size, group_max, face);
            let mut c = cnode(0.0, 0.0, -1);
            c.is_group = true;
            c.dir = node.dir.as_deref().map(|d| direction(Some(d)));
            c.pad_x = group_pad;
            c.pad_top = if label.height > 0.0 {
                label.height + 16.0
            } else {
                GROUP_PAD
            };
            c.pad_bottom = group_pad;
            c.min_w = label.width + 2.0 * group_pad - 12.0;
            let view = View {
                node,
                decls,
                style,
                label,
                shape: "group",
                w: 0.0,
                h: 0.0,
                dy: 0.0,
            };
            (view, c)
        } else {
            let shape = canonical_shape(Some(&node.shape));
            let img = node.img.as_deref().is_some_and(|s| !s.is_empty());
            let icon = node.icon.as_deref().is_some_and(|s| !s.is_empty());
            let text = if img || icon || shape_has_label(shape) {
                raw
            } else {
                ""
            };
            let label = layout_label(text, markdown, &set, font_size, wrap_width, face);
            let (w, h, mut dy);
            if img || icon {
                let bx = if img { 80.0 } else { 48.0 };
                let aw = asset_size(node.asset_width, bx);
                let ah = asset_size(node.asset_height, bx);
                w = max(aw, label.width + 8.0);
                h = ah
                    + if label.height > 0.0 {
                        label.height + 6.0
                    } else {
                        0.0
                    };
                dy = if node.pos.as_deref() == Some("t") {
                    -ah / 2.0
                } else {
                    ah / 2.0
                };
                if label.height == 0.0 {
                    dy = 0.0;
                }
            } else {
                let s = shape_size(shape, label.width, label.height);
                (w, h, dy) = (s.w, s.h, s.dy);
            }
            let view = View {
                node,
                decls,
                style,
                label,
                shape,
                w,
                h,
                dy,
            };
            (view, cnode(w, h, -1))
        };
        c.seq = views.len() as f64;
        views.push(view);
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
    // The edges on one side of a node spread along it. A picture or an icon keeps them at its
    // middle.
    for (i, view) in views.iter().enumerate() {
        let n = view.node;
        if n.is_group {
            cnodes[i].span = f64::INFINITY;
        } else if n.img.as_deref().is_none_or(str::is_empty)
            && n.icon.as_deref().is_none_or(str::is_empty)
        {
            cnodes[i].span = shape_span(
                view.shape,
                view.w,
                view.h,
                flows_sideways(&cnodes, root_dir, i),
            );
        }
    }

    let mut cedges: Vec<CEdge> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    let mut drawn = Vec::new();
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
        node_sep: number_option(flow, "nodeSpacing", NODE_SEP) * if tight { 0.5 } else { 1.0 },
        edge_sep: EDGE_SEP,
        rank_sep: number_option(flow, "rankSpacing", RANK_SEP) * if tight { 0.75 } else { 1.0 },
        port_sep: 20.0,
        tracks: curve.is_some_and(|c| c.starts_with("step")),
        straight: curve == Some("linear"),
        head_room: Some(marker_trim(Some(MarkerType::ArrowPoint))),
        ..LayeredOptions::default()
    };
    let result = match variant {
        Some(v) => (v.layout)(&mut cnodes, &mut cedges, root_dir, &opt, tight),
        None => compound_layout(&mut cnodes, &mut cedges, root_dir, &opt),
    };

    let title_max = if tight { title_room(o) } else { 4000.0 };
    let title = title_label(db.title.as_deref(), &set, size, title_max);
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
        diagram: variant.map_or("flowchart", |v| v.diagram).into(),
        width: (inner + 2.0 * pad).ceil(),
        height: (result.height + title_height + 2.0 * pad).ceil(),
        font_size: size,
        title: db.acc_title.clone(),
        desc: db.acc_descr.clone(),
        items: Vec::new(),
    };

    let mut crossings = Crossings::default();
    if cnodes.iter().any(|c| c.is_group) {
        for e in &cedges {
            mark_crossings(&mut crossings, &e.route);
        }
    }

    let (mut titles, mut clusters, mut casings, mut struck) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut nodes, mut edges, mut labels) = (Vec::new(), Vec::new(), Vec::new());
    // The groups whose authors set them a fill, which the edges are looked at against.
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
        let (class, tags) = clickable(class_names(crate::common::trim_default(&n.css_classes)));
        let author = declarations(&view.decls);
        if let Some(v) = variant
            && c.is_group
            && c.parent < 0
            && let Some(draw_group) = v.draw_group
        {
            if view.style.paints()
                && let Some(filled_box) = v.filled_box
            {
                fills.push(Fill {
                    bounds: filled_box(c, root_dir),
                    decls: view.decls.clone(),
                });
            }
            let mut group =
                draw_group(c, &view.label, &view.style, &author, class, &n.id, root_dir);
            group.tags = tags;
            clusters.push(group);
            continue;
        }
        if c.is_group {
            if view.style.paints() {
                fills.push(Fill {
                    bounds: [x - c.w / 2.0, y - c.h / 2.0, x + c.w / 2.0, y + c.h / 2.0],
                    decls: view.decls.clone(),
                });
            }
            let title_x = cluster_title_x(&node_of(c), view.label.width, &crossings, group_pad);
            let title_y = y - c.h / 2.0 + 8.0 + view.label.height / 2.0;
            let bounds = Rect::new(x - c.w / 2.0, y - c.h / 2.0, c.w, c.h);
            let mut cluster = Item::new(Kind::Group, ROLE_CLUSTER);
            cluster.id = Some(n.id.clone());
            (cluster.class, cluster.tags) = (class, tags);
            cluster.bounds = bounds;
            cluster.parts = vec![Part::rect(bounds, true, Painting::FillAndStroke)];
            cluster.style = Style {
                fill: Some(Paint::SURFACE_ALT),
                // A fill the author sets is drawn at full opacity, as Pele's setsFill leaves
                // out the group's own fill-opacity.
                fill_opacity: if view.style.fill_declared() {
                    None
                } else {
                    Some(0.5)
                },
                stroke: Some(Paint::BORDER),
                text: Some(Paint::MUTED),
                ..Style::default()
            };
            cluster.author = author.style.clone();
            if !view.label.is_empty() {
                let at = Point::new(title_x, title_y);
                if title_crossed(&node_of(c), view.label.width, title_x, &crossings) {
                    let parent = |k: usize| usize::try_from(cnodes[k].parent).ok();
                    let depth = struck_depth(parent, i);
                    let mut it = Item::new(Kind::Text, ROLE_CLUSTER_TITLE);
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
                    it.author = Style {
                        text: author.style.text,
                        ..Style::default()
                    };
                    struck.push(it);
                } else {
                    // The title lies on the group's fill, and takes a color that stands out
                    // on a fill its author sets. One struck over the edges sits on a halo
                    // instead.
                    cluster.label = Some(Placed::new(view.label.clone(), at));
                    cluster.style.on_fill = view.style.inks_text(&view.style);
                }
            }
            clusters.push(cluster);
            continue;
        }

        let (w, h) = (view.w, view.h);
        let mut it = Item::new(Kind::Node, ROLE_NODE);
        it.shape = Some(Shape::new(view.shape));
        it.id = Some(n.id.clone());
        (it.class, it.tags) = (class, tags);
        it.bounds = Rect::new(x - w / 2.0, y - h / 2.0, w, h);
        it.style = Style {
            fill: Some(Paint::SURFACE),
            stroke: Some(Paint::BORDER),
            ..Style::default()
        };
        it.author = author.style;
        if !view.label.is_empty() {
            it.label = Some(Placed::new(view.label.clone(), Point::new(x, y + view.dy)));
        }
        it.tooltip = n.tooltip.clone().filter(|t| !t.is_empty());
        let top_of = |bx: f64| {
            if view.label.height > 0.0 {
                if n.pos.as_deref() == Some("t") {
                    h / 2.0 - bx
                } else {
                    -h / 2.0
                }
            } else {
                -bx / 2.0
            }
        };
        if let Some(img) = n.img.as_deref().filter(|s| !s.is_empty()) {
            let aw = asset_size(n.asset_width, 80.0);
            let ah = asset_size(n.asset_height, 80.0);
            let top = top_of(ah);
            if let Some(src) = image_url(&safe_url(img), &schemes) {
                it.picture = Some(Picture {
                    source: PictureSource::Image {
                        src,
                        fit: n.constraint.as_deref() == Some("on"),
                    },
                    bounds: Rect::new(x - aw / 2.0, y + top, aw, ah),
                });
            }
        } else if let Some(icon) = n.icon.as_deref().filter(|s| !s.is_empty()) {
            let bx = asset_size(n.asset_height, 48.0);
            let top = top_of(bx);
            let frame = Rect::new(x - bx / 2.0, y + top, bx, bx);
            match n.shape.as_str() {
                "icon" => {}
                "iconCircle" => {
                    let mut p = Path::new();
                    p.ellipse(x, y + top + bx / 2.0, bx / 2.0, bx / 2.0);
                    it.parts = vec![Part::path(p, Painting::FillAndStroke)];
                }
                "iconRounded" => {
                    let mut p = Path::new();
                    p.round_rect(frame, 8.0);
                    it.parts = vec![Part::path(p, Painting::FillAndStroke)];
                }
                _ => it.parts = vec![Part::rect(frame, false, Painting::FillAndStroke)],
            }
            let glyph = bx * 0.6;
            it.picture = Some(Picture {
                source: PictureSource::Icon(icon.to_string()),
                bounds: Rect::new(x - glyph / 2.0, y + top + (bx - glyph) / 2.0, glyph, glyph),
            });
        } else {
            it.parts = draw_shape(view.shape, x, y, w, h);
            // The label lies on the shape's fill, unless the shape is drawn bare.
            it.style.on_fill = shape_filled(view.shape) && view.style.inks_text(&view.style);
        }
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
    }

    let marker = |name: &str| MarkerType::from_name(name);
    let mut reach = SideReach::default();
    for (i, ce) in cedges.iter().enumerate() {
        if ce.src == ce.dst
            || drawn[i].thickness.as_deref() == Some("invisible")
            || ce.route.len() < 6
        {
            continue;
        }
        let last = ce.route.len() - 3;
        reach.add(
            ce.src,
            side(end_side(&ce.route, 0, &node_of(&cnodes[ce.src]))),
            marker(&drawn[i].arrow_type_start),
        );
        reach.add(
            ce.dst,
            side(end_side(&ce.route, last, &node_of(&cnodes[ce.dst]))),
            marker(&drawn[i].arrow_type_end),
        );
    }

    let mut loop_seen = vec![0; views.len()];
    for i in 0..cedges.len() {
        let edge = drawn[i];
        let label = &edge_labels[i];
        let mut decls = edge.css_compiled_styles.clone();
        decls.extend(edge.style.iter().cloned());
        let (line, mut author) = if decls.is_empty() {
            (String::new(), Style::default())
        } else {
            (resolve_style(&decls).line, declarations(&decls).style)
        };
        // What styles an edge's line styles its markers, and fills nothing.
        author.fill = None;
        let thickness = edge.thickness.as_deref();
        if thickness != Some("invisible") {
            let (start, end) = (marker(&edge.arrow_type_start), marker(&edge.arrow_type_end));
            let mut route = Vec::new();
            let ce = &cedges[i];
            let path = if ce.src == ce.dst {
                let view = &views[ce.src];
                let k = loop_seen[ce.src];
                loop_seen[ce.src] += 1;
                let c = &cnodes[ce.src];
                let sideways = flows_sideways(&cnodes, root_dir, ce.src);
                let (path, at) = loop_path(
                    Point::new(c.x, c.y),
                    view.w,
                    view.h,
                    k,
                    sideways,
                    LOOP,
                    marker_trim(end),
                    ce.label_w,
                    ce.label_h,
                );
                (cedges[i].label_x, cedges[i].label_y) = (at.x, at.y);
                path
            } else {
                route = ce.route.clone();
                let last = route.len() - 3;
                let (sn, dn) = (node_of(&cnodes[ce.src]), node_of(&cnodes[ce.dst]));
                let leaves = reach.of(ce.src, side(end_side(&route, 0, &sn)));
                let arrives = reach.of(ce.dst, side(end_side(&route, last, &dn)));
                let (sv, dv) = (&views[ce.src], &views[ce.dst]);
                inset_route(&mut route, 0, sv.shape, sv.w, sv.h, &sn);
                inset_route(&mut route, last, dv.shape, dv.w, dv.h, &dn);
                let edge_curve = match curve {
                    Some(c) if o.routing != crate::layout::Routing::Auto => c,
                    _ => edge.curve.as_ref().and_then(Value::as_str).unwrap_or(""),
                };
                route_path(
                    &route,
                    Curve::of(edge_curve),
                    marker_trim(start),
                    marker_trim(end),
                    leaves,
                    arrives,
                )
            };
            // Pele draws the edges in a group with round caps. What the author sets on the
            // edge's own group is laid over them.
            let mut own = Style {
                stroke: Some(Paint::LINE),
                fill: Some(Paint::NONE),
                cap: Some(Cap::Round),
                ..Style::default()
            };
            match thickness {
                Some("thick") => own.stroke_width = Some(2.5),
                Some("dotted") => own.dash = Some(vec![3.0, 4.0]),
                _ => {}
            }
            let ends = path.ends(start, end);
            let mut points: Vec<Point> = route
                .as_chunks::<3>()
                .0
                .iter()
                .map(|&[x, y, _]| Point::new(x, y))
                .collect();
            if points.is_empty() {
                // A self-loop, which the layout gives no route: the corners its curve is drawn
                // about.
                points = path.path.corners();
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
                ROLE_EDGE,
            );
            it.id = Some(edge.id.clone());
            it.class = class_names(&classes);
            it.bounds = path.path.bounds();
            it.style = own;
            it.author = author.clone();
            // An edge inside one fill its author sets is drawn in that fill's label color, and
            // one that crosses the border of a fill is cased, as Pele's overFills decides.
            let tips = [
                path.start.at.x,
                path.start.at.y,
                path.end.at.x,
                path.end.at.y,
            ];
            let (ground, cased) = on_fills(&path.path, &tips, &fills, &line);
            it.style.ground = ground;
            if cased
                && let Some(c) = casing(
                    ROLE_CASING,
                    &path.path,
                    &ends,
                    &decls,
                    thickness == Some("thick"),
                )
            {
                casings.push(c);
            }
            it.parts = vec![Part::path(path.path, Painting::StrokeOnly)];
            edges.push(it);
        }
        if label.width > 0.0 {
            let ce = &cedges[i];
            let (x, y) = (ce.label_x, ce.label_y);
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
    }

    for layer in [titles, clusters, casings, edges, struck, labels, nodes] {
        sc.items.extend(layer);
    }
    sc
}
