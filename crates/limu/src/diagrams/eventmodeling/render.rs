//! An event modeling diagram laid out as a scene, as Pele's renderEventModel in render.ts
//! draws it: across, in a lane for each group of kinds and namespace, or, when the host is
//! narrow, down, a card to a frame.
//!
//! A frame is a node of the role of its kind, "eventmodeling.command" and the rest, its name
//! its label, with the tag "reset" for a reset frame. Its example data is a text item after
//! it. Across, a lane is a group, with its label. Down, each frame's lane is named by a text
//! item before it, and its number by one after it. A note and a specification are nodes,
//! their heading the label, their lines a text item after each.

use std::collections::{HashMap, HashSet};

use super::model::{EmBox, EmRelation, EmSpecification, EntityKind, EventModelingModel};
use crate::common::{break_label, line, literal_label, setting, turn_to_fit};
use crate::js::chars::len16;
use crate::js::math::{max, min, round};
use crate::layout::Options;
use crate::scene::style::{Paint, Style};
use crate::scene::text::{Face, Label, Span};
use crate::scene::{
    Align, Edge, Item, Kind, Marker, MarkerType, Painting, Part, Path, Placed, Point, Rect, Scene,
};
use crate::text::{Setting, decode_entities, layout_label};
use crate::value::Map;

/// The roles of the items of an event modeling diagram.
pub(crate) const ROLE_LANE: &str = "eventmodeling.lane";
pub(crate) const ROLE_LANE_LABEL: &str = "eventmodeling.lane-label";
pub(crate) const ROLE_RELATION: &str = "eventmodeling.relation";
pub(crate) const ROLE_DATA: &str = "eventmodeling.data";
pub(crate) const ROLE_FRAME_ID: &str = "eventmodeling.frame-id";
pub(crate) const ROLE_NOTE: &str = "eventmodeling.note";
pub(crate) const ROLE_NOTE_TEXT: &str = "eventmodeling.note-text";
pub(crate) const ROLE_SPECIFICATION: &str = "eventmodeling.specification";
pub(crate) const ROLE_SPEC_TEXT: &str = "eventmodeling.specification-text";
pub(crate) const ROLE_TITLE: &str = "eventmodeling.title";

/// The role of a frame of a kind.
pub(crate) fn frame_role(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Ui => "eventmodeling.ui",
        EntityKind::Processor => "eventmodeling.processor",
        EntityKind::Command => "eventmodeling.command",
        EntityKind::ReadModel => "eventmodeling.readmodel",
        EntityKind::Event => "eventmodeling.event",
    }
}

/// The series of the color of each kind of frame, as Pele's SERIES has them. A UI has none
/// and is drawn plain.
fn series(kind: EntityKind) -> Option<usize> {
    match kind {
        EntityKind::Command => Some(0),
        EntityKind::Event => Some(1),
        EntityKind::ReadModel => Some(2),
        EntityKind::Processor => Some(6),
        EntityKind::Ui => None,
    }
}

// The measures of Pele's render.ts.
const BOX_PAD_X: f64 = 12.0;
const BOX_PAD_Y: f64 = 10.0;
const BOX_MIN_WIDTH: f64 = 96.0;
const BOX_GAP: f64 = 16.0;
const STAGGER: f64 = 80.0;
const LANE_PAD: f64 = 12.0;
const LANE_GAP: f64 = 8.0;
const DATA_COLUMNS: usize = 48;
const NOTE_COLUMNS: usize = 80;
const CARD_GAP: f64 = 8.0;
const RAIL_GAP: f64 = 8.0;
const CARD_MIN: f64 = 120.0;
const ARROW_ROOM: f64 = 20.0;
/// The opacity of a frame's series color over the background.
const WASH: f64 = 0.22;

/// Lays an event modeling diagram out as a scene, as Pele's renderEventModel does: across,
/// or down when the host is narrow. The diagram has no configuration of its own.
pub(crate) fn layout(m: &EventModelingModel, _config: &Map, o: &Options) -> Scene {
    turn_to_fit(o, |down| {
        if down {
            vertical_event_model(m, o)
        } else {
            draw(m, o)
        }
    })
}

