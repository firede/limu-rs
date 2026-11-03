//! A use case diagram laid out as a scene, as Pele's renderUsecase in render.ts draws it.

use super::graph::{UsecaseGraphNode, build_usecase_graph};
use super::types::{LabelType, UsecaseModel};
use crate::common::{class_names, routing_curve, runs_across, setting, title_label, turn_to_fit};
use crate::css::{Author, declarations, resolve_style};
use crate::draw::{
    Crossings, Curve, EdgePath, Fill, Tip, casing, label_patch, label_patch_box, mark_crossings,
    marker_trim, on_fills, own_reach, route_path,
};
use crate::graphs::compound::{CEdge, CNode, Dir, cnode, compound_layout, direction, shift_layout};
use crate::graphs::layered::LayeredOptions;
use crate::js::math::{by, hypot, max, min, or1, round};
use crate::layout::Options;
use crate::scene::style::{Cap, Join, Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{
    Edge, Item, Kind, MarkerType, Painting, Part, Path, Picture, PictureSource, Placed, Point,
    Rect, Scene, Shape,
};
use crate::text::{Setting, layout_label};
use crate::value::{Map, Value};

/// The roles of the items of a use case diagram.
pub(crate) const ROLE_ACTOR: &str = "usecase.actor";
/// A casing under a relation that crosses the border of a fill an author sets, drawn in the
/// background so that the relation stands out on the fill.
pub(crate) const ROLE_CASING: &str = "usecase.casing";
pub(crate) const ROLE_EDGE_LABEL: &str = "usecase.edge-label";
pub(crate) const ROLE_JSON: &str = "usecase.json";
pub(crate) const ROLE_JSON_KEY: &str = "usecase.json-key";
pub(crate) const ROLE_JSON_VALUE: &str = "usecase.json-value";
pub(crate) const ROLE_NOTE: &str = "usecase.note";
pub(crate) const ROLE_NOTE_LINK: &str = "usecase.note-link";
pub(crate) const ROLE_RELATION: &str = "usecase.relation";
pub(crate) const ROLE_STEREOTYPE: &str = "usecase.stereotype";
pub(crate) const ROLE_SYSTEM: &str = "usecase.system";
pub(crate) const ROLE_TITLE: &str = "usecase.title";
pub(crate) const ROLE_USECASE: &str = "usecase.usecase";

// The spacing and measures of a use case diagram, as Pele's render.ts has them.
const NODE_SEP: f64 = 32.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 72.0;
const GROUP_PAD: f64 = 20.0;
const LOOP: f64 = 26.0;
const FIGURE_W: f64 = 40.0;
const FIGURE_H: f64 = 60.0;
const FIGURE_GAP: f64 = 6.0;
const TAB_PAD: f64 = 6.0;
const CELL_PAD: f64 = 10.0;
const ROW_PAD: f64 = 8.0;
const FOLD: f64 = 9.0;
const PORT_GAP: f64 = 14.0;

/// The kinds of node, Pele's Kind.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NodeKind {
    Actor,
    Ellipse,
    Box,
    Json,
    Boundary,
}

/// A node as it is drawn, Pele's View, with the declarations of its classes and styles,
/// which say whether it sets a fill and whether that fill picks the color of its texts. w
/// and h are the size of what is drawn. The layout's box is larger where the node carries a
/// self-relationship.
struct View<'g> {
    node: &'g UsecaseGraphNode,
    kind: NodeKind,
    author: Author,
    decls: Vec<String>,
    label: Label,
    stereotype: Label,
    w: f64,
    h: f64,
    rows: Vec<(Label, Label)>,
    key_w: f64,
    head_h: f64,
}

/// A number of the diagram's configuration, where it is one above 0, at most 2000, as Pele's
/// numberOption reads it.
fn number_option(config: &Map, key: &str, fallback: f64) -> f64 {
    match config
        .get("usecase")
        .and_then(Value::as_object)
        .and_then(|s| s.get(key))
    {
        Some(&Value::Number(v)) if v > 0.0 => min(v, 2000.0),
        _ => fallback,
    }
}

/// A plain label as the label parser is to show it, as Pele's literal makes it: no tags, no
/// markdown, no `\n` breaks. Its markup characters are handed over as character references,
/// which the parser turns back into characters.
fn literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' | '<' | '>' | '\\' => out.push_str(&format!("&#{};", c as u32)),
            _ => out.push(c),
        }
    }
    out
}

/// A label laid out as Pele's text lays it out, a plain one handed over as literal makes it.
fn text(raw: &str, markdown: bool, set: &Setting, size: f64, wrap: f64, base: Face) -> Label {
    if !raw.is_empty() && !markdown {
        return layout_label(&literal(raw), false, set, size, wrap, base);
    }
    layout_label(raw, markdown, set, size, wrap, base)
}

