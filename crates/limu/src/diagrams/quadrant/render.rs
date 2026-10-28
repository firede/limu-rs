//! A quadrant chart laid out as a scene, as Pele's renderQuadrant in render.ts draws it.

use super::db::{QuadrantDb, QuadrantText};
use crate::common::{cap_width, setting as text_setting};
use crate::css::declarations;
use crate::js::math::{max, min, round};
use crate::js::number::parse_int;
use crate::layout::Options;
use crate::scene::style::{Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{Item, Kind, Painting, Part, Path, Placed, Point, Rect, Scene, Shape};
use crate::text::layout_label;
use crate::value::{Map, Value};

/// The roles of the items of a quadrant chart: the title, each of the four quadrants, its id
/// its number, with its label, each label of the axes, those of the y axis turned, each
/// point, its id its label's text, drawn the last first, as Mermaid puts each new point under
/// the ones before it, and each point's label, in the order of the points.
pub(crate) const ROLE_TITLE: &str = "quadrant.title";
pub(crate) const ROLE_QUADRANT: &str = "quadrant.quadrant";
pub(crate) const ROLE_AXIS: &str = "quadrant.axis";
pub(crate) const ROLE_POINT: &str = "quadrant.point";
pub(crate) const ROLE_POINT_LABEL: &str = "quadrant.point-label";

/// The gap between the quadrants, Pele's GAP.
const GAP: f64 = 4.0;

/// The most points whose labels look for a free place, Pele's CAREFUL.
const CAREFUL: usize = 300;

/// A point drawn, with how far it reaches from its middle and its label, Pele's Dot.
struct Dot {
    cx: f64,
    cy: f64,
    reach: f64,
    text: Label,
}

/// A setting of the chart's own section of the config, as Pele's setting reads it.
fn setting<'a>(config: &'a Map, key: &str) -> Option<&'a Value> {
    config
        .get("quadrantChart")
        .and_then(Value::as_object)
        .and_then(|section| section.get(key))
}

/// A setting that is a measure, as Pele's length reads it: a number from 0 up, and at most
/// most, or fallback.
fn length(config: &Map, key: &str, fallback: f64, most: f64) -> f64 {
    match setting(config, key) {
        Some(&Value::Number(v)) if v >= 0.0 => min(v, most),
        _ => fallback,
    }
}

/// A hex code with its #, as Pele's hex writes it.
fn hex(color: &str) -> String {
    if color.starts_with('#') {
        color.to_string()
    } else {
        format!("#{color}")
    }
}

