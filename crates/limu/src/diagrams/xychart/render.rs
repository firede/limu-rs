//! An XY chart laid out as a scene, as Pele's renderXyChart in render.ts draws it.

use super::db::{PlotType, XAxis, XyChartDb};
use super::ticks::{tick_index, tick_step, tick_value, ticks_between};
use crate::common::{cap_width, line, setting as text_setting};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::math::{max, min, round};
use crate::js::number::to_string;
use crate::layout::Options;
use crate::scene::style::{Cap, Join, Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{Align, Item, Kind, Painting, Part, Path, Placed, Point, Rect, Scene, Shape};
use crate::text::layout_label;
use crate::value::{Map, Value};

/// The roles of the items of an XY chart: the title, the grid, the bars of each bar series,
/// each line series with its points, the axes with their ticks, the label of each mark of an
/// axis, the axes' titles, the value written past a bar, the label of a point of a line, and
/// each entry of the legend.
pub(crate) const ROLE_TITLE: &str = "xychart.title";
pub(crate) const ROLE_GRID: &str = "xychart.grid";
pub(crate) const ROLE_BARS: &str = "xychart.bars";
pub(crate) const ROLE_LINE: &str = "xychart.line";
pub(crate) const ROLE_AXIS: &str = "xychart.axis";
pub(crate) const ROLE_AXIS_LABEL: &str = "xychart.axis-label";
pub(crate) const ROLE_AXIS_TITLE: &str = "xychart.axis-title";
pub(crate) const ROLE_DATA_LABEL: &str = "xychart.data-label";
pub(crate) const ROLE_POINT_LABEL: &str = "xychart.point-label";
pub(crate) const ROLE_LEGEND: &str = "xychart.legend";

// The measures of Pele's render: the length of a tick, the side of a legend's swatch, the
// least height of a legend's row and the room before the legend, and the largest value
// drawn.
const TICK: f64 = 4.0;
const SWATCH: f64 = 12.0;
const ROW: f64 = 26.0;
const LEGEND_GAP: f64 = 24.0;
const LIMIT: f64 = 1e300;

/// A labelled place along an axis, as a fraction of its length, Pele's Mark.
struct Mark {
    at: f64,
    label: Label,
}

/// A section of the config that is an object, as Pele's record reads it, or none.
fn record<'a>(config: Option<&'a Map>, key: &str) -> Option<&'a Map> {
    config?.get(key).and_then(Value::as_object)
}

/// A setting that is true or false, or fallback when it is anything else, as Pele's on
/// reads it.
fn on(config: Option<&Map>, key: &str, fallback: bool) -> bool {
    match config.and_then(|c| c.get(key)) {
        Some(&Value::Bool(b)) => b,
        _ => fallback,
    }
}

/// A size the config gives, kept from 100 to 4000 and rounded, or fallback for anything but
/// a number above zero, as Pele's dimension reads it.
fn dimension(value: Option<&Value>, fallback: f64) -> f64 {
    match value {
        Some(&Value::Number(v)) if v > 0.0 => round(min(max(v, 100.0), 4000.0)),
        _ => fallback,
    }
}

/// A value kept within ±1e300, and none for one that is not a finite number, as Pele's
/// finite gives it.
fn finite(value: Option<f64>) -> Option<f64> {
    value
        .filter(|v| v.is_finite())
        .map(|v| min(max(v, -LIMIT), LIMIT))
}

/// The width of the widest label of marks, as Pele's widest gives it.
fn widest(marks: &[Mark]) -> f64 {
    marks.iter().fold(0.0, |w, m| max(w, m.label.width))
}

/// Whether a label is one line of one span of plain text, which Pele's aligned writes as one
/// text element anchored where it is placed.
fn plain(l: &Label) -> bool {
    l.lines.len() == 1
        && l.lines[0].len() == 1
        && l.lines[0][0].icon.is_none()
        && l.lines[0][0].face == Face::REGULAR
}

