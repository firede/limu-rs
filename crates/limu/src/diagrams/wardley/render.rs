//! A Wardley map laid out as a scene, as Pele's renderWardley in render.ts draws it.

use std::collections::{HashMap, HashSet};
use std::f64::consts::{PI, SQRT_2};

use super::builder::WardleyNode;
use super::model::WardleyModel;
use crate::common::{self, fit_width};
use crate::js::math::{by, cos, hypot, max, min, round, sin};
use crate::js::number::to_string;
use crate::layout::Options;
use crate::scene::style::{Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{
    Align, Edge, Item, Kind, Marker, MarkerType, Painting, Part, Path, Placed, Point, Rect, Scene,
    Shape,
};
use crate::text::{Setting, layout_label};
use crate::value::{Map, Value};

/// The roles of the items of a Wardley map, in the order Pele draws them: its title; the
/// dashed line between two stages of evolution, each pair of grid lines, and the axes; the
/// name of each stage and the titles of the axes, the one of the visibility axis turned; the
/// box of a pipeline, its id its parent's, and the dashed line through its components; a
/// link, its id its ends' ids joined by a hyphen, and its label; the dashed arrow to where a
/// component is evolving and the circle there, their ids the component's; for each node, a
/// node of its class, an anchor, a component, a pipeline's parent or a pipeline's component,
/// with its shape and its sourcing strategy as its tag, an inertia bar when it resists
/// change, and its label, all with the node's id; each numbered circle of an annotation, the
/// box that lists their texts and each line of it; each note; and the arrows of
/// accelerators and deaccelerators, their ids their names.
pub(crate) const ROLE_TITLE: &str = "wardley.title";
pub(crate) const ROLE_STAGE: &str = "wardley.stage";
pub(crate) const ROLE_GRID: &str = "wardley.grid";
pub(crate) const ROLE_AXIS: &str = "wardley.axis";
pub(crate) const ROLE_STAGE_LABEL: &str = "wardley.stage-label";
pub(crate) const ROLE_AXIS_LABEL: &str = "wardley.axis-label";
pub(crate) const ROLE_PIPELINE: &str = "wardley.pipeline";
pub(crate) const ROLE_PIPELINE_LINE: &str = "wardley.pipeline-line";
pub(crate) const ROLE_LINK: &str = "wardley.link";
pub(crate) const ROLE_LINK_LABEL: &str = "wardley.link-label";
pub(crate) const ROLE_TREND: &str = "wardley.trend";
pub(crate) const ROLE_TREND_TARGET: &str = "wardley.trend-target";
pub(crate) const ROLE_ANCHOR: &str = "wardley.anchor";
pub(crate) const ROLE_COMPONENT: &str = "wardley.component";
pub(crate) const ROLE_PIPELINE_PARENT: &str = "wardley.pipeline-parent";
pub(crate) const ROLE_PIPELINE_COMPONENT: &str = "wardley.pipeline-component";
pub(crate) const ROLE_INERTIA: &str = "wardley.inertia";
pub(crate) const ROLE_LABEL: &str = "wardley.label";
pub(crate) const ROLE_ANNOTATION: &str = "wardley.annotation";
pub(crate) const ROLE_ANNOTATIONS: &str = "wardley.annotations";
pub(crate) const ROLE_ANNOTATION_TEXT: &str = "wardley.annotation-text";
pub(crate) const ROLE_NOTE: &str = "wardley.note";
pub(crate) const ROLE_ACCELERATOR: &str = "wardley.accelerator";
pub(crate) const ROLE_DEACCELERATOR: &str = "wardley.deaccelerator";

const DEFAULT_STAGES: [&str; 4] = ["Genesis", "Custom Built", "Product", "Commodity"];
// The measures of Pele's render.ts. Mermaid's canvas has CANVAS_MARGIN of room around the
// plot on every side.
const CANVAS_MARGIN: f64 = 48.0;
const R: f64 = 6.0;
const SQUARE: f64 = 10.0;
const LABEL_OFFSET: f64 = 8.0;
const PIPELINE_PAD: f64 = 15.0;
const FORCE_WIDTH: f64 = 60.0;
const FORCE_HEIGHT: f64 = 30.0;
const FORCE_HEAD: f64 = 20.0;

/// The marker of links and trends.
const ARROW: MarkerType = MarkerType::ArrowPoint;

/// A node and where it is drawn, Pele's Placed: the node by its index in the model.
struct At {
    x: f64,
    y: f64,
    node: usize,
}

/// A number of the map's own section of the configuration, between lo and hi, or fallback,
/// as Pele's setting reads it.
fn setting(config: &Map, key: &str, fallback: f64, lo: f64, hi: f64) -> f64 {
    let value = config
        .get("wardley-beta")
        .and_then(Value::as_object)
        .and_then(|s| s.get(key));
    match value {
        Some(&Value::Number(v)) if v >= lo && v <= hi => v,
        _ => fallback,
    }
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

/// The width or the height of the plot, as Pele's dimension works it out: the text's size,
/// or the configuration's, or the default, held between 200 and 10,000, less the canvas's
/// margins.
fn dimension(config: &Map, given: Option<f64>, key: &str, fallback: f64) -> f64 {
    let value = match given {
        Some(v) if v.is_finite() => v,
        _ => setting(config, key, fallback, 1.0, 1e6),
    };
    clamp(value, 200.0, 10000.0) - 2.0 * CANVAS_MARGIN
}

/// Lays a Wardley map out as a scene, as Pele's renderWardley draws it: the plot as wide as
/// the map asks, or narrower to fit the host, with its stages under it and its axes' titles,
/// and on it the pipelines, links, trends, nodes, annotations, notes and forces.
pub(crate) fn layout(m: &WardleyModel, config: &Map, o: &Options) -> Scene {
    let natural = dimension(config, m.size.map(|s| s.0), "width", 900.0);
    fit_width(o, natural, 160.0, |plot_w, tight| {
        draw(m, config, o, plot_w, natural, tight)
    })
}

/// A circle as a path, as Pele's circle draws it.
fn circle(cx: f64, cy: f64, r: f64, paint: Painting) -> Part {
    let mut p = Path::new();
    p.ellipse(cx, cy, r, r);
    Part::path(p, paint)
}

fn circle_box(cx: f64, cy: f64, r: f64) -> Rect {
    Rect::new(cx - r, cy - r, 2.0 * r, 2.0 * r)
}

/// A line of a role, stroked only.
fn line(role: &'static str, p: Path, style: Style) -> Item {
    let mut it = Item::new(Kind::Mark, role);
    it.bounds = p.bounds();
    it.parts = vec![Part::path(p, Painting::StrokeOnly)];
    it.style = style;
    it
}

/// A straight edge of a role from one point to another, stroked only.
fn edge(role: &'static str, id: String, x1: f64, y1: f64, x2: f64, y2: f64, style: Style) -> Item {
    let mut p = Path::new();
    p.move_to(x1, y1).line_to(x2, y2);
    let route = vec![Point::new(x1, y1), Point::new(x2, y2)];
    let mut it = Item::new(
        Kind::Edge(Edge {
            route,
            ends: [None, None],
        }),
        role,
    );
    it.id = Some(id);
    it.bounds = p.bounds();
    it.parts = vec![Part::path(p, Painting::StrokeOnly)];
    it.style = style;
    it
}

fn stroke(paint: Paint) -> Style {
    Style {
        stroke: Some(paint),
        ..Style::default()
    }
}

fn dashed(paint: Paint, dash: f64, gap: f64) -> Style {
    Style {
        stroke: Some(paint),
        dash: Some(vec![dash, gap]),
        ..Style::default()
    }
}

fn filled(fill: Paint, line: Paint) -> Style {
    Style {
        fill: Some(fill),
        stroke: Some(line),
        ..Style::default()
    }
}

/// A label drawn in the weight of headings, measured in the weight it was laid out in, as
/// Pele's font-weight on the text draws it.
fn heading(mut l: Label) -> Label {
    for span in l.lines.iter_mut().flatten() {
        span.face |= Face::BOLD;
    }
    l
}

/// What Pele's draw keeps as it goes: the items of the plot, about its corner, and how far
/// they reach.
struct Plot {
    items: Vec<Item>,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl Plot {
    /// Pele's grow.
    fn grow(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        if x0 < self.min_x {
            self.min_x = x0;
        }
        if y0 < self.min_y {
            self.min_y = y0;
        }
        if x1 > self.max_x {
            self.max_x = x1;
        }
        if y1 > self.max_y {
            self.max_y = y1;
        }
    }

    /// A label of a role, unless it is empty.
    fn label(&mut self, role: &'static str, id: Option<&str>, p: Placed, style: Style) {
        if p.label.is_empty() {
            return;
        }
        let mut it = Item::new(Kind::Text, role);
        it.id = id.map(str::to_string);
        it.label = Some(p);
        it.style = style;
        self.items.push(it);
    }

    /// Places a label centered on cx, cy, as Pele's centered does.
    fn centered(
        &mut self,
        role: &'static str,
        id: Option<&str>,
        l: Label,
        cx: f64,
        cy: f64,
        style: Style,
    ) {
        self.grow(
            cx - l.width / 2.0,
            cy - l.height / 2.0,
            cx + l.width / 2.0,
            cy + l.height / 2.0,
        );
        self.label(role, id, Placed::new(l, Point::new(cx, cy)), style);
    }

    /// Places a label with its lines starting at x and its first baseline at y, as Pele's
    /// starting does. The scene centers it down on its point, which is then where Pele puts
    /// the middle of its lines.
    fn starting(&mut self, role: &'static str, id: Option<&str>, l: Label, x: f64, y: f64) {
        self.grow(x, y - l.size, x + l.width, y - l.size + l.height + 4.0);
        self.start(role, id, l, x, y);
    }

    /// A label with its lines starting at x and its first baseline at y, as Pele's startLabel
    /// writes it.
    fn start(&mut self, role: &'static str, id: Option<&str>, l: Label, x: f64, y: f64) {
        let at = y - l.size * 0.35 + l.height / 2.0 - l.line_height / 2.0;
        let p = Placed {
            align: Align::Start,
            ..Placed::new(l, Point::new(x, at))
        };
        self.label(role, id, p, Style::default());
    }
}

/// How far from a node's center a line to it stops, as Pele's reach finds it. An anchor's
/// label is laid out once, and kept.
struct Reach<'a> {
    nodes: &'a [WardleyNode],
    anchors: Vec<Option<Label>>,
    set: &'a Setting<'a>,
    small: f64,
    room: f64,
}

impl Reach<'_> {
    /// The label of the anchor at an index in the model, as Pele's anchorLabels keeps it.
    fn anchor(&mut self, node: usize) -> Label {
        let (set, small, room) = (self.set, self.small, self.room);
        let raw = &self.nodes[node].label;
        self.anchors[node]
            .get_or_insert_with(|| layout_label(raw, false, set, small, room, Face::BOLD))
            .clone()
    }

    fn reach(&mut self, at: &At, ux: f64, uy: f64) -> f64 {
        let nodes = self.nodes;
        let node = &nodes[at.node];
        if node.class_name == Some("anchor") {
            let small = self.small;
            let l = self.anchor(at.node);
            return 1.0 / hypot(ux / (l.width / 2.0 + 5.0), uy / (small * 0.8));
        }
        if node.is_pipeline_parent {
            return SQUARE / SQRT_2;
        }
        if node.source_strategy.is_some() {
            2.0 * R
        } else {
            R
        }
    }
}

/// Lays the map out with a plot plot_w wide, as Pele's draw does.
fn draw(
    m: &WardleyModel,
    config: &Map,
    o: &Options,
    plot_w: f64,
    natural_w: f64,
    tight: bool,
) -> Scene {
    let size = o.font_size;
    let small = round(size * 0.8125);
    let tiny = round(size * 0.6875);
    let set = common::setting(o);
    let pad = o.pad();

    // A narrower map is not as much shorter, so that components keep some room between them.
    let plot_h =
        dimension(config, m.size.map(|s| s.1), "height", 600.0) * max(0.75, plot_w / natural_w);
    let show_grid = config
        .get("wardley-beta")
        .and_then(Value::as_object)
        .and_then(|s| s.get("showGrid"))
        == Some(&Value::Bool(true));

    let mut plot = Plot {
        items: Vec::new(),
        min_x: 0.0,
        min_y: 0.0,
        max_x: plot_w,
        max_y: plot_h,
    };
    // The width Pele's text gives a label when it is not told one.
    let plain = if tight {
        max(64.0, min(120.0, plot_w / 2.0))
    } else {
        4000.0
    };
    let text = |raw: &str, font_size: f64, max_width: f64, face: Face| {
        layout_label(raw, false, &set, font_size, max_width, face)
    };
    let project_x = |value: f64| (value / 100.0) * plot_w;
    let project_y = |value: f64| plot_h - (value / 100.0) * plot_h;

    let stages: Vec<&str> = match &m.axes.stages {
        Some(stages) if !stages.is_empty() => stages.iter().map(String::as_str).collect(),
        _ => DEFAULT_STAGES.to_vec(),
    };
    let boundaries = m
        .axes
        .stage_boundaries
        .as_ref()
        .filter(|b| b.len() == stages.len());
    let count = stages.len() as f64;
    let mut stage_labels = Vec::new();
    let mut stage_bottom = plot_h;
    let mut previous = 0.0;
    for (index, stage) in stages.iter().enumerate() {
        let (start, end) = match boundaries {
            Some(b) => (previous, clamp(b[index], 0.0, 1.0)),
            None => (index as f64 / count, (index as f64 + 1.0) / count),
        };
        previous = end;
        if index > 0 {
            let mut p = Path::new();
            p.move_to(start * plot_w, 0.0)
                .line_to(start * plot_w, plot_h);
            plot.items
                .push(line(ROLE_STAGE, p, dashed(Paint::BORDER, 4.0, 4.0)));
        }
        let l = text(
            stage,
            small,
            max(48.0, (end - start).abs() * plot_w - 12.0),
            Face::default(),
        );
        let cx = ((start + end) / 2.0) * plot_w;
        let cy = plot_h + 8.0 + l.height / 2.0;
        plot.grow(
            cx - l.width / 2.0,
            cy - l.height / 2.0,
            cx + l.width / 2.0,
            cy + l.height / 2.0,
        );
        stage_bottom = max(stage_bottom, plot_h + 8.0 + l.height);
        if !l.is_empty() {
            let mut it = Item::new(Kind::Text, ROLE_STAGE_LABEL);
            it.label = Some(Placed::new(l, Point::new(cx, cy)));
            it.style.text = Some(Paint::MUTED);
            stage_labels.push(it);
        }
    }
    if show_grid {
        for i in 1..4 {
            let i = f64::from(i);
            let (x, y) = ((plot_w * i) / 4.0, (plot_h * i) / 4.0);
            let mut p = Path::new();
            p.move_to(x, 0.0)
                .line_to(x, plot_h)
                .move_to(0.0, y)
                .line_to(plot_w, y);
            plot.items
                .push(line(ROLE_GRID, p, dashed(Paint::BORDER, 2.0, 6.0)));
        }
    }
    let mut axis = Path::new();
    axis.move_to(0.0, 0.0)
        .line_to(0.0, plot_h)
        .line_to(plot_w, plot_h);
    plot.items.push(line(ROLE_AXIS, axis, stroke(Paint::LINE)));
    plot.items.extend(stage_labels);

    // The titles of the axes are measured in their own weight, and drawn in the weight of
    // headings.
    let x_label = m.axes.x_label.as_deref().unwrap_or("Evolution");
    let y_label = m.axes.y_label.as_deref().unwrap_or("Visibility");
    let x_title = heading(text(x_label, small, plain, Face::default()));
    let y_title = heading(text(y_label, small, plain, Face::default()));
    let x_title_height = x_title.height;
    let y_title_height = y_title.height;
    plot.centered(
        ROLE_AXIS_LABEL,
        None,
        x_title,
        plot_w / 2.0,
        stage_bottom + 6.0 + x_title_height / 2.0,
        Style::default(),
    );
    let turned = Placed {
        rotate: -90.0,
        ..Placed::new(y_title, Point::new(-16.0, plot_h / 2.0))
    };
    plot.label(ROLE_AXIS_LABEL, None, turned, Style::default());
    plot.grow(-16.0 - y_title_height / 2.0, 0.0, 0.0, 0.0);
    let legend_top = stage_bottom + 6.0 + x_title_height + 10.0;

    let mut positions: Vec<At> = m
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| At {
            x: project_x(node.x),
            y: project_y(node.y),
            node: i,
        })
        .collect();
    let by_id: HashMap<&str, usize> = m
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (node.id.as_str(), i))
        .collect();

    // Pipelines: a box around the components, with the parent as a square on its top edge.
    let mut members: HashMap<&str, HashSet<&str>> = HashMap::new();
    for pipeline in &m.pipelines {
        members.insert(
            &pipeline.node_id,
            pipeline.component_ids.iter().map(String::as_str).collect(),
        );
        let mut parts: Vec<(f64, f64)> = pipeline
            .component_ids
            .iter()
            .filter_map(|id| by_id.get(id.as_str()))
            .map(|&i| (positions[i].x, positions[i].y))
            .collect();
        if parts.is_empty() {
            continue;
        }
        parts.sort_by(|a, b| by(a.0 - b.0));
        let left = parts[0].0;
        let right = parts[parts.len() - 1].0;
        let y = parts[0].1;
        let top = y - 2.0 * R;
        if let Some(&parent) = by_id.get(pipeline.node_id.as_str()) {
            positions[parent].x = (left + right) / 2.0;
            positions[parent].y = top - SQUARE / 6.0;
        }
        plot.grow(
            left - PIPELINE_PAD,
            top,
            right + PIPELINE_PAD,
            top + 4.0 * R,
        );
        let bounds = Rect::new(
            left - PIPELINE_PAD,
            top,
            right - left + 2.0 * PIPELINE_PAD,
            4.0 * R,
        );
        let mut it = Item::new(Kind::Group, ROLE_PIPELINE);
        it.id = Some(pipeline.node_id.clone());
        it.bounds = bounds;
        it.parts = vec![Part::rect(bounds, true, Painting::FillAndStroke)];
        it.style = filled(Paint::SURFACE, Paint::BORDER);
        plot.items.push(it);
        if parts.len() > 1 {
            let mut p = Path::new();
            p.move_to(left, y).line_to(right, y);
            let mut it = line(ROLE_PIPELINE_LINE, p, dashed(Paint::LINE, 4.0, 4.0));
            it.id = Some(pipeline.node_id.clone());
            plot.items.push(it);
        }
    }

    let anchor_room = if tight {
        max(64.0, plot_w / 2.0)
    } else {
        4000.0
    };
    let mut reach = Reach {
        nodes: &m.nodes,
        anchors: vec![None; m.nodes.len()],
        set: &set,
        small,
        room: anchor_room,
    };

    // Quadrants around a node that a link runs through: 1 up and right, 2 down and right, 4
    // up and left, 8 down and left.
    let mut taken = vec![0u8; positions.len()];
    let mut occupy = |at: usize, ux: f64, uy: f64| {
        if ux.abs() < 0.25 || uy.abs() < 0.25 {
            return;
        }
        taken[at] |= match (ux > 0.0, uy < 0.0) {
            (true, true) => 1,
            (true, false) => 2,
            (false, true) => 4,
            (false, false) => 8,
        };
    };

    let trim = ARROW.trim();
    for link in &m.links {
        let (Some(&from), Some(&to)) = (
            by_id.get(link.source.as_str()),
            by_id.get(link.target.as_str()),
        ) else {
            continue;
        };
        if from == to {
            continue;
        }
        // A pipeline component belongs to its parent already.
        if members
            .get(link.target.as_str())
            .is_some_and(|set| set.contains(link.source.as_str()))
        {
            continue;
        }
        let (f, t) = (&positions[from], &positions[to]);
        let dx = t.x - f.x;
        let dy = t.y - f.y;
        let distance = hypot(dx, dy);
        if distance < 0.01 {
            continue;
        }
        let ux = dx / distance;
        let uy = dy / distance;
        let start = reach.reach(f, ux, uy);
        let end = reach.reach(t, ux, uy);
        if start + end >= distance {
            continue;
        }
        occupy(from, ux, uy);
        occupy(to, -ux, -uy);
        let mut x1 = f.x + ux * start;
        let mut y1 = f.y + uy * start;
        let mut x2 = t.x - ux * end;
        let mut y2 = t.y - uy * end;
        let mut ends = [None, None];
        if matches!(link.flow, Some("forward" | "bidirectional")) {
            ends[1] = Some(Marker {
                kind: ARROW,
                at: Point::new(x2, y2),
                dx: ux,
                dy: uy,
            });
            x2 -= ux * trim;
            y2 -= uy * trim;
        }
        if matches!(link.flow, Some("backward" | "bidirectional")) {
            ends[0] = Some(Marker {
                kind: ARROW,
                at: Point::new(x1, y1),
                dx: -ux,
                dy: -uy,
            });
            x1 += ux * trim;
            y1 += uy * trim;
        }
        let style = if link.dashed {
            dashed(Paint::LINE, 6.0, 6.0)
        } else {
            stroke(Paint::LINE)
        };
        let id = format!("{}-{}", link.source, link.target);
        let mut it = edge(ROLE_LINK, id.clone(), x1, y1, x2, y2, style);
        if let Kind::Edge(e) = &mut it.kind {
            e.ends = ends;
        }
        plot.items.push(it);
        if let Some(raw) = link.label.as_deref().filter(|l| !l.is_empty()) {
            let l = text(raw, tiny + 1.0, 200.0, Face::default());
            // Beside the middle of the line, on its upper side.
            let side = if ux >= 0.0 { 1.0 } else { -1.0 };
            let px = uy * side;
            let py = -ux * side;
            let away = 5.0 + (px.abs() * l.width) / 2.0 + (py.abs() * l.height) / 2.0;
            let muted = Style {
                text: Some(Paint::MUTED),
                ..Style::default()
            };
            plot.centered(
                ROLE_LINK_LABEL,
                Some(&id),
                l,
                (f.x + t.x) / 2.0 + px * away,
                (f.y + t.y) / 2.0 + py * away,
                muted,
            );
        }
    }

    // Evolution: a dashed arrow to where the component is heading, drawn as an open circle.
    for trend in &m.trends {
        let Some(&origin) = by_id.get(trend.node_id.as_str()) else {
            continue;
        };
        let origin = &positions[origin];
        let tx = project_x(trend.target_x);
        let ty = project_y(trend.target_y);
        let dx = tx - origin.x;
        let dy = ty - origin.y;
        let distance = hypot(dx, dy);
        let start = if distance > 0.0 {
            reach.reach(origin, dx / distance, dy / distance)
        } else {
            0.0
        };
        if distance > start + R + 4.0 {
            let ux = dx / distance;
            let uy = dy / distance;
            let ex = tx - ux * (R + 2.0);
            let ey = ty - uy * (R + 2.0);
            let mut it = edge(
                ROLE_TREND,
                trend.node_id.clone(),
                origin.x + ux * start,
                origin.y + uy * start,
                ex - ux * trim,
                ey - uy * trim,
                dashed(Paint::ACCENT, 4.0, 4.0),
            );
            if let Kind::Edge(e) = &mut it.kind {
                e.ends[1] = Some(Marker {
                    kind: ARROW,
                    at: Point::new(ex, ey),
                    dx: ux,
                    dy: uy,
                });
            }
            plot.items.push(it);
        }
        plot.grow(tx - R, ty - R, tx + R, ty + R);
        let mut it = Item::new(Kind::Mark, ROLE_TREND_TARGET);
        it.shape = Some(Shape::new("circle"));
        it.id = Some(trend.node_id.clone());
        it.bounds = circle_box(tx, ty, R);
        it.parts = vec![circle(tx, ty, R, Painting::FillAndStroke)];
        it.style = filled(Paint::BG, Paint::ACCENT);
        plot.items.push(it);
    }

    let tri = 1.2 * R;
    let dot = 0.7 * R;
    for (index, at) in positions.iter().enumerate() {
        let node = &m.nodes[at.node];
        let (x, y) = (at.x, at.y);
        let anchor = node.class_name == Some("anchor");
        let strategy = node.source_strategy.as_deref();
        let role = if anchor {
            ROLE_ANCHOR
        } else if node.is_pipeline_parent {
            ROLE_PIPELINE_PARENT
        } else if node.in_pipeline {
            ROLE_PIPELINE_COMPONENT
        } else {
            ROLE_COMPONENT
        };
        let ox = node.label_offset_x.map(|v| clamp(v, -5000.0, 5000.0));
        let oy = node.label_offset_y.map(|v| clamp(v, -5000.0, 5000.0));
        // An anchor is its label, unless it is a pipeline's parent too.
        let mut it = Item::new(Kind::Node, role);
        it.id = Some(node.id.clone());
        it.bounds = Rect::new(x, y, 0.0, 0.0);
        it.style = filled(Paint::BG, Paint::LINE);
        if anchor {
            let l = reach.anchor(at.node);
            let (cx, cy) = (x + ox.unwrap_or(0.0), y + oy.unwrap_or(-3.0));
            it.bounds = Rect::new(cx - l.width / 2.0, cy - l.height / 2.0, l.width, l.height);
        }
        // The shape on the node's own fill. A strategy's circle takes the fill, and what is on
        // it is hollow.
        let mut inner = Painting::FillAndStroke;
        if let Some(strategy) = strategy {
            it.tags = vec![strategy.to_string()];
            it.shape = Some(Shape::new("circle"));
            it.bounds = circle_box(x, y, 2.0 * R);
            match strategy {
                "outsource" => it.style.fill = Some(Paint::MUTED),
                "buy" => it.style.fill = Some(Paint::SURFACE_ALT),
                "build" => it.style.fill = Some(Paint::SURFACE),
                _ => {}
            }
            it.parts
                .push(circle(x, y, 2.0 * R, Painting::FillAndStroke));
            inner = Painting::Hollow;
        }
        if strategy == Some("market") {
            let bx = tri * cos(PI / 6.0);
            let by = y + tri * sin(PI / 6.0);
            it.shape = Some(Shape::new("market"));
            let mut p = Path::new();
            p.polygon(&[(x, y - tri), (x - bx, by), (x + bx, by)]);
            it.parts.push(Part::path(p, Painting::StrokeOnly));
            for (cx, cy) in [(x, y - tri), (x - bx, by), (x + bx, by)] {
                it.parts.push(circle(cx, cy, dot, Painting::Hollow));
            }
        } else if node.is_pipeline_parent {
            let bounds = Rect::new(x - SQUARE / 2.0, y - SQUARE / 2.0, SQUARE, SQUARE);
            if strategy.is_none() {
                it.shape = Some(Shape::new("square"));
                it.bounds = bounds;
            }
            it.parts.push(Part::rect(bounds, false, inner));
        } else if !anchor {
            if strategy.is_none() {
                it.shape = Some(Shape::new("circle"));
                it.bounds = circle_box(x, y, R);
            }
            it.parts.push(circle(x, y, R, inner));
        }
        plot.items.push(it);
        if node.inertia == Some(true) {
            let side = if node.is_pipeline_parent {
                SQUARE / 2.0
            } else {
                R
            };
            let ring = if strategy.is_some() { R + 4.0 } else { 0.0 };
            let offset = side + 10.0 + ring;
            let bounds = Rect::new(x + offset, y - R, 4.0, 2.0 * R);
            let mut it = Item::new(Kind::Mark, ROLE_INERTIA);
            it.id = Some(node.id.clone());
            it.bounds = bounds;
            it.parts = vec![Part::rect(bounds, false, Painting::FillOnly)];
            it.style.fill = Some(Paint::LINE);
            plot.items.push(it);
        }
        let id = Some(node.id.as_str());
        if anchor {
            let l = reach.anchor(at.node);
            let (cx, cy) = (x + ox.unwrap_or(0.0), y + oy.unwrap_or(-3.0));
            plot.centered(ROLE_LABEL, id, l, cx, cy, Style::default());
        } else {
            let l = text(&node.label, small, plain, Face::default());
            let lift = LABEL_OFFSET + if strategy.is_some() { 10.0 } else { 0.0 };
            if ox.is_some() || oy.is_some() {
                plot.starting(
                    ROLE_LABEL,
                    id,
                    l,
                    x + ox.unwrap_or(lift),
                    y + oy.unwrap_or(-lift),
                );
            } else if node.in_pipeline {
                let cy = y + 2.0 * R + 4.0 + l.height / 2.0;
                plot.centered(ROLE_LABEL, id, l, x, cy, Style::default());
            } else {
                // The label goes up and to the right unless a link leaves that way, and then
                // goes to the next free corner.
                let busy = taken[index];
                let corner = [1, 2, 4, 8]
                    .into_iter()
                    .find(|&c| busy & c == 0)
                    .unwrap_or(1);
                let right = corner == 1 || corner == 2;
                let up = corner == 1 || corner == 4;
                let lx = if right { x + lift } else { x - lift - l.width };
                let ly = if up {
                    y - lift
                } else {
                    y + lift + small * 0.75
                };
                plot.starting(ROLE_LABEL, id, l, lx, ly);
            }
        }
        plot.grow(x - 2.0 * R, y - 2.0 * R, x + 2.0 * R + 20.0, y + 2.0 * R);
    }

    for a in &m.annotations {
        let number = to_string(a.number);
        let l = text(&number, tiny, 4000.0, Face::BOLD);
        for &(cx, cy) in &a.coordinates {
            let x = project_x(cx);
            let y = project_y(cy);
            let r = max(10.0, l.width / 2.0 + 4.0);
            plot.grow(x - r, y - r, x + r, y + r);
            let mut it = Item::new(Kind::Node, ROLE_ANNOTATION);
            it.shape = Some(Shape::new("circle"));
            it.id = Some(number.clone());
            it.bounds = circle_box(x, y, r);
            it.parts = vec![circle(x, y, r, Painting::FillAndStroke)];
            it.style = filled(Paint::BG, Paint::LINE);
            if !l.is_empty() {
                it.label = Some(Placed::new(l.clone(), Point::new(x, y)));
            }
            plot.items.push(it);
        }
    }
    let mut listed: Vec<_> = m
        .annotations
        .iter()
        .filter(|a| !a.text.is_empty())
        .collect();
    listed.sort_by(|a, b| by(a.number - b.number));
    if !listed.is_empty() {
        let room = if tight {
            max(80.0, plot_w - 20.0)
        } else {
            320.0
        };
        let lines: Vec<Label> = listed
            .iter()
            .map(|a| {
                let raw = format!("{}. {}", to_string(a.number), a.text);
                text(&raw, small, room, Face::default())
            })
            .collect();
        let (mut width, mut height) = (0.0, 0.0);
        for l in &lines {
            width = max(width, l.width);
            height += l.height;
        }
        width += 20.0;
        height += 14.0;
        // Without a position the list goes under the map, where it covers nothing.
        let (bx, by) = match m.annotations_box {
            Some((x, y)) => (
                clamp(project_x(x), 0.0, max(0.0, plot_w - width)),
                clamp(project_y(y), 0.0, max(0.0, plot_h - height)),
            ),
            None => (0.0, legend_top),
        };
        plot.grow(bx, by, bx + width, by + height);
        let bounds = Rect::new(bx, by, width, height);
        let mut it = Item::new(Kind::Group, ROLE_ANNOTATIONS);
        it.bounds = bounds;
        it.parts = vec![Part::rect(bounds, true, Painting::FillAndStroke)];
        it.style = filled(Paint::BG, Paint::BORDER);
        plot.items.push(it);
        let mut y = by + 7.0;
        for l in lines {
            let baseline = y + l.line_height / 2.0 + small * 0.35;
            let height = l.height;
            plot.start(ROLE_ANNOTATION_TEXT, None, l, bx + 10.0, baseline);
            y += height;
        }
    }

    for note in &m.notes {
        let l = text(&note.text, small, min(360.0, plot_w * 0.6), Face::BOLD);
        plot.starting(ROLE_NOTE, None, l, project_x(note.x), project_y(note.y));
    }

    // Accelerators point right, deaccelerators left.
    let mut force = |name: &str, px: f64, py: f64, forward: bool| {
        let x = project_x(px);
        let y = project_y(py);
        let half = FORCE_HEIGHT / 2.0;
        let (tip, tail, neck, role) = if forward {
            (
                x + FORCE_WIDTH,
                x,
                x + FORCE_WIDTH - FORCE_HEAD,
                ROLE_ACCELERATOR,
            )
        } else {
            (x, x + FORCE_WIDTH, x + FORCE_HEAD, ROLE_DEACCELERATOR)
        };
        let mut p = Path::new();
        p.polygon(&[
            (tail, y - half),
            (neck, y - half),
            (neck, y - half - 8.0),
            (tip, y),
            (neck, y + half + 8.0),
            (neck, y + half),
            (tail, y + half),
        ]);
        plot.grow(x, y - half - 8.0, x + FORCE_WIDTH, y + half + 8.0);
        let at = Point::new(x + FORCE_WIDTH / 2.0, y + half + 8.0 + 12.0);
        let mut it = Item::new(Kind::Node, role);
        it.shape = Some(Shape::new("arrow"));
        it.id = Some(name.to_string());
        it.bounds = p.bounds();
        it.parts = vec![Part::path(p, Painting::FillAndStroke)];
        it.style = filled(Paint::SURFACE, Paint::LINE);
        let l = text(name, tiny + 1.0, 160.0, Face::BOLD);
        plot.grow(
            at.x - l.width / 2.0,
            at.y - l.height / 2.0,
            at.x + l.width / 2.0,
            at.y + l.height / 2.0,
        );
        if !l.is_empty() {
            it.label = Some(Placed::new(l, at));
        }
        plot.items.push(it);
    };
    for a in &m.accelerators {
        force(&a.name, a.x, a.y, true);
    }
    for d in &m.deaccelerators {
        force(&d.name, d.x, d.y, false);
    }

    let title_room = if tight { max(80.0, plot_w) } else { 4000.0 };
    let raw = m.title.as_deref().unwrap_or("");
    let title = layout_label(raw, false, &set, size, title_room, Face::BOLD);
    let title_height = if title.height > 0.0 {
        title.height + 12.0
    } else {
        0.0
    };
    let left = min(plot.min_x, plot_w / 2.0 - title.width / 2.0);
    let right = max(plot.max_x, plot_w / 2.0 + title.width / 2.0);
    let tx = pad - left;
    let ty = pad + title_height - plot.min_y;
    let mut items = Vec::with_capacity(plot.items.len() + 1);
    if !title.is_empty() {
        let at = Point::new(tx + plot_w / 2.0, pad + title.height / 2.0);
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        it.label = Some(Placed::new(title, at));
        items.push(it);
    }
    for mut it in plot.items {
        crate::items::move_by(&mut it, tx, ty);
        items.push(it);
    }
    Scene {
        diagram: "wardley".into(),
        width: (right - left + 2.0 * pad).ceil(),
        height: (plot.max_y - plot.min_y + title_height + 2.0 * pad).ceil(),
        font_size: size,
        title: m.acc_title.clone(),
        desc: m.acc_descr.clone(),
        items,
    }
}
