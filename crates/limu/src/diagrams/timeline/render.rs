//! A timeline laid out as a scene, as Pele's renderTimeline in render.ts draws it.

use super::TimelineDb;
use crate::common::{fit_label, plain_label, setting, turn_to_fit};
use crate::js::chars::trim;
use crate::js::math::{max, min, round};
use crate::layout::Options;
use crate::scene::style::{Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{
    Edge, Item, Kind, MARKER_SIZE, Marker, MarkerType, Painting, Part, Path, Placed, Point, Rect,
    Scene, Shape,
};
use crate::value::{Map, Value};

/// The roles of the items of a timeline: its title, the axis the periods lie along, a section
/// with its label, a period with its label, the line from a period to the axis and on to its
/// events, the dot where it meets the axis, and an event with its label.
pub(crate) const ROLE_TITLE: &str = "timeline.title";
pub(crate) const ROLE_AXIS: &str = "timeline.axis";
pub(crate) const ROLE_SECTION: &str = "timeline.section";
pub(crate) const ROLE_PERIOD: &str = "timeline.period";
pub(crate) const ROLE_CONNECTOR: &str = "timeline.connector";
pub(crate) const ROLE_DOT: &str = "timeline.dot";
pub(crate) const ROLE_EVENT: &str = "timeline.event";

// The measures of a timeline, as Pele's render.ts has them.
const PAD_X: f64 = 12.0;
const PAD_Y: f64 = 8.0;
const MIN_TEXT: f64 = 152.0;
const MAX_TEXT: f64 = 216.0;
const EVENT_TEXT_TD: f64 = 264.0;
const GAP: f64 = 20.0;
const AXIS: f64 = 20.0;
const SPINE: f64 = 14.0;
const EVENT_GAP: f64 = 8.0;
const SECTION_GAP: f64 = 8.0;
const TAIL: f64 = 12.0;
const ARROW: f64 = MARKER_SIZE;

/// A section and the periods in it, Pele's Group. Its section is -1 for the periods that
/// come before the first section.
struct Group {
    section: isize,
    periods: Vec<usize>,
}

/// Lays a timeline out as a scene, as Pele's renderTimeline draws it: across, unless the text
/// asks for it to run down or it runs down to fit the host's width.
pub(crate) fn layout(db: &TimelineDb, config: &Map, o: &Options) -> Scene {
    if db.direction == "TD" {
        return draw(db, config, o, true);
    }
    turn_to_fit(o, |down| draw(db, config, o, down))
}

/// A box whose corners take the theme's radius, as Pele's box draws it, with its label
/// centered in it.
fn rounded_box(kind: Kind, role: &'static str, r: Rect, label: Label, style: Style) -> Item {
    let mut it = Item::new(kind, role);
    it.shape = Some(Shape::new("rounded"));
    it.bounds = r;
    it.parts = vec![Part::rect(r, true, Painting::FillOnly)];
    it.label = Some(Placed::new(
        label,
        Point::new(r.x + r.w / 2.0, r.y + r.h / 2.0),
    ));
    it.style = style;
    it
}

/// The line from a period to the axis and on to its events.
fn connector(d: Path) -> Item {
    let mut it = Item::new(Kind::Mark, ROLE_CONNECTOR);
    it.bounds = d.bounds();
    it.parts = vec![Part::path(d, Painting::StrokeOnly)];
    it.style.stroke = Some(Paint::BORDER);
    it
}

/// The dot where a period meets the axis.
fn dot(x: f64, y: f64, color: Paint) -> Item {
    let mut p = Path::new();
    p.ellipse(x, y, 4.0, 4.0);
    let mut it = Item::new(Kind::Mark, ROLE_DOT);
    it.shape = Some(Shape::new("circle"));
    it.bounds = Rect::new(x - 4.0, y - 4.0, 8.0, 8.0);
    it.parts = vec![Part::path(p, Painting::FillAndStroke)];
    it.style = Style {
        fill: Some(color),
        stroke: Some(Paint::BG),
        stroke_width: Some(2.0),
        ..Style::default()
    };
    it
}

/// The axis, a line from (x0, y0) to a tip at (x1, y1), which it stops short of for the
/// arrowhead there, pointing along (dx, dy).
fn axis_line(x0: f64, y0: f64, x1: f64, y1: f64, dx: f64, dy: f64) -> Item {
    let mut p = Path::new();
    p.move_to(x0, y0);
    if dx != 0.0 {
        p.line_to(x1 - ARROW + 1.0, y0);
    } else {
        p.line_to(x0, y1 - ARROW + 1.0);
    }
    let tip = Point::new(x1, y1);
    let mut it = Item::new(
        Kind::Edge(Edge {
            route: vec![Point::new(x0, y0), tip],
            ends: [
                None,
                Some(Marker {
                    kind: MarkerType::ArrowPoint,
                    at: tip,
                    dx,
                    dy,
                }),
            ],
        }),
        ROLE_AXIS,
    );
    it.bounds = Rect::new(x0, y0, x1 - x0, y1 - y0);
    it.parts = vec![Part::path(p, Painting::StrokeOnly)];
    it.style.stroke = Some(Paint::LINE);
    it
}

/// Lays the timeline out across or down, as Pele's draw does.
fn draw(db: &TimelineDb, config: &Map, o: &Options, vertical: bool) -> Scene {
    let size = o.font_size;
    let small = round(size * 0.875);
    let set = setting(o);
    let pad = o.padding;
    let multicolor = config
        .get("timeline")
        .and_then(Value::as_object)
        .and_then(|s| s.get("disableMulticolor"))
        != Some(&Value::Bool(true));
    let periods = &db.periods;
    let has_sections = !db.sections.is_empty();

    // Mermaid draws nothing for periods that come before the first section. Here they lead,
    // in a neutral color.
    let mut groups: Vec<Group> = (0..db.sections.len())
        .map(|section| Group {
            section: section as isize,
            periods: Vec::new(),
        })
        .collect();
    let mut leading = Group {
        section: -1,
        periods: Vec::new(),
    };
    for (i, period) in periods.iter().enumerate() {
        match usize::try_from(period.section_index)
            .ok()
            .and_then(|k| groups.get_mut(k))
        {
            Some(g) => g.periods.push(i),
            None => leading.periods.push(i),
        }
    }
    if !leading.periods.is_empty() {
        groups.insert(0, leading);
    }

    // Columns widen, up to a point, for a word that would not fit. Past that the word is
    // cut. Running down, the two columns share the width the host has.
    let room = if vertical && o.max_width > 0.0 {
        o.max_width - 2.0 * pad
    } else {
        f64::INFINITY
    };
    let taken = AXIS + SPINE * 2.0 + 4.0 * PAD_X;
    let period_text = min(MIN_TEXT, max(80.0, round((room - taken) * 0.38)));
    let event_text = min(EVENT_TEXT_TD, max(110.0, room - taken - period_text));
    let mut text_w = period_text;
    let event_text_w = |text_w: f64| if vertical { event_text } else { text_w };
    let lay_labels = |lay: fn(&str, &crate::text::Setting, f64, f64, Face) -> Label,
                      text_w: f64| {
        let period_labels: Vec<Label> = periods
            .iter()
            .map(|p| lay(&p.text, &set, size, text_w, Face::default()))
            .collect();
        let event_labels: Vec<Vec<Label>> = periods
            .iter()
            .map(|p| {
                p.events
                    .iter()
                    .map(|e| lay(e, &set, small, event_text_w(text_w), Face::default()))
                    .collect()
            })
            .collect();
        (period_labels, event_labels)
    };
    let (mut period_labels, mut event_labels) = lay_labels(plain_label, text_w);
    let mut widest = 0.0;
    for label in &period_labels {
        widest = max(widest, label.width);
    }
    if !vertical {
        for labels in &event_labels {
            for label in labels {
                widest = max(widest, label.width);
            }
        }
    }
    let mut overflow = widest > text_w;
    if vertical {
        for labels in &event_labels {
            for label in labels {
                overflow = overflow || label.width > event_text;
            }
        }
    }
    if overflow {
        let limit = if room == f64::INFINITY {
            MAX_TEXT
        } else {
            period_text
        };
        text_w = min(max(f64::ceil(widest), period_text), limit);
        (period_labels, event_labels) = lay_labels(fit_label, text_w);
    }

    let line_height = round(size * 1.5);
    let event_line_height = round(small * 1.5);
    let col_w = text_w + 2.0 * PAD_X;
    let event_w = event_text_w(text_w) + 2.0 * PAD_X;
    let chip_height = |label: &Label| max(label.height, line_height) + 2.0 * PAD_Y;
    let event_height = |label: &Label| max(label.height, event_line_height) + 2.0 * PAD_Y;
    let color_of = |g: &Group, period: usize| {
        if g.section >= 0 {
            Paint::series(g.section as usize)
        } else if has_sections {
            Paint::MUTED
        } else {
            Paint::series(if multicolor { period } else { 0 })
        }
    };

    let has_events = event_labels.iter().any(|labels| !labels.is_empty());
    let mut slots = 0;
    for g in &groups {
        slots += g.periods.len().max(1);
    }

    let pitch = col_w + GAP;
    let drawn = !periods.is_empty();
    let body_w = if vertical {
        col_w
            + if has_events {
                AXIS + SPINE * 2.0 + event_w
            } else if drawn {
                AXIS + TAIL
            } else {
                0.0
            }
    } else if slots > 0 {
        slots as f64 * pitch - GAP + if drawn { TAIL + ARROW } else { 0.0 }
    } else {
        0.0
    };
    let title = fit_label(
        db.title.as_deref().unwrap_or(""),
        &set,
        size,
        max(body_w, 320.0),
        Face::BOLD,
    );
    let title_h = if title.height > 0.0 {
        title.height + 16.0
    } else {
        0.0
    };
    let width = max(body_w, title.width);
    let left = pad + (width - body_w) / 2.0;
    let top = pad + title_h;

    let section_label = |g: &Group, w: f64| {
        let name = &db.sections[g.section as usize];
        fit_label(name, &set, size, w - 2.0 * PAD_X, Face::BOLD)
    };
    let mut items: Vec<Item> = Vec::new();
    let header = |items: &mut Vec<Item>, g: &Group, label: Label, r: Rect| {
        let style = Style {
            fill: Some(Paint::series(g.section as usize)),
            text: Some(Paint::BG),
            ..Style::default()
        };
        let mut it = rounded_box(Kind::Group, ROLE_SECTION, r, label, style);
        it.id = Some(trim(&db.sections[g.section as usize]).to_string());
        items.push(it);
    };
    let period = |items: &mut Vec<Item>, i: usize, color: Paint, x: f64, y: f64, h: f64| {
        let style = Style {
            fill: Some(color),
            text: Some(Paint::BG),
            ..Style::default()
        };
        let r = Rect::new(x, y, col_w, h);
        let mut it = rounded_box(Kind::Node, ROLE_PERIOD, r, period_labels[i].clone(), style);
        it.id = Some(trim(&periods[i].text).to_string());
        items.push(it);
    };
    let event = |label: &Label, color: Paint, x: f64, y: f64, h: f64| {
        let style = Style {
            fill: Some(color),
            fill_opacity: Some(0.16),
            ..Style::default()
        };
        let r = Rect::new(x, y, event_w, h);
        rounded_box(Kind::Node, ROLE_EVENT, r, label.clone(), style)
    };

    let height = if vertical {
        let axis_x = left + col_w + AXIS;
        let spine_x = axis_x + SPINE;
        let events_x = spine_x + SPINE;
        let mut y = top;
        for g in &groups {
            if g.section >= 0 {
                let label = section_label(g, body_w);
                let h = chip_height(&label);
                header(&mut items, g, label, Rect::new(left, y, body_w, h));
                y += h + SECTION_GAP + 4.0;
            }
            for &i in &g.periods {
                let color = color_of(g, i);
                let h = chip_height(&period_labels[i]);
                // A period and its first event share one center line, so the line between
                // them runs straight.
                let first = match event_labels[i].first() {
                    Some(label) => event_height(label),
                    None => h,
                };
                let cy = y + max(h, first) / 2.0;
                let mut d = Path::new();
                d.move_to(left + col_w, cy).line_to(axis_x, cy);
                let mut cards = Vec::new();
                let mut ey = cy - first / 2.0;
                let mut last = cy;
                for label in &event_labels[i] {
                    let eh = event_height(label);
                    last = ey + eh / 2.0;
                    d.move_to(spine_x, last).line_to(events_x, last);
                    cards.push(event(label, color, events_x, ey, eh));
                    ey += eh + EVENT_GAP;
                }
                if !cards.is_empty() {
                    d.move_to(axis_x, cy)
                        .line_to(spine_x, cy)
                        .line_to(spine_x, last);
                }
                period(&mut items, i, color, left, cy - h / 2.0, h);
                items.push(connector(d));
                items.push(dot(axis_x, cy, color));
                items.extend(cards);
                y = max(cy + h / 2.0, ey - EVENT_GAP) + GAP;
            }
            if g.periods.is_empty() {
                y += GAP - SECTION_GAP - 4.0;
            }
        }
        if !groups.is_empty() {
            y -= GAP;
        }
        if drawn {
            let tip = y + TAIL + ARROW;
            items.insert(0, axis_line(axis_x, top, axis_x, tip, 0.0, 1.0));
            y = tip;
        }
        y + pad
    } else {
        let mut header_h = 0.0;
        let section_labels: Vec<Option<Label>> = groups
            .iter()
            .map(|g| {
                if g.section < 0 {
                    return None;
                }
                let label = section_label(g, g.periods.len().max(1) as f64 * pitch - GAP);
                header_h = max(header_h, chip_height(&label));
                Some(label)
            })
            .collect();
        let mut chip_h = 0.0;
        for label in &period_labels {
            chip_h = max(chip_h, chip_height(label));
        }
        let periods_y = top
            + if has_sections {
                header_h + SECTION_GAP
            } else {
                0.0
            };
        let axis_y = periods_y + chip_h + if chip_h > 0.0 { AXIS } else { 0.0 };
        let events_y = axis_y + AXIS;
        let mut bottom = if drawn { axis_y + TAIL } else { top + header_h };
        let mut slot = 0;
        for (g, label) in groups.iter().zip(section_labels) {
            let span = g.periods.len().max(1);
            let gx = left + slot as f64 * pitch;
            if let Some(label) = label {
                let r = Rect::new(gx, top, span as f64 * pitch - GAP, header_h);
                header(&mut items, g, label, r);
            }
            for (k, &i) in g.periods.iter().enumerate() {
                let color = color_of(g, i);
                let x = gx + k as f64 * pitch;
                let cx = x + col_w / 2.0;
                let end = if event_labels[i].is_empty() {
                    axis_y
                } else {
                    events_y
                };
                let mut d = Path::new();
                d.move_to(cx, periods_y + chip_h).line_to(cx, end);
                let mut cards = Vec::new();
                let mut ey = events_y;
                for label in &event_labels[i] {
                    let eh = event_height(label);
                    if ey > events_y {
                        d.move_to(cx, ey - EVENT_GAP).line_to(cx, ey);
                    }
                    cards.push(event(label, color, x, ey, eh));
                    ey += eh + EVENT_GAP;
                }
                bottom = max(bottom, ey - EVENT_GAP);
                period(&mut items, i, color, x, periods_y, chip_h);
                items.push(connector(d));
                items.push(dot(cx, axis_y, color));
                items.extend(cards);
            }
            slot += span;
        }
        if drawn {
            items.insert(0, axis_line(left, axis_y, left + body_w, axis_y, 1.0, 0.0));
        }
        bottom + pad
    };

    let total_width = (width + 2.0 * pad).ceil();
    let total_height = max(height, pad * 2.0).ceil();
    if !title.is_empty() {
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        let at = Point::new(total_width / 2.0, pad + title.height / 2.0);
        it.label = Some(Placed::new(title, at));
        it.style.text = Some(Paint::FG);
        items.insert(0, it);
    }
    Scene {
        diagram: "timeline".into(),
        width: total_width,
        height: total_height,
        font_size: size,
        title: db.acc_title.clone(),
        desc: db.acc_descr.clone(),
        items,
    }
}
