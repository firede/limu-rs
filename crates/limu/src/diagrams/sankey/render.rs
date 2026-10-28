//! A sankey diagram laid out as a scene, as Pele's renderSankey in render.ts draws it.

use std::collections::HashMap;

use super::db::SankeyDb;
use super::layout::{Alignment, LayoutOptions, PlacedLink, PlacedNode, layout_sankey};
use crate::common::{fit_width, line, setting, title_room};
use crate::css::{declarations, resolve_style};
use crate::errors::Error;
use crate::js::math::{max, min, round};
use crate::js::number::to_string;
use crate::layout::Options;
use crate::scene::style::{Paint, Style};
use crate::scene::text::Face;
use crate::scene::{Align, Item, Kind, Painting, Part, Path, Placed, Point, Rect, Scene, Shape};
use crate::text::{decode_entities, layout_label};
use crate::value::{Map, Value};

/// The roles of the items of a sankey diagram: its title, the band of a link, the bar of a
/// node, a node's name beside its bar, and the value written after the name.
pub(crate) const ROLE_TITLE: &str = "sankey.title";
pub(crate) const ROLE_LINK: &str = "sankey.link";
pub(crate) const ROLE_NODE: &str = "sankey.node";
pub(crate) const ROLE_LABEL: &str = "sankey.label";
pub(crate) const ROLE_VALUE: &str = "sankey.value";

// The measures of Pele's render.ts: the room between a bar and its label, what the nodes are
// kept further apart by where their values are shown, about how many lines of label each
// node of the most crowded column has room for, and the most nodes whose labels are kept
// apart.
const LABEL_GAP: f64 = 6.0;
const VALUE_GAP: f64 = 15.0;
const LABEL_ROOM: f64 = 1.8;
const CAREFUL: usize = 300;

/// The opacity Pele fills the bands with, on their group.
const LINK_OPACITY: f64 = 0.4;

/// A number the configuration gives, at most most, or fallback for anything but a number of
/// at least least, as Pele's numberOption reads it.
fn number_option(conf: &Map, key: &str, fallback: f64, least: f64, most: f64) -> f64 {
    match conf.get(key) {
        Some(&Value::Number(v)) if v >= least => min(v, most),
        _ => fallback,
    }
}

/// A string the configuration gives, or a finite number written as JavaScript writes it, and
/// "" for anything else, as Pele's textOption reads it.
fn text_option(conf: &Map, key: &str) -> String {
    match conf.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(&Value::Number(v)) if v.is_finite() => to_string(v),
        _ => String::new(),
    }
}

/// A value to two decimals, as Mermaid shows values, and a number too large for that as it
/// is, as Pele's rounded gives it.
fn rounded(value: f64) -> f64 {
    if value.abs() < 1e15 {
        round(value * 100.0) / 100.0
    } else {
        value
    }
}

/// The style of a color the configuration gives, which Pele writes as an inline style over
/// the series color, and none where Pele writes none: for a value that is not a string, and
/// for one its resolveStyle drops.
fn fill_style(color: &Value) -> Option<Style> {
    let Value::String(s) = color else {
        return None;
    };
    let decls = [format!("fill:{s}")];
    if resolve_style(&decls).shape.is_empty() {
        return None;
    }
    Some(declarations(&decls).style)
}

/// Lays a sankey diagram out as a scene, as Pele's renderSankey draws it: the chart at the
/// width and height its configuration gives, narrower where the host is, with the bands of
/// the links under the bars of the nodes, a label beside each bar, and the title above. A
/// diagram whose links run in a circle is an error, as in Pele.
pub(crate) fn layout(model: &SankeyDb, config: &Map, o: &Options) -> Result<Scene, Error> {
    let empty = Map::new();
    let conf = config
        .get("sankey")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let natural = number_option(conf, "width", 600.0, 1.0, 10000.0);
    let mut failed = None;
    let sc = fit_width(o, natural, 160.0, |chart_width, _| {
        match draw(model, conf, o, chart_width, natural) {
            Ok(sc) => sc,
            Err(e) => {
                failed = Some(e);
                Scene::default()
            }
        }
    });
    match failed {
        Some(e) => Err(e),
        None => Ok(sc),
    }
}

/// Where a node's label goes, the whole of its name and value, as Pele's draw keeps it.
struct Placing {
    value: String,
    width: f64,
    after: bool,
    left: f64,
    y: f64,
}