/// The point of a node's outline on one of its four sides (0 top, 1 right, 2 bottom, 3
/// left), along away from the middle of that side, as Pele's anchor finds it. Lines meet an
/// actor at its figure, not at its label.
fn anchor(v: &View, c: &CNode, side: usize, along: f64) -> (f64, f64) {
    let sign = if side == 1 || side == 2 { 1.0 } else { -1.0 };
    let upright = side == 1 || side == 3;
    let mut reach = (if upright { v.w } else { v.h }) / 2.0;
    let mut middle = 0.0;
    if v.kind == NodeKind::Actor {
        if upright {
            reach = FIGURE_W / 2.0 + 3.0;
            middle = (FIGURE_H - v.h) / 2.0;
        } else {
            reach += 2.0;
        }
    } else if v.kind == NodeKind::Ellipse {
        let span = (if upright { v.h } else { v.w }) / 2.0;
        let q = along / span;
        reach *= max(1.0 - q * q, 0.0).sqrt();
    }
    if upright {
        (c.x + sign * reach, c.y + middle + along)
    } else {
        (c.x + along, c.y + sign * reach)
    }
}

/// How much of a side edges may spread over, as Pele's sideLength gives it.
fn side_length(v: &View, side: usize) -> f64 {
    let upright = side == 1 || side == 3;
    if v.kind == NodeKind::Actor {
        return if upright {
            FIGURE_H * 0.5
        } else {
            FIGURE_W * 0.6
        };
    }
    let length = if upright { v.h } else { v.w };
    if v.kind == NodeKind::Ellipse {
        length * 0.6
    } else {
        length - 16.0
    }
}

/// An end of a route on a side of a node: the edge, the index of the point in its route,
/// and where the route leaves toward, across the side.
struct End {
    edge: usize,
    at: usize,
    toward: f64,
}

/// Spaces the edges that end on one side of a node along the side, in the order they fan
/// out, and puts each on the node's outline, as Pele's attachEnds does: the layout brings
/// them all to the middle of the side.
fn attach_ends(cedges: &mut [CEdge], cnodes: &[CNode], views: &[View]) {
    // The ends on each side, by node and side, in the order the sides were first met, the
    // order Pele's Map gives them in.
    let mut sides: Vec<(usize, Vec<End>)> = Vec::new();
    let mut add = |index: usize, edge: usize, route: &[f64], at: usize, next: usize| {
        let c = &cnodes[index];
        if c.is_group {
            return;
        }
        let dx = route[at] - c.x;
        let dy = route[at + 1] - c.y;
        let side = if (dy.abs() - c.h / 2.0).abs() < 0.5 {
            if dy < 0.0 { 0 } else { 2 }
        } else if (dx.abs() - c.w / 2.0).abs() < 0.5 {
            if dx < 0.0 { 3 } else { 1 }
        } else {
            return;
        };
        let key = index * 4 + side;
        let end = End {
            edge,
            at,
            toward: route[next + (side & 1)],
        };
        match sides.iter_mut().find(|(k, _)| *k == key) {
            Some((_, list)) => list.push(end),
            None => sides.push((key, vec![end])),
        }
    };
    for (i, e) in cedges.iter().enumerate() {
        if e.src == e.dst || e.route.len() < 6 {
            continue;
        }
        let last = e.route.len() - 3;
        add(e.src, i, &e.route, 0, 3);
        add(e.dst, i, &e.route, last, last - 3);
    }
    for (key, mut ends) in sides {
        let (index, side) = (key >> 2, key & 3);
        let v = &views[index];
        let step = if ends.len() > 1 {
            min(PORT_GAP, side_length(v, side) / (ends.len() - 1) as f64)
        } else {
            0.0
        };
        ends.sort_by(|a, b| by(a.toward - b.toward));
        let n = ends.len() as f64;
        for (i, end) in ends.iter().enumerate() {
            let (x, y) = anchor(v, &cnodes[index], side, (i as f64 - (n - 1.0) / 2.0) * step);
            let route = &mut cedges[end.edge].route;
            route[end.at] = x;
            route[end.at + 1] = y;
        }
    }
}

/// The scene's marker of one of Pele's types: none for "none", and the hollow triangle of a
/// generalization, Pele's extension, under the diagram's own name.
fn marker_type(t: &str) -> Option<MarkerType> {
    match t {
        "none" | "" => None,
        "extension" => Some(MarkerType::UsecaseExtension),
        _ => MarkerType::from_name(t),
    }
}

/// How far an edge stops short of its end for a marker of a type, as Pele's trim says.
fn trim(t: &str) -> f64 {
    marker_trim(marker_type(t))
}

