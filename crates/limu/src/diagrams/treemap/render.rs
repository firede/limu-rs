//! A treemap laid out as a scene, as Pele's renderTreemap in render.ts draws it.

use super::format::formatter;
use super::layout::{Insets, layout_treemap};
use super::model::TreemapModel;
use crate::common::{cap_width, class_names, fit_text, line, setting, title_room};
use crate::css::{declarations, resolve_style};
use crate::js::chars::trim;
use crate::js::math::{log10, max, min, round};
use crate::layout::Options;
use crate::scene::style::{Paint, Style};
use crate::scene::text::Face;
use crate::scene::{Align, Item, Kind, Painting, Part, Placed, Point, Rect, Scene, Shape};
use crate::text::{decode_entities, layout_label};
use crate::value::{Map, Value};

/// The roles of the items of a treemap: a section's box, with its name in its header,
/// aligned at its start, a leaf's box, with its name centered in it, the value of either,
/// aligned at the end of a section's header or centered under a leaf's name, and the title
/// above the treemap.
pub(crate) const ROLE_SECTION: &str = "treemap.section";
pub(crate) const ROLE_LEAF: &str = "treemap.leaf";
pub(crate) const ROLE_VALUE: &str = "treemap.value";
pub(crate) const ROLE_TITLE: &str = "treemap.title";

// The room a section keeps for its header, or for a short one, and about its children at
// most, and the height of a line of a leaf's text, in ems, as Pele's HEADER, SHORT_HEADER,
// SIDE and LINE.
const HEADER: f64 = 28.0;
const SHORT_HEADER: f64 = 18.0;
const SIDE: f64 = 6.0;
const LINE: f64 = 1.25;

/// A name as a tooltip writes it, as Pele's titlePart gives it: its entities decoded and
/// trimmed, without the characters XML does not allow, and trimmed again.
fn title_part(raw: &str) -> String {
    let s: String = trim(&decode_entities(raw))
        .chars()
        .filter(|&c| {
            !(c < ' ' && !matches!(c, '\t' | '\n' | '\r')
                || matches!(c, '\x7f' | '\u{fffe}' | '\u{ffff}'))
        })
        .collect();
    trim(&s).to_string()
}

/// How many leaves of positive value the tree holds, as Pele's leafCount counts them.
fn leaf_count(model: &TreemapModel) -> usize {
    let mut count = 0;
    let mut stack = model.roots.clone();
    while let Some(i) = stack.pop() {
        let node = &model.nodes[i];
        match &node.children {
            Some(children) => stack.extend(children),
            None => {
                if node.value.is_some_and(|v| v.is_finite() && v > 0.0) {
                    count += 1;
                }
            }
        }
    }
    count
}

/// A number of the treemap's configuration, at most hi, or fallback for anything but a
/// number of at least lo, as Pele's option reads it.
fn option(config: &Map, key: &str, fallback: f64, lo: f64, hi: f64) -> f64 {
    let value = config
        .get("treemap")
        .and_then(Value::as_object)
        .and_then(|s| s.get(key));
    match value {
        Some(&Value::Number(v)) if v >= lo => min(v, hi),
        _ => fallback,
    }
}