/// The key of a band of a line's height, as a number Pele's Map holds: -0 is 0 there.
fn band_key(band: f64) -> u64 {
    (band + 0.0).to_bits()
}

/// Lays the diagram out with its chart chart_width wide, of the natural width its
/// configuration gives, as Pele's draw does.
fn draw(
    model: &SankeyDb,
    conf: &Map,
    o: &Options,
    chart_width: f64,
    natural: f64,
) -> Result<Scene, Error> {
    let size = o.font_size;
    let least = round(size * 0.625);
    // Labels shrink with a chart narrowed to fit, as they would if the whole picture were
    // scaled down.
    let mut small = max(least, round(size * 0.875 * min(1.0, chart_width / natural)));
    let set = setting(o);
    let pad = o.pad();

    let chart_height = number_option(conf, "height", 400.0, 1.0, 10000.0);
    let node_width = min(
        number_option(conf, "nodeWidth", 10.0, 1.0, 200.0),
        chart_width,
    );
    let show_values = conf.get("showValues") != Some(&Value::Bool(false));
    // The affixes are decoded once, here, as Pele's draw decodes them, and not again where
    // they are written.
    let prefix = decode_entities(&text_option(conf, "prefix"));
    let suffix = decode_entities(&text_option(conf, "suffix"));
    let outlined = conf.get("labelStyle").and_then(Value::as_str) == Some("outlined");
    let alignment = match conf.get("nodeAlignment").and_then(Value::as_str) {
        Some("left") => Alignment::Left,
        Some("right") => Alignment::Right,
        Some("center") => Alignment::Center,
        _ => Alignment::Justify,
    };
    let link_color = conf
        .get("linkColor")
        .and_then(Value::as_str)
        .unwrap_or("gradient");

    let extra = if show_values { VALUE_GAP } else { 0.0 };
    let lay = |label_size: f64| -> Result<(Vec<PlacedNode>, Vec<PlacedLink>), Error> {
        layout_sankey(
            model,
            &LayoutOptions {
                width: chart_width,
                height: chart_height,
                node_width,
                node_padding: number_option(conf, "nodePadding", 12.0, 0.0, 200.0) + extra,
                label_height: label_size * 1.15,
                alignment,
            },
        )
    };
    let (mut nodes, mut links) = lay(small)?;
    // Labels also shrink with the most crowded column, so that each keeps about LABEL_ROOM
    // lines.
    let mut rows: HashMap<usize, usize> = HashMap::new();
    let mut fullest = 1;
    for node in &nodes {
        let count = rows.entry(node.layer).or_insert(0);
        *count += 1;
        fullest = fullest.max(*count);
    }
    let crowded = max(least, round(chart_height / fullest as f64 / LABEL_ROOM));
    if crowded < small {
        small = crowded;
        (nodes, links) = lay(small)?;
    }
    let line_height = small * 1.5;

    let mut custom: HashMap<&str, Style> = HashMap::new();
    if let Some(colors) = conf.get("nodeColors").and_then(Value::as_object) {
        for (id, color) in colors.iter() {
            if let Some(s) = fill_style(color) {
                custom.insert(id, s);
            }
        }
    }
    let author = |id: &str| custom.get(id).cloned().unwrap_or_default();

    // Labels sit beside the bars, on the side that faces the middle of the chart unless that
    // side is crowded.
    let mut central = 0;
    let mut most = 0.0;
    for node in &nodes {
        if node.value > most {
            most = node.value;
            central = node.layer;
        }
    }
    let (mut min_x, mut max_x) = (0.0, chart_width);
    let (mut min_y, mut max_y) = (0.0, chart_height);
    let mut placing: Vec<Placing> = Vec::with_capacity(nodes.len());
    for (i, node) in nodes.iter().enumerate() {
        let name = decode_entities(&model.nodes[i].id);
        let value = if show_values {
            format!("{prefix}{}{suffix}", to_string(rounded(node.value)))
        } else {
            String::new()
        };
        let whole = if value.is_empty() {
            name
        } else {
            format!("{name} {value}")
        };
        let width = set.width(&whole, small, Face::default());
        let after = if outlined {
            node.layer >= central
        } else {
            node.x < chart_width / 2.0
        };
        let x = if after {
            node.x + node_width + LABEL_GAP
        } else {
            node.x - LABEL_GAP
        };
        let y = node.y + node.height / 2.0;
        min_x = min(min_x, if after { x } else { x - width });
        max_x = max(max_x, if after { x + width } else { x });
        min_y = min(min_y, y - small * 0.75);
        max_y = max(max_y, y + small * 0.75);
        placing.push(Placing {
            value,
            width,
            after,
            left: if after { x } else { x - width },
            y,
        });
    }
    // A label moves at most one line, so each band of a line's height holds the labels two
    // bands either side of it.
    let mut bands: HashMap<u64, Vec<usize>> = HashMap::new();
    if nodes.len() <= CAREFUL {
        for (k, l) in placing.iter().enumerate() {
            let band = (l.y / line_height).floor();
            for d in [-2.0, -1.0, 0.0, 1.0, 2.0] {
                bands.entry(band_key(band + d)).or_default().push(k);
            }
        }
    }

    let halo = if outlined {
        Style {
            halo: Some(Paint::BG),
            halo_width: Some(3.0),
            ..Style::default()
        }
    } else {
        Style::default()
    };
    let (mut labels, mut bars) = (Vec::new(), Vec::new());
    for (i, node) in nodes.iter().enumerate() {
        let id = &model.nodes[i].id;
        let name = decode_entities(id);
        let height = max(node.height, 1.0);
        let mut it = Item::new(Kind::Node, ROLE_NODE);
        it.shape = Some(Shape::new("rect"));
        it.id = Some(id.clone());
        let r = Rect::new(node.x, node.y, node_width, height);
        it.bounds = r;
        it.parts = vec![Part::rect(r, true, Painting::FillOnly)];
        it.style = Style {
            fill: Some(Paint::series(i)),
            radius: Some(min(2.0, height / 2.0)),
            ..Style::default()
        };
        it.author = author(id);
        bars.push(it);

        let overlaps = |placing: &[Placing], left: f64, y: f64| {
            let l = &placing[i];
            bands
                .get(&band_key((y / line_height).floor()))
                .is_some_and(|nearby| {
                    nearby.iter().any(|&k| {
                        let other = &placing[k];
                        k != i
                            && (other.y - y).abs() < line_height
                            && left < other.left + other.width + LABEL_GAP
                            && left + l.width > other.left - LABEL_GAP
                    })
                })
        };
        // A label that overlaps another tries the other side, and then a line above or below
        // where both sides are crowded, as Pele's draw places it.
        if nodes.len() <= CAREFUL && overlaps(&placing, placing[i].left, placing[i].y) {
            let l = &placing[i];
            let (width, y0, after0) = (l.width, l.y, l.after);
            'place: for dy in [0.0, -line_height, line_height] {
                for after in [after0, !after0] {
                    let left = if after {
                        node.x + node_width + LABEL_GAP
                    } else {
                        node.x - LABEL_GAP - width
                    };
                    let y = y0 + dy;
                    let (top, bottom) = (y - small * 0.75, y + small * 0.75);
                    if overlaps(&placing, left, y)
                        || left < min_x
                        || left + width > max_x
                        || top < min_y
                        || bottom > max_y
                    {
                        continue;
                    }
                    let crosses = nodes.iter().enumerate().any(|(j, bar)| {
                        j != i
                            && left < bar.x + node_width + LABEL_GAP
                            && left + width > bar.x - LABEL_GAP
                            && top < bar.y + bar.height
                            && bottom > bar.y
                    });
                    if crosses {
                        continue;
                    }
                    // A label stays nearer its own bar than any other in the column, so that
                    // it can't name that one.
                    let own = max(0.0, max(node.y - y, y - node.y - node.height));
                    let nearer = nodes.iter().enumerate().any(|(j, bar)| {
                        j != i
                            && bar.x == node.x
                            && max(0.0, max(bar.y - y, y - bar.y - bar.height)) <= own
                    });
                    if nearer {
                        continue;
                    }
                    let l = &mut placing[i];
                    (l.left, l.y, l.after) = (left, y, after);
                    break 'place;
                }
            }
        }
        let l = &placing[i];
        let (after, y, text_width) = (l.after, l.y, l.width);
        let value = l.value.clone();
        let x = if after {
            node.x + node_width + LABEL_GAP
        } else {
            node.x - LABEL_GAP
        };

        // Pele writes the value in a span of the muted color after the name, in one text
        // anchored at x. A label is of one color, so the name and the value are two items:
        // the name starts where the whole starts, and the value ends where it ends.
        let text_item = |role: &str, text: &str, paint: Paint| {
            let mut it = Item::new(Kind::Text, role.to_string());
            it.id = Some(id.clone());
            let l = line(
                text,
                set.width(text, small, Face::default()),
                small,
                Face::default(),
            );
            let mut placed = Placed::new(l, Point::new(x, y));
            placed.align = Align::End;
            it.label = Some(placed);
            it.style = Style {
                text: Some(paint),
                ..halo.clone()
            };
            it
        };
        let mut name_item = text_item(ROLE_LABEL, &name, Paint::FG);
        let mut value_item = text_item(ROLE_VALUE, &value, Paint::MUTED);
        let name_at = name_item.label.as_mut().expect("a label");
        let value_at = value_item.label.as_mut().expect("a label");
        if value.is_empty() {
            if after {
                name_at.align = Align::Start;
            }
        } else if after {
            name_at.align = Align::Start;
            value_at.at.x = x + text_width;
        } else {
            name_at.at.x = x - text_width;
            name_at.align = Align::Start;
        }
        labels.push(name_item);
        if !value.is_empty() {
            labels.push(value_item);
        }
    }

    let fixed = if matches!(link_color, "gradient" | "source" | "target") {
        None
    } else {
        fill_style(&Value::String(link_color.to_string()))
    };
    let mut bands_out = Vec::new();
    for link in &links {
        // Without definitions there are no gradients, so a gradient band takes its source's
        // color.
        let end = if link_color == "target" {
            link.target
        } else {
            link.source
        };
        let half = max(link.thickness, 1.0) / 2.0;
        let x0 = nodes[link.source].x + node_width;
        let x1 = nodes[link.target].x;
        let mid = (x0 + x1) / 2.0;
        let from = &model.nodes[link.source].id;
        let to = &model.nodes[link.target].id;
        let mut path = Path::new();
        path.move_to(x0, link.source_y - half)
            .cube_to(
                mid,
                link.source_y - half,
                mid,
                link.target_y - half,
                x1,
                link.target_y - half,
            )
            .line_to(x1, link.target_y + half)
            .cube_to(
                mid,
                link.target_y + half,
                mid,
                link.source_y + half,
                x0,
                link.source_y + half,
            )
            .close();
        let mut it = Item::new(Kind::Mark, ROLE_LINK);
        it.id = Some(format!("{from}->{to}"));
        it.bounds = path.bounds();
        it.parts = vec![Part::path(path, Painting::FillOnly)];
        // The bands are drawn in a group with a fill opacity, which an author's fill takes
        // too.
        it.style = Style {
            fill: Some(Paint::series(end)),
            fill_opacity: Some(LINK_OPACITY),
            ..Style::default()
        };
        it.author = fixed
            .clone()
            .unwrap_or_else(|| author(&model.nodes[end].id));
        it.tooltip = Some(format!(
            "{} → {}: {prefix}{}{suffix}",
            decode_entities(from),
            decode_entities(to),
            to_string(rounded(link.value))
        ));
        bands_out.push(it);
    }

    let raw = model.title.as_deref().unwrap_or("");
    let title = layout_label(raw, false, &set, size, title_room(o), Face::BOLD);
    let title_height = if title.height > 0.0 {
        title.height + 12.0
    } else {
        0.0
    };
    let inner = max(max_x - min_x, title.width);
    let ox = pad - min_x + (inner - (max_x - min_x)) / 2.0;
    let oy = pad + title_height - min_y;
    let width = (inner + 2.0 * pad).ceil();
    let height = (max_y - min_y + title_height + 2.0 * pad).ceil();

    let mut sc = Scene {
        diagram: "sankey".into(),
        width,
        height,
        font_size: size,
        title: None,
        desc: None,
        items: Vec::new(),
    };
    if !title.is_empty() {
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        let at = Point::new(width / 2.0, pad + title.height / 2.0);
        it.label = Some(Placed::new(title, at));
        it.style.text = Some(Paint::FG);
        sc.items.push(it);
    }
    for group in [bands_out, bars, labels] {
        for mut it in group {
            it.move_by(ox, oy);
            sc.items.push(it);
        }
    }
    Ok(sc)
}
