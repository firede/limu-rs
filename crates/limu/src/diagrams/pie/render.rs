//! A pie chart laid out as a scene, as Pele's renderPie in render.ts draws it.

use std::f64::consts::PI;

use super::PieModel;
use crate::common::{fit_width, line, setting, title_room};
use crate::js::math::{cos, max, round, sin};
use crate::js::number::{to_fixed, to_string};
use crate::layout::Options;
use crate::scene::style::{Join, Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{Item, Kind, Painting, Part, Path, Placed, Point, Rect, Scene, Shape};
use crate::text::layout_label;
use crate::value::{Map, Value};

/// The roles of the items of a pie chart: its title, a slice or the empty circle of a pie with
/// no slice to draw, a slice's percentage, on the slice in the background's color or outside
/// the rim in the text's, and an entry of the legend, its swatch with its label.
pub(crate) const ROLE_TITLE: &str = "pie.title";
pub(crate) const ROLE_SLICE: &str = "pie.slice";
pub(crate) const ROLE_SLICE_LABEL: &str = "pie.slice-label";
pub(crate) const ROLE_LEGEND: &str = "pie.legend";

// The measures of a pie chart, as Pele's render.ts has them.
const RADIUS: f64 = 120.0;
const SWATCH: f64 = 12.0;
const ROW: f64 = 26.0;
const GAP: f64 = 32.0;
const OUTSIDE: f64 = 28.0;

/// A setting of the pie's configuration, as Pele's option reads it: none where the
/// configuration has no pie section that is an object.
fn option<'a>(config: &'a Map, key: &str) -> Option<&'a Value> {
    config
        .get("pie")
        .and_then(Value::as_object)
        .and_then(|s| s.get(key))
}

/// Where an angle, clockwise from the top, meets a circle of a radius about the pie's center,
/// as Pele's point gives it, about that center.
fn point(angle: f64, radius: f64) -> (f64, f64) {
    (sin(angle) * radius, -cos(angle) * radius)
}

/// The outline of a slice from angle start to end, with an outer radius and an inner one for
/// a donut, about the center cx, cy, as Pele's slice draws it: a whole ring as two half arcs,
/// or a wedge from the center, or a piece of a ring.
fn slice(cx: f64, cy: f64, start: f64, end: f64, outer: f64, inner: f64) -> Path {
    let mut p = Path::new();
    if end - start >= PI * 2.0 - 1e-9 {
        let mut ring = |r: f64, sweep: bool| {
            p.move_to(cx, cy - r)
                .arc_to(r, r, 0.0, true, sweep, cx, cy + r)
                .arc_to(r, r, 0.0, true, sweep, cx, cy - r)
                .close();
        };
        ring(outer, true);
        if inner > 0.0 {
            ring(inner, false);
        }
        return p;
    }
    let large = end - start > PI;
    let at = |angle: f64, r: f64| {
        let (x, y) = point(angle, r);
        (cx + x, cy + y)
    };
    if inner <= 0.0 {
        let (sx, sy) = at(start, outer);
        let (ex, ey) = at(end, outer);
        p.move_to(cx, cy)
            .line_to(sx, sy)
            .arc_to(outer, outer, 0.0, large, true, ex, ey)
            .close();
        return p;
    }
    let (sx, sy) = at(start, outer);
    let (ex, ey) = at(end, outer);
    let (ix, iy) = at(end, inner);
    let (jx, jy) = at(start, inner);
    p.move_to(sx, sy)
        .arc_to(outer, outer, 0.0, large, true, ex, ey)
        .line_to(ix, iy)
        .arc_to(inner, inner, 0.0, large, false, jx, jy)
        .close();
    p
}

/// Lays a pie chart out as a scene, as Pele's renderPie draws it: the pie, its percentages on
/// the slices, or outside the rim for a slice too thin for its own, and a legend beside,
/// above, below or in the middle of it, as the configuration says. A pie that does not fit
/// the host's width is drawn smaller, with a legend at its side put under it.
pub(crate) fn layout(model: &PieModel, config: &Map, o: &Options) -> Scene {
    fit_width(o, RADIUS * 2.0, 140.0, |diameter, tight| {
        draw(model, config, o, diameter / 2.0, tight)
    })
}

/// A label placed outside the rim, about the pie's center, as Pele's placedOutside holds it.
struct Clearing {
    x: f64,
    y: f64,
    width: f64,
}

