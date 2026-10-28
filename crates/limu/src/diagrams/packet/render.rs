//! A packet diagram laid out as a scene, as Pele's renderPacket in render.ts draws it.

use std::collections::HashMap;

use super::PacketModel;
use super::model::{PacketBlock, packet_option};
use crate::common::{Fitted, fit_text, line, literal_label, setting, title_room, turn_to_fit};
use crate::js::chars::trim;
use crate::js::math::{max, min, round};
use crate::js::number::to_string;
use crate::layout::Options;
use crate::scene::style::{Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{Align, Item, Kind, Painting, Part, Placed, Point, Rect, Scene, Shape};
use crate::text::{decode_entities, layout_label};
use crate::value::{Map, Value};

/// The roles of the items of a packet diagram: a field's box, with its label, in the order of
/// the rows, the number of a field's first or last bit above it, aligned at its start or end,
/// or centered over a field of one bit, and the title, which is under the fields, as Mermaid
/// puts it.
pub(crate) const ROLE_FIELD: &str = "packet.field";
pub(crate) const ROLE_BIT: &str = "packet.bit";
pub(crate) const ROLE_TITLE: &str = "packet.title";

/// Lays a packet diagram out as a scene, as Pele's renderPacket does: across in rows of its
/// bits, or, where the host is narrow, down in rows of fewer bits, with labels that wrap.
pub(crate) fn layout(model: &PacketModel, config: &Map, o: &Options) -> Scene {
    turn_to_fit(o, |down| draw(model, config, o, down))
}

/// Lays the packet out across, or down, as Pele's draw does.
fn draw(model: &PacketModel, config: &Map, o: &Options, down: bool) -> Scene {
    let size = o.font_size;
    let label_size = round(size * 0.875);
    let bit_size = round(size * 0.625);
    let set = setting(o);
    let pad = o.padding;

    let packet = config.get("packet").and_then(Value::as_object);
    let row_height = packet_option(config, "rowHeight", 32.0, 1.0, 400.0);
    let mut bit_width = packet_option(config, "bitWidth", 32.0, 1.0, 400.0);
    let padding_x = packet_option(config, "paddingX", 5.0, 0.0, 400.0);
    let padding_y = packet_option(config, "paddingY", 5.0, 0.0, 400.0);
    let show_bits = packet.and_then(|p| p.get("showBits")) != Some(&Value::Bool(false));
    let descending = packet
        .and_then(|p| p.get("bitOrder"))
        .and_then(Value::as_str)
        == Some("descending");
    let mut bits_per_row = model.bits_per_row;
    let mut split: Vec<Vec<PacketBlock>> = Vec::new();
    let rows = if down {
        let count = max(
            1.0,
            ((o.max_width - 2.0 * pad) / min(bit_width, 18.0)).floor(),
        );
        // A byte is the shortest row. Below that every label is one character to a line,
        // and a long packet is a row to a bit.
        // Rust's log2 stands for V8's here. For a whole count, the two differ only near 2^53.
        bits_per_row = min(bits_per_row, max(8.0, 2f64.powf(count.log2().floor())));
        let last = model
            .rows
            .last()
            .and_then(|row| row.last())
            .map_or(0.0, |b| b.end);
        if ((last + 1.0) / bits_per_row).ceil() > 10000.0 {
            bits_per_row = model.bits_per_row;
        }
        bit_width = min(
            bit_width,
            max(18.0, (o.max_width - 2.0 * pad) / bits_per_row),
        );
        // The model has split each field at its own rows. Its pieces are joined again first,
        // so that a field is only split where one of the new rows ends. A piece counts one
        // bit short.
        let piece = |b: &PacketBlock| b.bits == b.end - b.start;
        let mut whole: Vec<PacketBlock> = Vec::new();
        for b in model.rows.iter().flatten() {
            if let Some(prev) = whole.last_mut()
                && piece(prev)
                && piece(b)
                && prev.label == b.label
                && b.start == prev.end + 1.0
                && b.start % model.bits_per_row == 0.0
            {
                prev.end = b.end;
                prev.bits = prev.end - prev.start;
                continue;
            }
            whole.push(b.clone());
        }
        for b in &whole {
            let mut start = b.start;
            while start <= b.end {
                let end = min(
                    b.end,
                    ((start / bits_per_row).floor() + 1.0) * bits_per_row - 1.0,
                );
                let index = (start / bits_per_row).floor() as usize;
                if split.len() <= index {
                    split.resize_with(index + 1, Vec::new);
                }
                split[index].push(PacketBlock {
                    start,
                    end,
                    ..b.clone()
                });
                start = end + 1.0;
            }
        }
        &split
    } else {
        &model.rows
    };
    let labels: Vec<Vec<Label>> = if down {
        rows.iter()
            .map(|row| {
                row.iter()
                    .map(|b| {
                        let room = max(
                            1.0,
                            (b.end - b.start + 1.0) * bit_width
                                - min(padding_x, bit_width - 1.0)
                                - 6.0,
                        );
                        let text = decode_entities(&b.label);
                        literal_label(trim(&text), &set, label_size, room, Face::default())
                    })
                    .collect()
            })
            .collect()
    } else {
        Vec::new()
    };
    // Bit numbers sit above each row and need room of their own.
    let numbers = if show_bits { bit_size + 4.0 } else { 0.0 };
    let row_heights: Vec<f64> = (0..rows.len())
        .map(|i| {
            let mut height = row_height;
            if down {
                for l in &labels[i] {
                    height = max(height, l.height + 10.0);
                }
            }
            height
        })
        .collect();
    let mut tops = Vec::with_capacity(rows.len());
    let mut rows_height = 0.0;
    for &height in &row_heights {
        tops.push(rows_height);
        rows_height += numbers + height + padding_y;
    }
    if !rows.is_empty() {
        rows_height -= padding_y;
    }

    let gap = min(padding_x, bit_width - 1.0);
    let width = if rows.is_empty() {
        0.0
    } else {
        bits_per_row * bit_width - gap
    };
    let raw = model.title.as_deref().unwrap_or("");
    let room = if down { title_room(o) } else { 4000.0 };
    let title = layout_label(raw, false, &set, size, room, Face::BOLD);
    let title_height = if title.height > 0.0 {
        title.height + if rows_height > 0.0 { 12.0 } else { 0.0 }
    } else {
        0.0
    };

    // The pieces of a field split across rows share its label, so each width is fitted once.
    let mut fits: HashMap<(String, u64), Option<Fitted>> = HashMap::new();
    let mut fit = |label: &str, room: f64| -> Option<Fitted> {
        fits.entry((label.to_string(), room.to_bits()))
            .or_insert_with(|| {
                let text = decode_entities(label);
                fit_text(
                    trim(&text),
                    room,
                    label_size,
                    round(size * 0.5),
                    &set,
                    Face::default(),
                )
            })
            .clone()
    };

    let left = pad + max(0.0, (title.width - width) / 2.0);
    let (mut fields, mut bits) = (Vec::new(), Vec::new());
    for (index, row) in rows.iter().enumerate() {
        let y = pad + tops[index] + numbers;
        let row_height = row_heights[index];
        for (k, b) in row.iter().enumerate() {
            let count = b.end - b.start + 1.0;
            let first = b.start % bits_per_row;
            let cells = if descending {
                bits_per_row - first - count
            } else {
                first
            };
            let x = left + cells * bit_width;
            let w = count * bit_width - gap;
            let bounds = Rect::new(x, y, w, row_height);
            let mut field = Item::new(Kind::Node, ROLE_FIELD);
            field.shape = Some(Shape::new("rect"));
            field.id = Some(format!("{}-{}", to_string(b.start), to_string(b.end)));
            field.bounds = bounds;
            field.parts = vec![Part::rect(bounds, true, Painting::FillAndStroke)];
            field.style = Style {
                fill: Some(Paint::SURFACE),
                stroke: Some(Paint::BORDER),
                text: Some(Paint::FG),
                ..Style::default()
            };
            let at = Point::new(x + w / 2.0, y + row_height / 2.0);
            if down {
                field.label = Some(Placed::new(labels[index][k].clone(), at));
            } else if let Some(f) = fit(&b.label, w - 6.0)
                && f.size <= row_height
            {
                let l = line(&f.text, f.width, f.size, Face::default());
                field.label = Some(Placed::new(l, at));
            }
            fields.push(field);
            if !show_bits {
                continue;
            }
            let (leading, trailing) = if descending {
                (b.end, b.start)
            } else {
                (b.start, b.end)
            };
            let mut number = |n: f64, at: f64, align: Align| {
                let s = to_string(n);
                let l = line(
                    &s,
                    set.width(&s, bit_size, Face::default()),
                    bit_size,
                    Face::default(),
                );
                let mut it = Item::new(Kind::Text, ROLE_BIT);
                // The number's baseline is 4 above the row.
                let mut placed = Placed::new(l, Point::new(at, y - 4.0 - bit_size * 0.35));
                placed.align = align;
                it.label = Some(placed);
                it.style.text = Some(Paint::MUTED);
                bits.push(it);
            };
            if count == 1.0 {
                number(leading, x + w / 2.0, Align::Middle);
            } else {
                number(leading, x, Align::Start);
                number(trailing, x + w, Align::End);
            }
        }
    }

    let mut sc = Scene {
        diagram: "packet".into(),
        width: (max(width, title.width) + 2.0 * pad).ceil(),
        height: (rows_height + title_height + 2.0 * pad).ceil(),
        font_size: size,
        title: model.acc_title.clone(),
        desc: model.acc_descr.clone(),
        items: fields,
    };
    sc.items.extend(bits);
    if !title.is_empty() {
        // Mermaid puts a packet's title under it.
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        let at = Point::new(sc.width / 2.0, sc.height - pad - title.height / 2.0);
        it.label = Some(Placed::new(title, at));
        it.style.text = Some(Paint::FG);
        sc.items.push(it);
    }
    sc
}
