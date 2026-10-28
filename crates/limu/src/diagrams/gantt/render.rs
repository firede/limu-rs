//! A gantt chart laid out as a scene, as Pele's renderGantt in render.ts draws it: a row for
//! each task, on a time axis as wide as the chart may be.

use std::collections::HashMap;

use super::axis::{auto_ticks, interval_ticks, time_format};
use super::dates::{Unit, add, add_days};
use super::db::{GanttDb, Task, value};
use crate::common::{cap_width, line, setting};
use crate::css::{declarations, resolve_style};
use crate::errors::Error;
use crate::js::chars::{has_prefix_fold, space_run};
use crate::js::math::{max, min, round};
use crate::layout::Options;
use crate::scene::style::{Join, Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{Item, Kind, Painting, Part, Path, Placed, Point, Rect, Scene, Shape};
use crate::text::{Setting, layout_label};
use crate::url::link_url;
use crate::value::{Map, Value};

/// The roles of the items of a gantt chart: its title, the band of a section and a section's
/// label, a run of excluded days, the grid of ticks, the axis and a tick's label, a vertical
/// marker and its label, the marker of today, and a task, its bar or diamond with its label.
pub(crate) const ROLE_TITLE: &str = "gantt.title";
pub(crate) const ROLE_SECTION: &str = "gantt.section";
pub(crate) const ROLE_SECTION_LABEL: &str = "gantt.section-label";
pub(crate) const ROLE_EXCLUDED: &str = "gantt.excluded";
pub(crate) const ROLE_GRID: &str = "gantt.grid";
pub(crate) const ROLE_AXIS: &str = "gantt.axis";
pub(crate) const ROLE_TICK: &str = "gantt.tick";
pub(crate) const ROLE_VERT: &str = "gantt.vert";
pub(crate) const ROLE_VERT_LABEL: &str = "gantt.vert-label";
pub(crate) const ROLE_TODAY: &str = "gantt.today";
pub(crate) const ROLE_TASK: &str = "gantt.task";

// The measures of a gantt chart, as Pele's render.ts has them.
const WIDTH: f64 = 800.0;
const MIN_PLOT: f64 = 240.0;
const LABEL_GAP: f64 = 8.0;

/// A setting of the chart's configuration, as Pele's settings read it.
fn option<'a>(config: &'a Map, key: &str) -> Option<&'a Value> {
    config
        .get("gantt")
        .and_then(Value::as_object)
        .and_then(|g| g.get(key))
}

/// A number of the chart's configuration, if it is a finite one, held between least and
/// most, as Pele's setting reads it.
fn number_setting(config: &Map, key: &str, least: f64, most: f64) -> Option<f64> {
    match option(config, key) {
        Some(&Value::Number(v)) if v.is_finite() => Some(min(most, max(least, v))),
        _ => None,
    }
}

/// A string of the chart's configuration, as Pele's text reads it.
fn text_setting<'a>(config: &'a Map, key: &str) -> Option<&'a str> {
    option(config, key).and_then(Value::as_str)
}

/// Whether a setting of the chart's configuration is true, as Pele's flag reads it.
fn flag(config: &Map, key: &str) -> bool {
    matches!(option(config, key), Some(Value::Bool(true)))
}

/// The length of a match of `/<br\s*\/?>/i` at byte i of s, or none: Pele's RE_BREAK, written
/// out by hand.
fn break_at(s: &str, i: usize) -> Option<usize> {
    if !s[i..].starts_with('<') || !has_prefix_fold(&s[i + 1..], "br") {
        return None;
    }
    let mut j = space_run(s, i + 3);
    if s.as_bytes().get(j) == Some(&b'/') {
        j += 1;
    }
    (s.as_bytes().get(j) == Some(&b'>')).then_some(j + 1 - i)
}

