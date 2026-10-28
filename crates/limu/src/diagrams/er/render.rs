//! An entity relationship diagram laid out as a scene, as Pele's renderEr in render.ts draws
//! it.

use std::collections::HashMap;

use super::db::ErDb;
use super::graph::{ErGraphEdge, ErGraphNode, build_er_graph};
use crate::common::{
    class_names, parse_generic_types, routing_curve, runs_across, setting, title_label,
    trim_default, turn_to_fit,
};
use crate::css::{Resolved, declarations, resolve_style};
use crate::draw::{
    Crossings, Curve, Fill, Node, casing, cluster_title_x, label_patch, label_patch_box,
    mark_crossings, on_fills, own_reach, route_path,
};
use crate::graphs::compound::{CEdge, CNode, Dir, cnode, compound_layout, direction, shift_layout};
use crate::graphs::layered::LayeredOptions;
use crate::js::math::{by, hypot, max, min, round, sign};
use crate::layout::{Monospace, Options};
use crate::scene::style::{Cap, Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{
    Align, Edge, Item, Kind, Marker, MarkerType, Painting, Part, Path, Placed, Point, Rect, Scene,
    Shape, Step,
};
use crate::text::{decode_entities, layout_label};
use crate::url::{Schemes, internal_link};
use crate::value::{Map, Value};

/// The roles of the items of an entity relationship diagram.
pub(crate) const ROLE_TITLE: &str = "er.title";
pub(crate) const ROLE_CLUSTER: &str = "er.cluster";
/// A casing under a relationship that crosses the border of a fill an author sets, drawn in
/// the background so that the relationship stands out on the fill.
pub(crate) const ROLE_CASING: &str = "er.casing";
pub(crate) const ROLE_RELATIONSHIP: &str = "er.relationship";
pub(crate) const ROLE_RELATIONSHIP_LABEL: &str = "er.relationship-label";
pub(crate) const ROLE_ENTITY: &str = "er.entity";
pub(crate) const ROLE_DIVIDER: &str = "er.divider";
pub(crate) const ROLE_ATTRIBUTE_TYPE: &str = "er.attribute-type";
pub(crate) const ROLE_ATTRIBUTE_NAME: &str = "er.attribute-name";
pub(crate) const ROLE_ATTRIBUTE_KEYS: &str = "er.attribute-keys";
pub(crate) const ROLE_ATTRIBUTE_COMMENT: &str = "er.attribute-comment";

/// The roles of the four cells of an attribute's row, in their order.
const CELL_ROLES: [&str; 4] = [
    ROLE_ATTRIBUTE_TYPE,
    ROLE_ATTRIBUTE_NAME,
    ROLE_ATTRIBUTE_KEYS,
    ROLE_ATTRIBUTE_COMMENT,
];

// The measures of an entity relationship diagram, as Pele's render.ts has them.
const NODE_SEP: f64 = 48.0;
const EDGE_SEP: f64 = 20.0;
const RANK_SEP: f64 = 64.0;
const GROUP_PAD: f64 = 20.0;
/// How far a self-relationship loops out beyond its markers.
const LOOP: f64 = 28.0;
/// The least stretch of a side a self-relationship takes.
const LOOP_BAND: f64 = 38.0;
const MIN_WIDTH: f64 = 96.0;
const BOX_PAD_X: f64 = 18.0;
const BOX_PAD_Y: f64 = 12.0;
const HEAD_PAD_Y: f64 = 9.0;
const CELL_PAD_X: f64 = 12.0;
const CELL_GAP: f64 = 16.0;
const ROW_PAD: f64 = 9.0;
const COMMENT_WRAP: f64 = 260.0;
const NO_WRAP: f64 = 1e6;
const PORT_GAP: f64 = 18.0;
const PORT_MARGIN: f64 = 14.0;
/// How far a marker reaches back along its line from the box it touches, Pele's
/// MARKER_LENGTH in markers.ts.
const MARKER_LENGTH: f64 = 18.0;

/// A number of the diagram's er configuration, where it is one above 0, at most 2000, as
/// Pele's numberOption reads it.
fn number_option(config: &Map, key: &str, fallback: f64) -> f64 {
    match config
        .get("er")
        .and_then(Value::as_object)
        .and_then(|s| s.get(key))
    {
        Some(&Value::Number(v)) if v > 0.0 => min(v, 2000.0),
        _ => fallback,
    }
}

/// A row of an entity's table, Pele's Row: its type, name, keys and comment, and its height.
struct Row {
    cells: [Label; 4],
    h: f64,
}

/// An entity or a subgraph as it is drawn, Pele's NodeView, with the declarations its style
/// is read from.
struct View<'g> {
    node: &'g ErGraphNode,
    decls: Vec<String>,
    style: Resolved,
    label: Label,
    w: f64,
    h: f64,
    head_h: f64,
    rows: Vec<Row>,
    /// The left edge of each column, from the left edge of the box.
    cols: Vec<f64>,
    row_size: f64,
    /// Whether self-relationships loop out of the bottom side rather than the right.
    below: bool,
}