/// Lays a treemap out as a scene, as Pele's renderTreemap does: the tree squarified into a
/// rectangle ten times the nodes' width by ten times their height, or the host's width where
/// that is less, sections tinted in the color of the top-level branch they are in, with their
/// names in their headers, and leaves filled in it, with their names and values in them in
/// the largest type that fits, or cut short at the size a class sets. Boxes are spaced less
/// the more leaves share the area. A section too small for a header draws no box and gives
/// its children all of it, and their tooltips name it.
pub(crate) fn layout(model: &TreemapModel, config: &Map, o: &Options) -> Scene {
    let size = o.font_size;
    let small = round(size * 0.875);
    let tiny = round(size * 0.75);
    let least = round(size * 0.5625);
    let set = setting(o);
    let empty = Map::new();
    let treemap = config
        .get("treemap")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    // Pele takes the host's padding where it gives one, and the treemap's diagramPadding
    // where it does not. The host's padding is always given here, 8 by default, so the
    // treemap's own takes its place where the configuration gives one.
    let pad = option(config, "diagramPadding", o.padding, 0.0, 2000.0);

    // Mermaid sizes the diagram as ten times nodeWidth by ten times nodeHeight.
    let natural = option(config, "nodeWidth", 64.0, 1.0, 400.0) * 10.0;
    let width = round(cap_width(o, natural, 200.0, 2.0 * pad));
    let height = round(option(config, "nodeHeight", 40.0, 1.0, 400.0) * 10.0);
    let show_values = treemap.get("showValues") != Some(&Value::Bool(false));
    let format = formatter(
        treemap
            .get("valueFormat")
            .and_then(Value::as_str)
            .unwrap_or(","),
    );
    // Gaps and insets shrink as the average area of a leaf does.
    let leaves = leaf_count(model).max(1) as f64;
    let crowd = min(1.0, max(0.0, log10((width * height) / leaves / 800.0)));
    let (boxes, placed) = layout_treemap(
        model,
        width,
        height,
        &Insets {
            gap: option(config, "padding", round(2.0 + 2.0 * crowd), 0.0, 200.0),
            header: HEADER,
            short: SHORT_HEADER,
            side: round(3.0 + 3.0 * crowd),
        },
    );

    let raw = model.title.as_deref().unwrap_or("");
    let title = layout_label(raw, false, &set, size, title_room(o), Face::BOLD);
    let title_height = if title.height > 0.0 {
        title.height + 8.0
    } else {
        0.0
    };
    let left = pad + max(0.0, (title.width - width) / 2.0);
    let top = pad + title_height;

    let (mut sections, mut leaves) = (Vec::new(), Vec::new());
    for &k in &placed {
        let b = &boxes[k];
        let node = &model.nodes[b.node];
        let css = node.css.as_deref().unwrap_or(&[]);
        let author = declarations(css);
        let resolved = resolve_style(css);
        // The value's text takes what of the author's style a text does.
        let value_author = Style {
            text: author.style.text,
            opacity: author.style.opacity,
            ..Style::default()
        };
        // Text on a fill the author sets is drawn in the color that stands out on it, as
        // Pele's ink picks it: a name on its box's own fill, and a value, an item of its own,
        // on the box's fill as its ground.
        let on_fill = resolved.inks_text(&resolved);
        let ground = if on_fill { author.style.fill } else { None };
        let color = Paint::series(b.series);
        let (x, y) = (left + b.x, top + b.y);
        let name = trim(&decode_entities(&node.name)).to_string();
        let value = if show_values {
            format(b.value)
        } else {
            String::new()
        };
        // A tooltip names the sections above with no header, as their children cover them.
        let own = title_part(&node.name);
        let mut path = Vec::new();
        if !own.is_empty() {
            path.push(own.clone());
        }
        let mut p = b.parent;
        while let Some(i) = p.filter(|&i| !boxes[i].headed) {
            let section = title_part(&model.nodes[boxes[i].node].name);
            if !section.is_empty() {
                path.insert(0, section);
            }
            p = boxes[i].parent;
        }
        let full = path.join(" › ");
        let tooltip = |drawn: Option<&str>| {
            (!(full.is_empty() || drawn == Some(name.as_str()) && full == own))
                .then(|| full.clone())
        };
        // A size the author sets is kept, as Pele's inline style overrides the size it
        // writes, so text is cut short instead of made smaller.
        let fixed = author.font_size;
        let mut face = Face::default();
        if author.bold {
            face |= Face::BOLD;
        }
        if author.italic {
            face |= Face::ITALIC;
        }
        let rect = Rect::new(x, y, b.w, b.h);
        let mut it = Item::new(Kind::Node, ROLE_LEAF);
        it.shape = Some(Shape::new("rect"));
        it.id = Some(node.name.clone());
        it.class = class_names(node.class_selector.as_deref().unwrap_or(""));
        it.bounds = rect;
        it.parts = vec![Part::rect(rect, true, Painting::FillAndStroke)];
        it.author = author.style.clone();
        // The corners of a small box are rounded by the theme's radius and a quarter of its
        // shorter side at most, as Pele's corners caps them where the cap is under its
        // default radius of 4.
        let cap = min(b.w, b.h) / 4.0;
        if cap < 4.0 {
            it.style.max_radius = Some(cap);
        }

        if node.children.is_some() {
            it.kind = Kind::Group;
            it.role = ROLE_SECTION.into();
            if !b.headed {
                // A section with no header draws no box, so that its children blend into the
                // grid.
                it.parts = vec![Part::rect(rect, false, Painting::FillAndStroke)];
                it.style = Style {
                    fill: Some(Paint::NONE),
                    stroke: Some(Paint::NONE),
                    ..Style::default()
                };
                it.author = Style::default();
                it.tooltip = tooltip(None);
                sections.push(it);
                continue;
            }
            // A section is a tint of its color, unless a class gives it a fill of its own.
            it.style.fill = Some(color);
            it.style.stroke = Some(color);
            it.style.text = Some(Paint::FG);
            it.style.on_fill = on_fill;
            if !resolved.fill_declared() {
                it.style.fill_opacity = Some(0.12);
            }
            let short = b.header < HEADER;
            let label_size = fixed.unwrap_or(if short { tiny } else { small });
            let value_size = fixed.unwrap_or(tiny);
            let room = b.w - 2.0 * SIDE - 4.0;
            let value_width = if value.is_empty() {
                0.0
            } else {
                set.width(&value, value_size, face)
            };
            let with_value =
                !short && value_width > 0.0 && room - value_width - 12.0 >= 3.0 * label_size;
            let fit = if with_value {
                room - value_width - 12.0
            } else {
                room
            };
            let baseline = y + b.header / 2.0 + 1.0;
            let label = if label_size * LINE < b.header {
                fit_text(&name, fit, label_size, label_size, &set, face | Face::BOLD)
            } else {
                None
            };
            it.tooltip = tooltip(label.as_ref().map(|f| f.text.as_str()));
            if let Some(f) = label {
                let l = line(&f.text, f.width, f.size, face | Face::BOLD);
                let mut placed = Placed::new(l, Point::new(x + SIDE + 2.0, baseline));
                placed.align = Align::Start;
                it.label = Some(placed);
            }
            sections.push(it);
            if with_value && value_size * LINE <= HEADER {
                let mut v = Item::new(Kind::Text, ROLE_VALUE);
                v.id = Some(node.name.clone());
                let l = line(&value, value_width, value_size, face);
                let mut placed = Placed::new(l, Point::new(x + b.w - SIDE - 2.0, baseline));
                placed.align = Align::End;
                v.label = Some(placed);
                v.style = Style {
                    text: Some(Paint::MUTED),
                    ground,
                    ..Style::default()
                };
                v.author = value_author;
                sections.push(v);
            }
            continue;
        }

        it.style.fill = Some(color);
        it.style.stroke = Some(Paint::NONE);
        it.style.text = Some(Paint::BG);
        it.style.on_fill = on_fill;
        let room = b.w - 8.0;
        let (smallest, tallest) = match fixed {
            None => (least, min(size, ((b.h - 4.0) / LINE).floor())),
            Some(f) => (f, if f * LINE <= b.h - 4.0 { f } else { 0.0 }),
        };
        let label = if tallest >= smallest {
            fit_text(&name, room, tallest, smallest, &set, face)
        } else {
            None
        };
        it.tooltip = tooltip(label.as_ref().map(|f| f.text.as_str()));
        let mut shown = None;
        if let Some(f) = label {
            let (cx, cy) = (x + b.w / 2.0, y + b.h / 2.0);
            let value_size = fixed.unwrap_or_else(|| max(least, round(f.size * 0.8)));
            let lines = (f.size + value_size) * LINE;
            let mut at = Point::new(cx, cy);
            let fits = !value.is_empty() && lines <= b.h - 4.0;
            let value_width = if fits {
                set.width(&value, value_size, face)
            } else {
                0.0
            };
            if fits && value_width <= room {
                at.y = cy - lines / 2.0 + (f.size * LINE) / 2.0;
                let mut v = Item::new(Kind::Text, ROLE_VALUE);
                v.id = Some(node.name.clone());
                let l = line(&value, value_width, value_size, face);
                let below = Point::new(cx, cy + lines / 2.0 - (value_size * LINE) / 2.0);
                v.label = Some(Placed::new(l, below));
                v.style = Style {
                    text: Some(Paint::BG),
                    ground,
                    ..Style::default()
                };
                v.author = value_author;
                shown = Some(v);
            }
            it.label = Some(Placed::new(line(&f.text, f.width, f.size, face), at));
        }
        leaves.push(it);
        leaves.extend(shown);
    }

    let total_width = (max(width, title.width) + 2.0 * pad).ceil();
    let mut sc = Scene {
        diagram: "treemap".into(),
        width: total_width,
        height: (height + title_height + 2.0 * pad).ceil(),
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
    sc.items.extend(sections);
    sc.items.extend(leaves);
    sc
}