/// A label with its left edge, center or right edge at x and its middle at y, as Pele's
/// aligned writes it: a plain one-line label is anchored at x, so that it keeps its place
/// when the text is wider or narrower than measured, and any other is centered on where its
/// measured edge puts its middle. An empty label is nothing, as Pele draws nothing for it.
fn aligned(
    role: &'static str,
    label: Label,
    x: f64,
    y: f64,
    align: Align,
    paint: Paint,
) -> Option<Item> {
    if label.is_empty() {
        return None;
    }
    let mut it = Item::new(Kind::Text, role);
    it.style.text = Some(paint);
    let placed = if plain(&label) {
        let mut p = Placed::new(label, Point::new(x, y));
        p.align = align;
        p
    } else {
        let x = match align {
            Align::Start => x + label.width / 2.0,
            Align::End => x + -label.width / 2.0,
            Align::Middle => x,
        };
        Placed::new(label, Point::new(x, y))
    };
    it.label = Some(placed);
    Some(it)
}

/// The smallest box that holds a and b.
fn union(a: Rect, b: Rect) -> Rect {
    let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
    let (x1, y1) = ((a.x + a.w).max(b.x + b.w), (a.y + a.h).max(b.y + b.h));
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// Lays an XY chart out as a scene, as Pele's renderXyChart does: the plot at the width and
/// height the chart's config gives, narrower when the host is, with its axes, the bars and
/// lines of each series over a grid, and a legend at its right. A chart with no series is an
/// error, as in Mermaid.
pub(crate) fn layout(model: &XyChartDb, config: &Map, o: &Options) -> Result<Scene, Error> {
    if model.plots.is_empty() {
        let mut e = Error::new(
            "No Plot to render, please provide a plot with some data",
            ErrorCode::Semantic,
        );
        e.diagram = Some(DiagramType::XyChart);
        return Err(e);
    }
    let chart = config.get("xyChart").and_then(Value::as_object);
    let size = o.font_size;
    let small = round(size * 0.875);
    let tiny = round(size * 0.75);
    let set = text_setting(o);
    let pad = o.pad();
    let natural = dimension(chart.and_then(|c| c.get("width")), 700.0);
    let width = cap_width(o, natural, 240.0, 0.0);
    // A narrower chart is not as much shorter, so that the plot keeps room for its marks.
    let height =
        round(dimension(chart.and_then(|c| c.get("height")), 500.0) * max(0.6, width / natural));
    let orientation = model.orientation.or_else(|| {
        chart
            .and_then(|c| c.get("chartOrientation"))
            .and_then(Value::as_str)
    });
    let horizontal = orientation == Some("horizontal");
    // The value of a bar is always written past its end, which is what
    // showDataLabelOutsideBar asks for in Mermaid. Inside, whether it fits changes from bar to
    // bar and with the width of the chart.
    let show_values = on(chart, "showDataLabel", false);
    let x = &model.x_axis;
    let band = match x {
        XAxis::Band(b) => Some(b),
        XAxis::Linear(_) => None,
    };
    let plots = &model.plots;
    let text = |raw: &str, font_size: f64, most: f64| {
        layout_label(raw, false, &set, font_size, most, Face::REGULAR)
    };
    let line_height = text("0", small, 4000.0).height;
    let tiny_height = text("0", tiny, 4000.0).height;

    let title_text = if on(chart, "showTitle", true) {
        model.title.as_deref().unwrap_or("")
    } else {
        ""
    };
    let title = layout_label(title_text, false, &set, size, width - 2.0 * pad, Face::BOLD);
    let title_space = if title.height > 0.0 {
        title.height + 12.0
    } else {
        0.0
    };

    let mut legend = Vec::new();
    let mut legend_width = 0.0;
    if on(chart, "showLegend", true) {
        for (index, plot) in plots.iter().enumerate() {
            if plot.title.is_empty() {
                continue;
            }
            let label = text(&plot.title, small, width * 0.3);
            legend_width = max(legend_width, label.width + SWATCH + 8.0);
            legend.push((index, label));
        }
    }
    let legend_space = if legend.is_empty() {
        0.0
    } else {
        legend_width + LEGEND_GAP
    };

    // The value range: as written, or taken from the data, where bars stand on zero and the
    // range grows to whole steps.
    let bars = plots.iter().filter(|p| p.kind == PlotType::Bar).count();
    let mut lo = finite(Some(model.y_axis.min)).unwrap_or(f64::INFINITY);
    let mut hi = finite(Some(model.y_axis.max)).unwrap_or(f64::NEG_INFINITY);
    let exact = model.has_set_y_axis && lo.is_finite() && hi.is_finite() && lo != hi;
    if !exact {
        lo = f64::INFINITY;
        hi = f64::NEG_INFINITY;
        for plot in plots {
            for &(_, raw) in &plot.data {
                let Some(value) = finite(raw) else {
                    continue;
                };
                if value < lo {
                    lo = value;
                }
                if value > hi {
                    hi = value;
                }
            }
        }
        if lo > hi {
            lo = 0.0;
            hi = 0.0;
        }
        if bars > 0 {
            lo = min(lo, 0.0);
            hi = max(hi, 0.0);
        }
        if lo == hi {
            if lo == 0.0 {
                hi = 1.0;
            } else {
                lo -= lo.abs() / 2.0;
                hi += hi.abs() / 2.0;
            }
        }
    }

    let value_marks = |lo: &mut f64, hi: &mut f64, length: f64, spacing: f64| -> Vec<Mark> {
        let step = tick_step(
            (*hi - *lo).abs(),
            max(2.0, min(10.0, (length / spacing).floor())),
        );
        if !exact {
            *lo = tick_value(step, (tick_index(step, *lo) + 1e-9).floor());
            *hi = tick_value(step, (tick_index(step, *hi) - 1e-9).ceil());
        }
        let (lo, hi) = (*lo, *hi);
        ticks_between(min(lo, hi), max(lo, hi), step)
            .into_iter()
            .map(|value| Mark {
                at: (value - lo) / (hi - lo),
                label: text(&to_string(value), small, 4000.0),
            })
            .collect()
    };

    // Places along the category axis are fractions of its length. Categories and bars sit in
    // the middle of equal slots, and the points of a line on a numeric axis run from end to
    // end.
    let count = match band {
        Some(b) => b.categories.len().max(1),
        None => plots.iter().fold(1, |n, p| n.max(p.data.len())),
    };
    let slot = 1.0 / count as f64;
    let inset = if band.is_some() || bars > 0 {
        slot / 2.0
    } else {
        0.0
    };
    let (x0, x1) = match x {
        XAxis::Linear(l) => (finite(Some(l.min)), finite(Some(l.max))),
        XAxis::Band(_) => (None, None),
    };
    let range = match (x0, x1) {
        (Some(a), Some(b)) if a != b => Some((a, b)),
        _ => None,
    };
    let place = |i: usize, n: usize| {
        if band.is_some() {
            return (i as f64 + 0.5) * slot;
        }
        let at = if n > 1 {
            i as f64 / (n - 1) as f64
        } else if range.is_some() {
            0.0
        } else {
            0.5
        };
        inset + at * (1.0 - 2.0 * inset)
    };

    let category_conf = record(chart, "xAxis");
    let value_conf = record(chart, "yAxis");
    let (left_conf, bottom_conf) = if horizontal {
        (category_conf, value_conf)
    } else {
        (value_conf, category_conf)
    };
    let (left_text, bottom_text) = if horizontal {
        (x.title(), model.y_axis.title.as_str())
    } else {
        (model.y_axis.title.as_str(), x.title())
    };
    let left_text = if on(left_conf, "showTitle", true) {
        left_text
    } else {
        ""
    };
    let bottom_text = if on(bottom_conf, "showTitle", true) {
        bottom_text
    } else {
        ""
    };
    let left_title = text(left_text, small, height * 0.8);
    let bottom_title = text(bottom_text, small, width * 0.8);
    let left_tick = if on(left_conf, "showTick", true) {
        TICK
    } else {
        0.0
    };
    let bottom_tick = if on(bottom_conf, "showTick", true) {
        TICK
    } else {
        0.0
    };
    let left_labels = on(left_conf, "showLabel", true);
    let bottom_labels = on(bottom_conf, "showLabel", true);

    let wrap_at = if horizontal {
        width * 0.3
    } else {
        max(48.0, (width - 2.0 * pad - 64.0 - legend_space) * slot - 8.0)
    };
    let categories: Vec<Label> = match band {
        Some(b) if on(category_conf, "showLabel", true) => b
            .categories
            .iter()
            .map(|c| text(c, small, wrap_at))
            .collect(),
        _ => Vec::new(),
    };
    let (mut category_width, mut category_height) = (0.0, 0.0);
    for label in &categories {
        category_width = max(category_width, label.width);
        category_height = max(category_height, label.height);
    }

    // The labelled places of the category axis. Labels that would run into each other are
    // thinned out.
    let category_marks = |length: f64| -> Vec<Mark> {
        let mut marks = Vec::new();
        if let Some(b) = band {
            let need = if categories.is_empty() {
                8.0
            } else if horizontal {
                category_height
            } else {
                category_width + 12.0
            };
            let every = max(1.0, (need / (length * slot)).ceil());
            let mut i = 0.0;
            while i < b.categories.len() as f64 {
                let label = categories.get(i as usize).cloned().unwrap_or_default();
                marks.push(Mark {
                    at: (i + 0.5) * slot,
                    label,
                });
                i += every;
            }
        } else if let Some((x0, x1)) = range {
            let spacing = if horizontal { 32.0 } else { 80.0 };
            let step = tick_step(
                (x1 - x0).abs(),
                max(2.0, min(10.0, (length / spacing).floor())),
            );
            for value in ticks_between(min(x0, x1), max(x0, x1), step) {
                marks.push(Mark {
                    at: inset + ((value - x0) / (x1 - x0)) * (1.0 - 2.0 * inset),
                    label: text(&to_string(value), small, 4000.0),
                });
            }
        } else if let Some(x0) = x0 {
            marks.push(Mark {
                at: 0.5,
                label: text(&to_string(x0), small, 4000.0),
            });
        }
        marks
    };

    // Room past the end of the value axis for what is written beside a point or a bar.
    let (mut point_label_width, mut value_width) = (0.0, 0.0);
    for plot in plots {
        for label in plot.point_labels.iter().flatten() {
            point_label_width = max(point_label_width, set.width(label, tiny, Face::REGULAR));
        }
        if show_values && horizontal && plot.kind == PlotType::Bar {
            for &(_, value) in &plot.data {
                let s = finite(value).map_or(String::new(), to_string);
                value_width = max(value_width, set.width(&s, tiny, Face::REGULAR));
            }
        }
    }

    let bottom_label_height = if !bottom_labels {
        0.0
    } else if horizontal || band.is_none() {
        line_height
    } else {
        category_height
    };
    let bottom_space = bottom_tick
        + if bottom_label_height > 0.0 {
            bottom_label_height + 4.0
        } else {
            0.0
        }
        + if bottom_title.height > 0.0 {
            bottom_title.height + 4.0
        } else {
            0.0
        };
    let headroom = if horizontal {
        0.0
    } else if point_label_width > 0.0 || show_values {
        tiny_height + 4.0
    } else if left_labels {
        line_height / 2.0
    } else {
        0.0
    };
    let top = pad + title_space + headroom;
    let plot_height = max(1.0, height - pad - bottom_space - top);
    let bottom = top + plot_height;

    let left_marks = if horizontal {
        category_marks(plot_height)
    } else {
        value_marks(&mut lo, &mut hi, plot_height, 56.0)
    };
    let left_label_width = if left_labels {
        widest(&left_marks)
    } else {
        0.0
    };
    let left =
        pad + if left_title.height > 0.0 {
            left_title.height + 6.0
        } else {
            0.0
        } + if left_label_width > 0.0 {
            left_label_width + 6.0
        } else {
            0.0
        } + left_tick;
    let beside = max(
        if point_label_width > 0.0 {
            point_label_width + 12.0
        } else {
            0.0
        },
        if value_width > 0.0 {
            value_width + 8.0
        } else {
            0.0
        },
    );
    let reach = if horizontal {
        min(width * 0.25, beside)
    } else {
        0.0
    };
    let mut plot_width = max(1.0, width - pad - left - legend_space - reach);
    let bottom_marks = if horizontal {
        value_marks(&mut lo, &mut hi, plot_width, 80.0)
    } else {
        category_marks(plot_width)
    };
    if bottom_labels && legend_space + reach == 0.0 {
        let mut over = 0.0;
        for mark in &bottom_marks {
            over = max(
                over,
                mark.at * plot_width + mark.label.width / 2.0 - plot_width,
            );
        }
        plot_width = max(1.0, plot_width - over);
    }
    let right = left + plot_width;

    let px = |c: f64, v: f64| {
        if horizontal {
            left + v * plot_width
        } else {
            left + c * plot_width
        }
    };
    let py = |c: f64, v: f64| {
        if horizontal {
            top + c * plot_height
        } else {
            bottom - v * plot_height
        }
    };
    let scale = |value: f64| (value - lo) / (hi - lo);
    let clamp = |v: f64| min(max(v, 0.0), 1.0);

    // A line across the plot at a place on the value axis.
    let rule = |p: &mut Path, v: f64| {
        if horizontal {
            p.move_to(px(0.0, v), top).line_to(px(0.0, v), bottom);
        } else {
            p.move_to(left, py(0.0, v)).line_to(right, py(0.0, v));
        }
    };

    let mut grid = Path::new();
    for mark in if horizontal {
        &bottom_marks
    } else {
        &left_marks
    } {
        rule(&mut grid, mark.at);
    }

    let mut axis = Path::new();
    let mut labels = Vec::new();
    if on(left_conf, "showAxisLine", true) {
        axis.move_to(left, top).line_to(left, bottom);
    }
    if on(bottom_conf, "showAxisLine", true) {
        axis.move_to(left, bottom).line_to(right, bottom);
    }
    for mark in left_marks {
        let y = if horizontal {
            top + mark.at * plot_height
        } else {
            bottom - mark.at * plot_height
        };
        if left_tick != 0.0 {
            axis.move_to(left, y).line_to(left + -left_tick, y);
        }
        if left_labels {
            labels.extend(aligned(
                ROLE_AXIS_LABEL,
                mark.label,
                left - left_tick - 6.0,
                y,
                Align::End,
                Paint::MUTED,
            ));
        }
    }
    for mark in bottom_marks {
        let at = left + mark.at * plot_width;
        if bottom_tick != 0.0 {
            axis.move_to(at, bottom).line_to(at, bottom + bottom_tick);
        }
        if bottom_labels {
            let y = bottom + bottom_tick + 4.0 + mark.label.height / 2.0;
            labels.extend(aligned(
                ROLE_AXIS_LABEL,
                mark.label,
                at,
                y,
                Align::Middle,
                Paint::MUTED,
            ));
        }
    }

    let span = (if horizontal { plot_height } else { plot_width }) * slot;
    let group = span * 0.7;
    let thickness = group / bars.max(1) as f64;
    let gap = if bars > 1 {
        min(2.0, thickness * 0.2)
    } else {
        0.0
    };
    let base = clamp(scale(0.0));
    // Bars that go both ways get a line to stand on.
    if bars > 0 && base > 0.0 && base < 1.0 {
        rule(&mut axis, base);
    }
    // Lines are drawn over the bars wherever they were declared, as a bar would hide part of
    // one under it. A series keeps the color of its place in the source.
    let (mut series, mut lines, mut outside, mut point_labels) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut bar = 0;
    for (index, plot) in plots.iter().enumerate() {
        let color = Paint::series(index);
        let id = (!plot.title.is_empty()).then(|| plot.title.clone());
        // A series declared before the categories can hold more values than there are
        // categories.
        let n = if band.is_some() {
            plot.data.len().min(count)
        } else {
            plot.data.len()
        };
        if plot.kind == PlotType::Bar {
            let offset =
                (bar as f64 - (bars as f64 - 1.0) / 2.0) * thickness - (thickness - gap) / 2.0;
            bar += 1;
            let mut parts = Vec::new();
            let mut bounds = Rect::default();
            for i in 0..n {
                let Some(value) = finite(plot.data[i].1) else {
                    continue;
                };
                let c = place(i, n);
                let v = clamp(scale(value));
                let from = min(base, v);
                let to = max(base, v);
                let (bx, by, bw, bh) = if horizontal {
                    (
                        px(c, from),
                        py(c, 0.0) + offset,
                        (to - from) * plot_width,
                        thickness - gap,
                    )
                } else {
                    (
                        px(c, 0.0) + offset,
                        py(c, to),
                        thickness - gap,
                        (to - from) * plot_height,
                    )
                };
                let r = Rect::new(bx, by, bw, bh);
                bounds = if parts.is_empty() {
                    r
                } else {
                    union(bounds, r)
                };
                parts.push(Part::rect(r, false, Painting::FillOnly));
                if !show_values {
                    continue;
                }
                let s = to_string(value);
                let w = set.width(&s, tiny, Face::REGULAR);
                // Towards the end of the bar: 1 when it runs up or right from the baseline,
                // -1 the other way.
                let out = if v >= base { 1.0 } else { -1.0 };
                let mut tx = bx + bw / 2.0;
                let mut ty = by + bh / 2.0;
                let mut align = Align::Middle;
                if horizontal {
                    if bh < tiny {
                        continue;
                    }
                    tx = (if out > 0.0 { bx + bw } else { bx }) + out * 6.0;
                    align = if out > 0.0 { Align::Start } else { Align::End };
                } else {
                    if w + 2.0 > span / bars as f64 {
                        continue;
                    }
                    ty = (if out > 0.0 { by } else { by + bh }) - out * (tiny_height / 2.0 + 2.0);
                }
                let mut it = Item::new(Kind::Text, ROLE_DATA_LABEL);
                let mut placed = Placed::new(line(&s, w, tiny, Face::REGULAR), Point::new(tx, ty));
                placed.align = align;
                it.label = Some(placed);
                it.style.text = Some(Paint::FG);
                outside.push(it);
            }
            let mut it = Item::new(Kind::Mark, ROLE_BARS);
            it.id = id;
            it.bounds = bounds;
            it.parts = parts;
            it.style.fill = Some(color);
            series.push(it);
            continue;
        }

        // A line is cut where it leaves the value range, so it stays inside the plot.
        let mut d = Path::new();
        let mut dots = Vec::new();
        let mut pen = false;
        let mut pc = 0.0;
        let mut pv = f64::NAN;
        for i in 0..n {
            let Some(value) = finite(plot.data[i].1) else {
                pv = f64::NAN;
                continue;
            };
            let c = place(i, n);
            let v = scale(value);
            let visible = (0.0..=1.0).contains(&v);
            if pv.is_nan() {
                if visible {
                    d.move_to(px(c, v), py(c, v));
                }
                pen = visible;
            } else {
                let dv = v - pv;
                let (mut t0, mut t1) = (0.0, 1.0);
                if dv != 0.0 {
                    let a = -pv / dv;
                    let b = (1.0 - pv) / dv;
                    t0 = max(t0, min(a, b));
                    t1 = min(t1, max(a, b));
                } else if !visible {
                    t0 = 2.0;
                }
                if t0 <= t1 {
                    let c0 = pc + (c - pc) * t0;
                    let v0 = pv + dv * t0;
                    let c1 = pc + (c - pc) * t1;
                    let v1 = pv + dv * t1;
                    if !pen || t0 > 0.0 {
                        d.move_to(px(c0, v0), py(c0, v0));
                    }
                    d.line_to(px(c1, v1), py(c1, v1));
                }
                pen = visible;
            }
            pc = c;
            pv = v;
            if !visible {
                continue;
            }
            let cx = px(c, v);
            let cy = py(c, v);
            if span >= 12.0 {
                let mut dot = Path::new();
                dot.ellipse(cx, cy, 3.0, 3.0);
                dots.push(Part::path(dot, Painting::FillOnly));
            }
            let raw = plot
                .point_labels
                .as_ref()
                .and_then(|l| l.get(i))
                .map_or("", String::as_str);
            let label = text(raw, tiny, 4000.0);
            if label.height > 0.0 {
                let half = label.width / 2.0;
                point_labels.extend(if horizontal {
                    aligned(
                        ROLE_POINT_LABEL,
                        label,
                        cx + 10.0,
                        cy,
                        Align::Start,
                        Paint::FG,
                    )
                } else {
                    let y = cy - 6.0 - label.height / 2.0;
                    let x = max(min(cx, right - half), left + half);
                    aligned(ROLE_POINT_LABEL, label, x, y, Align::Middle, Paint::FG)
                });
            }
        }
        let mut parts = Vec::new();
        if !d.is_empty() {
            parts.push(Part::path(d, Painting::StrokeOnly));
        }
        parts.extend(dots);
        let mut it = Item::new(Kind::Mark, ROLE_LINE);
        it.id = id;
        for (k, part) in parts.iter().enumerate() {
            if let crate::scene::Outline::Path(p) = &part.outline {
                it.bounds = if k == 0 {
                    p.bounds()
                } else {
                    union(it.bounds, p.bounds())
                };
            }
        }
        it.parts = parts;
        it.style = Style {
            fill: Some(color),
            stroke: Some(color),
            stroke_width: Some(2.0),
            cap: Some(Cap::Round),
            join: Some(Join::Round),
            ..Style::default()
        };
        lines.push(it);
    }

    let mut titles = Vec::new();
    if left_title.height > 0.0 {
        let at = Point::new(pad + left_title.height / 2.0, top + plot_height / 2.0);
        let mut placed = Placed::new(left_title, at);
        placed.rotate = -90.0;
        let mut it = Item::new(Kind::Text, ROLE_AXIS_TITLE);
        it.label = Some(placed);
        it.style.text = Some(Paint::FG);
        titles.push(it);
    }
    if !bottom_title.is_empty() {
        let at = Point::new(
            left + plot_width / 2.0,
            height - pad - bottom_title.height / 2.0,
        );
        let mut it = Item::new(Kind::Text, ROLE_AXIS_TITLE);
        it.label = Some(Placed::new(bottom_title, at));
        it.style.text = Some(Paint::FG);
        titles.push(it);
    }

    let mut entries = Vec::new();
    let legend_height = legend
        .iter()
        .fold(0.0, |h, (_, label)| h + max(ROW, label.height + 6.0));
    let mut ly = top + max(0.0, (plot_height - legend_height) / 2.0);
    let lx = right + reach + LEGEND_GAP;
    for (index, label) in legend {
        let row = max(ROW, label.height + 6.0);
        if ly + row > height {
            break;
        }
        let y = ly + row / 2.0;
        let color = Paint::series(index);
        let plot = &plots[index];
        let mut it = Item::new(Kind::Node, ROLE_LEGEND);
        it.id = Some(plot.title.clone());
        it.bounds = Rect::new(lx, ly, SWATCH + 8.0 + label.width, row);
        if plot.kind == PlotType::Bar {
            it.shape = Some(Shape::new("rect"));
            let swatch = Rect::new(lx, y - SWATCH / 2.0, SWATCH, SWATCH);
            it.parts = vec![Part::rect(swatch, true, Painting::FillOnly)];
            it.style = Style {
                fill: Some(color),
                radius: Some(2.0),
                ..Style::default()
            };
        } else {
            it.shape = Some(Shape::new("line"));
            let mut p = Path::new();
            p.move_to(lx, y).line_to(lx + SWATCH, y);
            it.parts = vec![Part::path(p, Painting::StrokeOnly)];
            it.style = Style {
                stroke: Some(color),
                stroke_width: Some(2.0),
                cap: Some(Cap::Round),
                ..Style::default()
            };
        }
        if !label.is_empty() {
            let at = Point::new(lx + SWATCH + 8.0 + label.width / 2.0, y);
            it.label = Some(Placed::new(label, at));
        }
        entries.push(it);
        ly += row;
    }

    let mut items = Vec::new();
    if !title.is_empty() {
        let at = Point::new(width / 2.0, pad + title.height / 2.0);
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        it.label = Some(Placed::new(title, at));
        it.style.text = Some(Paint::FG);
        items.push(it);
    }
    if !grid.is_empty() {
        let mut it = Item::new(Kind::Mark, ROLE_GRID);
        it.bounds = grid.bounds();
        it.parts = vec![Part::path(grid, Painting::StrokeOnly)];
        it.style.stroke = Some(Paint::SURFACE_ALT);
        items.push(it);
    }
    items.extend(series);
    items.extend(lines);
    if !axis.is_empty() {
        let mut it = Item::new(Kind::Mark, ROLE_AXIS);
        it.bounds = axis.bounds();
        it.parts = vec![Part::path(axis, Painting::StrokeOnly)];
        it.style.stroke = Some(Paint::BORDER);
        items.push(it);
    }
    items.extend(labels);
    items.extend(titles);
    items.extend(outside);
    items.extend(point_labels);
    items.extend(entries);
    Ok(Scene {
        diagram: "xychart".into(),
        width,
        height,
        font_size: size,
        title: model.acc_title.clone(),
        desc: model.acc_descr.clone(),
        items,
    })
}