/// A relationship from an entity to itself, Pele's Loop: its label, as the index of its
/// edge, and the stretch of the side it loops out of.
struct Loop {
    label: usize,
    band: f64,
    at: f64,
}

/// An attribute's text as it is laid out, as Pele's cellText makes it. Mermaid reads `~`
/// pairs as angle brackets, which are written as entities so that the label is not read as
/// holding a tag.
fn cell_text(raw: &str) -> String {
    let converted = parse_generic_types(raw);
    if converted == raw {
        return converted;
    }
    converted.replace('<', "&lt;").replace('>', "&gt;")
}

/// Lays out a parsed diagram as a scene, as Pele's renderEr draws it: in its own direction,
/// and, for one that runs across and does not fit the host's width, down instead where that
/// is narrower.
pub(crate) fn layout(db: &ErDb, config: &Map, o: &Options) -> Scene {
    if !runs_across(Some(db.direction)) {
        return draw(db, config, o, false);
    }
    turn_to_fit(o, |down| draw(db, config, o, down))
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

/// Lays the diagram out once, as Pele's draw does. turned lays out a diagram that runs across
/// as one that runs down.
fn draw(db: &ErDb, config: &Map, o: &Options, turned: bool) -> Scene {
    let graph = build_er_graph(db);
    let monospaced = o.monospace.contains(Monospace::ATTRIBUTES);
    let size = o.font_size;
    let small_size = round(size * 0.875);
    let set = setting(o);
    let pad = o.padding;
    let schemes = o.schemes();

    let mut index: HashMap<&str, usize> = HashMap::new();
    for (i, node) in graph.nodes.iter().enumerate() {
        index.insert(node.id.as_str(), i);
    }

    let mut cedges: Vec<CEdge> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    let mut drawn: Vec<&ErGraphEdge> = Vec::new();
    let mut loops: Vec<Loop> = Vec::new();
    // The loops of each node, as indices into loops, and the nodes in the order their first
    // loop came, as Pele's Map keeps them.
    let mut own_loops: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut loop_order: Vec<usize> = Vec::new();
    let mut loop_of: Vec<Option<usize>> = Vec::new();
    for edge in &graph.edges {
        let (Some(&src), Some(&dst)) =
            (index.get(edge.start.as_str()), index.get(edge.end.as_str()))
        else {
            continue;
        };
        let label = layout_label(&edge.label, true, &set, small_size, 200.0, Face::default());
        let mut lp = None;
        if src == dst {
            lp = Some(loops.len());
            loops.push(Loop {
                label: drawn.len(),
                band: 0.0,
                at: 0.0,
            });
            own_loops
                .entry(src)
                .or_insert_with(|| {
                    loop_order.push(src);
                    Vec::new()
                })
                .push(loops.len() - 1);
        }
        loop_of.push(lp);
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

    // Whether the level a node is laid out in runs across: its nearest subgraph's direction
    // that sets one, or the diagram's. Subgraphs can name each other as members, so the walk
    // up is bounded.
    let horizontal = |i: usize| {
        let mut at = i;
        for _ in 0..graph.nodes.len() {
            let Some(&up) = graph.nodes[at]
                .parent_id
                .as_deref()
                .and_then(|p| index.get(p))
            else {
                break;
            };
            if let Some(dir) = graph.nodes[up].dir {
                return dir == "LR" || dir == "RL";
            }
            at = up;
        }
        db.direction == "LR" || db.direction == "RL"
    };

    let mut views: Vec<View> = Vec::with_capacity(graph.nodes.len());
    let mut cnodes: Vec<CNode> = Vec::with_capacity(graph.nodes.len());
    for (i, node) in graph.nodes.iter().enumerate() {
        let mut decls = node.css_compiled_styles.clone();
        decls.extend(node.css_styles.iter().cloned());
        let style = resolve_style(&decls);
        let mut base = Face::default();
        if style.bold {
            base |= Face::BOLD;
        }
        if style.italic {
            base |= Face::ITALIC;
        }
        let mono = if monospaced && !style.font_family_declared() {
            Face::MONO
        } else {
            Face::default()
        };
        let name_size = style.font_size.unwrap_or(size);
        let row_size = style.font_size.unwrap_or(small_size);
        let key_size = style.font_size.unwrap_or(round(size * 0.75));
        let label = if node.is_group {
            layout_label(&node.label, true, &set, name_size, 4000.0, base)
        } else {
            let name = if node.alias.is_empty() {
                &node.label
            } else {
                &node.alias
            };
            layout_label(name, true, &set, name_size, 240.0, base)
        };
        let own: &[usize] = own_loops.get(&i).map_or(&[], Vec::as_slice);
        // Loops leave by a side the layout keeps free of other edges: the right when ranks run
        // down, the bottom when they run across.
        let below = !own.is_empty() && !node.is_group && horizontal(i);
        let mut view = View {
            node,
            decls,
            style,
            label,
            w: 0.0,
            h: 0.0,
            head_h: 0.0,
            rows: Vec::new(),
            cols: Vec::new(),
            row_size,
            below,
        };
        let (mut reach, mut bands) = (0.0, 0.0);
        for &k in own {
            let l = &edge_labels[loops[k].label];
            loops[k].band = max(
                LOOP_BAND,
                if below {
                    l.width + 12.0
                } else {
                    l.height + 6.0
                },
            );
            bands += loops[k].band;
            reach = max(reach, if below { l.height } else { l.width });
        }
        let min_w = max(MIN_WIDTH, if below { bands + 8.0 } else { 0.0 });
        let mut c;
        if node.is_group {
            c = cnode(0.0, 0.0, -1);
            c.is_group = true;
            c.dir = node.dir.map(|d| direction(Some(d)));
            c.pad_x = GROUP_PAD;
            c.pad_top = if view.label.height > 0.0 {
                view.label.height + 16.0
            } else {
                GROUP_PAD
            };
            c.pad_bottom = GROUP_PAD;
            c.min_w = max(view.label.width + 2.0 * GROUP_PAD - 12.0, MIN_WIDTH);
        } else {
            let label = &view.label;
            let name_h = max(label.height, round(name_size * 1.5));
            if node.attributes.is_empty() {
                view.w = max(label.width + 2.0 * BOX_PAD_X, min_w);
                view.h = max(
                    name_h + 2.0 * BOX_PAD_Y,
                    if below { 0.0 } else { bands + 8.0 },
                );
            } else {
                let row_line = round(row_size * 1.5);
                let mut widths = [0.0; 4];
                for a in &node.attributes {
                    let cells = [
                        layout_label(
                            &cell_text(&a.kind),
                            true,
                            &set,
                            row_size,
                            NO_WRAP,
                            base | mono,
                        ),
                        layout_label(
                            &cell_text(&a.name),
                            true,
                            &set,
                            row_size,
                            NO_WRAP,
                            base | mono,
                        ),
                        layout_label(
                            &a.keys.join(", "),
                            false,
                            &set,
                            key_size,
                            NO_WRAP,
                            base | mono,
                        ),
                        layout_label(
                            &cell_text(&a.comment),
                            true,
                            &set,
                            row_size,
                            COMMENT_WRAP,
                            base,
                        ),
                    ];
                    let mut h = row_line;
                    for (k, cell) in cells.iter().enumerate() {
                        if cell.width > widths[k] {
                            widths[k] = cell.width;
                        }
                        if cell.height > h {
                            h = cell.height;
                        }
                    }
                    view.rows.push(Row {
                        cells,
                        h: h + ROW_PAD,
                    });
                    view.h += h + ROW_PAD;
                }
                let (mut columns, mut inner) = (0, 0.0);
                for &w in &widths {
                    if w == 0.0 {
                        continue;
                    }
                    inner += w + if columns > 0 { CELL_GAP } else { 0.0 };
                    columns += 1;
                }
                view.w = max(
                    max(inner + 2.0 * CELL_PAD_X, view.label.width + 2.0 * BOX_PAD_X),
                    min_w,
                );
                // A box wider than its table shares the spare width between the columns.
                let spare = if columns > 0 {
                    (view.w - inner - 2.0 * CELL_PAD_X) / f64::from(columns)
                } else {
                    0.0
                };
                let mut x = CELL_PAD_X;
                for &w in &widths {
                    view.cols.push(x);
                    if w > 0.0 {
                        x += w + spare + CELL_GAP;
                    }
                }
                view.head_h = name_h + 2.0 * HEAD_PAD_Y;
                view.h += view.head_h;
            }
            // The layout keeps a node centered in its box, so room for loops is added on both
            // sides.
            let room = if own.is_empty() {
                0.0
            } else {
                2.0 * (MARKER_LENGTH + LOOP * 0.75 + 10.0 + reach)
            };
            c = if below {
                cnode(view.w, view.h + room, -1)
            } else {
                cnode(view.w + room, view.h, -1)
            };
        }
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

    let root_dir = if turned {
        Dir::TB
    } else {
        direction(Some(db.direction))
    };
    // The host may ask for edges at right angles, or straight, as it may of a flowchart.
    let curve = routing_curve(o.routing);
    let opt = LayeredOptions {
        node_sep: number_option(config, "nodeSpacing", NODE_SEP),
        edge_sep: EDGE_SEP,
        rank_sep: number_option(config, "rankSpacing", RANK_SEP),
        port_sep: 24.0,
        tracks: curve == Some("step"),
        straight: curve == Some("linear"),
        ..LayeredOptions::default()
    };
    let result = compound_layout(&mut cnodes, &mut cedges, root_dir, &opt);

    spread_ends(&mut cedges, &cnodes, &views);

    // A subgraph gets its size from the layout, so its loops hang outside it and may widen
    // the diagram.
    let mut layout_width = result.width;
    for &i in &loop_order {
        let c = &cnodes[i];
        if !c.is_group {
            continue;
        }
        let mut reach = 0.0;
        for &k in &own_loops[&i] {
            reach = max(reach, edge_labels[loops[k].label].width);
        }
        layout_width = max(
            layout_width,
            c.x + c.w / 2.0 + MARKER_LENGTH + LOOP * 0.75 + 10.0 + reach,
        );
    }

    let title = title_label(db.title.as_deref(), &set, size, 4000.0);
    let title_height = if title.height > 0.0 {
        title.height + 12.0
    } else {
        0.0
    };
    let inner = max(layout_width, title.width);
    let ox = pad + (inner - layout_width) / 2.0;
    let oy = pad + title_height;
    shift_layout(&mut cnodes, &mut cedges, ox, oy);
    let mut sc = Scene {
        diagram: "er".into(),
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

    let (mut titles, mut clusters, mut casings) = (Vec::new(), Vec::new(), Vec::new());
    let (mut nodes, mut edges, mut labels) = (Vec::new(), Vec::new(), Vec::new());
    // The subgraphs whose authors paint them a fill, which relationships inside them are drawn
    // on, as Pele's fills.
    let mut fills: Vec<Fill> = Vec::new();
    if !title.is_empty() {
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        let at = Point::new(sc.width / 2.0, pad + title.height / 2.0);
        it.label = Some(Placed::new(title, at));
        titles.push(it);
    }
    for (i, view) in views.iter_mut().enumerate() {
        let c = &cnodes[i];
        let n = view.node;
        let class = class_names(trim_default(&n.css_classes));
        let author = declarations(&view.decls);
        if c.is_group {
            (view.w, view.h) = (c.w, c.h);
            let bounds = Rect::new(c.x - c.w / 2.0, c.y - c.h / 2.0, c.w, c.h);
            let title_x = cluster_title_x(&node_of(c), view.label.width, &crossings, GROUP_PAD);
            if view.style.paints() {
                fills.push(Fill {
                    bounds: [bounds.x, bounds.y, bounds.x + c.w, bounds.y + c.h],
                    decls: view.decls.clone(),
                });
            }
            let mut cluster = Item::new(Kind::Group, ROLE_CLUSTER);
            cluster.id = Some(n.id.clone());
            cluster.class = class;
            cluster.bounds = bounds;
            cluster.parts = vec![Part::rect(bounds, true, Painting::FillAndStroke)];
            // The title is set on the subgraph's fill, in the color that stands out on a fill
            // the author gives it, as Pele's inkFill picks it. A subgraph is drawn at half its
            // opacity, unless its author sets a fill, as Pele's setsFill tests.
            cluster.style = Style {
                fill: Some(Paint::SURFACE_ALT),
                fill_opacity: if view.style.fill_declared() {
                    None
                } else {
                    Some(0.5)
                },
                stroke: Some(Paint::BORDER),
                text: Some(Paint::MUTED),
                on_fill: view.style.inks_text(&view.style),
                ..Style::default()
            };
            cluster.author = author.style;
            if !view.label.is_empty() {
                let at = Point::new(title_x, bounds.y + 8.0 + view.label.height / 2.0);
                cluster.label = Some(Placed::new(view.label.clone(), at));
            }
            clusters.push(cluster);
            continue;
        }
        nodes.extend(entity(view, c.x, c.y, class, &author.style, o, &schemes));
    }

    // Self-relationships share the side they loop out of, squeezed together if it is too
    // short for them.
    for &i in &loop_order {
        let view = &views[i];
        let own = &own_loops[&i];
        let mut total = 0.0;
        for &k in own {
            total += loops[k].band;
        }
        let side = if view.below { view.w } else { view.h };
        let scale = min(1.0, max(side - 8.0, 0.0) / total);
        let mut at = (-total * scale) / 2.0;
        for &k in own {
            loops[k].band *= scale;
            loops[k].at = at + loops[k].band / 2.0;
            at += loops[k].band;
        }
    }

    for i in 0..cedges.len() {
        let edge = drawn[i];
        let label = &edge_labels[i];
        let marker = |name: &str| MarkerType::from_name(&format!("er.{name}"));
        let (start, end) = (marker(&edge.arrow_type_start), marker(&edge.arrow_type_end));
        let path: Path;
        let ends: [Option<Marker>; 2];
        let mut route: Vec<Point> = Vec::new();
        let at = |kind: Option<MarkerType>, x: f64, y: f64, dx: f64, dy: f64| {
            kind.map(|kind| Marker {
                kind,
                at: Point::new(x, y),
                dx,
                dy,
            })
        };
        if let Some(k) = loop_of[i] {
            let lp = &loops[k];
            let ce = &mut cedges[i];
            let c = &cnodes[ce.src];
            let view = &views[ce.src];
            let spread = max(min(lp.band / 2.0 - 6.0, 13.0), 1.0);
            let far = MARKER_LENGTH + LOOP * 0.75 + 6.0;
            let mut p = Path::new();
            if view.below {
                let x = c.x + lp.at;
                let y = c.y + view.h / 2.0;
                let turn = y + MARKER_LENGTH;
                let out = y + MARKER_LENGTH + LOOP;
                p.move_to(x - spread, y)
                    .line_to(x - spread, turn)
                    .cube_to(x - spread, out, x + spread, out, x + spread, turn)
                    .line_to(x + spread, y);
                ends = [
                    at(start, x - spread, y, 0.0, -1.0),
                    at(end, x + spread, y, 0.0, -1.0),
                ];
                ce.label_x = x;
                ce.label_y = y + far + label.height / 2.0;
            } else {
                let x = c.x + view.w / 2.0;
                let y = c.y + lp.at;
                let turn = x + MARKER_LENGTH;
                let out = x + MARKER_LENGTH + LOOP;
                p.move_to(x, y - spread)
                    .line_to(turn, y - spread)
                    .cube_to(out, y - spread, out, y + spread, turn, y + spread)
                    .line_to(x, y + spread);
                ends = [
                    at(start, x, y - spread, -1.0, 0.0),
                    at(end, x, y + spread, -1.0, 0.0),
                ];
                ce.label_x = x + far + label.width / 2.0;
                ce.label_y = y;
            }
            path = p;
        } else {
            let ce = &cedges[i];
            let mut r = straighten(&ce.route);
            if r.len() < 6 {
                continue;
            }
            let last = r.len() - 3;
            inset(&mut r, 0, &views[ce.src], &cnodes[ce.src]);
            inset(&mut r, last, &views[ce.dst], &cnodes[ce.dst]);
            // A curve stops short of each box so that the marker sits on a straight run of
            // line.
            let dx = (r[last] - r[0]).abs();
            let dy = (r[last + 1] - r[1]).abs();
            let direct =
                r.len() == 6 && (dx < 0.01 || dy < 0.01 || hypot(dx, dy) <= 3.0 * MARKER_LENGTH);
            let stub = if direct { 0.0 } else { MARKER_LENGTH };
            let ep = route_path(
                &r,
                Curve::of(curve.unwrap_or("")),
                stub,
                stub,
                own_reach(stub),
                own_reach(stub),
            );
            ends = ep.ends(start, end);
            path = if stub > 0.0 {
                // The marker's straight run, from the box to where the curve starts, and from
                // where it ends to the box.
                let mut p = Path::new();
                p.move_to(ep.start.at.x, ep.start.at.y);
                for step in ep.path.steps() {
                    match step {
                        Step::MoveTo(to) | Step::LineTo(to) => p.line_to(to.x, to.y),
                        Step::QuadTo(c, to) => p.quad_to(c.x, c.y, to.x, to.y),
                        Step::CubeTo(c1, c2, to) => p.cube_to(c1.x, c1.y, c2.x, c2.y, to.x, to.y),
                        Step::Close => p.close(),
                    };
                }
                p.line_to(ep.end.at.x, ep.end.at.y);
                p
            } else {
                ep.path
            };
            route = r
                .as_chunks::<3>()
                .0
                .iter()
                .map(|&[x, y, _]| Point::new(x, y))
                .collect();
        }
        if route.is_empty() {
            // A self-loop, which the layout gives no route: the corners its curve is drawn
            // about.
            route = path.corners();
        }
        // Pele draws the edges in a group that ends their lines round.
        let mut own = Style {
            stroke: Some(Paint::LINE),
            fill: Some(Paint::NONE),
            cap: Some(Cap::Round),
            ..Style::default()
        };
        if edge.pattern == "dashed" {
            own.dash = Some(vec![4.0, 3.0]);
        }
        // A relationship inside one fill is drawn in the color of its labels, with its open
        // markers filled with the fill, and one that crosses the border of a fill is laid on
        // a casing, as Pele's fillUnder and edgeInk find.
        let (ground, cased) = on_fills(&path, &[], &fills, "");
        own.ground = ground;
        if cased && let Some(mut c) = casing(ROLE_CASING, &path, &ends, &[], false) {
            c.id = Some(edge.id.clone());
            casings.push(c);
        }
        let mut it = Item::new(Kind::Edge(Edge { route, ends }), ROLE_RELATIONSHIP);
        it.id = Some(edge.id.clone());
        it.bounds = path.bounds();
        it.parts = vec![Part::path(path, Painting::StrokeOnly)];
        it.style = own;
        edges.push(it);
        if label.width > 0.0 {
            let ce = &cedges[i];
            let (x, y) = (ce.label_x, ce.label_y);
            let mut it = Item::new(Kind::Text, ROLE_RELATIONSHIP_LABEL);
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

    for layer in [titles, clusters, casings, edges, labels, nodes] {
        sc.items.extend(layer);
    }
    sc
}

/// An entity drawn about x, y, as Pele's draw draws one: its box, with its name, and, where
/// it has attributes, its table. The stripes of every other row and its outline drawn again
/// over them are parts of the entity, and the line under its name and the text of each cell
/// are items of their own after it.
fn entity(
    view: &View,
    x: f64,
    y: f64,
    class: Vec<String>,
    author: &Style,
    o: &Options,
    schemes: &Schemes,
) -> Vec<Item> {
    let n = view.node;
    let (w, h) = (view.w, view.h);
    let (x0, y0) = (-w / 2.0, -h / 2.0);
    let bounds = Rect::new(x + x0, y + y0, w, h);
    let mut it = Item::new(Kind::Node, ROLE_ENTITY);
    it.shape = Some(Shape::new("rounded"));
    it.id = Some(n.label.clone());
    it.class = class;
    it.bounds = bounds;
    it.parts = vec![Part::rect(bounds, true, Painting::FillAndStroke)];
    it.style = Style {
        fill: Some(Paint::SURFACE),
        stroke: Some(Paint::BORDER),
        on_fill: view.style.inks_text(&view.style),
        ..Style::default()
    };
    it.author = author.clone();
    // The texts are set on the entity's fill, in the color that stands out on a fill the
    // author gives it, as Pele's inkFill picks it: the name on the box's own fill, and the
    // cells on the fill under them. The stripes under every other row are shaded as the
    // look's shade picks on that fill.
    let ground = if it.style.on_fill { author.fill } else { None };
    if !view.label.is_empty() {
        let at_y = if view.rows.is_empty() {
            y
        } else {
            y + (y0 + view.head_h / 2.0)
        };
        it.label = Some(Placed::new(view.label.clone(), Point::new(x, at_y)));
        if it.class.iter().any(|c| c == "internal-link") && o.links {
            // The name ends up in an href, so it gets the same check as an address.
            let name = if n.alias.is_empty() {
                &n.label
            } else {
                &n.alias
            };
            let display = decode_entities(name);
            if internal_link(&display, schemes) {
                it.link = Some(display);
            }
        }
    }
    if view.rows.is_empty() {
        return vec![it];
    }
    let row_line = round(view.row_size * 1.5);
    let text_style = Style {
        text: author.text,
        opacity: author.opacity,
        ..Style::default()
    };
    let mut cells = Vec::new();
    let mut top = y0 + view.head_h;
    for (r, row) in view.rows.iter().enumerate() {
        if r % 2 == 0 {
            let stripe = Rect::new(x + x0, y + top, w, row.h);
            if r == view.rows.len() - 1 {
                // The last row follows the box's rounded bottom corners: a rounded box,
                // squared off on top.
                let half = Rect {
                    h: row.h / 2.0,
                    ..stripe
                };
                it.parts.push(Part::rect(stripe, true, Painting::Shade));
                it.parts.push(Part::rect(half, false, Painting::Shade));
            } else {
                it.parts.push(Part::rect(stripe, false, Painting::Shade));
            }
        }
        let baseline = top + ROW_PAD / 2.0 + row_line / 2.0 + view.row_size * 0.35;
        for (k, l) in row.cells.iter().enumerate() {
            if l.is_empty() {
                continue;
            }
            let own = Style {
                text: if k == 1 { None } else { Some(Paint::MUTED) },
                text_ground: ground,
                ..Style::default()
            };
            // The label is centered on its point down, and its first line's baseline is where
            // Pele writes it.
            let at = Point::new(
                x + (x0 + view.cols[k]),
                y + (baseline - l.size * 0.35 + l.height / 2.0 - l.line_height / 2.0),
            );
            let mut cell = Item::new(Kind::Text, CELL_ROLES[k]);
            cell.id = Some(n.label.clone());
            let mut placed = Placed::new(l.clone(), at);
            placed.align = Align::Start;
            cell.label = Some(placed);
            cell.style = own;
            cell.author = text_style.clone();
            cells.push(cell);
        }
        top += row.h;
    }
    it.parts
        .push(Part::rect(bounds, true, Painting::StrokeOnly));
    // The line under the name is an item of its own, as Pele gives it only the stroke color
    // the author set and none of the rest of the entity's style. Pele draws it before the
    // outline drawn again over the stripes. Here it is drawn after it, in the same color.
    let at = y + (y0 + view.head_h);
    let mut line = Path::new();
    line.move_to(x + x0, at).line_to(x + (x0 + w), at);
    let mut divider = Item::new(Kind::Mark, ROLE_DIVIDER);
    divider.id = Some(n.label.clone());
    divider.bounds = line.bounds();
    divider.parts = vec![Part::path(line, Painting::StrokeOnly)];
    divider.style = Style {
        stroke: Some(Paint::BORDER),
        ..Style::default()
    };
    divider.author = Style {
        stroke: author.stroke,
        ..Style::default()
    };
    let mut out = vec![it, divider];
    out.extend(cells);
    out
}

/// A route without the points that lie on a straight run between their neighbours, as
/// Pele's straighten makes it.
fn straighten(route: &[f64]) -> Vec<f64> {
    let mut out: Vec<f64> = route[..route.len().min(3)].to_vec();
    let mut k = 3;
    while k < route.len() {
        let prev = out.len() - 3;
        // Whether point k lies on a straight run along axis between the last point kept and
        // the next, as the flat of Pele's straighten says.
        let flat = |axis: usize| {
            (out[prev + axis] - route[k + axis]).abs() < 0.01
                && (route[k + axis] - route[k + 3 + axis]).abs() < 0.01
        };
        if !(k + 3 < route.len() && (flat(0) || flat(1))) {
            out.extend_from_slice(&route[k..k + 3]);
        }
        k += 3;
    }
    out
}

/// The end of a route on a side of a box, Pele's End: the edge, the index of the end in its
/// route, and where along the side the route's next point lies.
struct End {
    edge: usize,
    at: usize,
    toward: f64,
}

/// Spaces the ends of the edges on each side of a box along it, in the order they fan out,
/// as Pele's spreadEnds does. The layout brings every edge that ends on one side of a box to
/// the middle of that side, where their markers would cover each other.
fn spread_ends(cedges: &mut [CEdge], cnodes: &[CNode], views: &[View]) {
    let mut sides: HashMap<usize, Vec<End>> = HashMap::new();
    let mut order: Vec<usize> = Vec::new();
    // Files the end of route at index at under the side of node's box it lies on, with where
    // the next point lies along that side, as the add of Pele's spreadEnds does.
    let mut add = |node: usize, edge: usize, route: &[f64], at: usize, next: usize| {
        let c = &cnodes[node];
        let dx = route[at] - c.x;
        let dy = route[at + 1] - c.y;
        let side = if (dy.abs() - c.h / 2.0).abs() < 0.5 {
            if dy < 0.0 { 0 } else { 2 }
        } else if (dx.abs() - c.w / 2.0).abs() < 0.5 {
            if dx < 0.0 { 3 } else { 1 }
        } else {
            return;
        };
        let key = node * 4 + side;
        let end = End {
            edge,
            at,
            toward: route[next + (side & 1)],
        };
        sides
            .entry(key)
            .or_insert_with(|| {
                order.push(key);
                Vec::new()
            })
            .push(end);
    };
    for (i, e) in cedges.iter().enumerate() {
        if e.src == e.dst || e.route.len() < 6 {
            continue;
        }
        let last = e.route.len() - 3;
        add(e.src, i, &e.route, 0, 3);
        add(e.dst, i, &e.route, last, last - 3);
    }
    for key in order {
        let ends = sides.get_mut(&key).expect("a side filed");
        if ends.len() < 2 {
            continue;
        }
        let node = key >> 2;
        let vertical = key & 1 == 1;
        let c = &cnodes[node];
        let (w, h) = if c.is_group {
            (c.w, c.h)
        } else {
            (views[node].w, views[node].h)
        };
        let (length, center, axis) = if vertical { (h, c.y, 1) } else { (w, c.x, 0) };
        let count = ends.len() as f64;
        let step = max(
            min(PORT_GAP, (length - 2.0 * PORT_MARGIN) / (count - 1.0)),
            0.0,
        );
        ends.sort_by(|a, b| by(a.toward - b.toward));
        for (i, end) in ends.iter().enumerate() {
            cedges[end.edge].route[end.at + axis] =
                center + (i as f64 - (count - 1.0) / 2.0) * step;
        }
    }
}

/// Moves a route's end from the side of the larger box an entity with a self-relationship is
/// laid out in onto the entity's own side, as Pele's inset does.
fn inset(route: &mut [f64], at: usize, view: &View, c: &CNode) {
    if c.is_group {
        return;
    }
    let dx = route[at] - c.x;
    let dy = route[at + 1] - c.y;
    if c.w != view.w && (dx.abs() - c.w / 2.0).abs() < 0.5 {
        route[at] -= sign(dx) * (c.w - view.w) / 2.0;
    }
    if c.h != view.h && (dy.abs() - c.h / 2.0).abs() < 0.5 {
        route[at + 1] -= sign(dy) * (c.h - view.h) / 2.0;
    }
}
