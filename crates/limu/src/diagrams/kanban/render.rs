//! A kanban board laid out as a scene, as Pele's renderKanban in render.ts draws it.

use super::db::{KanbanDb, KanbanNode};
use crate::common::{class_names, has_icon, icon_svg, setting, title_label, title_room};
use crate::js::chars::{to_lower, trim};
use crate::js::math::{max, min, round};
use crate::layout::Options;
use crate::scene::style::{Cap, Paint, Style};
use crate::scene::text::{Face, Label, Span};
use crate::scene::{Align, Item, Kind, Painting, Part, Path, Placed, Point, Rect, Scene, Shape};
use crate::text::{Setting, layout_label};
use crate::url::{link_url, safe_url};
use crate::value::{Map, Value};

/// The roles of the items of a kanban board.
pub(crate) const ROLE_ASSIGNED: &str = "kanban.assigned";
pub(crate) const ROLE_CARD: &str = "kanban.card";
pub(crate) const ROLE_COLUMN: &str = "kanban.column";
pub(crate) const ROLE_COUNT: &str = "kanban.count";
pub(crate) const ROLE_PRIORITY: &str = "kanban.priority";
pub(crate) const ROLE_TICKET: &str = "kanban.ticket";
pub(crate) const ROLE_TITLE: &str = "kanban.title";

// The spacing of a board, as Pele's render.ts has it.
const COLUMN_GAP: f64 = 12.0;
const COLUMN_PAD: f64 = 8.0;
const CARD_PAD: f64 = 10.0;
const CARD_GAP: f64 = 8.0;
const META_GAP: f64 = 4.0;
const ICON_GAP: f64 = 6.0;

/// The style of the stripe a priority marks a card with, as Pele's PRIORITY has them. Mermaid
/// names five priorities and gives Medium no color, and matching here ignores case. The high
/// ones are marked in the critical color and the low ones in the muted one, the nearer to
/// Medium the fainter. They are not series colors, as a theme may put those in any order.
/// Each stripe ends round, as Pele's does.
fn priority_style(priority: &str) -> Option<Style> {
    let (stroke, opacity) = match priority {
        "very high" => (Paint::CRITICAL, None),
        "high" => (Paint::CRITICAL, Some(0.5)),
        "low" => (Paint::MUTED, Some(0.5)),
        "very low" => (Paint::MUTED, None),
        _ => return None,
    };
    Some(Style {
        stroke: Some(stroke),
        stroke_width: Some(3.0),
        opacity,
        cap: Some(Cap::Round),
        ..Style::default()
    })
}

/// Lays out text for a fixed width, as Pele's fitLabel does. A word too long for a line is
/// broken between characters, because a card cannot grow sideways to hold it.
fn fit_label(
    raw: Option<&str>,
    markdown: bool,
    set: &Setting,
    size: f64,
    max_width: f64,
    base: Face,
) -> Label {
    let mut label = layout_label(raw.unwrap_or(""), markdown, set, size, max_width, base);
    if label.width <= max_width {
        return label;
    }
    let mut lines: Vec<Vec<Span>> = Vec::new();
    let mut widths = Vec::new();
    let mut widest = 0.0;
    let mut line: Vec<Span> = Vec::new();
    let mut width = 0.0;
    let mut commit = |line: &mut Vec<Span>, width: &mut f64| {
        lines.push(std::mem::take(line));
        widths.push(*width);
        widest = max(widest, *width);
        *width = 0.0;
    };
    for (i, spans) in label.lines.iter().enumerate() {
        if label.widths[i] <= max_width {
            line = spans.clone();
            width = label.widths[i];
            commit(&mut line, &mut width);
            continue;
        }
        for span in spans {
            if span.icon.is_some() {
                if width > 0.0 && width + span.width > max_width {
                    commit(&mut line, &mut width);
                }
                line.push(span.clone());
                width += span.width;
                continue;
            }
            let mut text = String::new();
            let mut from = width;
            for ch in span.text.chars() {
                let w = set.width(ch.encode_utf8(&mut [0; 4]), size, span.face);
                if width > 0.0 && width + w > max_width {
                    if !text.is_empty() {
                        line.push(Span {
                            text: std::mem::take(&mut text),
                            face: span.face,
                            icon: None,
                            width: width - from,
                        });
                    }
                    commit(&mut line, &mut width);
                    from = 0.0;
                }
                text.push(ch);
                width += w;
            }
            if !text.is_empty() {
                line.push(Span {
                    text,
                    face: span.face,
                    icon: None,
                    width: width - from,
                });
            }
        }
        commit(&mut line, &mut width);
    }
    label.height = lines.len() as f64 * label.line_height;
    label.lines = lines;
    label.widths = widths;
    label.width = widest;
    label
}