/// Splits text into lines of at most columns characters, keeping its own line breaks, as
/// Pele's hardWrap does. A line of no more than columns code units, or of no more than
/// columns code points, stays whole. A longer one is cut every columns code points, so that
/// no character past the Basic Multilingual Plane is split.
fn hard_wrap(text: &str, columns: usize) -> Vec<String> {
    let mut out = Vec::new();
    for line in decode_entities(text).split('\n') {
        let chars: Vec<char> = line.chars().collect();
        if len16(line) <= columns || chars.len() <= columns {
            out.push(line.to_string());
            continue;
        }
        for chunk in chars.chunks(columns) {
            out.push(chunk.iter().collect());
        }
    }
    out
}

/// Rows of text as Pele's lines writes them: a line each, step apart, in a size and face. An
/// empty row is a line with no text, which nothing is drawn for.
fn rows_label(rows: &[String], set: &Setting, size: f64, step: f64, face: Face) -> Label {
    let mut l = Label {
        size,
        line_height: step,
        height: rows.len() as f64 * step,
        ..Label::default()
    };
    for row in rows {
        let w = set.width(row, size, face);
        l.lines.push(vec![Span {
            text: row.clone(),
            face,
            icon: None,
            width: w,
        }]);
        l.widths.push(w);
        l.width = max(l.width, w);
    }
    l
}

/// Whether a label has a line with text, as Pele writes a <text> only for one.
fn has_text(l: &Label) -> bool {
    l.lines.iter().flatten().any(|span| !span.text.is_empty())
}

/// A text item of lines starting at x, the first of them below top, or nothing where no
/// line has text.
fn rows(role: &'static str, l: Label, x: f64, top: f64) -> Option<Item> {
    if !has_text(&l) {
        return None;
    }
    let mut it = Item::new(Kind::Text, role);
    let at = Point::new(x, top + l.height / 2.0);
    it.label = Some(Placed {
        align: Align::Start,
        ..Placed::new(l, at)
    });
    it.style = Style {
        text: Some(Paint::FG),
        ..Style::default()
    };
    Some(it)
}

/// The node of a frame in its box: plain for a UI, and for any other kind its series color
/// laid over the background.
fn frame_item(b: &EmBox, r: Rect, label: Label, at: Point) -> Item {
    let mut it = Item::new(Kind::Node, frame_role(b.kind));
    it.shape = Some("rect".into());
    it.id = Some(b.id.clone());
    it.bounds = r;
    if b.reset {
        it.tags = vec!["reset".into()];
    }
    match series(b.kind) {
        Some(k) => {
            // Pele draws the box in the background first, so that the wash hides what is
            // under it.
            it.parts = vec![
                Part::rect(r, true, Painting::Shade),
                Part::rect(r, true, Painting::FillAndStroke),
            ];
            it.style = Style {
                fill: Some(Paint::series(k)),
                fill_opacity: Some(WASH),
                stroke: Some(Paint::series(k)),
                text: Some(Paint::FG),
                ..Style::default()
            };
        }
        None => {
            it.parts = vec![Part::rect(r, true, Painting::FillAndStroke)];
            it.style = Style {
                fill: Some(Paint::BG),
                stroke: Some(Paint::BORDER),
                text: Some(Paint::FG),
                ..Style::default()
            };
        }
    }
    it.label = Some(Placed::new(label, at));
    it
}

