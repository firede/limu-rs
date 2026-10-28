//! A requirement diagram laid out as a scene, as Pele's renderRequirement in render.ts draws
//! it.

use std::collections::HashMap;

use super::db::RequirementDb;
use super::graph::{GraphEdge, GraphNode, build_requirement_graph};
use crate::common::{
    class_names, routing_curve, runs_across, setting, title_label, trim_default, turn_to_fit,
};
use crate::css::{Author, declarations, resolve_style};
use crate::draw::{Curve, EdgePath, Tip, label_patch, label_patch_box, own_reach, route_path};
use crate::graphs::compound::{CEdge, CNode, Dir, cnode, compound_layout, direction, shift_layout};
use crate::graphs::layered::LayeredOptions;
use crate::js::math::{by, max, min, round};
use crate::layout::Options;
use crate::scene::style::{Cap, Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{
    Align, Edge, Item, Kind, MarkerType, Painting, Part, Path, Placed, Point, Rect, Scene, Shape,
};
use crate::text::{Setting, layout_label};
use crate::value::Map;

/// The roles of the items of a requirement diagram.
pub(crate) const ROLE_ELEMENT: &str = "requirement.element";
pub(crate) const ROLE_FIELD: &str = "requirement.field";
pub(crate) const ROLE_KIND: &str = "requirement.kind";
pub(crate) const ROLE_RELATIONSHIP: &str = "requirement.relationship";
pub(crate) const ROLE_RELATIONSHIP_LABEL: &str = "requirement.relationship-label";
pub(crate) const ROLE_REQUIREMENT: &str = "requirement.requirement";
pub(crate) const ROLE_TITLE: &str = "requirement.title";
pub(crate) const ROLE_VALUE: &str = "requirement.value";

// The measures of a requirement diagram, as Pele's render.ts has them.
const NODE_SEP: f64 = 40.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 56.0;
/// The room inside a box, either side of its text.
const PAD_X: f64 = 14.0;
/// The room above and below a box's head and its fields.
const PAD_Y: f64 = 10.0;
const ROW_GAP: f64 = 2.0;
/// The room between a field's name and its value.
const COLUMN_GAP: f64 = 12.0;
const MIN_WIDTH: f64 = 120.0;
const NAME_WRAP: f64 = 280.0;
const VALUE_WRAP: f64 = 240.0;
const LOOP: f64 = 26.0;
/// The most the ends of edges on one side of a box are set apart by.
const END_STEP: f64 = 28.0;
/// The room the ends of edges leave at the corners of a box.
const END_MARGIN: f64 = 14.0;

/// A field of a box: its name, its value and the height of the taller, Pele's Row.
struct Row {
    key: Label,
    value: Label,
    height: f64,
}

/// A requirement or an element as it is drawn, Pele's NodeView.
struct View<'g> {
    node: &'g GraphNode<'g>,
    author: Author,
    kind: Label,
    name: Label,
    rows: Vec<Row>,
    key_width: f64,
    head_height: f64,
    w: f64,
    h: f64,
    /// Whether Pele's inkFill picks the color of the box's texts on the fill its author gives
    /// it.
    inks: bool,
}

/// Adds bold or italic to a whole label after its markdown is read, since emphasis markers
/// only toggle styles the label does not start with, as Pele's restyle does.
fn restyle(mut label: Label, bits: Face, set: &Setting) -> Label {
    if bits == Face::REGULAR || label.lines.is_empty() {
        return label;
    }
    let mut width = 0.0;
    let mut widths = Vec::with_capacity(label.lines.len());
    for line in &mut label.lines {
        let mut total = 0.0;
        for span in line.iter_mut() {
            span.face |= bits;
            if span.icon.is_none() {
                span.width = set.width(&span.text, label.size, span.face);
            }
            total += span.width;
        }
        widths.push(total);
        if total > width {
            width = total;
        }
    }
    label.widths = widths;
    label.width = width;
    label
}

/// An end of an edge on a side of a box, as Pele's spreadEnds gathers them: the edge, the
/// point of its route at the end, and where its route heads from there.
struct End {
    edge: usize,
    at: usize,
    toward: f64,
}