/// The parts of an actor's figure, drawn about fx, fy, and its icon, as Pele's figure draws
/// them: a head and limbs, a hollow body, a solid silhouette, or a box for an icon, with the
/// slash of a business actor across its head.
fn figure(v: &View, fx: f64, fy: f64) -> (Vec<Part>, Option<Picture>) {
    let n = v.node;
    let (mut head, mut radius) = (-21.0, 9.0);
    let pt = |x: f64, y: f64| (fx + x, fy + y);
    let mut parts = Vec::new();
    let mut picture = None;
    match n.actor_type {
        Some("hollow") => {
            radius = 8.0;
            let body = [
                (-20.0, -9.0),
                (20.0, -9.0),
                (20.0, 0.0),
                (5.5, 0.0),
                (20.0, 15.5),
                (12.0, 25.5),
                (0.0, 12.0),
                (-12.0, 25.5),
                (-20.0, 15.5),
                (-5.5, 0.0),
                (-20.0, 0.0),
            ]
            .map(|(x, y)| pt(x, y));
            let mut head = Path::new();
            head.ellipse(fx, fy - 21.0, 8.0, 8.0);
            let mut outline = Path::new();
            outline.polygon(&body);
            parts.push(Part::path(head, Painting::Hollow));
            parts.push(Part::path(outline, Painting::Hollow));
        }
        Some("awesome") => {
            let mut body = Path::new();
            let cubic = |body: &mut Path, [a, b, c, d, e, f]: [f64; 6]| {
                let ((x1, y1), (x2, y2), (x3, y3)) = (pt(a, b), pt(c, d), pt(e, f));
                body.cube_to(x1, y1, x2, y2, x3, y3);
            };
            let (x, y) = pt(-20.0, 25.0);
            body.move_to(x, y);
            cubic(&mut body, [-20.0, 9.0, -12.0, -3.0, 0.0, -3.0]);
            cubic(&mut body, [12.0, -3.0, 20.0, 9.0, 20.0, 25.0]);
            cubic(&mut body, [20.0, 28.0, 18.0, 30.0, 15.0, 30.0]);
            let (x, y) = pt(-15.0, 30.0);
            body.line_to(x, y);
            cubic(&mut body, [-18.0, 30.0, -20.0, 28.0, -20.0, 25.0]);
            body.close();
            // Pele draws the head as two arcs of one path, which make the same circle.
            let mut head = Path::new();
            head.ellipse(fx, fy - 19.0, 11.0, 11.0);
            parts.push(Part::path(head, Painting::FillOnly));
            parts.push(Part::path(body, Painting::FillOnly));
        }
        Some("icon") => {
            (head, radius) = (-14.0, 12.0);
            let frame = Rect::new(fx - 22.0, fy - 26.0, 44.0, 44.0);
            parts.push(Part::rect(frame, true, Painting::FillAndStroke));
            picture = Some(Picture {
                source: PictureSource::Icon(n.icon.clone().unwrap_or_default()),
                bounds: Rect::new(fx - 14.0, fy - 18.0, 28.0, 28.0),
            });
        }
        _ => {
            let mut head = Path::new();
            head.ellipse(fx, fy - 21.0, 9.0, 9.0);
            let mut limbs = Path::new();
            for [x0, y0, x1, y1] in [
                [0.0, -12.0, 0.0, 10.0],
                [-14.0, -3.0, 14.0, -3.0],
                [0.0, 10.0, -12.0, 30.0],
                [0.0, 10.0, 12.0, 30.0],
            ] {
                let ((ax, ay), (bx, by)) = (pt(x0, y0), pt(x1, y1));
                limbs.move_to(ax, ay).line_to(bx, by);
            }
            parts.push(Part::path(head, Painting::FillAndStroke));
            parts.push(Part::path(limbs, Painting::StrokeOnly));
        }
    }
    if n.business == Some(true) {
        // The slash of a business actor: a chord across the lower right of the head.
        let ((ax, ay), (bx, by)) = (
            pt(0.12 * radius, head + 0.99 * radius),
            pt(0.92 * radius, head - 0.39 * radius),
        );
        let mut slash = Path::new();
        slash.move_to(ax, ay).line_to(bx, by);
        parts.push(Part::path(slash, Painting::StrokeOnly));
    }
    (parts, picture)
}

/// Where a boundary's title sits along its top, as Pele's titleX places it: in the middle,
/// unless edges enter through the top there, and then in the stretch between them that is
/// nearest the middle, or the widest if none has room.
fn title_x(c: &CNode, width: f64, crossings: &Crossings) -> f64 {
    let xs = crossings.at(c.y - c.h / 2.0);
    let Some(xs) = xs.filter(|_| width != 0.0) else {
        return c.x;
    };
    let left = c.x - c.w / 2.0 + 6.0;
    let right = c.x + c.w / 2.0 - 6.0;
    let mut cuts: Vec<f64> = xs
        .iter()
        .copied()
        .filter(|&x| x > left && x < right)
        .collect();
    cuts.sort_by(|a, b| by(a - b));
    cuts.push(right);
    let need = width / 2.0 + 6.0;
    let (mut best, mut best_off, mut widest, mut from) = (c.x, f64::INFINITY, 0.0, left);
    for cut in cuts {
        if cut - from >= 2.0 * need {
            let x = min(max(c.x, from + need), cut - need);
            if (x - c.x).abs() < best_off {
                best_off = (x - c.x).abs();
                best = x;
            }
        } else if best_off == f64::INFINITY && cut - from > widest {
            widest = cut - from;
            best = (from + cut) / 2.0;
        }
        from = cut;
    }
    best
}