/// A task's label laid out on one row, as Pele's oneLine lays it out: `<br>` is a space, and
/// only the first line is kept.
fn one_line(raw: &str, set: &Setting, size: f64) -> Label {
    let mut text = String::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if let Some(n) = break_at(raw, i) {
            text.push(' ');
            i += n;
            continue;
        }
        let c = raw[i..].chars().next().unwrap_or(' ');
        text.push(c);
        i += c.len_utf8();
    }
    let mut label = layout_label(&text, false, set, size, 4000.0, Face::default());
    if label.lines.len() <= 1 {
        return label;
    }
    label.lines.truncate(1);
    label.widths.truncate(1);
    label.width = label.widths[0];
    label.height = label.line_height;
    label
}

/// Whether scheduling gave the task a start and an end, as Pele's scheduled says. One that
/// waits on a task that never resolves has neither.
fn scheduled(t: &Task) -> bool {
    value(t.start_time).is_finite() && value(t.end_time).is_finite()
}

/// Puts each task of a section on the first row that is free when it starts, as Pele's
/// packRows does for Mermaid's compact mode, and gives the number of rows taken.
fn pack_rows(
    tasks: &[usize],
    all: &[Task],
    offset: usize,
    row_of: &mut HashMap<usize, usize>,
) -> usize {
    let mut sorted: Vec<usize> = tasks
        .iter()
        .copied()
        .filter(|&t| scheduled(&all[t]))
        .collect();
    sorted.sort_by(|&a, &b| {
        let d = value(all[a].start_time) - value(all[b].start_time);
        let d = if d == 0.0 || d.is_nan() {
            (all[a].order - all[b].order) as f64
        } else {
            d
        };
        d.partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut free: Vec<f64> = Vec::new();
    for &t in &sorted {
        let start = value(all[t].start_time);
        let mut row = 0;
        while row < free.len() && start < free[row] {
            row += 1;
        }
        if row == free.len() {
            free.push(0.0);
        }
        free[row] = value(all[t].end_time);
        row_of.insert(t, offset + row);
    }
    let mut rows = free.len();
    for &t in tasks {
        row_of.entry(t).or_insert_with(|| {
            rows += 1;
            offset + rows - 1
        });
    }
    rows.max(1)
}

/// A task as it is drawn, Pele's Bar.
struct Bar {
    task: usize,
    label: Label,
    row: usize,
    x0: f64,
    x1: f64,
    label_x: f64,
    inside: bool,
}

/// A path of one line, from one point to another.
fn segment(x0: f64, y0: f64, x1: f64, y1: f64) -> Path {
    let mut p = Path::new();
    p.move_to(x0, y0).line_to(x1, y1);
    p
}

/// A mark that strokes a path, in a style.
fn stroked(role: &'static str, path: Path, style: Style) -> Item {
    let mut it = Item::new(Kind::Mark, role);
    it.bounds = path.bounds();
    it.parts = vec![Part::path(path, Painting::StrokeOnly)];
    it.style = style;
    it
}

/// A mark that is a box, filled and not stroked.
fn filled_box(role: &'static str, b: Rect, fill: Paint) -> Item {
    let mut it = Item::new(Kind::Mark, role);
    it.shape = Some(Shape::new("rect"));
    it.bounds = b;
    it.parts = vec![Part::rect(b, false, Painting::FillAndStroke)];
    it.style = Style {
        fill: Some(fill),
        stroke: Some(Paint::NONE),
        ..Style::default()
    };
    it
}

/// A text item of a label centered at a point.
fn text_item(role: &'static str, id: Option<&str>, label: Label, at: Point) -> Item {
    let mut it = Item::new(Kind::Text, role);
    it.id = id.map(str::to_string);
    let placed = Placed::new(label, at);
    it.bounds = placed.bounds();
    it.label = Some(placed);
    it
}

/// Schedules a gantt chart's tasks and lays the chart out as a scene, as Pele's renderGantt
/// draws it. The tasks are scheduled in the options' time zone, against their time, and the
/// chart is left as it was. Its errors are those of scheduling.
pub(crate) fn layout(db: &GanttDb, config: &Map, o: &Options) -> Result<Scene, Error> {
    let size = o.font_size;
    let set = setting(o);
    let pad = o.pad();
    let now = o.clock();

    let label_size =
        number_setting(config, "fontSize", 4.0, 72.0).unwrap_or_else(|| round(size * 0.8125));
    let section_size = number_setting(config, "sectionFontSize", 4.0, 72.0).unwrap_or(label_size);
    let tick_size = round(size * 0.75);
    let bar_height =
        number_setting(config, "barHeight", 4.0, 400.0).unwrap_or_else(|| round(label_size * 1.85));
    let bar_gap =
        number_setting(config, "barGap", 0.0, 400.0).unwrap_or_else(|| round(bar_height / 4.0));
    let row_height = bar_height + bar_gap;
    let section_styles =
        round(number_setting(config, "numberSectionStyles", 1.0, 1000.0).unwrap_or(4.0));
    let compact = db.display_mode == "compact";
    let top_axis = db.top_axis || flag(config, "topAxis");

    let mut schedule = db.schedule(now, o.time_zone)?;
    let z = schedule.zone;
    let tasks = schedule.tasks.clone();
    let row_tasks: Vec<usize> = (0..tasks.len()).filter(|&t| !tasks[t].vert).collect();
    let mut verts: Vec<usize> = (0..tasks.len())
        .filter(|&t| tasks[t].vert && scheduled(&tasks[t]))
        .collect();

    // The sections, in the order their first tasks come.
    let mut categories: Vec<&str> = Vec::new();
    for &t in &row_tasks {
        if !categories.contains(&tasks[t].section.as_str()) {
            categories.push(&tasks[t].section);
        }
    }
    let category = |section: &str| categories.iter().position(|&s| s == section).unwrap_or(0);

    let mut row_of: HashMap<usize, usize> = HashMap::new();
    let mut row_section: Vec<&str> = Vec::new();
    if compact {
        for &section in &categories {
            let group: Vec<usize> = row_tasks
                .iter()
                .copied()
                .filter(|&t| tasks[t].section == section)
                .collect();
            let rows = pack_rows(&group, &tasks, row_section.len(), &mut row_of);
            row_section.extend(std::iter::repeat_n(section, rows));
        }
    } else {
        for &t in &row_tasks {
            row_of.insert(t, row_section.len());
            row_section.push(&tasks[t].section);
        }
    }
    let row_count = row_section.len();

    let (mut start, mut stop) = (f64::INFINITY, f64::NEG_INFINITY);
    for t in tasks.iter().filter(|t| scheduled(t)) {
        start = min(start, value(t.start_time));
        stop = max(stop, value(t.end_time));
    }
    let has_plot = start <= stop;

    let title_raw = db.title.as_deref().unwrap_or("");
    let title = layout_label(title_raw, false, &set, size, 4000.0, Face::BOLD);
    let title_height = if title.height > 0.0 {
        title.height + 16.0
    } else {
        0.0
    };
    let mut sc = Scene {
        diagram: "gantt".into(),
        width: 0.0,
        height: 0.0,
        font_size: size,
        title: db.acc_title.clone(),
        desc: db.acc_descr.clone(),
        items: Vec::new(),
    };
    // The chart's title, which Pele's renderGantt writes first.
    let title_item = |width: f64| -> Option<Item> {
        (!title.is_empty()).then(|| {
            let at = Point::new(width / 2.0, pad + title.height / 2.0);
            text_item(ROLE_TITLE, None, title.clone(), at)
        })
    };

    if !has_plot {
        sc.width = (max(title.width, 1.0) + 2.0 * pad).ceil();
        sc.height = (max(title.height, 1.0) + 2.0 * pad).ceil();
        sc.items.extend(title_item(sc.width));
        return Ok(sc);
    }

    let mut section_labels: Vec<Label> = Vec::new();
    let mut section_width = 0.0;
    for &section in &categories {
        let label = layout_label(section, false, &set, section_size, 180.0, Face::default());
        section_width = max(section_width, label.width);
        section_labels.push(label);
    }

    let axis_format = if !db.axis_format.is_empty() {
        db.axis_format.as_str()
    } else if db.date_format == "D" {
        "%d"
    } else {
        text_setting(config, "axisFormat").unwrap_or("%Y-%m-%d")
    };
    // Pele's tickLabel: a tick's time in the axis format.
    let tick_label = |t: f64| time_format(z, axis_format, t);
    let regular = Face::default();
    let sample_width = max(
        set.width(&tick_label(start), tick_size, regular),
        set.width(&tick_label(stop), tick_size, regular),
    );

    let left_padding = number_setting(config, "leftPadding", 0.0, 2000.0).unwrap_or_else(|| {
        let sections = if section_width > 0.0 {
            section_width + 24.0
        } else {
            0.0
        };
        max(sections, sample_width / 2.0)
    });
    let left = pad + left_padding;
    let right_padding = number_setting(config, "rightPadding", 0.0, 2000.0)
        .unwrap_or_else(|| max(16.0, sample_width / 2.0));
    let natural = number_setting(config, "useWidth", 100.0, 20000.0).unwrap_or(WIDTH);
    let target = cap_width(o, natural, 280.0, 0.0);
    let plot_width = max(MIN_PLOT, target - left - right_padding - pad);
    let right = left + plot_width;
    let scale = if stop > start {
        plot_width / (stop - start)
    } else {
        0.0
    };
    // Pele's x: where a time falls on the plot, its middle where the chart spans no time.
    let x = |t: f64| {
        if scale == 0.0 {
            left + plot_width / 2.0
        } else {
            left + (t - start) * scale
        }
    };

    let tick_height = round(tick_size * 1.75);
    let top_padding = number_setting(config, "topPadding", 0.0, 2000.0);
    let grid_start = number_setting(config, "gridLineStartPadding", 0.0, 2000.0);
    let head_height = pad + title_height + if top_axis { tick_height } else { 0.0 };
    let plot_top = max(head_height, top_padding.unwrap_or(0.0));
    // Mermaid starts its grid lines above the first row by the difference between these two
    // settings. Here that only uses room a larger topPadding leaves free.
    let grid_lead = match (top_padding, grid_start) {
        (Some(top), Some(grid)) => max(0.0, min(top - grid, plot_top - head_height)),
        _ => 0.0,
    };
    let plot_height = max(row_count as f64, 1.0) * row_height;
    let plot_bottom = plot_top + plot_height;

    // Bars and where their labels go: inside where they fit, and otherwise beside the bar on
    // the side with room.
    let mut bars: Vec<Bar> = Vec::new();
    let mut extent = right + right_padding;
    for &t in &row_tasks {
        let task = &tasks[t];
        let label = one_line(&task.task, &set, label_size);
        let row = row_of[&t];
        if !scheduled(task) {
            let label_x = left + LABEL_GAP + label.width / 2.0;
            extent = max(extent, label_x + label.width / 2.0);
            bars.push(Bar {
                task: t,
                label,
                row,
                x0: f64::NAN,
                x1: f64::NAN,
                label_x,
                inside: false,
            });
            continue;
        }
        let mut x0 = x(value(task.start_time));
        let end = task.render_end_time.or(task.end_time);
        let grow = if task.milestone { 0.0 } else { 2.0 };
        let mut x1 = max(x(value(end)), x0 + grow);
        if task.milestone {
            let mid = x0 + (x(value(task.end_time)) - x0) / 2.0;
            x0 = mid - bar_height / 2.0;
            x1 = mid + bar_height / 2.0;
        }
        let inside = !task.milestone && label.width + 2.0 * LABEL_GAP <= x1 - x0;
        let mut label_x = (x0 + x1) / 2.0;
        if !inside {
            let fits_right = x1 + LABEL_GAP + label.width <= right + right_padding;
            let fits_left = x0 - LABEL_GAP - label.width >= left;
            label_x = if fits_right || !fits_left {
                x1 + LABEL_GAP + label.width / 2.0
            } else {
                x0 - LABEL_GAP - label.width / 2.0
            };
            extent = max(extent, label_x + label.width / 2.0);
        }
        bars.push(Bar {
            task: t,
            label,
            row,
            x0,
            x1,
            label_x,
            inside,
        });
    }

    let interval = match db.tick_interval.as_deref() {
        Some(i) if !i.is_empty() => i,
        _ => text_setting(config, "tickInterval").unwrap_or(""),
    };
    let weekday = db
        .weekday
        .as_deref()
        .or_else(|| text_setting(config, "weekday"))
        .unwrap_or("sunday");
    let count = max(2.0, min(10.0, (plot_width / (sample_width + 24.0)).floor()));
    let ticks = interval_ticks(z, interval, weekday, start, stop, (plot_width / 3.0).ceil())
        .unwrap_or_else(|| auto_ticks(z, start, stop, count));

    let mut grid = Path::new();
    let (mut axis_bottom, mut axis_top) = (Vec::new(), Vec::new());
    let mut last_right = f64::NEG_INFINITY;
    let mut last_label = String::new();
    let bottom_y = plot_bottom + tick_height / 2.0 + 2.0;
    let top_y = plot_top - grid_lead - tick_height / 2.0 - 2.0;
    for &t in &ticks {
        let tx = x(t);
        grid.move_to(tx, plot_top - grid_lead)
            .line_to(tx, plot_bottom);
        let label = tick_label(t);
        let width = set.width(&label, tick_size, regular);
        // A label is skipped where it would run into the one before it or only repeat it.
        // Its grid line stays.
        if tx - width / 2.0 < last_right + 8.0 || label == last_label {
            continue;
        }
        last_right = tx + width / 2.0;
        extent = max(extent, last_right);
        let l = line(&label, width, tick_size, regular);
        last_label = label;
        let mut tick = text_item(ROLE_TICK, None, l, Point::new(tx, bottom_y));
        tick.style.text = Some(Paint::MUTED);
        if top_axis {
            let mut top = tick.clone();
            top.move_by(0.0, top_y - bottom_y);
            axis_top.push(top);
        }
        axis_bottom.push(tick);
    }

    // Vertical markers take no row. Their labels go under the axis, on as many lines as it
    // takes to keep them apart.
    let (mut vert_lines, mut vert_labels) = (Vec::new(), Vec::new());
    let mut vert_ends: Vec<f64> = Vec::new();
    let vert_top = plot_bottom + tick_height + 4.0;
    let vert_line = round(label_size * 1.5);
    verts.sort_by(|&a, &b| {
        let d = value(tasks[a].start_time) - value(tasks[b].start_time);
        d.partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal)
    });
    for &t in &verts {
        let task = &tasks[t];
        let vx = x(value(task.start_time));
        let label = one_line(&task.task, &set, label_size);
        let cx = max(pad + label.width / 2.0, vx);
        let mut row = 0;
        while row < vert_ends.len() && cx - label.width / 2.0 < vert_ends[row] + 8.0 {
            row += 1;
        }
        if row == vert_ends.len() {
            vert_ends.push(0.0);
        }
        vert_ends[row] = cx + label.width / 2.0;
        extent = max(extent, vert_ends[row]);
        let path = segment(vx, plot_top - grid_lead, vx, plot_bottom);
        let mut it = stroked(
            ROLE_VERT,
            path,
            Style {
                fill: Some(Paint::NONE),
                stroke: Some(Paint::LINE),
                stroke_width: Some(1.5),
                dash: Some(vec![4.0, 3.0]),
                ..Style::default()
            },
        );
        it.id = Some(task.id.clone());
        vert_lines.push(it);
        if !label.is_empty() {
            let at = Point::new(cx, vert_top + row as f64 * vert_line + vert_line / 2.0);
            vert_labels.push(text_item(ROLE_VERT_LABEL, Some(&task.id), label, at));
        }
    }

    let vert_room = if vert_ends.is_empty() {
        0.0
    } else {
        4.0 + vert_ends.len() as f64 * vert_line
    };
    sc.width = (max(extent, pad + title.width) + pad).ceil();
    sc.height = (plot_bottom + tick_height + vert_room + pad).ceil();

    // Section bands, one for each run of rows that share a section, shaded on alternate
    // sections.
    let (mut bands, mut section_out) = (Vec::new(), Vec::new());
    let mut row = 0;
    while row < row_count {
        let section = row_section[row];
        let mut end = row + 1;
        while end < row_count && row_section[end] == section {
            end += 1;
        }
        let c = category(section);
        let index = c as f64 % section_styles;
        let y = plot_top + row as f64 * row_height;
        let h = (end - row) as f64 * row_height;
        if categories.len() > 1 || !section.is_empty() {
            let fill = if index % 2.0 == 0.0 {
                Paint::SURFACE
            } else {
                Paint::NONE
            };
            let mut it = filled_box(ROLE_SECTION, Rect::new(0.0, y, sc.width, h), fill);
            it.id = Some(section.to_string());
            it.tags = vec![format!("section-{}", super::dates::num_str(index))];
            bands.push(it);
        }
        let label = &section_labels[c];
        if !label.is_empty() {
            let at = Point::new(pad + 8.0 + label.width / 2.0, y + h / 2.0);
            section_out.push(text_item(
                ROLE_SECTION_LABEL,
                Some(section),
                label.clone(),
                at,
            ));
        }
        row = end;
    }

    // Excluded days, as Mermaid marks them: a day at a time from the first task's start, and
    // not at all where the chart spans more than five years.
    let mut excluded = Vec::new();
    if (!db.excludes.is_empty() || !db.includes.is_empty()) && add(z, start, 6.0, Unit::Y) > stop {
        let mut run: Option<(f64, f64)> = None;
        // Pele's flush: it marks the run of excluded days, if there is one.
        let mut flush = |run: &mut Option<(f64, f64)>| {
            let Some((from, to)) = run.take() else {
                return;
            };
            let day_start = z.set_hours(from, 0.0, 0.0, 0.0, 0.0);
            let day_end = z.set_hours(to, 23.0, 59.0, 59.0, 999.0);
            let ex0 = max(left, x(day_start));
            let ex1 = min(right, x(day_end));
            if ex1 > ex0 {
                let b = Rect::new(ex0, plot_top, ex1 - ex0, plot_height);
                excluded.push(filled_box(ROLE_EXCLUDED, b, Paint::SURFACE_ALT));
            }
        };
        let mut day = start;
        while day <= stop {
            if schedule.is_invalid_date(day)? {
                run = Some((run.map_or(day, |r| r.0), day));
            } else {
                flush(&mut run);
            }
            day = add_days(z, day, 1.0);
        }
        flush(&mut run);
    }

    let schemes = o.schemes();
    let mut task_items = Vec::new();
    for bar in bars {
        let task = &tasks[bar.task];
        let cy = plot_top + bar.row as f64 * row_height + row_height / 2.0;
        let state = if task.active {
            "active"
        } else if task.done {
            "done"
        } else {
            ""
        };
        // The classes Pele gives the task besides pele-task, the task's tags.
        let mut tags: Vec<String> = Vec::new();
        if !state.is_empty() {
            tags.push(state.into());
        }
        if task.crit {
            tags.push("crit".into());
        }
        if task.milestone {
            tags.push("milestone".into());
        }
        if task.classes.iter().any(|c| c == "clickable") {
            tags.push("clickable".into());
        }
        let mut it = Item::new(Kind::Node, ROLE_TASK);
        it.id = Some(task.id.clone());
        if bar.x0.is_nan() {
            tags.push("unscheduled".into());
            it.style.text = Some(Paint::MUTED);
        } else {
            // A critical task takes the critical color in place of the accent. Done, it keeps
            // the fill of finished work and only its border is in that color.
            let color = if task.crit {
                Paint::CRITICAL
            } else {
                Paint::ACCENT
            };
            match state {
                "done" => it.style.fill = Some(Paint::SURFACE_ALT),
                "active" => {
                    it.style.fill = Some(color);
                    it.style.fill_opacity = Some(0.25);
                }
                _ => it.style.fill = Some(color),
            }
            it.style.stroke = Some(if state == "done" && !task.crit {
                Paint::BORDER
            } else {
                color
            });
            if task.milestone {
                let cx = (bar.x0 + bar.x1) / 2.0;
                let r = bar_height / 2.0;
                let mut path = Path::new();
                path.polygon(&[(cx, cy - r), (cx + r, cy), (cx, cy + r), (cx - r, cy)]);
                it.shape = Some(Shape::new("milestone"));
                it.bounds = path.bounds();
                it.parts = vec![Part::path(path, Painting::FillAndStroke)];
                // Pele turns a milestone's corners round.
                it.style.join = Some(Join::Round);
            } else {
                let b = Rect::new(bar.x0, cy - bar_height / 2.0, bar.x1 - bar.x0, bar_height);
                it.shape = Some(Shape::new("rect"));
                it.bounds = b;
                it.parts = vec![Part::rect(b, true, Painting::FillAndStroke)];
            }
            if bar.inside {
                if state.is_empty() {
                    it.style.text = Some(Paint::BG);
                }
            } else if state == "done" {
                it.style.text = Some(Paint::MUTED);
            }
        }
        it.tags = tags;
        if !bar.label.is_empty() {
            let placed = Placed::new(bar.label, Point::new(bar.label_x, cy));
            if it.parts.is_empty() {
                it.bounds = placed.bounds();
            }
            it.label = Some(placed);
        }
        if let Some(url) = db.link(&task.id) {
            it.link = link_url(url, &schemes);
        }
        task_items.push(it);
    }

    let mut today = Vec::new();
    if db.today_marker != "off" && now >= start && now <= stop && stop > start {
        let tx = x(now);
        let top = plot_top - grid_lead;
        let mut it = stroked(
            ROLE_TODAY,
            segment(tx, top, tx, plot_bottom),
            Style {
                fill: Some(Paint::ACCENT),
                stroke: Some(Paint::ACCENT),
                stroke_width: Some(2.0),
                ..Style::default()
            },
        );
        let decls: Vec<String> = db.today_marker.split(',').map(str::to_string).collect();
        if !db.today_marker.is_empty() && resolve_style(&decls).line_declared() {
            // The author's style is that of a line: it fills nothing and colors no text.
            let mut author = declarations(&decls).style;
            author.fill = None;
            author.text = None;
            it.author = author;
        } else {
            let mut mark = Path::new();
            mark.move_to(tx - 4.0, top - 5.0)
                .line_to(tx + 4.0, top - 5.0)
                .line_to(tx, top + 1.0)
                .close();
            it.parts.push(Part::path(mark, Painting::FillOnly));
            it.bounds = Rect::new(tx - 4.0, top - 5.0, 8.0, plot_bottom - (top - 5.0));
        }
        today.push(it);
    }

    let mut axis_line = segment(left, plot_bottom, right, plot_bottom);
    if top_axis {
        axis_line
            .move_to(left, plot_top - grid_lead)
            .line_to(right, plot_top - grid_lead);
    }
    let mut lines = Vec::new();
    if !grid.is_empty() {
        lines.push(stroked(
            ROLE_GRID,
            grid,
            Style {
                fill: Some(Paint::NONE),
                stroke: Some(Paint::BORDER),
                dash: Some(vec![1.0, 3.0]),
                ..Style::default()
            },
        ));
    }
    lines.push(stroked(
        ROLE_AXIS,
        axis_line,
        Style {
            fill: Some(Paint::NONE),
            stroke: Some(Paint::BORDER),
            ..Style::default()
        },
    ));

    sc.items.extend(title_item(sc.width));
    for layer in [
        bands,
        excluded,
        lines,
        vert_lines,
        today,
        task_items,
        section_out,
        axis_bottom,
        axis_top,
        vert_labels,
    ] {
        sc.items.extend(layer);
    }
    Ok(sc)
}