/// Sets apart the ends of edges that share a side of a box, as Pele's spreadEnds does. The
/// layout attaches every edge to the middle of a side. Ends that share one are spread along
/// it, in the order of where they head, so that their markers do not sit on each other.
fn spread_ends(cnodes: &[CNode], cedges: &mut [CEdge], views: &[View]) {
    // The ends by node and side, the sides in the order they were found, as a Map keeps them.
    let mut sides: Vec<(usize, Vec<End>)> = Vec::new();
    let mut at_key: HashMap<usize, usize> = HashMap::new();
    for (e, edge) in cedges.iter().enumerate() {
        let route = &edge.route;
        if edge.src == edge.dst || route.len() < 6 {
            continue;
        }
        for at in [0, route.len() - 3] {
            let (node, next) = if at == 0 {
                (edge.src, 3)
            } else {
                (edge.dst, at - 3)
            };
            let c = &cnodes[node];
            let dx = route[at] - c.x;
            let dy = route[at + 1] - c.y;
            let side = if (dy.abs() - c.h / 2.0).abs() < 0.5 {
                if dy < 0.0 { 0 } else { 2 }
            } else if (dx.abs() - c.w / 2.0).abs() < 0.5 {
                if dx < 0.0 { 3 } else { 1 }
            } else {
                continue;
            };
            let key = node * 4 + side;
            let end = End {
                edge: e,
                at,
                toward: route[if side % 2 == 0 { next } else { next + 1 }],
            };
            match at_key.get(&key) {
                Some(&k) => sides[k].1.push(end),
                None => {
                    at_key.insert(key, sides.len());
                    sides.push((key, vec![end]));
                }
            }
        }
    }
    for (key, mut list) in sides {
        if list.len() < 2 {
            continue;
        }
        let node = key >> 2;
        let vertical = key % 2 == 1;
        let span = (if vertical {
            views[node].h
        } else {
            views[node].w
        }) - 2.0 * END_MARGIN;
        let step = max(0.0, min(END_STEP, span / (list.len() - 1) as f64));
        list.sort_by(|a, b| by(a.toward - b.toward));
        for (j, end) in list.iter().enumerate() {
            let offset = (j as f64 - (list.len() - 1) as f64 / 2.0) * step;
            let route = &mut cedges[end.edge].route;
            if vertical {
                route[end.at + 1] = cnodes[node].y + offset;
            } else {
                route[end.at] = cnodes[node].x + offset;
            }
        }
    }
}

/// Lays out a parsed requirement diagram as a scene, as Pele's renderRequirement draws it. A
/// diagram that runs across and does not fit the host's width is laid out running down
/// instead, where that is narrower.
pub(crate) fn layout(db: &RequirementDb, _config: &Map, o: &Options) -> Scene {
    if !runs_across(Some(&db.direction)) {
        return draw(db, o, false);
    }
    turn_to_fit(o, |down| draw(db, o, down))
}