/// The items of a label whose lines start at left, about cy, inside a node whose middle is
/// x, y, as Pele's leftLabel draws it: the label centered on the middle of its line where it
/// has one, and otherwise one label for each line, centered on the middle of that line. left
/// and cy are about the node's middle, as Pele works them out, and each point is moved to the
/// node last, as Pele's SVG moves it. Each label is drawn as a copy of it.
fn left_label(l: &Label, x: f64, y: f64, left: f64, cy: f64, it: &Item) -> Vec<Item> {
    if l.lines.len() < 2 {
        let mut it = it.clone();
        let at = Point::new(x + (left + l.width / 2.0), y + cy);
        it.label = Some(Placed::new(l.clone(), at));
        return vec![it];
    }
    (0..l.lines.len())
        .map(|i| {
            // A line on its own, as wide as it is.
            let line = Label {
                lines: l.lines[i..i + 1].to_vec(),
                widths: l.widths[i..i + 1].to_vec(),
                width: l.widths[i],
                height: l.line_height,
                size: l.size,
                line_height: l.line_height,
            };
            let at = Point::new(
                x + (left + l.widths[i] / 2.0),
                y + (cy - l.height / 2.0 + (i as f64 + 0.5) * l.line_height),
            );
            let mut it = it.clone();
            it.label = Some(Placed::new(line, at));
            it
        })
        .collect()
}

/// Lays out a parsed use case diagram as a scene, as Pele's renderUsecase draws it: as its
/// direction says, and, where that runs across and does not fit the host's width, down
/// instead, if that is narrower.
pub(crate) fn layout(m: &UsecaseModel, config: &Map, o: &Options) -> Scene {
    if !runs_across(Some(m.direction)) {
        return draw(m, config, o, false);
    }
    turn_to_fit(o, |down| draw(m, config, o, down))
}

