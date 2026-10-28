//! A radar chart laid out as a scene, as Pele's renderRadar in render.ts draws it.

use std::f64::consts::PI;

use super::RadarModel;
use crate::common::{fit_width, setting, title_room};
use crate::js::math::{cos, max, min, round, sin};
use crate::layout::Options;
use crate::scene::style::{Join, Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{Item, Kind, Painting, Part, Path, Placed, Point, Rect, Scene, Shape};
use crate::text::layout_label;
use crate::value::{Map, Value};

/// The roles of the items of a radar chart: its title, the rings of the graticule, circles or
/// polygons, as the parts of one item, the spokes, one line from the center for each axis, a
/// curve drawn, its id its name, the label of an axis, its id the axis's name, and an entry
/// of the legend, its id the curve's name, a swatch with the curve's label.
pub(crate) const ROLE_TITLE: &str = "radar.title";
pub(crate) const ROLE_GRATICULE: &str = "radar.graticule";
pub(crate) const ROLE_AXES: &str = "radar.axes";
pub(crate) const ROLE_CURVE: &str = "radar.curve";
pub(crate) const ROLE_AXIS_LABEL: &str = "radar.axis-label";
pub(crate) const ROLE_LEGEND: &str = "radar.legend";

// The measures of Pele's render.ts: the side of a legend's swatch, the height of a row of the
// legend, and the gap between the chart and its legend.
const SWATCH: f64 = 12.0;
const ROW: f64 = 26.0;
const GAP: f64 = 24.0;

/// A number of the chart's own section of the configuration, at most hi, or fallback for
/// anything but a number of at least lo, as Pele's option reads it.
fn option(config: &Map, key: &str, fallback: f64, lo: f64, hi: f64) -> f64 {
    let value = config
        .get("radar")
        .and_then(Value::as_object)
        .and_then(|s| s.get(key));
    match value {
        Some(&Value::Number(v)) if v >= lo => min(v, hi),
        _ => fallback,
    }
}

/// How far from the center a value is drawn, clipped to the range from lo to hi, as Pele's
/// relativeRadius gives it. It is scaled before it is divided, unless the product overflows,
/// where it is divided first.
pub(crate) fn relative_radius(value: f64, lo: f64, hi: f64, radius: f64) -> f64 {
    let clipped = min(max(value, lo), hi);
    let scaled = radius * (clipped - lo);
    if scaled.is_finite() {
        scaled / (hi - lo)
    } else {
        radius * ((clipped - lo) / (hi - lo))
    }
}

/// A closed Catmull-Rom spline through the points, as cubic Bézier segments, as Pele's
/// closedRoundCurve draws it.
pub(crate) fn closed_round_curve(points: &[(f64, f64)], tension: f64) -> Path {
    let n = points.len();
    let mut p = Path::new();
    p.move_to(points[0].0, points[0].1);
    for i in 0..n {
        let p0 = points[(i + n - 1) % n];
        let p1 = points[i];
        let p2 = points[(i + 1) % n];
        let p3 = points[(i + 2) % n];
        p.cube_to(
            p1.0 + (p2.0 - p0.0) * tension,
            p1.1 + (p2.1 - p0.1) * tension,
            p2.0 - (p3.0 - p1.0) * tension,
            p2.1 - (p3.1 - p1.1) * tension,
            p2.0,
            p2.1,
        );
    }
    p.close();
    p
}

/// Lays a radar chart out as a scene, as Pele's renderRadar draws it: the chart at the width
/// and height its configuration gives, narrower where the host is, with its legend at its
/// right, or below it where the chart does not fit the host's width with the legend beside
/// it.
pub(crate) fn layout(model: &RadarModel, config: &Map, o: &Options) -> Scene {
    let natural = option(config, "width", 320.0, 1.0, 4000.0);
    fit_width(o, natural, 160.0, |width, tight| {
        let height = (option(config, "height", 320.0, 1.0, 4000.0) * width) / natural;
        draw(model, config, o, width, height, tight)
    })
}

/// Lays the chart out at a width and a height, as Pele's draw does: about its center, which
/// is then moved to where the margins, the labels, the legend and the title leave it. tight
/// is set where the chart does not fit the width it has with its legend beside it.
fn draw(
    model: &RadarModel,
    config: &Map,
    o: &Options,
    width: f64,
    height: f64,
    tight: bool,
) -> Scene {
    let size = o.font_size;
    let small = round(size * 0.875);
    let set = setting(o);
    let pad = o.pad();
    let label = |raw: &str| layout_label(raw, false, &set, small, 4000.0, Face::default());

    let axis_scale = option(config, "axisScaleFactor", 1.0, 0.0, 10.0);
    let label_factor = option(config, "axisLabelFactor", 1.05, 0.0, 10.0);
    let tension = option(config, "curveTension", 0.17, 0.0, 1.0);
    let radius = min(width, height) / 2.0;

    let (axes, curves) = (&model.axes, &model.curves);
    let ticks = model.options.ticks;
    let graticule = model.options.graticule.as_str();
    let lo = model.options.min;
    let count = axes.len();
    let unit: Vec<(f64, f64)> = (0..count)
        .map(|i| {
            let angle = (2.0 * PI * i as f64) / count as f64 - PI / 2.0;
            (cos(angle), sin(angle))
        })
        .collect();

    let hi = model.options.max.unwrap_or_else(|| {
        let mut hi = f64::NEG_INFINITY;
        for curve in curves {
            for &value in &curve.entries {
                if value > hi {
                    hi = value;
                }
            }
        }
        hi
    });

    // The chart is laid out about its center. Margins reserve room, and labels and the
    // legend widen it.
    let mut x0 = -width / 2.0 - option(config, "marginLeft", 16.0, 0.0, 2000.0);
    let mut x1 = width / 2.0 + option(config, "marginRight", 16.0, 0.0, 2000.0);
    let mut y0 = -height / 2.0 - option(config, "marginTop", 16.0, 0.0, 2000.0);
    let mut y1 = height / 2.0 + option(config, "marginBottom", 16.0, 0.0, 2000.0);

    let mut items = Vec::new();
    let mut grid = Path::new();
    let mut i = 0.0;
    while i < ticks {
        let r = min(radius, (radius * (i + 1.0)) / ticks);
        if graticule == "circle" {
            grid.ellipse(0.0, 0.0, r, r);
        } else if count > 0 {
            // A polygon of no axes has no points, and draws nothing.
            let ring: Vec<(f64, f64)> = unit.iter().map(|u| (u.0 * r, u.1 * r)).collect();
            grid.polygon(&ring);
        }
        i += 1.0;
    }
    if !grid.is_empty() {
        let mut it = Item::new(Kind::Mark, ROLE_GRATICULE);
        it.shape = Some(Shape::new(if graticule == "circle" {
            "circle"
        } else {
            "polygon"
        }));
        it.bounds = grid.bounds();
        it.parts = vec![Part::path(grid, Painting::StrokeOnly)];
        it.style.stroke = Some(Paint::SURFACE_ALT);
        items.push(it);
    }

    let mut spokes = Path::new();
    let mut labels = Vec::new();
    for (i, u) in unit.iter().enumerate() {
        let reach = radius * axis_scale;
        spokes.move_to(0.0, 0.0).line_to(u.0 * reach, u.1 * reach);
        let l = label(&axes[i].label);
        let at = radius * label_factor + 6.0;
        // Each label hangs off its axis on the side away from the center.
        let dx = if u.0 > 0.01 {
            l.width / 2.0
        } else if u.0 < -0.01 {
            -l.width / 2.0
        } else {
            0.0
        };
        let dy = if u.1 > 0.01 {
            l.height / 2.0
        } else if u.1 < -0.01 {
            -l.height / 2.0
        } else {
            0.0
        };
        let cx = u.0 * at + dx;
        let cy = u.1 * at + dy;
        x0 = min(x0, cx - l.width / 2.0);
        x1 = max(x1, cx + l.width / 2.0);
        y0 = min(y0, cy - l.height / 2.0);
        y1 = max(y1, cy + l.height / 2.0);
        if !l.is_empty() {
            let mut it = Item::new(Kind::Text, ROLE_AXIS_LABEL);
            it.id = Some(axes[i].name.clone());
            it.label = Some(Placed::new(l, Point::new(cx, cy)));
            it.style.text = Some(Paint::FG);
            labels.push(it);
        }
    }
    if !spokes.is_empty() {
        let mut it = Item::new(Kind::Mark, ROLE_AXES);
        it.bounds = spokes.bounds();
        it.parts = vec![Part::path(spokes, Painting::StrokeOnly)];
        it.style.stroke = Some(Paint::BORDER);
        items.push(it);
    }

    for (index, curve) in curves.iter().enumerate() {
        // Mermaid leaves out a curve that does not have a value for every axis.
        if curve.entries.len() != count {
            continue;
        }
        let points: Vec<(f64, f64)> = curve
            .entries
            .iter()
            .enumerate()
            .map(|(i, &value)| {
                let r = relative_radius(value, lo, hi, radius);
                let safe = if r.is_finite() { r } else { 0.0 };
                (unit[i].0 * safe, unit[i].1 * safe)
            })
            .collect();
        let path = if graticule == "circle" {
            closed_round_curve(&points, tension)
        } else {
            let mut p = Path::new();
            p.polygon(&points);
            p
        };
        let color = Paint::series(index);
        let mut it = Item::new(Kind::Mark, ROLE_CURVE);
        it.id = Some(curve.name.clone());
        it.bounds = path.bounds();
        it.parts = vec![Part::path(path, Painting::FillAndStroke)];
        it.style = Style {
            fill: Some(color),
            fill_opacity: Some(0.2),
            stroke: Some(color),
            stroke_width: Some(1.5),
            join: Some(Join::Round),
            ..Style::default()
        };
        items.push(it);
    }
    items.extend(labels);

    let legend: Vec<Label> = if model.options.show_legend {
        curves.iter().map(|curve| label(&curve.label)).collect()
    } else {
        Vec::new()
    };
    let mut legend_width = 0.0;
    for l in &legend {
        legend_width = max(legend_width, l.width);
    }
    let legend_height = legend.len() as f64 * ROW;
    let (legend_x, legend_top) = if tight {
        (-(SWATCH + 8.0 + legend_width) / 2.0, y1 + GAP / 2.0)
    } else {
        (x1 + GAP, -legend_height / 2.0)
    };
    if !legend.is_empty() && tight {
        x0 = min(x0, legend_x);
        x1 = max(x1, -legend_x);
        y1 = legend_top + legend_height;
    } else if !legend.is_empty() {
        x1 = legend_x + SWATCH + 8.0 + legend_width;
        y0 = min(y0, -legend_height / 2.0);
        y1 = max(y1, legend_height / 2.0);
    }
    for (index, l) in legend.into_iter().enumerate() {
        let y = legend_top + index as f64 * ROW + ROW / 2.0;
        let color = Paint::series(index);
        let mut it = Item::new(Kind::Node, ROLE_LEGEND);
        it.shape = Some(Shape::new("rect"));
        it.id = Some(curves[index].name.clone());
        it.bounds = Rect::new(legend_x, y - ROW / 2.0, SWATCH + 8.0 + l.width, ROW);
        let swatch = Rect::new(legend_x, y - SWATCH / 2.0, SWATCH, SWATCH);
        it.parts = vec![Part::rect(swatch, true, Painting::FillAndStroke)];
        it.style = Style {
            fill: Some(color),
            fill_opacity: Some(0.4),
            stroke: Some(color),
            text: Some(Paint::FG),
            radius: Some(2.0),
            ..Style::default()
        };
        if !l.is_empty() {
            let at = Point::new(legend_x + SWATCH + 8.0 + l.width / 2.0, y);
            it.label = Some(Placed::new(l, at));
        }
        items.push(it);
    }

    let raw = model.title.as_deref().unwrap_or("");
    let title = layout_label(raw, false, &set, size, title_room(o), Face::BOLD);
    let title_height = if title.height > 0.0 {
        title.height + 8.0
    } else {
        0.0
    };
    let body_width = max(x1 - x0, title.width);
    let total_width = (body_width + 2.0 * pad).ceil();
    let total_height = (y1 - y0 + title_height + 2.0 * pad).ceil();
    let ox = pad + (body_width - (x1 - x0)) / 2.0 - x0;
    let oy = pad + title_height - y0;
    for it in &mut items {
        it.move_by(ox, oy);
    }

    let mut sc = Scene {
        diagram: "radar".into(),
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
    sc.items.extend(items);
    sc
}
