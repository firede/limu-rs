//! A tree view laid out as a scene, as Pele's renderTreeView in render.ts draws it: a row to
//! an entry, each indented under its directory, with a guide down from the directory to its
//! entries.
//!
//! An icon the host can draw is a picture of its entry. One of the built-in pack's, or one
//! the host does not draw, is an item of its own right after its entry, Pele's glyph of a
//! folder or a file, stroked round in the muted color.

use std::collections::HashMap;

use super::icons::{BUILTIN_PACK, GLYPH_STROKE, get_node_icon, glyph, icon_config};
use super::model::{NodeType, TreeViewModel};
use crate::common::{self, class_names, has_icon, icon_svg, literal_label, tighten, title_room};
use crate::js::chars::is_space_char;
use crate::js::math::{max, min, round};
use crate::layout::{Monospace, Options};
use crate::scene::style::{Cap, Join, Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{Align, Item, Kind, Painting, Part, Path, Placed, Point, Rect, Scene, Shape};
use crate::text::{decode_entities, layout_label};
use crate::value::{Map, Value};

/// The roles of the items of a tree view: its title above the tree, the guide that runs down
/// from a directory to its entries, with a tick to each, its id the directory's name, an
/// entry, a directory or a file, its name its label, its classes those of its :::class, its
/// id its name, with a box behind its row where the author highlights it, an icon Pele draws
/// itself, right after its entry, its shape the glyph's name and its id the entry's, and the
/// text after an entry's ##.
pub(crate) const ROLE_TITLE: &str = "treeview.title";
pub(crate) const ROLE_GUIDE: &str = "treeview.guide";
pub(crate) const ROLE_DIRECTORY: &str = "treeview.directory";
pub(crate) const ROLE_FILE: &str = "treeview.file";
pub(crate) const ROLE_ICON: &str = "treeview.icon";
pub(crate) const ROLE_DESCRIPTION: &str = "treeview.description";

// The measures of Pele's render.ts: the room between an icon and its name, between the
// widest name and the column of descriptions, how far in from an entry its guide runs, the
// most the guide's corner is rounded by, and how far a highlight reaches past its row.
const ICON_GAP: f64 = 6.0;
const DESC_GAP: f64 = 16.0;
const GUIDE_INSET: f64 = 7.0;
const ELBOW: f64 = 4.0;
const HIGHLIGHT_OVERHANG: f64 = 4.0;

/// The opacity of the accent over a highlighted row.
const HIGHLIGHT: f64 = 0.14;

/// A number of the diagram's section of the config if it is at least lo and at most hi, or
/// fallback, as Pele's setting reads it.
fn setting(config: &Map, key: &str, fallback: f64, lo: f64, hi: f64) -> f64 {
    let section = config.get("treeView").and_then(Value::as_object);
    match section.and_then(|s| s.get(key)) {
        Some(&Value::Number(v)) if v >= lo && v <= hi => v,
        _ => fallback,
    }
}

/// Mermaid's layout options of a tree view, with Pele's defaults, as Pele's treeSettings
/// reads them.
pub(crate) struct TreeSettings {
    pub(crate) padding_x: f64,
    pub(crate) padding_y: f64,
    pub(crate) row_indent: f64,
    pub(crate) line_thickness: f64,
}

/// The layout options of the config, as Pele's treeSettings reads them.
pub(crate) fn tree_settings(config: &Map) -> TreeSettings {
    TreeSettings {
        padding_x: setting(config, "paddingX", 5.0, 0.0, 100.0),
        padding_y: setting(config, "paddingY", 2.0, 0.0, 100.0),
        row_indent: setting(config, "rowIndent", 15.0, 0.0, 400.0),
        line_thickness: setting(config, "lineThickness", 1.0, 0.25, 12.0),
    }
}

/// An entry laid out, Pele's Row: where it starts across, the middle of its icon down, its
/// icon, its name decoded, where its name starts, and, laid out tightly, its name and
/// description as labels, its top and its height.
struct Row {
    node: usize,
    x: f64,
    cy: f64,
    icon: Option<String>,
    text: String,
    label_x: f64,
    highlighted: bool,
    label: Option<Label>,
    description: Option<Label>,
    top: f64,
    height: f64,
}

/// Lays a tree view out as a scene, as Pele's renderTreeView does. A tree wider than the
/// width the host has is laid out tightly, its indents narrower and its names and
/// descriptions wrapped, when that is narrower.
pub(crate) fn layout(model: &TreeViewModel, config: &Map, o: &Options) -> Scene {
    tighten(o, |tight| draw(model, config, o, tight))
}

/// Lays the tree out, tightly or not, as Pele's draw does.
fn draw(model: &TreeViewModel, config: &Map, o: &Options, tight: bool) -> Scene {
    let size = o.font_size;
    let set = common::setting(o);
    let pad = o.pad();
    let icons = icon_config(config);
    // The names of the entries are set in the monospace face where the options list files,
    // as Pele's draw sets them where its monospace option does.
    let files = if o.monospace.contains(Monospace::FILES) {
        Face::MONO
    } else {
        Face::REGULAR
    };
    // A host that gives no width is taken to give 320, as Pele's `?? 320` takes it.
    let max_width = if o.max_width.is_finite() {
        o.max_width
    } else {
        320.0
    };
    let nodes = &model.nodes;

    let s = tree_settings(config);
    let (padding_x, padding_y) = (s.padding_x, s.padding_y);
    let mut max_depth = 0;
    if tight {
        let mut depths: Vec<(usize, usize)> =
            model.root().children.iter().map(|&n| (n, 0)).collect();
        let mut i = 0;
        while i < depths.len() {
            let (node, depth) = depths[i];
            max_depth = max_depth.max(depth);
            depths.extend(nodes[node].children.iter().map(|&c| (c, depth + 1)));
            i += 1;
        }
    }
    let step = if tight {
        min(
            s.row_indent + padding_x,
            max(2.0, (max_width - 120.0) / max_depth.max(1) as f64),
        )
    } else {
        s.row_indent + padding_x
    };
    let row_height = round(size * 1.5) + 2.0 * padding_y;
    let icon_size = size;

    let room = if tight { title_room(o) } else { 4000.0 };
    let raw = model.title.as_deref().unwrap_or("");
    let title = layout_label(raw, false, &set, size, room, Face::BOLD);
    let title_height = if title.height > 0.0 {
        title.height + 12.0
    } else {
        0.0
    };

    // Siblings line up: when one entry of a folder has an icon, the others leave room for one.
    let has_icon_in = |list: &[usize]| {
        list.iter()
            .any(|&child| get_node_icon(&nodes[child], &icons).is_some())
    };
    let mut rows: Vec<Row> = Vec::new();
    let mut row_of: HashMap<usize, usize> = HashMap::new();
    let mut pending: Vec<(usize, usize, bool)> = Vec::new();
    let push = |pending: &mut Vec<(usize, usize, bool)>, list: &[usize], depth: usize| {
        let slot = has_icon_in(list);
        pending.extend(list.iter().rev().map(|&n| (n, depth, slot)));
    };
    push(&mut pending, &model.root().children, 0);
    let mut label_right = 0.0;
    let mut any_highlight = false;
    let mut rows_height = 0.0;
    while let Some((n, depth, slot)) = pending.pop() {
        let node = &nodes[n];
        let x = depth as f64 * step;
        let text = decode_entities(&node.name);
        let label_x = x + if slot { icon_size + ICON_GAP } else { 0.0 };
        let room = max(
            40.0,
            max_width - 2.0 * pad - 2.0 * HIGHLIGHT_OVERHANG - label_x,
        );
        let directory = node.node_type == NodeType::Directory;
        let face = (if directory { Face::BOLD } else { Face::REGULAR }) | files;
        let label = tight.then(|| literal_label(&text, &set, size, room, face));
        let description = match &node.description {
            Some(d) if tight => Some(literal_label(
                &decode_entities(d),
                &set,
                size,
                room,
                Face::ITALIC,
            )),
            _ => None,
        };
        let width = match &label {
            Some(l) => l.width,
            None => set.width(&text, size, face),
        };
        if label_x + width > label_right {
            label_right = label_x + width;
        }
        let highlighted = node
            .css_class
            .as_deref()
            .is_some_and(|c| c.split(is_space_char).any(|name| name == "highlight"));
        any_highlight |= highlighted;
        let label_height = label.as_ref().map_or(0.0, |l| l.height);
        let height = if tight {
            let description_height = description.as_ref().map_or(0.0, |d| d.height);
            max(
                row_height,
                label_height + description_height + 2.0 * padding_y,
            )
        } else {
            row_height
        };
        let cy = rows_height
            + if tight {
                padding_y + max(icon_size, label_height) / 2.0
            } else {
                height / 2.0
            };
        row_of.insert(n, rows.len());
        rows.push(Row {
            node: n,
            x,
            cy,
            icon: get_node_icon(node, &icons),
            text,
            label_x,
            highlighted,
            label,
            description,
            top: rows_height,
            height,
        });
        rows_height += height;
        push(&mut pending, &node.children, depth + 1);
    }

    let desc_x = label_right + DESC_GAP;
    let mut width = label_right;
    for row in &rows {
        let Some(description) = &nodes[row.node].description else {
            continue;
        };
        let w = match &row.description {
            Some(d) => d.width,
            None => set.width(&decode_entities(description), size, Face::ITALIC),
        };
        width = max(width, (if tight { row.label_x } else { desc_x }) + w);
    }
    width = max(width, title.width);
    let inset = pad
        + if any_highlight {
            HIGHLIGHT_OVERHANG
        } else {
            0.0
        };
    let top = pad + title_height;
    let total_width = (width + 2.0 * inset).ceil();
    let total_height = (top + rows_height + pad).ceil();

    let mut items = Vec::new();
    if !title.is_empty() {
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        let at = Point::new(total_width / 2.0, pad + title.height / 2.0);
        it.label = Some(Placed::new(title, at));
        it.style.text = Some(Paint::FG);
        items.push(it);
    }

    let cy_of = |child: usize| rows[row_of[&child]].cy;
    for row in &rows {
        let children = &nodes[row.node].children;
        let Some(&last_child) = children.last() else {
            continue;
        };
        let gx = inset + row.x + GUIDE_INSET;
        let tick = max(gx, inset + row.x + step - padding_x);
        let last = top + cy_of(last_child);
        let r = min(ELBOW, tick - gx);
        let mut p = Path::new();
        p.move_to(gx, top + row.top + row.height - 2.0);
        p.line_to(gx, last - r);
        if r > 0.0 {
            p.quad_to(gx, last, gx + r, last);
            p.line_to(tick, last);
        }
        if tick > gx {
            for &child in &children[..children.len() - 1] {
                let y = top + cy_of(child);
                p.move_to(gx, y);
                p.line_to(tick, y);
            }
        }
        let mut it = Item::new(Kind::Mark, ROLE_GUIDE);
        it.id = Some(nodes[row.node].name.clone());
        it.bounds = p.bounds();
        it.parts = vec![Part::path(p, Painting::StrokeOnly)];
        it.style = Style {
            stroke: Some(Paint::BORDER),
            fill: Some(Paint::NONE),
            stroke_width: Some(s.line_thickness),
            ..Style::default()
        };
        items.push(it);
    }

    let starting = |l: Label, at: Point| Placed {
        align: Align::Start,
        ..Placed::new(l, at)
    };
    for row in &rows {
        let node = &nodes[row.node];
        let cy = top + row.cy;
        let directory = node.node_type == NodeType::Directory;
        let (role, face) = if directory {
            (ROLE_DIRECTORY, Face::BOLD | files)
        } else {
            (ROLE_FILE, files)
        };
        let mut it = Item::new(Kind::Node, role);
        it.id = Some(node.name.clone());
        it.class = class_names(node.css_class.as_deref().unwrap_or(""));
        it.bounds = Rect::new(
            inset + row.x,
            top + row.top,
            total_width - inset - (inset + row.x),
            row.height,
        );
        it.style.text = Some(Paint::FG);
        if row.highlighted {
            let x = inset + row.x - HIGHLIGHT_OVERHANG;
            let r = Rect::new(
                x,
                top + row.top + 1.0,
                total_width - inset + HIGHLIGHT_OVERHANG - x,
                row.height - 2.0,
            );
            it.parts = vec![Part::rect(r, true, Painting::FillOnly)];
            it.style.fill = Some(Paint::ACCENT);
            it.style.fill_opacity = Some(HIGHLIGHT);
        }
        let mut icon = None;
        if let Some(name) = &row.icon {
            let (x, y) = (inset + row.x, cy - icon_size / 2.0);
            if has_icon(o, name) {
                it.picture = Some(icon_svg(name, x, y, icon_size));
            } else {
                let builtin = name
                    .strip_prefix(BUILTIN_PACK)
                    .and_then(|rest| rest.strip_prefix(':'))
                    .and_then(glyph);
                let fallback = if directory { "folder" } else { "file" };
                let (shape, outline) = builtin.or_else(|| glyph(fallback)).expect("a glyph");
                let bounds = Rect::new(x, y, icon_size, icon_size);
                icon = Some(glyph_item(shape, outline, &node.name, bounds));
            }
        }
        if let Some(label) = &row.label {
            if !label.is_empty() {
                let at = Point::new(
                    inset + row.label_x,
                    top + row.top + padding_y + label.height / 2.0,
                );
                it.label = Some(starting(label.clone(), at));
            }
        } else if !row.text.is_empty() {
            let l = common::line(&row.text, set.width(&row.text, size, face), size, face);
            it.label = Some(starting(l, Point::new(inset + row.label_x, cy)));
        }
        items.push(it);
        items.extend(icon);

        let mut description = Item::new(Kind::Text, ROLE_DESCRIPTION);
        description.id = Some(node.name.clone());
        description.style.text = Some(Paint::MUTED);
        if let Some(l) = &row.description {
            let label_height = row.label.as_ref().map_or(0.0, |l| l.height);
            let at = Point::new(
                inset + row.label_x,
                top + row.top + padding_y + label_height + l.height / 2.0,
            );
            description.label = Some(starting(l.clone(), at));
            items.push(description);
        } else if let Some(d) = &node.description {
            let text = decode_entities(d);
            let l = common::line(
                &text,
                set.width(&text, size, Face::ITALIC),
                size,
                Face::ITALIC,
            );
            description.label = Some(starting(l, Point::new(inset + desc_x, cy)));
            items.push(description);
        }
    }

    Scene {
        diagram: "treeView".into(),
        width: total_width,
        height: total_height,
        font_size: size,
        title: model.acc_title.clone(),
        desc: model.acc_descr.clone(),
        items,
    }
}

/// The item of a glyph of the built-in pack in a square box, its outline scaled from its grid
/// to the box and stroked round in the muted color, as Pele strokes the path data it writes
/// in place of an icon the host does not draw. id is that of the entry it is the icon of.
fn glyph_item(name: &'static str, outline: Path, id: &str, bounds: Rect) -> Item {
    let mut it = Item::new(Kind::Mark, ROLE_ICON);
    it.shape = Some(Shape::new(name));
    it.id = Some(id.to_string());
    it.bounds = bounds;
    let part = outline.scaled(bounds.w / 24.0, bounds.x, bounds.y);
    it.parts = vec![Part::path(part, Painting::StrokeOnly)];
    it.style = Style {
        stroke: Some(Paint::MUTED),
        fill: Some(Paint::NONE),
        stroke_width: Some(GLYPH_STROKE * bounds.w / 24.0),
        cap: Some(Cap::Round),
        join: Some(Join::Round),
        ..Style::default()
    };
    it
}