/// The edge of a relation along a path, with an arrow at its tip pointing along dx, dy.
fn relation(
    m: &EventModelingModel,
    r: EmRelation,
    p: Path,
    mut route: Vec<Point>,
    tip: Point,
    dx: f64,
    dy: f64,
) -> Item {
    route.push(tip);
    let mut it = Item::new(
        Kind::Edge(Edge {
            route,
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
        ROLE_RELATION,
    );
    it.id = Some(format!("{}-{}", m.boxes[r.source].id, m.boxes[r.target].id));
    it.bounds = p.bounds();
    it.parts = vec![Part::path(p, Painting::StrokeOnly)];
    it.style = Style {
        stroke: Some(Paint::LINE),
        ..Style::default()
    };
    it
}

/// The node of a note or a specification in its box, its heading the label.
fn card(role: &'static str, frame: &str, r: Rect, head: Label, at: Point) -> Item {
    let mut it = Item::new(Kind::Node, role);
    it.shape = Some("rect".into());
    it.id = Some(frame.to_string());
    it.bounds = r;
    it.parts = vec![Part::rect(r, true, Painting::FillAndStroke)];
    it.label = Some(Placed::new(head, at));
    it.style = Style {
        fill: Some(Paint::BG),
        stroke: Some(Paint::BORDER),
        text: Some(Paint::FG),
        ..Style::default()
    };
    it
}

/// The heading of a card on a frame, as Pele writes it: the kind of card, the frame's number,
/// and the name of the frame of that number, if there is one.
fn heading(kind: &str, frame: &str, names: &HashMap<&str, &str>) -> String {
    match names.get(frame) {
        Some(name) => format!("{kind} · {frame} {name}"),
        None => format!("{kind} · {frame}"),
    }
}

/// The rows of a specification, as Pele writes them.
fn spec_rows(spec: &EmSpecification) -> Vec<String> {
    let mut rows = vec![format!("Given  {}", spec.given.join(", "))];
    if !spec.when.is_empty() {
        rows.push(format!("When  {}", spec.when.join(", ")));
    }
    rows.push(format!("Then  {}", spec.then.join(", ")));
    rows
}

/// The title, centered at the top of a scene width wide, pad under its top, or none.
fn title_item(title: Label, width: f64, pad: f64) -> Option<Item> {
    if title.is_empty() {
        return None;
    }
    let mut it = Item::new(Kind::Text, ROLE_TITLE);
    let at = Point::new(width / 2.0, pad + title.height / 2.0);
    it.label = Some(Placed::new(title, at));
    it.style = Style {
        text: Some(Paint::FG),
        ..Style::default()
    };
    Some(it)
}

/// The names of the frames, by their numbers.
fn frame_names(m: &EventModelingModel) -> HashMap<&str, &str> {
    m.boxes
        .iter()
        .map(|b| (b.id.as_str(), b.name.as_str()))
        .collect()
}

/// A frame laid out across, Pele's Placed.
struct Laid {
    name: Label,
    data: Vec<String>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// A lane laid out across, Pele's Band.
struct Band {
    label: Label,
    right: f64,
    y: f64,
    h: f64,
}

/// Lays the diagram out across, as Pele's draw does. Time runs left to right. A frame in a
/// new lane starts under the end of the one before it, so that the arrow between them can
/// drop straight down, and a frame in the same lane follows on.
fn draw(m: &EventModelingModel, o: &Options) -> Scene {
    let size = o.font_size;
    let name_size = round(size * 0.875);
    let small = round(size * 0.8125);
    let mono = round(size * 0.75);
    let mono_step = round(mono * 1.4);
    let small_step = round(small * 1.5);
    let set = setting(o);
    let pad = o.pad();
    let widest = |rows: &[String], size: f64, face: Face| {
        rows.iter()
            .fold(0.0, |w, row| max(w, set.width(row, size, face)))
    };
    let label = |s: &str, size: f64| layout_label(s, false, &set, size, 4000.0, Face::BOLD);

    let mut bands: Vec<Band> = Vec::new();
    let mut label_column: f64 = 0.0;
    for lane in &m.lanes {
        let l = label(&lane.label, small);
        label_column = max(label_column, l.width);
        bands.push(Band {
            label: l,
            right: f64::NEG_INFINITY,
            y: 0.0,
            h: 0.0,
        });
    }
    label_column += 2.0 * LANE_PAD;

    let mut laid: Vec<Laid> = Vec::with_capacity(m.boxes.len());
    for (k, b) in m.boxes.iter().enumerate() {
        let name = label(&b.name, name_size);
        let data = match &b.data {
            Some(d) if !d.is_empty() => hard_wrap(d, DATA_COLUMNS),
            _ => Vec::new(),
        };
        let w = max(
            BOX_MIN_WIDTH,
            max(name.width, widest(&data, mono, Face::MONO)) + 2.0 * BOX_PAD_X,
        );
        let mut h = 2.0 * BOX_PAD_Y + name.height;
        if !data.is_empty() {
            h += 6.0 + data.len() as f64 * mono_step;
        }
        let band = &bands[b.lane];
        let mut x = 0.0;
        if k > 0 {
            let previous = &laid[k - 1];
            let stagger = previous.x + max(previous.w / 2.0, previous.w - STAGGER);
            x = if m.boxes[k - 1].lane == b.lane {
                band.right + BOX_GAP
            } else {
                max(stagger, band.right + BOX_GAP)
            };
        }
        let band = &mut bands[b.lane];
        band.right = x + w;
        band.h = max(band.h, h);
        laid.push(Laid {
            name,
            data,
            x,
            y: 0.0,
            w,
            h,
        });
    }

    let content_right = laid.iter().fold(0.0, |r, item| max(r, item.x + item.w));
    let lane_width = label_column + content_right + LANE_PAD;

    let title = label(m.title.as_deref().unwrap_or(""), size);
    let title_height = if title.height > 0.0 {
        title.height + 12.0
    } else {
        0.0
    };
    let left = pad;
    let top = pad + title_height;

    let mut y = top;
    let mut lanes = Vec::new();
    for (band, lane) in bands.iter_mut().zip(&m.lanes) {
        band.y = y;
        band.h += 2.0 * LANE_PAD;
        let r = Rect::new(left, y, lane_width, band.h);
        let mut it = Item::new(Kind::Group, ROLE_LANE);
        it.shape = Some("rect".into());
        it.id = Some(lane.label.clone());
        it.bounds = r;
        it.parts = vec![Part::rect(r, true, Painting::FillOnly)];
        let at = Point::new(left + LANE_PAD + band.label.width / 2.0, y + band.h / 2.0);
        it.label = Some(Placed::new(band.label.clone(), at));
        it.style = Style {
            fill: Some(Paint::SURFACE),
            text: Some(Paint::MUTED),
            ..Style::default()
        };
        lanes.push(it);
        y += band.h + LANE_GAP;
    }
    let mut bottom = if bands.is_empty() { top } else { y - LANE_GAP };

    let mut nodes = Vec::new();
    for (item, b) in laid.iter_mut().zip(&m.boxes) {
        let band = &bands[b.lane];
        item.x += left + label_column;
        item.y = band.y + (band.h - item.h) / 2.0;
        let at = Point::new(
            item.x + item.w / 2.0,
            item.y + BOX_PAD_Y + item.name.height / 2.0,
        );
        let r = Rect::new(item.x, item.y, item.w, item.h);
        nodes.push(frame_item(b, r, item.name.clone(), at));
        nodes.extend(rows(
            ROLE_DATA,
            rows_label(&item.data, &set, mono, mono_step, Face::MONO),
            item.x + BOX_PAD_X,
            item.y + BOX_PAD_Y + item.name.height + 6.0,
        ));
    }

    let trim = MarkerType::ArrowPoint.trim();
    let mut edges = Vec::new();
    for &r in &m.relations {
        if r.source == r.target {
            continue;
        }
        let (s, t) = (&laid[r.source], &laid[r.target]);
        let mut p = Path::default();
        if m.boxes[r.source].lane == m.boxes[r.target].lane {
            // Along the lane, from the side of one box to the side of the other.
            let forward = t.x >= s.x;
            let (sx, tx) = if forward {
                (s.x + s.w, t.x)
            } else {
                (s.x, t.x + t.w)
            };
            let sy = s.y + s.h / 2.0;
            let ty = t.y + t.h / 2.0;
            let dir = if tx >= sx { 1.0 } else { -1.0 };
            p.move_to(sx, sy).line_to(tx - dir * trim, ty);
            let route = vec![Point::new(sx, sy)];
            edges.push(relation(m, r, p, route, Point::new(tx, ty), dir, 0.0));
            continue;
        }
        // Between lanes, straight down or up where the boxes overlap, else in an S.
        let down = t.y > s.y;
        let from = max(s.x, t.x);
        let to = min(s.x + s.w, t.x + t.w);
        let inset = |w: f64| min(24.0, w / 4.0);
        let (sx, tx) = if to - from >= 16.0 {
            let sx = (from + to) / 2.0;
            (sx, sx)
        } else if t.x > s.x {
            (s.x + s.w - inset(s.w), t.x + inset(t.w))
        } else {
            (s.x + inset(s.w), t.x + t.w - inset(t.w))
        };
        let (sy, ty, dir) = if down {
            (s.y + s.h, t.y, 1.0)
        } else {
            (s.y, t.y + t.h, -1.0)
        };
        let end = ty - dir * trim;
        let mid = (sy + end) / 2.0;
        p.move_to(sx, sy);
        let mut route = vec![Point::new(sx, sy)];
        if sx == tx {
            p.line_to(sx, end);
        } else {
            p.cube_to(sx, mid, tx, mid, tx, end);
            route.push(Point::new(sx, mid));
            route.push(Point::new(tx, mid));
        }
        edges.push(relation(m, r, p, route, Point::new(tx, ty), 0.0, dir));
    }

    // Notes and given/when/then specifications, listed under the lanes with the frame they
    // belong to.
    let names = frame_names(m);
    let mut cards: Vec<Item> = Vec::new();
    let mut cards_right: f64 = 0.0;
    let mut add_card = |role: &'static str,
                        text_role: &'static str,
                        kind: &str,
                        frame: &str,
                        lines: Vec<String>| {
        let head = label(&heading(kind, frame, &names), small);
        let w = max(head.width, widest(&lines, small, Face::REGULAR)) + 2.0 * BOX_PAD_X;
        let h = 2.0 * BOX_PAD_Y + head.height + lines.len() as f64 * small_step;
        let mut cy = bottom;
        if !cards.is_empty() || !bands.is_empty() {
            cy += CARD_GAP;
        }
        let r = Rect::new(left, cy, w, h);
        let at = Point::new(
            left + BOX_PAD_X + head.width / 2.0,
            cy + BOX_PAD_Y + head.height / 2.0,
        );
        let below = cy + BOX_PAD_Y + head.height;
        cards.push(card(role, frame, r, head, at));
        cards.extend(rows(
            text_role,
            rows_label(&lines, &set, small, small_step, Face::REGULAR),
            left + BOX_PAD_X,
            below,
        ));
        bottom = cy + h;
        cards_right = max(cards_right, w);
    };
    for note in &m.notes {
        let lines = hard_wrap(&note.text, NOTE_COLUMNS);
        add_card(ROLE_NOTE, ROLE_NOTE_TEXT, "Note", &note.frame, lines);
    }
    for spec in &m.specifications {
        let lines = spec_rows(spec)
            .iter()
            .flat_map(|row| hard_wrap(row, NOTE_COLUMNS))
            .collect();
        let kind = "Given / When / Then";
        add_card(ROLE_SPECIFICATION, ROLE_SPEC_TEXT, kind, &spec.frame, lines);
    }

    let lanes_width = if bands.is_empty() { 0.0 } else { lane_width };
    let width = max(max(lanes_width, cards_right), title.width);
    let width = (width + 2.0 * pad).ceil();
    let mut items: Vec<Item> = title_item(title, width, pad).into_iter().collect();
    items.extend(lanes);
    items.extend(edges);
    items.extend(nodes);
    items.extend(cards);
    Scene {
        diagram: "eventmodeling".into(),
        width,
        height: (bottom + pad).ceil(),
        font_size: size,
        title: given(&m.acc_title),
        desc: given(&m.acc_descr),
        items,
    }
}

/// A frame laid out down, as Pele's verticalEventModel places it.
struct CardPlace {
    y: f64,
    h: f64,
    w: f64,
    index: usize,
    /// The width of its lane's heading.
    lane: f64,
}

/// Lays the diagram out down, a card to a frame under its lane's heading, as Pele's
/// verticalEventModel does. A relation between neighbouring cards drops straight down beside
/// the lower card's heading. One that skips a card runs down a rail beside the cards.
/// Relations into one card share a rail, and others whose runs overlap take the next rail
/// out, so that no two follow the same line.
fn vertical_event_model(m: &EventModelingModel, o: &Options) -> Scene {
    let size = o.font_size;
    let small = round(size * 0.875);
    let mono = round(size * 0.75);
    let pad = o.pad();
    let set = setting(o);
    let room = max(160.0, o.max_width - 2.0 * pad);

    // The run of the relations into each card, in the order they are first met.
    let mut runs: Vec<(usize, [usize; 2])> = Vec::new();
    // A relation between neighbouring cards passes the lower card's lane heading.
    let mut passed = HashSet::new();
    for r in &m.relations {
        let (s, t) = (r.source, r.target);
        if s == t {
            continue;
        }
        if s.abs_diff(t) < 2 {
            passed.insert(s.max(t));
            continue;
        }
        match runs.iter_mut().find(|(target, _)| *target == t) {
            Some((_, run)) => {
                run[0] = run[0].min(s);
                run[1] = run[1].max(s);
            }
            None => runs.push((t, [s.min(t), s.max(t)])),
        }
    }
    runs.sort_by_key(|&(_, run)| run[0]);
    let mut rail_of: HashMap<usize, usize> = HashMap::new();
    let mut rail_ends: Vec<usize> = Vec::new();
    for &(target, [from, to]) in &runs {
        let rail = match rail_ends.iter().position(|&end| end < from) {
            Some(rail) => rail,
            None => {
                rail_ends.push(0);
                rail_ends.len() - 1
            }
        };
        rail_ends[rail] = to;
        rail_of.insert(target, rail);
    }
    let gutter = 24.0 + RAIL_GAP * rail_ends.len().saturating_sub(1) as f64;
    // Rails are never shared to save room: past what the width allows, the drawing is wider.
    let card_width = max(CARD_MIN, room - gutter);
    let text_room = card_width - 2.0 * BOX_PAD_X;
    let label =
        |s: &str, size: f64, room: f64| layout_label(s, false, &set, size, room, Face::BOLD);
    let literal = |raw: &str, size: f64, face: Face| {
        literal_label(&decode_entities(raw), &set, size, text_room, face)
    };
    let title = label(m.title.as_deref().unwrap_or(""), size, room);
    let mut y = pad;
    if title.height != 0.0 {
        y += title.height + 12.0;
    }
    let mut bottom = pad + title.height;
    let mut width = max(title.width, card_width + gutter);

    let mut nodes = Vec::new();
    let mut places: Vec<CardPlace> = Vec::with_capacity(m.boxes.len());
    for (index, b) in m.boxes.iter().enumerate() {
        let lane_room = if passed.contains(&index) {
            text_room - ARROW_ROOM
        } else {
            text_room
        };
        let lane_label = &m.lanes[b.lane].label;
        let lane = break_label(label(lane_label, small, lane_room), &set, lane_room);
        let name = label(&b.name, small, text_room - 36.0);
        let data = b
            .data
            .as_deref()
            .filter(|d| !d.is_empty())
            .map(|d| literal(d, mono, Face::MONO));
        let data_width = data.as_ref().map_or(0.0, |d| d.width);
        let w = max(
            max(
                max(card_width, lane.width + 2.0 * BOX_PAD_X),
                name.width + 2.0 * BOX_PAD_X + 36.0,
            ),
            data_width + 2.0 * BOX_PAD_X,
        );
        let head_h = lane.height + 12.0;
        let mut h = 2.0 * BOX_PAD_Y + name.height;
        if let Some(d) = &data {
            h += d.height + 6.0;
        }
        let cy = y + head_h;
        let lane_width = lane.width;
        let mut heading = Item::new(Kind::Text, ROLE_LANE_LABEL);
        heading.id = Some(lane_label.clone());
        let at = Point::new(pad + BOX_PAD_X, y + lane.height / 2.0);
        heading.label = Some(Placed {
            align: Align::Start,
            ..Placed::new(lane, at)
        });
        heading.style = Style {
            text: Some(Paint::MUTED),
            ..Style::default()
        };
        nodes.push(heading);
        let at = Point::new(
            pad + BOX_PAD_X + name.width / 2.0,
            cy + BOX_PAD_Y + name.height / 2.0,
        );
        let name_height = name.height;
        nodes.push(frame_item(b, Rect::new(pad, cy, w, h), name, at));
        // The frame's number, its baseline as far below the top of the box as the size of
        // the frame's name.
        let id = line(
            &b.id,
            set.width(&b.id, mono, Face::REGULAR),
            mono,
            Face::REGULAR,
        );
        let mut number = Item::new(Kind::Text, ROLE_FRAME_ID);
        number.id = Some(b.id.clone());
        let at = Point::new(pad + w - BOX_PAD_X, cy + BOX_PAD_Y + small - mono * 0.35);
        number.label = Some(Placed {
            align: Align::End,
            ..Placed::new(id, at)
        });
        number.style = Style {
            text: Some(Paint::MUTED),
            ..Style::default()
        };
        nodes.push(number);
        if let Some(d) = data {
            let top = cy + BOX_PAD_Y + name_height + 6.0;
            nodes.extend(rows(ROLE_DATA, d, pad + BOX_PAD_X, top));
        }
        places.push(CardPlace {
            y: cy,
            h,
            w,
            index,
            lane: lane_width,
        });
        width = max(width, w + gutter);
        bottom = cy + h;
        y = bottom + 28.0;
    }

    let trim = MarkerType::ArrowPoint.trim();
    let mut edges = Vec::new();
    for &r in &m.relations {
        if r.source == r.target {
            continue;
        }
        let (s, t) = (&places[r.source], &places[r.target]);
        let down = t.y > s.y;
        let (dir, sy, ty) = if down {
            (1.0, s.y + s.h, t.y)
        } else {
            (-1.0, s.y, t.y + t.h)
        };
        let mut p = Path::default();
        if s.index.abs_diff(t.index) == 1 {
            // The lower card's lane heading sits in the gap, and wraps to leave the arrow room
            // beside it.
            let lower = if down { t } else { s };
            let cx = pad + max(min(s.w, t.w) / 2.0, BOX_PAD_X + lower.lane + 16.0);
            p.move_to(cx, sy).line_to(cx, ty - dir * trim);
            let route = vec![Point::new(cx, sy)];
            edges.push(relation(m, r, p, route, Point::new(cx, ty), 0.0, dir));
            continue;
        }
        let rail_at = rail_of.get(&r.target).copied().unwrap_or(0) as f64;
        let rail = pad + width - gutter + 12.0 + RAIL_GAP * rail_at;
        // A card can send one relation and receive another: they leave above and arrive below
        // its middle.
        let (from, to) = (s.y + s.h / 2.0 - 5.0, t.y + t.h / 2.0 + 5.0);
        p.move_to(pad + s.w, from)
            .line_to(rail, from)
            .line_to(rail, to)
            .line_to(pad + t.w + trim, to);
        let route = vec![
            Point::new(pad + s.w, from),
            Point::new(rail, from),
            Point::new(rail, to),
        ];
        edges.push(relation(
            m,
            r,
            p,
            route,
            Point::new(pad + t.w, to),
            -1.0,
            0.0,
        ));
    }

    let names = frame_names(m);
    let mut cards = Vec::new();
    let mut add_card =
        |role: &'static str, text_role: &'static str, kind: &str, frame: &str, raw: &str| {
            let head = label(&heading(kind, frame, &names), small, text_room);
            let body = literal(raw, small, Face::REGULAR);
            let w = max(
                max(card_width, head.width + 2.0 * BOX_PAD_X),
                body.width + 2.0 * BOX_PAD_X,
            );
            let h = 2.0 * BOX_PAD_Y + head.height + body.height;
            let r = Rect::new(pad, y, w, h);
            let at = Point::new(
                pad + BOX_PAD_X + head.width / 2.0,
                y + BOX_PAD_Y + head.height / 2.0,
            );
            let below = y + BOX_PAD_Y + head.height;
            cards.push(card(role, frame, r, head, at));
            cards.extend(rows(text_role, body, pad + BOX_PAD_X, below));
            bottom = y + h;
            y = bottom + CARD_GAP;
            width = max(width, w);
        };
    for note in &m.notes {
        add_card(ROLE_NOTE, ROLE_NOTE_TEXT, "Note", &note.frame, &note.text);
    }
    for spec in &m.specifications {
        let rows = spec_rows(spec).join("\n");
        let kind = "Given / When / Then";
        add_card(ROLE_SPECIFICATION, ROLE_SPEC_TEXT, kind, &spec.frame, &rows);
    }

    let width = (width + 2.0 * pad).ceil();
    let mut items: Vec<Item> = title_item(title, width, pad).into_iter().collect();
    items.extend(edges);
    items.extend(nodes);
    items.extend(cards);
    Scene {
        diagram: "eventmodeling".into(),
        width,
        height: (bottom + pad).ceil(),
        font_size: size,
        title: given(&m.acc_title),
        desc: given(&m.acc_descr),
        items,
    }
}

/// An accessible title or description that is not empty. Pele's svgRoot writes neither when it
/// is empty, and `accTitle:` with nothing after it reads as an empty title.
fn given(s: &Option<String>) -> Option<String> {
    s.clone().filter(|s| !s.is_empty())
}