/// Lays the diagram out once, as Pele's draw does. turned lays out a diagram that runs
/// across as one that runs down.
fn draw(m: &UsecaseModel, config: &Map, o: &Options, turned: bool) -> Scene {
    let graph = build_usecase_graph(m);
    let size = o.font_size;
    let small = round(size * 0.875);
    let tiny = round(size * 0.8125);
    let set = setting(o);
    let pad = o.pad();
    let root_dir = if turned {
        Dir::TB
    } else {
        direction(Some(m.direction))
    };
    let sideways = matches!(root_dir, Dir::LR | Dir::RL);

    // The classes each element's author gave it, which the scene holds apart from those
    // Pele's classes say the element is.
    let classes_of = |id: &str| -> Vec<String> {
        let own = m
            .actors
            .iter()
            .find(|a| a.id == id)
            .map(|a| &a.classes)
            .or_else(|| m.use_cases.iter().find(|u| u.id == id).map(|u| &u.classes))
            .or_else(|| m.json_nodes.iter().find(|j| j.id == id).map(|j| &j.classes));
        class_names(&own.map_or(String::new(), |c| c.join(" ")))
    };
    let boundary_classes = |id: &str| -> Vec<String> {
        let own = m
            .system_boundaries
            .iter()
            .find(|b| b.id == id)
            .map(|b| &b.classes);
        class_names(&own.map_or(String::new(), |c| c.join(" ")))
    };

    let mut index = std::collections::HashMap::new();
    for (i, n) in graph.nodes.iter().enumerate() {
        index.insert(n.id.as_str(), i);
    }
    let mut views: Vec<View> = Vec::with_capacity(graph.nodes.len());
    let mut cnodes: Vec<CNode> = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let mut decls = node.css_compiled_styles.clone();
        decls.extend(node.css_styles.iter().cloned());
        let author = declarations(&decls);
        let mut base = Face::default();
        if author.bold {
            base |= Face::BOLD;
        }
        if author.italic {
            base |= Face::ITALIC;
        }
        let font_size = author.font_size.unwrap_or(size);
        let markdown = node.label_type == LabelType::Markdown;
        let stereotype = match node.stereotype.as_deref() {
            Some(s) if !s.is_empty() => text(&format!("«{s}»"), false, &set, tiny, 200.0, base),
            _ => Label::default(),
        };
        let mut v = View {
            node,
            kind: NodeKind::Box,
            author,
            decls,
            label: stereotype.clone(),
            stereotype: stereotype.clone(),
            w: 0.0,
            h: 0.0,
            rows: Vec::new(),
            key_w: 0.0,
            head_h: 0.0,
        };
        let mut c;
        if node.is_group {
            v.kind = NodeKind::Boundary;
            v.label = text(&node.label, markdown, &set, font_size, 4000.0, base);
            let title_h = v.label.height;
            c = cnode(0.0, 0.0, -1);
            c.is_group = true;
            c.pad_x = GROUP_PAD;
            c.pad_bottom = GROUP_PAD;
            c.pad_top = if node.boundary_type == Some("package") {
                title_h + 2.0 * TAB_PAD + 16.0
            } else if title_h > 0.0 {
                title_h + 16.0
            } else {
                GROUP_PAD
            };
            c.min_w = v.label.width + 2.0 * GROUP_PAD;
        } else {
            if node.shape == "usecaseJsonTable" {
                v.kind = NodeKind::Json;
                let mut bold = base;
                bold |= Face::BOLD;
                v.label = text(&node.label, false, &set, font_size, 4000.0, bold);
                let mut value_w = 0.0;
                for row in node.json_rows.iter().flatten() {
                    let key = text(&row.key, false, &set, small, 4000.0, base);
                    let value = text(&row.value, false, &set, small, 260.0, base);
                    v.key_w = max(v.key_w, key.width);
                    value_w = max(value_w, value.width);
                    v.h += max(max(key.height, value.height), round(small * 1.5)) + ROW_PAD;
                    v.rows.push((key, value));
                }
                // The root of an empty object has a value and no key, and then there is no
                // key column.
                if v.key_w > 0.0 {
                    v.key_w += 2.0 * CELL_PAD;
                }
                v.head_h = max(v.label.height, round(font_size * 1.5)) + ROW_PAD;
                let rows_w = if v.rows.is_empty() {
                    0.0
                } else {
                    v.key_w + value_w + 2.0 * CELL_PAD
                };
                v.w = max(max(v.label.width + 2.0 * CELL_PAD, rows_w), 72.0);
                v.h += v.head_h;
            } else if node.actor_type.is_some_and(|t| !t.is_empty()) {
                v.kind = NodeKind::Actor;
                v.label = text(&node.label, markdown, &set, font_size, 140.0, base);
                v.w = max(max(FIGURE_W, v.label.width), stereotype.width);
                v.h = FIGURE_H + FIGURE_GAP + stereotype.height + v.label.height;
            } else {
                let label_size = if node.shape == "note" {
                    small
                } else {
                    font_size
                };
                v.label = text(&node.label, markdown, &set, label_size, 180.0, base);
                let tw = max(v.label.width, stereotype.width);
                let th = v.label.height + stereotype.height;
                if node.shape == "usecaseEllipse" || node.shape == "usecaseBusiness" {
                    v.kind = NodeKind::Ellipse;
                    // An oval needs more room around its text than a box, and a business oval
                    // room for its slash.
                    let extra = if node.business == Some(true) {
                        16.0
                    } else {
                        4.0
                    };
                    v.w = max(tw + 2.0 * node.padding + extra, 88.0);
                    v.h = max(th + 28.0, 48.0);
                } else if node.shape == "note" {
                    v.w = tw + 2.0 * node.padding + 6.0;
                    v.h = th + 18.0;
                } else {
                    v.w = max(tw + 2.0 * node.padding + 12.0, 80.0);
                    v.h = max(th + 24.0, 44.0);
                }
            }
            c = cnode(v.w, v.h, -1);
        }
        c.seq = views.len() as f64;
        views.push(v);
        cnodes.push(c);
    }
    for (i, node) in graph.nodes.iter().enumerate() {
        cnodes[i].parent = node
            .parent_id
            .as_deref()
            .map_or(-1, |p| index.get(p).map_or(-1, |&i| i as i32));
    }

    let mut cedges: Vec<CEdge> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    // The edges drawn, with the classes of the relationship each is, where it is one.
    let mut drawn = Vec::new();
    for (k, edge) in graph.edges.iter().enumerate() {
        let (Some(&src), Some(&dst)) =
            (index.get(edge.start.as_str()), index.get(edge.end.as_str()))
        else {
            continue;
        };
        let semantic = edge.relationship_type == "include" || edge.relationship_type == "extend";
        let raw = edge.label.as_deref().unwrap_or("");
        let raw = if semantic {
            format!("«{raw}»")
        } else {
            raw.to_string()
        };
        let markdown = edge.label_type == Some(LabelType::Markdown);
        let label = text(&raw, markdown, &set, small, 200.0, Face::default());
        if src == dst {
            // A self-relationship loops out across the flow, and the node's box grows to hold
            // it.
            if sideways {
                let room = if label.height > 0.0 {
                    label.height + 8.0
                } else {
                    0.0
                };
                cnodes[src].h += 2.0 * (LOOP + room);
            } else {
                let room = if label.width > 0.0 {
                    label.width + 12.0
                } else {
                    0.0
                };
                cnodes[src].w += 2.0 * (LOOP + room);
            }
        }
        let classes = m
            .relationships
            .get(k)
            .map_or(Vec::new(), |r| r.classes.clone());
        drawn.push((edge, classes));
        cedges.push(CEdge {
            src,
            dst,
            minlen: edge.minlen,
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

    // The host may ask for edges at right angles, or straight, as it may of a flowchart.
    let curve = routing_curve(o.routing);
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

    let title = title_label(m.title.as_deref(), &set, size, 4000.0);
    let title_height = if title.height > 0.0 {
        title.height + 12.0
    } else {
        0.0
    };
    let inner = max(result.width, title.width);
    let ox = pad + (inner - result.width) / 2.0;
    let oy = pad + title_height;
    shift_layout(&mut cnodes, &mut cedges, ox, oy);
    let nonempty = |s: &str| (!s.is_empty()).then(|| s.to_string());
    let mut sc = Scene {
        diagram: "usecase".into(),
        width: (inner + 2.0 * pad).ceil(),
        height: (result.height + title_height + 2.0 * pad).ceil(),
        font_size: size,
        title: nonempty(&m.acc_title),
        desc: nonempty(&m.acc_descr),
        items: Vec::new(),
    };

    // Where edges cross each boundary's top border, so that a title can keep out of their
    // way.
    let mut crossings = Crossings::default();
    for e in &cedges {
        mark_crossings(&mut crossings, &e.route);
    }
    attach_ends(&mut cedges, &cnodes, &views);

    let (mut titles, mut clusters, mut casings) = (Vec::new(), Vec::new(), Vec::new());
    let (mut edges, mut labels, mut nodes) = (Vec::new(), Vec::new(), Vec::new());
    // The boundaries whose authors paint them a fill, which relations inside them are drawn
    // on, as Pele's fills.
    let mut fills: Vec<Fill> = Vec::new();
    if !title.is_empty() {
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        let at = Point::new(sc.width / 2.0, pad + title.height / 2.0);
        it.label = Some(Placed::new(title, at));
        titles.push(it);
    }
    for (i, v) in views.iter().enumerate() {
        let c = &cnodes[i];
        let n = v.node;
        let (x, y) = (c.x, c.y);
        let resolved = resolve_style(&v.decls);
        // The texts are set on the node's fill, in the color that stands out on a fill the
        // author gives it, as Pele's inkFill picks it: its label on its own fill, and the rest
        // on the fill under them.
        let inks = resolved.inks_text(&resolved);
        let ground = if inks { v.author.style.fill } else { None };
        if v.kind == NodeKind::Boundary {
            let (left, top) = (x - c.w / 2.0, y - c.h / 2.0);
            if resolved.paints() {
                fills.push(Fill {
                    bounds: [left, top, left + c.w, top + c.h],
                    decls: v.decls.clone(),
                });
            }
            let bounds = Rect::new(left, top, c.w, c.h);
            let mut it = Item::new(Kind::Group, ROLE_SYSTEM);
            it.shape = Some(Shape::new("rect"));
            it.id = Some(n.id.clone());
            it.class = boundary_classes(&n.id);
            it.bounds = bounds;
            it.style = Style {
                fill: Some(Paint::SURFACE_ALT),
                stroke: Some(Paint::BORDER),
                text: Some(Paint::MUTED),
                on_fill: inks,
                // A boundary is drawn at half its opacity, unless its author sets a fill, as
                // Pele's setsFill tests.
                fill_opacity: if resolved.fill_declared() {
                    None
                } else {
                    Some(0.5)
                },
                ..Style::default()
            };
            it.author = v.author.style.clone();
            let at = if n.boundary_type == Some("package") {
                it.shape = Some(Shape::new("package"));
                let tab_w = min(v.label.width + 2.0 * TAB_PAD + 8.0, c.w);
                let tab_h = v.label.height + 2.0 * TAB_PAD;
                it.parts = vec![
                    Part::rect(
                        Rect::new(left, top, tab_w, tab_h),
                        false,
                        Painting::FillAndStroke,
                    ),
                    Part::rect(
                        Rect::new(left, top + tab_h, c.w, c.h - tab_h),
                        false,
                        Painting::FillAndStroke,
                    ),
                ];
                Point::new(left + tab_w / 2.0, top + tab_h / 2.0)
            } else {
                it.parts = vec![Part::rect(bounds, true, Painting::FillAndStroke)];
                Point::new(
                    title_x(c, v.label.width, &crossings),
                    top + 8.0 + v.label.height / 2.0,
                )
            };
            if !v.label.is_empty() {
                it.label = Some(Placed::new(v.label.clone(), at));
            }
            clusters.push(it);
            continue;
        }

        let (w, h) = (v.w, v.h);
        let mut it = Item::new(Kind::Node, ROLE_USECASE);
        it.id = Some(n.id.clone());
        it.class = classes_of(&n.id);
        it.bounds = Rect::new(x - w / 2.0, y - h / 2.0, w, h);
        it.style = Style {
            fill: Some(Paint::SURFACE),
            stroke: Some(Paint::BORDER),
            on_fill: inks,
            ..Style::default()
        };
        it.author = v.author.style.clone();
        let mut stereotype = None;
        let mut cells = Vec::new();
        let label_at;
        match v.kind {
            NodeKind::Actor => {
                it.role = ROLE_ACTOR.into();
                let kind = n.actor_type.unwrap_or("normal");
                let mut shape = String::from("actor");
                if kind != "normal" {
                    shape.push('-');
                    shape.push_str(kind);
                }
                if n.business == Some(true) {
                    shape.push_str("-business");
                }
                it.shape = Some(Shape::from(shape));
                if kind == "awesome" {
                    it.style.fill = Some(Paint::BORDER);
                }
                // An actor's texts lie under its figure, not on its fill, and keep their
                // colors, as Pele draws them.
                it.style.on_fill = false;
                let top = -h / 2.0;
                (it.parts, it.picture) = figure(v, x, y + (top + FIGURE_H / 2.0));
                if kind == "hollow" {
                    // Pele turns the corners of a hollow body round. Its head and a business
                    // actor's slash, a circle and a single stroke, have no corners to turn.
                    it.style.join = Some(Join::Round);
                }
                if !v.stereotype.is_empty() {
                    let mut s = Item::new(Kind::Text, ROLE_STEREOTYPE);
                    s.id = Some(n.id.clone());
                    let at = Point::new(
                        x,
                        y + (top + FIGURE_H + FIGURE_GAP + v.stereotype.height / 2.0),
                    );
                    s.label = Some(Placed::new(v.stereotype.clone(), at));
                    s.style.text = Some(Paint::MUTED);
                    stereotype = Some(s);
                }
                label_at = Point::new(x, y + (h / 2.0 - v.label.height / 2.0));
            }
            NodeKind::Json => {
                it.role = ROLE_JSON.into();
                it.shape = Some(Shape::new("table"));
                let x0 = -w / 2.0;
                let mut ry = -h / 2.0 + v.head_h;
                let mut lines = Path::new();
                lines.move_to(x + x0, y + ry).line_to(x + (x0 + w), y + ry);
                if v.key_w > 0.0 {
                    lines
                        .move_to(x + (x0 + v.key_w), y + ry)
                        .line_to(x + (x0 + v.key_w), y + h / 2.0);
                }
                it.parts = vec![
                    Part::rect(it.bounds, true, Painting::FillAndStroke),
                    Part::path(lines, Painting::StrokeOnly),
                ];
                label_at = Point::new(x, y + (-h / 2.0 + v.head_h / 2.0));
                let mut value_item = Item::new(Kind::Text, ROLE_JSON_VALUE);
                value_item.id = Some(n.id.clone());
                value_item.style.text_ground = ground;
                value_item.author.text = v.author.style.text;
                for (key, value) in &v.rows {
                    let row_h = max(max(key.height, value.height), round(small * 1.5)) + ROW_PAD;
                    let mut k = Item::new(Kind::Text, ROLE_JSON_KEY);
                    k.id = Some(n.id.clone());
                    let at = Point::new(
                        x + (x0 + CELL_PAD + key.width / 2.0),
                        y + (ry + row_h / 2.0),
                    );
                    k.label = Some(Placed::new(key.clone(), at));
                    k.style.text = Some(Paint::MUTED);
                    k.style.text_ground = ground;
                    cells.push(k);
                    cells.extend(left_label(
                        value,
                        x,
                        y,
                        x0 + v.key_w + CELL_PAD,
                        ry + row_h / 2.0,
                        &value_item,
                    ));
                    ry += row_h;
                }
            }
            _ => {
                if v.kind == NodeKind::Ellipse {
                    it.shape = Some(Shape::new("ellipse"));
                    let mut oval = Path::new();
                    oval.ellipse(x, y, w / 2.0, h / 2.0);
                    it.parts = vec![Part::path(oval, Painting::FillAndStroke)];
                    if n.business == Some(true) {
                        it.shape = Some(Shape::new("ellipse-business"));
                        // The slash of a business use case, cutting off the right end of the
                        // oval beside the text.
                        let (from, to) = (w / 2.0 - 15.0, w / 2.0 - 5.0);
                        let arc = |at: f64| {
                            let q = at / (w / 2.0);
                            (h / 2.0) * (1.0 - q * q).sqrt()
                        };
                        let mut slash = Path::new();
                        slash
                            .move_to(x + from, y + arc(from))
                            .line_to(x + to, y + -arc(to));
                        it.parts.push(Part::path(slash, Painting::StrokeOnly));
                    }
                } else if n.shape == "note" {
                    it.role = ROLE_NOTE.into();
                    it.shape = Some(Shape::new("note"));
                    it.style.fill = Some(Paint::SURFACE_ALT);
                    // Pele turns the corners of the note and of its fold round.
                    it.style.join = Some(Join::Round);
                    let (r, b) = (w / 2.0, h / 2.0);
                    let mut outline = Path::new();
                    outline.polygon(&[
                        (x - r, y - b),
                        (x + (r - FOLD), y - b),
                        (x + r, y + (FOLD - b)),
                        (x + r, y + b),
                        (x - r, y + b),
                    ]);
                    let mut corner = Path::new();
                    corner
                        .move_to(x + (r - FOLD), y - b)
                        .line_to(x + (r - FOLD), y + (FOLD - b))
                        .line_to(x + r, y + (FOLD - b));
                    it.parts = vec![
                        Part::path(outline, Painting::FillAndStroke),
                        Part::path(corner, Painting::StrokeOnly),
                    ];
                } else {
                    it.shape = Some(Shape::new("rect"));
                    it.parts = vec![Part::rect(it.bounds, true, Painting::FillAndStroke)];
                }
                if !v.stereotype.is_empty() {
                    let mut s = Item::new(Kind::Text, ROLE_STEREOTYPE);
                    s.id = Some(n.id.clone());
                    let at = Point::new(x, y + (-v.label.height / 2.0));
                    s.label = Some(Placed::new(v.stereotype.clone(), at));
                    s.style.text = Some(Paint::MUTED);
                    s.style.text_ground = ground;
                    stereotype = Some(s);
                }
                label_at = Point::new(x, y + v.stereotype.height / 2.0);
            }
        }
        if !v.label.is_empty() {
            it.label = Some(Placed::new(v.label.clone(), label_at));
        }
        nodes.push(it);
        nodes.extend(stereotype);
        nodes.extend(cells);
    }

    let mut loop_seen = vec![0usize; views.len()];
    for i in 0..cedges.len() {
        let (edge, classes) = &drawn[i];
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
        let (start, end) = (edge.arrow_type_start, edge.arrow_type_end);
        let mut route = Vec::new();
        let ce = &cedges[i];
        let path = if ce.src == ce.dst {
            let c = &cnodes[ce.src];
            let v = &views[ce.src];
            let k = loop_seen[ce.src] as f64;
            loop_seen[ce.src] += 1;
            let side = if sideways { 2 } else { 1 };
            let spread = min(side_length(v, side) / 2.0, 8.0 + k * 6.0);
            let reach = LOOP + k * 8.0;
            let (ax, ay) = anchor(v, c, side, -spread);
            let (bx, by) = anchor(v, c, side, spread);
            let len = or1(hypot(reach, spread));
            let (ux, uy) = if sideways {
                (spread / len, reach / len)
            } else {
                (reach / len, spread / len)
            };
            let (st, et) = (trim(start), trim(end));
            let (sx, sy) = if sideways {
                (ax - ux * st, ay + uy * st)
            } else {
                (ax + ux * st, ay - uy * st)
            };
            let (ex, ey) = (bx + ux * et, by + uy * et);
            // Both ends leave the node leaning away from each other, and the curve joins them
            // outside it.
            let out = |x: f64, y: f64, sign: f64| {
                if sideways {
                    (x + sign * spread, y + reach)
                } else {
                    (x + reach, y + sign * spread)
                }
            };
            let ((c1x, c1y), (c2x, c2y)) = (out(ax, ay, -1.0), out(bx, by, 1.0));
            let mut p = Path::new();
            p.move_to(sx, sy).cube_to(c1x, c1y, c2x, c2y, ex, ey);
            let (sdx, sdy) = if sideways { (ux, -uy) } else { (-ux, uy) };
            let path = EdgePath {
                path: p,
                start: Tip {
                    at: Point::new(ax, ay),
                    dx: sdx,
                    dy: sdy,
                },
                end: Tip {
                    at: Point::new(bx, by),
                    dx: -ux,
                    dy: -uy,
                },
            };
            let (lx, ly) = if sideways {
                (c.x, max(ay, by) + reach * 0.75 + 6.0 + label.height / 2.0)
            } else {
                (max(ax, bx) + reach * 0.75 + 8.0 + label.width / 2.0, c.y)
            };
            (cedges[i].label_x, cedges[i].label_y) = (lx, ly);
            path
        } else {
            if ce.route.len() < 6 {
                continue;
            }
            route = ce.route.clone();
            let (st, et) = (trim(start), trim(end));
            route_path(
                &route,
                Curve::of(curve.unwrap_or("")),
                st,
                et,
                own_reach(st),
                own_reach(et),
            )
        };
        // A line ends round, as Pele draws it in the group of edges, which sets round caps.
        let mut own = Style {
            stroke: Some(Paint::LINE),
            fill: Some(Paint::NONE),
            cap: Some(Cap::Round),
            ..Style::default()
        };
        own.dash = if edge.internal {
            Some(vec![2.0, 3.0])
        } else if edge.pattern == "dotted" {
            Some(vec![5.0, 4.0])
        } else if edge.animate {
            Some(vec![9.0, 5.0])
        } else {
            None
        };
        let role = if edge.internal {
            ROLE_NOTE_LINK
        } else {
            ROLE_RELATION
        };
        // A relation inside one fill is drawn in the color of its labels, with its open
        // markers filled with the fill, and one that crosses the border of a fill is laid on
        // a casing, as Pele's fillUnder and edgeInk find.
        let ends = path.ends(marker_type(start), marker_type(end));
        let tips = [
            path.start.at.x,
            path.start.at.y,
            path.end.at.x,
            path.end.at.y,
        ];
        let (ground, cased) = on_fills(&path.path, &tips, &fills, &line);
        own.ground = ground;
        if cased && let Some(mut c) = casing(ROLE_CASING, &path.path, &ends, &decls, false) {
            c.id = Some(edge.id.clone());
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
        let mut it = Item::new(
            Kind::Edge(Edge {
                route: points,
                ends,
            }),
            role,
        );
        it.shape = Some(Shape::new(edge.relationship_type));
        it.id = Some(edge.id.clone());
        it.class = class_names(&classes.join(" "));
        it.bounds = path.path.bounds();
        it.parts = vec![Part::path(path.path, Painting::StrokeOnly)];
        it.style = own;
        it.author = author.clone();
        edges.push(it);
        if label.width > 0.0 {
            let (x, y) = (cedges[i].label_x, cedges[i].label_y);
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
            if edge.relationship_type == "include" || edge.relationship_type == "extend" {
                it.style.text = Some(Paint::MUTED);
            }
            it.author = Style {
                text: author.text,
                opacity: author.opacity,
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