/// Lays the pie out with a radius, as Pele's draw does. tight is set where the chart does not
/// fit the width it has with its legend beside it.
fn draw(model: &PieModel, config: &Map, o: &Options, radius: f64, tight: bool) -> Scene {
    let size = o.font_size;
    let small = round(size * 0.875);
    let set = setting(o);
    let pad = o.padding;

    let text_position = match option(config, "textPosition") {
        Some(&Value::Number(v)) if (0.0..=1.0).contains(&v) => v,
        _ => 0.75,
    };
    let inner = match option(config, "donutHole") {
        Some(&Value::Number(v)) if v > 0.0 && v <= 0.9 => v * radius,
        _ => 0.0,
    };
    let placed = match option(config, "legendPosition") {
        None | Some(Value::Null) => "right".to_string(),
        Some(v) => v.to_js_string(),
    };
    let legend_position = if tight && (placed == "left" || placed == "right") {
        "bottom"
    } else {
        placed.as_str()
    };

    let entries = &model.sections;
    let mut sum = 0.0;
    for (_, value) in entries {
        sum += value;
    }

    let legend: Vec<Label> = entries
        .iter()
        .map(|(label, value)| {
            let raw = if model.show_data {
                format!("{label} [{}]", to_string(*value))
            } else {
                label.clone()
            };
            layout_label(&raw, false, &set, small, 4000.0, Face::default())
        })
        .collect();
    let mut legend_width = 0.0;
    for label in &legend {
        legend_width = max(legend_width, label.width);
    }
    legend_width += if legend.is_empty() { 0.0 } else { SWATCH + 8.0 };
    let legend_height = legend.len() as f64 * ROW;

    let raw = model.title.as_deref().unwrap_or("");
    let title = layout_label(raw, false, &set, size, title_room(o), Face::BOLD);
    let title_height = if title.height > 0.0 {
        title.height + 16.0
    } else {
        0.0
    };

    // A slice too thin for its percentage gets the label outside the rim, which needs a
    // margin.
    let shares: Vec<f64> = entries
        .iter()
        .map(|(_, value)| {
            let share = if sum > 0.0 { value / sum } else { 0.0 };
            if share * 100.0 >= 1.0 && to_fixed(share * 100.0, 0) != "0" {
                share
            } else {
                0.0
            }
        })
        .collect();
    let narrow: Vec<bool> = shares
        .iter()
        .map(|&share| {
            if share == 0.0 || share >= 0.5 {
                return false;
            }
            let chord = 2.0 * radius * text_position * sin(share * PI);
            let text = format!("{}%", to_fixed(share * 100.0, 0));
            chord < set.width(&text, small, Face::default()) + 6.0
        })
        .collect();
    let margin = if narrow.contains(&true) { OUTSIDE } else { 0.0 };

    let side = match legend_position {
        "left" | "right" | "center" => legend_position,
        _ => "",
    };
    let stacked = legend_position == "top" || legend_position == "bottom";
    let diameter = (radius + margin) * 2.0;
    let mut width = diameter;
    let mut height = diameter;
    if side == "left" || side == "right" {
        width += if legend_width > 0.0 {
            GAP + legend_width
        } else {
            0.0
        };
        height = max(height, legend_height);
    } else if stacked {
        width = max(width, legend_width);
        height += if legend_height > 0.0 {
            20.0 + legend_height
        } else {
            0.0
        };
    }
    width = max(width, title.width);

    let cx = pad
        + match side {
            "left" => legend_width + GAP + diameter / 2.0,
            "right" => diameter / 2.0,
            _ => width / 2.0,
        };
    let body_top = pad + title_height;
    let cy = body_top
        + if legend_position == "top" {
            legend_height + 20.0 + diameter / 2.0
        } else if stacked {
            diameter / 2.0
        } else {
            height / 2.0
        };

    let (mut slices, mut labels, mut outsides) = (Vec::new(), Vec::new(), Vec::new());
    let mut angle = 0.0;
    let rim = radius + OUTSIDE / 2.0;
    let mut placed_outside: Vec<Clearing> = Vec::new();
    let slice_style = Style {
        stroke: Some(Paint::BG),
        stroke_width: Some(1.0),
        join: Some(Join::Round),
        ..Style::default()
    };
    for (index, (label, _)) in entries.iter().enumerate() {
        let share = shares[index];
        if share == 0.0 {
            continue;
        }
        let end = angle + share * PI * 2.0;
        let path = slice(cx, cy, angle, end, radius, inner);
        let mut it = Item::new(Kind::Mark, ROLE_SLICE);
        it.id = Some(label.clone());
        it.bounds = path.bounds();
        it.parts = vec![Part::path(path, Painting::FillAndStroke)];
        it.style = Style {
            fill: Some(Paint::series(index)),
            ..slice_style.clone()
        };
        slices.push(it);
        let mid = (angle + end) / 2.0;
        let text = format!("{}%", to_fixed(share * 100.0, 0));
        let mut it = Item::new(Kind::Text, ROLE_SLICE_LABEL);
        if narrow[index] {
            let width = set.width(&text, small, Face::default());
            let clear = |x: f64, y: f64, placed: &[Clearing]| {
                placed.iter().all(|other| {
                    (x - other.x).abs() >= (width + other.width) / 2.0 + 2.0
                        || (y - other.y).abs() >= small + 2.0
                })
            };
            let mut at = mid;
            let mut step = 0;
            while step < 360 {
                let (x, y) = point(at, rim);
                if clear(x, y, &placed_outside) {
                    break;
                }
                at += 2.0 / rim;
                step += 1;
            }
            let (x, y) = point(at, rim);
            placed_outside.push(Clearing { x, y, width });
            let l = line(&text, width, small, Face::default());
            it.label = Some(Placed::new(l, Point::new(cx + x, cy + y)));
            it.style.text = Some(Paint::FG);
            outsides.push(it);
        } else {
            let (x, y) = point(mid, radius * text_position);
            let l = line(
                &text,
                set.width(&text, small, Face::default()),
                small,
                Face::default(),
            );
            it.label = Some(Placed::new(l, Point::new(cx + x, cy + y)));
            it.style.text = Some(Paint::BG);
            labels.push(it);
        }
        angle = end;
    }
    if slices.is_empty() {
        let mut path = Path::new();
        path.ellipse(cx, cy, radius, radius);
        let mut it = Item::new(Kind::Mark, ROLE_SLICE);
        it.bounds = path.bounds();
        it.parts = vec![Part::path(path, Painting::StrokeOnly)];
        it.style = Style {
            stroke: Some(Paint::BORDER),
            ..slice_style
        };
        slices.push(it);
    }

    let (lx, ly) = match side {
        "right" => (
            cx + diameter / 2.0 + GAP,
            body_top + (height - legend_height) / 2.0,
        ),
        "left" => (pad, body_top + (height - legend_height) / 2.0),
        "center" => (cx - legend_width / 2.0, cy - legend_height / 2.0),
        _ => (
            pad + (width - legend_width) / 2.0,
            if legend_position == "top" {
                body_top
            } else {
                body_top + diameter + 20.0
            },
        ),
    };
    let mut legend_items = Vec::new();
    for (index, label) in legend.into_iter().enumerate() {
        let y = ly + index as f64 * ROW + ROW / 2.0;
        let mut it = Item::new(Kind::Node, ROLE_LEGEND);
        it.shape = Some(Shape::new("rect"));
        it.id = Some(entries[index].0.clone());
        it.bounds = Rect::new(lx, y - ROW / 2.0, SWATCH + 8.0 + label.width, ROW);
        let swatch = Rect::new(lx, y - SWATCH / 2.0, SWATCH, SWATCH);
        it.parts = vec![Part::rect(swatch, true, Painting::FillOnly)];
        it.style = Style {
            fill: Some(Paint::series(index)),
            text: Some(Paint::FG),
            radius: Some(2.0),
            ..Style::default()
        };
        if !label.is_empty() {
            let at = Point::new(lx + SWATCH + 8.0 + label.width / 2.0, y);
            it.label = Some(Placed::new(label, at));
        }
        legend_items.push(it);
    }

    let total_width = (width + 2.0 * pad).ceil();
    let total_height = (height + title_height + 2.0 * pad).ceil();
    let mut sc = Scene {
        diagram: "pie".into(),
        width: total_width,
        height: total_height,
        font_size: size,
        title: model.acc_title.clone(),
        desc: model.acc_descr.clone(),
        items: Vec::new(),
    };
    if !title.is_empty() {
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        let at = Point::new(total_width / 2.0, pad + title.height / 2.0);
        it.label = Some(Placed::new(title, at));
        it.style.text = Some(Paint::FG);
        sc.items.push(it);
    }
    sc.items.extend(slices);
    sc.items.extend(labels);
    sc.items.extend(outsides);
    sc.items.extend(legend_items);
    sc
}