/// A card laid out, Pele's Card.
struct Card<'a> {
    item: &'a KanbanNode,
    label: Label,
    ticket: Label,
    assigned: Label,
    /// The height of the rows under the text that hold the ticket and the assignee.
    meta: f64,
    height: f64,
}

/// A label placed at a point with its lines starting or ending there, or none for a label
/// with nothing to draw.
fn placed(label: &Label, at: Point, align: Align) -> Option<Placed> {
    (!label.is_empty()).then(|| Placed {
        align,
        ..Placed::new(label.clone(), at)
    })
}

/// Lays a board out as a scene, as Pele's renderKanban draws it: a column for each section,
/// side by side, each with its title, the count of its cards and its cards in a list. A board
/// wider than the width the host has is folded into rows of columns, which then share the row
/// between them.
pub(crate) fn layout(m: &KanbanDb, config: &Map, o: &Options) -> Scene {
    let size = o.font_size;
    let small = round(size * 0.875);
    let line = round(size * 1.5);
    let small_line = round(small * 1.5);
    let set = setting(o);
    let pad = o.pad();
    let conf = config.get("kanban").and_then(Value::as_object);
    let option = |key: &str| conf.and_then(|c| c.get(key));
    let glyph = small * 1.25;
    // Pele widens the columns to hold the widest math, which limu-rs does not set. With none,
    // the math is no width.
    let math_width = 0.0;
    let own_width = max(
        match option("sectionWidth").and_then(Value::as_f64) {
            Some(w) if w > 0.0 => min(max(w, 80.0), 2000.0),
            _ => 200.0,
        },
        math_width,
    );
    // A board wider than the width the host has is folded into rows of columns, which then
    // share the row between them.
    let room = if o.max_width > 0.0 {
        o.max_width - 2.0 * pad
    } else {
        f64::INFINITY
    };
    let sections = &m.sections;
    let count = sections.len() as f64;
    let fits = count * (own_width + COLUMN_GAP) - COLUMN_GAP <= room;
    let per_row = if fits {
        max(count, 1.0)
    } else {
        max(
            1.0,
            ((room + COLUMN_GAP) / (own_width + COLUMN_GAP)).floor(),
        )
    };
    let column_width = if fits {
        own_width
    } else {
        max(
            max(
                min(own_width, room),
                min(
                    ((room + COLUMN_GAP) / per_row).floor() - COLUMN_GAP,
                    own_width * 2.0,
                ),
            ),
            math_width,
        )
    };
    let base_url = option("ticketBaseUrl")
        .and_then(Value::as_str)
        .unwrap_or("");
    let card_width = column_width - 2.0 * COLUMN_PAD;
    let text_width = card_width - 2.0 * CARD_PAD;
    let has = |name: Option<&str>| name.is_some_and(|name| !name.is_empty() && has_icon(o, name));
    let icon_room = |name: Option<&str>| if has(name) { glyph + ICON_GAP } else { 0.0 };

    let mut titles = Vec::with_capacity(sections.len());
    let mut count_widths = Vec::with_capacity(sections.len());
    let mut cards: Vec<Vec<Card>> = Vec::with_capacity(sections.len());
    let mut head_height = line;
    let mut body_heights = Vec::with_capacity(sections.len());
    for section in sections {
        let count_width = set.width(&section.items.len().to_string(), small, Face::REGULAR);
        let reserved = count_width + 12.0 + icon_room(section.node.icon.as_deref());
        let title = fit_label(
            Some(&section.node.label),
            true,
            &set,
            size,
            card_width - 8.0 - reserved,
            Face::BOLD,
        );
        head_height = max(head_height, title.height);
        count_widths.push(count_width);
        titles.push(title);

        let mut list = Vec::with_capacity(section.items.len());
        let mut height = 0.0;
        for item in &section.items {
            let label = fit_label(
                Some(&item.label),
                true,
                &set,
                small,
                text_width - icon_room(item.icon.as_deref()),
                Face::REGULAR,
            );
            let ticket = fit_label(
                item.ticket.as_deref(),
                false,
                &set,
                small,
                text_width,
                Face::REGULAR,
            );
            let assigned = fit_label(
                item.assigned.as_deref(),
                false,
                &set,
                small,
                text_width,
                Face::REGULAR,
            );
            let meta = if ticket.width + assigned.width + 8.0 > text_width {
                ticket.height + assigned.height
            } else {
                max(ticket.height, assigned.height)
            };
            let card_height = 2.0 * CARD_PAD
                + max(label.height, small_line)
                + if meta != 0.0 { META_GAP + meta } else { 0.0 };
            height += card_height + CARD_GAP;
            list.push(Card {
                item,
                label,
                ticket,
                assigned,
                meta,
                height: card_height,
            });
        }
        cards.push(list);
        body_heights.push(height);
    }
    head_height += 2.0 * COLUMN_PAD;
    let per = per_row as usize;
    let mut row_heights = Vec::new();
    let mut row_tops = Vec::new();
    let mut y = 0.0;
    for first in (0..sections.len()).step_by(per) {
        let mut tallest = CARD_GAP;
        for &body in &body_heights[first..sections.len().min(first + per)] {
            tallest = max(tallest, body);
        }
        row_tops.push(y);
        row_heights.push(head_height + tallest);
        y += head_height + tallest + COLUMN_GAP;
    }
    let board_height = match (row_tops.last(), row_heights.last()) {
        (Some(top), Some(h)) => top + h,
        _ => 0.0,
    };

    let title = title_label(m.title.as_deref(), &set, size, title_room(o));
    let title_height = if title.height > 0.0 {
        title.height + 16.0
    } else {
        0.0
    };
    let board_width = max(
        min(count, per_row) * (column_width + COLUMN_GAP) - COLUMN_GAP,
        0.0,
    );
    let width = max(board_width, title.width);
    let left = pad + (width - board_width) / 2.0;
    let edge = column_width - COLUMN_PAD - 4.0;

    let mut sc = Scene {
        diagram: "kanban".into(),
        width: (width + 2.0 * pad).ceil(),
        height: (board_height + title_height + 2.0 * pad).ceil(),
        font_size: size,
        title: None,
        desc: None,
        items: Vec::new(),
    };
    let text = |role: &'static str, id: &str, label: Option<Placed>, ink: Paint| {
        let mut it = Item::new(Kind::Text, role);
        it.id = Some(id.to_string());
        it.label = label;
        it.style = Style {
            text: Some(ink),
            ..Style::default()
        };
        it
    };
    if !title.is_empty() {
        let at = Point::new(sc.width / 2.0, pad + title.height / 2.0);
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        it.label = Some(Placed::new(title, at));
        it.style = Style {
            text: Some(Paint::FG),
            ..Style::default()
        };
        sc.items.push(it);
    }
    for (index, section) in sections.iter().enumerate() {
        let row = index / per;
        let gx = left + (index % per) as f64 * (column_width + COLUMN_GAP);
        let gy = pad + title_height + row_tops[row];
        let bounds = Rect::new(gx, gy, column_width, row_heights[row]);
        let mut column = Item::new(Kind::Group, ROLE_COLUMN);
        column.shape = Some(Shape::new("rect"));
        column.id = Some(section.node.id.clone());
        column.class = class_names(section.node.css_classes.as_deref().unwrap_or(""));
        column.bounds = bounds;
        column.parts = vec![Part::rect(bounds, true, Painting::FillOnly)];
        let title = &titles[index];
        let at = Point::new(
            gx + (COLUMN_PAD + 4.0),
            gy + (COLUMN_PAD + title.height / 2.0),
        );
        column.label = placed(title, at, Align::Start);
        column.style = Style {
            fill: Some(Paint::SURFACE),
            text: Some(Paint::FG),
            ..Style::default()
        };
        if let Some(icon) = section.node.icon.as_deref().filter(|&i| has(Some(i))) {
            column.picture = Some(icon_svg(
                icon,
                gx + (edge - count_widths[index] - ICON_GAP - glyph),
                gy + (COLUMN_PAD + (line - glyph) / 2.0),
                glyph,
            ));
        }
        sc.items.push(column);
        let number = layout_label(
            &section.items.len().to_string(),
            false,
            &set,
            small,
            f64::INFINITY,
            Face::REGULAR,
        );
        let at = Point::new(gx + edge, gy + (COLUMN_PAD + line / 2.0));
        sc.items.push(text(
            ROLE_COUNT,
            &section.node.id,
            placed(&number, at, Align::End),
            Paint::MUTED,
        ));

        let mut y = head_height;
        for card in &cards[index] {
            let item = card.item;
            let (cx, cy) = (gx + COLUMN_PAD, gy + y);
            let bounds = Rect::new(cx, cy, card_width, card.height);
            let mut it = Item::new(Kind::Node, ROLE_CARD);
            it.shape = Some(Shape::new("rect"));
            it.id = Some(item.id.clone());
            it.class = class_names(item.css_classes.as_deref().unwrap_or(""));
            it.bounds = bounds;
            it.parts = vec![Part::rect(bounds, true, Painting::FillAndStroke)];
            let at = Point::new(cx + CARD_PAD, cy + (CARD_PAD + card.label.height / 2.0));
            it.label = placed(&card.label, at, Align::Start);
            it.style = Style {
                fill: Some(Paint::BG),
                stroke: Some(Paint::BORDER),
                text: Some(Paint::FG),
                ..Style::default()
            };
            if let Some(icon) = item.icon.as_deref().filter(|&i| has(Some(i))) {
                it.picture = Some(icon_svg(
                    icon,
                    cx + (card_width - CARD_PAD - glyph),
                    cy + (CARD_PAD + (small_line - glyph) / 2.0),
                    glyph,
                ));
            }
            sc.items.push(it);
            let priority = to_lower(trim(item.priority.as_deref().unwrap_or("")));
            if let Some(own) = priority_style(&priority) {
                let mut stripe = Path::new();
                stripe
                    .move_to(cx + 2.5, cy + 5.0)
                    .line_to(cx + 2.5, cy + (card.height - 5.0));
                let mut it = Item::new(Kind::Mark, ROLE_PRIORITY);
                it.shape = Some(Shape::from(priority.replacen(' ', "-", 1)));
                it.id = Some(item.id.clone());
                it.bounds = stripe.bounds();
                it.parts = vec![Part::path(stripe, Painting::StrokeOnly)];
                it.style = own;
                sc.items.push(it);
            }
            let bottom = card.height - CARD_PAD;
            if let Some(ticket) = item.ticket.as_deref().filter(|t| !t.is_empty()) {
                let top = bottom - card.meta;
                let at = Point::new(cx + CARD_PAD, cy + (top + card.ticket.height / 2.0));
                let mut it = text(
                    ROLE_TICKET,
                    &item.id,
                    placed(&card.ticket, at, Align::Start),
                    Paint::MUTED,
                );
                if !base_url.is_empty()
                    && let Some(href) = link_url(
                        &safe_url(&base_url.replacen("#TICKET#", ticket, 1)),
                        &o.schemes(),
                    )
                {
                    it.link = Some(href);
                    it.style.text = Some(Paint::ACCENT);
                }
                sc.items.push(it);
            }
            if item.assigned.as_deref().is_some_and(|a| !a.is_empty()) {
                let at = Point::new(
                    cx + (card_width - CARD_PAD),
                    cy + (bottom - card.assigned.height + card.assigned.height / 2.0),
                );
                sc.items.push(text(
                    ROLE_ASSIGNED,
                    &item.id,
                    placed(&card.assigned, at, Align::End),
                    Paint::MUTED,
                ));
            }
            y += card.height + CARD_GAP;
        }
    }
    sc
}