/// Lays a quadrant chart out as a scene, as Pele's renderQuadrant does: four quadrants in a
/// plot as wide as the host allows, the labels of the axes about it, and each point with its
/// label below it, or above, right or left of it where that is free.
pub(crate) fn layout(model: &QuadrantDb, config: &Map, o: &Options) -> Scene {
    let size = o.font_size;
    let axis_size = round(size * 0.875);
    let point_size = round(size * 0.75);
    let set = text_setting(o);
    let natural = round(max(100.0, length(config, "chartWidth", 500.0, 4000.0)));
    let width = round(cap_width(o, natural, 280.0, 0.0));
    let height = round(max(100.0, length(config, "chartHeight", 500.0, 4000.0)) * width / natural);
    let outer = length(config, "quadrantPadding", o.padding, 200.0);
    let x_pad = length(config, "xAxisLabelPadding", 5.0, 200.0);
    let y_pad = length(config, "yAxisLabelPadding", 5.0, 200.0);
    let has_points = !model.points.is_empty();

    let label = |text: Option<&QuadrantText>, font_size: f64, most: f64| {
        layout_label(
            text.map_or("", |t| t.text.as_str()),
            text.is_some_and(|t| t.markdown),
            &set,
            font_size,
            max(most, 40.0),
            Face::REGULAR,
        )
    };

    let title = layout_label(
        model.title.as_deref().unwrap_or(""),
        false,
        &set,
        size,
        max(width - 2.0 * outer, 40.0),
        Face::BOLD,
    );
    let title_pad = length(config, "titlePadding", 10.0, 200.0);
    let title_space = if title.height > 0.0 {
        title.height + title_pad
    } else {
        0.0
    };

    let filled = |t: &Option<QuadrantText>| t.as_ref().is_some_and(|t| !t.text.is_empty());
    let has_x = filled(&model.x_axis_left) || filled(&model.x_axis_right);
    let has_y = filled(&model.y_axis_bottom) || filled(&model.y_axis_top);
    let is = |key: &str, value: &str| setting(config, key).and_then(Value::as_str) == Some(value);
    // Mermaid moves the x axis labels below the chart as soon as there are points.
    let x_below = has_points || is("xAxisPosition", "bottom");
    let y_right = is("yAxisPosition", "right");

    let side = width - 2.0 * outer;
    let room = if model.x_axis_right.is_some() {
        side / 2.0 - 16.0
    } else {
        side
    };
    let x_left = label(model.x_axis_left.as_ref(), axis_size, room);
    let x_right = label(model.x_axis_right.as_ref(), axis_size, side / 2.0 - 16.0);
    let x_height = max(x_left.height, x_right.height);
    let x_space = if has_x { x_height + x_pad } else { 0.0 };
    let tall = height - 2.0 * outer - title_space - x_space;
    let room = if model.y_axis_top.is_some() {
        tall / 2.0 - 16.0
    } else {
        tall
    };
    let y_bottom = label(model.y_axis_bottom.as_ref(), axis_size, room);
    let y_top = label(model.y_axis_top.as_ref(), axis_size, tall / 2.0 - 16.0);
    let y_height = max(y_bottom.height, y_top.height);
    let y_space = if has_y { y_height + y_pad } else { 0.0 };

    let left = outer + if y_right { 0.0 } else { y_space };
    let top = outer + title_space + if x_below { 0.0 } else { x_space };
    let w = max(0.0, side - y_space);
    let h = max(0.0, tall);
    let tile_w = max(0.0, (w - GAP) / 2.0);
    let tile_h = max(0.0, (h - GAP) / 2.0);

    // Boxes a point label keeps clear of: the gaps between quadrants, quadrant labels,
    // points, and the labels placed before it.
    let mut taken = vec![
        left + tile_w,
        top,
        left + w - tile_w,
        top + h,
        left,
        top + tile_h,
        left + w,
        top + h - tile_h,
    ];
    let is_clear = |taken: &[f64], from: usize, x0: f64, y0: f64, x1: f64, y1: f64| {
        taken[from..]
            .as_chunks::<4>()
            .0
            .iter()
            .all(|&[l, t, r, b]| {
                // Empty quadrant labels must not block point labels.
                l >= r || t >= b || !(x0 < r && x1 > l && y0 < b && y1 > t)
            })
    };

    let mut items = Vec::new();
    if !title.is_empty() {
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        let at = Point::new(width / 2.0, title_pad / 2.0 + title.height / 2.0);
        it.label = Some(Placed::new(title, at));
        it.style.text = Some(Paint::FG);
        items.push(it);
    }

    let text_top = length(config, "quadrantTextTopPadding", 5.0, 200.0);
    for (k, q) in model.quadrants.iter().enumerate() {
        let x = if k == 0 || k == 3 {
            left + w - tile_w
        } else {
            left
        };
        let y = if k < 2 { top } else { top + h - tile_h };
        let text = label(q.as_ref(), size, tile_w - 16.0);
        let cx = x + tile_w / 2.0;
        let cy = if has_points {
            y + text_top + text.height / 2.0
        } else {
            y + tile_h / 2.0
        };
        taken.extend([
            cx - text.width / 2.0,
            cy - text.height / 2.0,
            cx + text.width / 2.0,
            cy + text.height / 2.0,
        ]);
        let bounds = Rect::new(x, y, tile_w, tile_h);
        let mut it = Item::new(Kind::Node, ROLE_QUADRANT);
        it.shape = Some(Shape::new("rect"));
        it.id = Some((k + 1).to_string());
        it.bounds = bounds;
        it.parts = vec![Part::rect(bounds, true, Painting::FillAndStroke)];
        it.style = Style {
            fill: Some(Paint::SURFACE),
            stroke: Some(Paint::BORDER),
            text: Some(Paint::FG),
            ..Style::default()
        };
        if !text.is_empty() {
            it.label = Some(Placed::new(text, Point::new(cx, cy)));
        }
        items.push(it);
    }

    let mut axis = |l: Label, x: f64, y: f64, turn: f64| {
        if l.is_empty() {
            return;
        }
        let mut it = Item::new(Kind::Text, ROLE_AXIS);
        let mut placed = Placed::new(l, Point::new(x, y));
        placed.rotate = turn;
        it.label = Some(placed);
        it.style.text = Some(Paint::MUTED);
        items.push(it);
    };
    if has_x {
        let y = if x_below {
            top + h + x_pad + x_height / 2.0
        } else {
            top - x_pad - x_height / 2.0
        };
        let x = if x_right.height > 0.0 {
            left + w / 4.0
        } else {
            left + x_left.width / 2.0
        };
        axis(x_left, x, y, 0.0);
        axis(x_right, left + (w * 3.0) / 4.0, y, 0.0);
    }
    if has_y {
        let x = if y_right {
            left + w + y_pad + y_height / 2.0
        } else {
            left - y_pad - y_height / 2.0
        };
        let y = if y_top.height > 0.0 {
            top + (h * 3.0) / 4.0
        } else {
            top + h - y_bottom.width / 2.0
        };
        axis(y_bottom, x, y, -90.0);
        axis(y_top, x, top + h / 4.0, -90.0);
    }

    let radius = length(config, "pointRadius", 5.0, 200.0);
    let text_pad = length(config, "pointTextPadding", 5.0, 200.0);
    let fill = Paint::series(0);
    let mut dots = Vec::new();
    let mut circles = Vec::new();
    for point in &model.points {
        if !(point.x >= 0.0 && point.x <= 1.0 && point.y >= 0.0 && point.y <= 1.0) {
            continue;
        }
        let shared = model.classes.get(&point.class_name).map(|(_, s)| s);
        let r = min(
            point
                .style
                .radius
                .or_else(|| shared.and_then(|s| s.radius))
                .unwrap_or(radius),
            2000.0,
        );
        let pick = |own: &Option<String>, of: fn(&super::db::PointStyle) -> &Option<String>| {
            own.clone().or_else(|| shared.and_then(|s| of(s).clone()))
        };
        let color = pick(&point.style.color, |s| &s.color);
        let stroke_color = pick(&point.style.stroke_color, |s| &s.stroke_color);
        let stroke_width = pick(&point.style.stroke_width, |s| &s.stroke_width);
        let mut styles = Vec::new();
        if let Some(color) = &color {
            styles.push(format!("fill:{}", hex(color)));
        }
        if let Some(width) = &stroke_width {
            if let Some(color) = &stroke_color {
                styles.push(format!("stroke:{}", hex(color)));
            }
            styles.push(format!("stroke-width:{width}"));
        }
        let cx = left + point.x * w;
        let cy = top + (1.0 - point.y) * h;
        let reach = r + stroke_width
            .as_deref()
            .map_or(0.0, |s| min(parse_int(s, 10), 400.0) / 2.0);
        dots.push(Dot {
            cx,
            cy,
            reach,
            text: label(Some(&point.text), point_size, 200.0),
        });
        let mut path = Path::new();
        path.ellipse(cx, cy, r, r);
        let mut it = Item::new(Kind::Mark, ROLE_POINT);
        it.shape = Some(Shape::new("circle"));
        it.id = Some(point.text.text.clone());
        if !point.class_name.is_empty() {
            it.class = vec![point.class_name.clone()];
        }
        it.bounds = Rect::new(cx - r, cy - r, 2.0 * r, 2.0 * r);
        it.parts = vec![Part::path(path, Painting::FillAndStroke)];
        it.style = Style {
            fill: Some(fill),
            // A stroke given a width and no color is drawn in the points' color.
            stroke: Some(if stroke_width.is_some() && stroke_color.is_none() {
                fill
            } else {
                Paint::NONE
            }),
            ..Style::default()
        };
        it.author = declarations(&styles).style;
        circles.push(it);
    }
    // Mermaid puts each new point under the ones before it.
    circles.reverse();
    items.extend(circles);

    // A label goes below its point, or above, right or left of it, or below its quadrant's
    // title, when that is the first place that is inside the chart and free. A second round
    // lets it cross the gaps between quadrants. Large charts skip the search for a free
    // place.
    let careful = dots.len() <= CAREFUL;
    if careful {
        for d in &dots {
            taken.extend([
                d.cx - d.reach,
                d.cy - d.reach,
                d.cx + d.reach,
                d.cy + d.reach,
            ]);
        }
    }
    for Dot {
        cx,
        cy,
        reach,
        text,
    } in dots
    {
        let half_w = text.width / 2.0;
        let half_h = text.size * 0.6 + (text.height - text.line_height) / 2.0;
        let away = reach + text_pad;
        let inset = min(away, max(0.0, (tile_w - text.width) / 2.0));
        // The box of the label of the quadrant the point is in, which taken holds from 8 on,
        // in the quadrants' order.
        let quadrant = if cx < left + w / 2.0 {
            if cy < top + h / 2.0 { 1 } else { 2 }
        } else if cy < top + h / 2.0 {
            0
        } else {
            3
        };
        let heading = 8 + 4 * quadrant;
        let below = max(cy + away, taken[heading + 3] + text_pad) + half_h;
        let spots = [
            cx,
            cy + away + half_h,
            cx,
            cy - away - half_h,
            cx + away + half_w,
            cy,
            cx - away - half_w,
            cy,
            cx,
            below,
        ];
        let (mut tx, mut ty) = (cx, spots[1]);
        let mut found = false;
        for k in (0..20).step_by(2) {
            let spot = k % 10;
            // A label moved below a title keeps clear of the plot's edge.
            let margin = if spot == 8 { inset } else { 0.0 };
            let x = if (spot < 4 || spot == 8) && text.width < w {
                min(
                    max(spots[spot], left + half_w + margin),
                    left + w - half_w - margin,
                )
            } else {
                spots[spot]
            };
            let y = spots[spot + 1];
            if x - half_w < left
                || x + half_w > left + w
                || y - half_h < top
                || y + half_h > top + h
            {
                continue;
            }
            let from = if k < 10 { 0 } else { 8 };
            let free =
                !careful || is_clear(&taken, from, x - half_w, y - half_h, x + half_w, y + half_h);
            if !found || free {
                (tx, ty) = (x, y);
                found = true;
            }
            if free {
                break;
            }
        }
        if careful {
            taken.extend([tx - half_w, ty - half_h, tx + half_w, ty + half_h]);
        }
        if !text.is_empty() {
            let mut it = Item::new(Kind::Text, ROLE_POINT_LABEL);
            it.label = Some(Placed::new(text, Point::new(tx, ty)));
            it.style.text = Some(Paint::FG);
            items.push(it);
        }
    }

    Scene {
        diagram: "quadrantChart".into(),
        width,
        height,
        font_size: size,
        title: model.acc_title.clone(),
        desc: model.acc_descr.clone(),
        items,
    }
}