/// Lays the diagram out once, as Pele's draw does. turned lays out a diagram that runs across
/// as one that runs down.
fn draw(db: &RequirementDb, o: &Options, turned: bool) -> Scene {
    let graph = build_requirement_graph(db);
    let size = o.font_size;
    let small = round(size * 0.875);
    let set = setting(o);
    let pad = o.pad();
    let label = |raw: &str, markdown: bool, size: f64, max_width: f64| {
        layout_label(raw, markdown, &set, size, max_width, Face::REGULAR)
    };

    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut views: Vec<View> = Vec::with_capacity(graph.nodes.len());
    let mut cnodes: Vec<CNode> = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let style = resolve_style(node.css_styles);
        let mut base = Face::REGULAR;
        if style.bold {
            base |= Face::BOLD;
        }
        if style.italic {
            base |= Face::ITALIC;
        }
        let name_size = style.font_size.unwrap_or(size);
        let body_size = style.font_size.unwrap_or(small);
        let (kind_name, fields): (&str, Vec<(&str, &str)>) = match (node.requirement, node.element)
        {
            (Some(r), _) => (
                &r.kind,
                vec![
                    ("ID", &r.requirement_id),
                    ("Text", &r.text),
                    ("Risk", &r.risk),
                    ("Verification", &r.verify_method),
                ],
            ),
            (None, Some(e)) => ("Element", vec![("Type", &e.kind), ("Doc Ref", &e.doc_ref)]),
            (None, None) => ("Element", Vec::new()),
        };
        let kind = restyle(
            label(&format!("«{kind_name}»"), false, body_size, 4000.0),
            base,
            &set,
        );
        let name = restyle(
            label(node.id, true, name_size, NAME_WRAP),
            base | Face::BOLD,
            &set,
        );
        let mut rows = Vec::new();
        let (mut key_width, mut value_width, mut body_height) = (0.0, 0.0, 0.0);
        for (field, text) in fields {
            if text.is_empty() {
                continue;
            }
            let key = restyle(label(field, false, body_size, 4000.0), base, &set);
            let value = restyle(label(text, true, body_size, VALUE_WRAP), base, &set);
            let height = max(key.height, value.height);
            key_width = max(key_width, key.width);
            value_width = max(value_width, value.width);
            body_height += height + ROW_GAP;
            rows.push(Row { key, value, height });
        }
        if !rows.is_empty() {
            body_height += 2.0 * PAD_Y - ROW_GAP;
        }
        let head_height = 2.0 * PAD_Y + kind.height + name.height;
        let w = max(
            MIN_WIDTH,
            max(
                max(kind.width, name.width) + 2.0 * PAD_X,
                if rows.is_empty() {
                    0.0
                } else {
                    key_width + COLUMN_GAP + value_width + 2.0 * PAD_X
                },
            ),
        );
        // A requirement and an element may share a name. Relationships then go to the
        // requirement.
        index.entry(node.id).or_insert(views.len());
        let mut c = cnode(w, head_height + body_height, -1);
        c.seq = views.len() as f64;
        views.push(View {
            node,
            author: declarations(node.css_styles),
            kind,
            name,
            rows,
            key_width,
            head_height,
            w,
            h: head_height + body_height,
            inks: style.inks_text(&style),
        });
        cnodes.push(c);
    }

    let root_dir = if turned {
        Dir::TB
    } else {
        direction(Some(graph.direction))
    };
    let sideways = matches!(root_dir, Dir::LR | Dir::RL);
    let mut cedges: Vec<CEdge> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    let mut drawn: Vec<&GraphEdge> = Vec::new();
    for edge in &graph.edges {
        let (Some(&src), Some(&dst)) = (
            edge.start.and_then(|s| index.get(s)),
            edge.end.and_then(|e| index.get(e)),
        ) else {
            continue;
        };
        let l = label(&format!("«{}»", edge.kind), false, small, 4000.0);
        if src == dst {
            // A self-loop sits beside the node, across the flow, and the node's box grows to
            // hold it.
            if sideways {
                cnodes[src].h += 2.0 * (LOOP + l.height + 8.0);
            } else {
                cnodes[src].w += 2.0 * (LOOP + l.width + 12.0);
            }
        }
        cedges.push(CEdge {
            src,
            dst,
            minlen: 1.0,
            label_w: l.width + 12.0,
            label_h: l.height + 4.0,
            route: Vec::new(),
            label_x: 0.0,
            label_y: 0.0,
        });
        drawn.push(edge);
        edge_labels.push(l);
    }

    // The host may ask for edges at right angles, or straight, as it may of a flowchart.
    let curve = routing_curve(o.routing);
    let opt = LayeredOptions {
        node_sep: NODE_SEP,
        edge_sep: EDGE_SEP,
        rank_sep: RANK_SEP,
        port_sep: 20.0,
        tracks: curve == Some("step"),
        straight: curve == Some("linear"),
        ..LayeredOptions::default()
    };
    let result = compound_layout(&mut cnodes, &mut cedges, root_dir, &opt);

    spread_ends(&cnodes, &mut cedges, &views);

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
        diagram: "requirement".into(),
        width: (inner + 2.0 * pad).ceil(),
        height: (result.height + title_height + 2.0 * pad).ceil(),
        font_size: size,
        title: db.acc_title.clone(),
        desc: db.acc_descr.clone(),
        items: Vec::new(),
    };

    let (mut titles, mut edges, mut labels, mut nodes) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    if !title.is_empty() {
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        let at = Point::new(sc.width / 2.0, pad + title.height / 2.0);
        it.label = Some(Placed::new(title, at));
        titles.push(it);
    }

    for (i, v) in views.iter().enumerate() {
        let (x, y) = (cnodes[i].x, cnodes[i].y);
        let (left, top) = (-v.w / 2.0, -v.h / 2.0);
        let bounds = Rect::new(x + left, y + top, v.w, v.h);
        let role = if v.node.requirement.is_some() {
            ROLE_REQUIREMENT
        } else {
            ROLE_ELEMENT
        };
        let mut it = Item::new(Kind::Node, role);
        it.shape = Some(Shape::new("rect"));
        it.id = Some(v.node.id.to_string());
        it.class = class_names(trim_default(&v.node.css_classes));
        it.bounds = bounds;
        it.parts = vec![Part::rect(bounds, true, Painting::FillAndStroke)];
        let at = Point::new(x, y + (top + PAD_Y + v.kind.height + v.name.height / 2.0));
        it.label = Some(Placed::new(v.name.clone(), at));
        it.style = Style {
            fill: Some(Paint::SURFACE),
            stroke: Some(Paint::BORDER),
            on_fill: v.inks,
            ..Style::default()
        };
        it.author = v.author.style.clone();
        // The texts are set on the box's fill, in the color that stands out on a fill the
        // author gives it, as Pele's inkFill picks it: the name on the box's own fill, and the
        // rest on the fill under them.
        let ground = if v.inks { v.author.style.fill } else { None };
        let text = |role: &'static str, l: &Label, align: Align, x: f64, y: f64, own: Paint| {
            text_item(role, l, align, x, y, own, &v.author, ground)
        };
        let mut texts = vec![text(
            ROLE_KIND,
            &v.kind,
            Align::Middle,
            x,
            y + (top + PAD_Y + v.kind.height / 2.0),
            Paint::MUTED,
        )];
        if !v.rows.is_empty() {
            let divider = top + v.head_height;
            let mut p = Path::new();
            p.move_to(x + left, y + divider)
                .line_to(x - left, y + divider);
            it.parts.push(Part::path(p, Painting::StrokeOnly));
            let mut at = divider + PAD_Y;
            for r in &v.rows {
                texts.push(text(
                    ROLE_FIELD,
                    &r.key,
                    Align::Start,
                    x + (left + PAD_X),
                    y + (at + r.key.height / 2.0),
                    Paint::MUTED,
                ));
                texts.push(text(
                    ROLE_VALUE,
                    &r.value,
                    Align::Start,
                    x + (left + PAD_X + v.key_width + COLUMN_GAP),
                    y + (at + r.value.height / 2.0),
                    Paint::FG,
                ));
                at += r.height + ROW_GAP;
            }
        }
        nodes.push(it);
        nodes.extend(texts);
    }

    let mut loop_seen = vec![0usize; views.len()];
    for (i, ce) in cedges.iter().enumerate() {
        let edge = drawn[i];
        let l = &edge_labels[i];
        let (start, end) = if edge.contains {
            (Some(MarkerType::RequirementContains), None)
        } else {
            (None, Some(MarkerType::RequirementArrow))
        };
        let start_trim = start.map_or(0.0, MarkerType::trim);
        let (mut label_x, mut label_y) = (ce.label_x, ce.label_y);
        let mut route = Vec::new();
        let path = if ce.src == ce.dst {
            let v = &views[ce.src];
            let (cx, cy) = (cnodes[ce.src].x, cnodes[ce.src].y);
            let k = loop_seen[ce.src] as f64;
            loop_seen[ce.src] += 1;
            let half = (if sideways { v.w } else { v.h }) / 2.0;
            let spread = min(half - 4.0, 12.0 + k * 6.0);
            let reach = LOOP + k * 8.0;
            // In the loop's own frame, out points away from the node and along runs along its
            // side.
            let at = |out: f64, along: f64| {
                if sideways {
                    (cx + along, cy + v.h / 2.0 + out)
                } else {
                    (cx + v.w / 2.0 + out, cy + along)
                }
            };
            // A cubic whose control points are this far out peaks at reach.
            let pull = reach / 0.75;
            let mut p = Path::new();
            let (x0, y0) = at(start_trim, -spread);
            let (ax, ay) = at(pull, -spread);
            let (bx, by) = at(pull, spread);
            let (ex, ey) = at(0.0, spread);
            p.move_to(x0, y0).cube_to(ax, ay, bx, by, ex, ey);
            let tip = |x, y, dx, dy| Tip {
                at: Point::new(x, y),
                dx,
                dy,
            };
            let (start, end) = if sideways {
                (label_x, label_y) = (cx, cy + v.h / 2.0 + reach + 4.0 + ce.label_h / 2.0);
                (
                    tip(cx - spread, cy + v.h / 2.0, 0.0, -1.0),
                    tip(cx + spread, cy + v.h / 2.0, 0.0, -1.0),
                )
            } else {
                (label_x, label_y) = (cx + v.w / 2.0 + reach + 4.0 + ce.label_w / 2.0, cy);
                (
                    tip(cx + v.w / 2.0, cy - spread, -1.0, 0.0),
                    tip(cx + v.w / 2.0, cy + spread, -1.0, 0.0),
                )
            };
            EdgePath {
                path: p,
                start,
                end,
            }
        } else {
            route = ce.route.clone();
            route_path(
                &route,
                Curve::of(curve.unwrap_or("")),
                start_trim,
                0.0,
                own_reach(start_trim),
                own_reach(0.0),
            )
        };
        // Pele draws the edges in a group that ends their lines round.
        let own = Style {
            stroke: Some(Paint::LINE),
            fill: Some(Paint::NONE),
            cap: Some(Cap::Round),
            dash: (!edge.contains).then(|| vec![5.0, 4.0]),
            ..Style::default()
        };
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
        let mut it = Item::new(
            Kind::Edge(Edge {
                route: points,
                ends: path.ends(start, end),
            }),
            ROLE_RELATIONSHIP,
        );
        it.id = Some(edge.id.clone());
        it.bounds = path.path.bounds();
        it.style = own;
        it.parts = vec![Part::path(path.path, Painting::StrokeOnly)];
        edges.push(it);

        let mut it = Item::new(Kind::Text, ROLE_RELATIONSHIP_LABEL);
        it.id = Some(edge.id.clone());
        it.bounds = label_patch_box(l.width, l.height, label_x, label_y);
        it.parts = vec![label_patch(l.width, l.height, label_x, label_y)];
        it.label = Some(Placed::new(l.clone(), Point::new(label_x, label_y)));
        it.style = Style {
            fill: Some(Paint::BG),
            stroke: Some(Paint::NONE),
            text: Some(Paint::MUTED),
            ..Style::default()
        };
        labels.push(it);
    }

    for layer in [titles, edges, labels, nodes] {
        sc.items.extend(layer);
    }
    sc
}

/// A text of a box beside its name: its kind, or a field's name or value, drawn about x, y
/// in the color own and the color of text its author gave the box, or in the color that
/// stands out on ground, the fill under it, where it is given.
#[allow(clippy::too_many_arguments)]
fn text_item(
    role: &'static str,
    l: &Label,
    align: Align,
    x: f64,
    y: f64,
    own: Paint,
    author: &Author,
    ground: Option<Paint>,
) -> Item {
    let mut it = Item::new(Kind::Text, role);
    let placed = Placed {
        label: l.clone(),
        at: Point::new(x, y),
        align,
        rotate: 0.0,
    };
    it.bounds = placed.bounds();
    it.label = Some(placed);
    it.style = Style {
        text: Some(own),
        text_ground: ground,
        ..Style::default()
    };
    it.author = Style {
        text: author.style.text,
        ..Style::default()
    };
    it
}
