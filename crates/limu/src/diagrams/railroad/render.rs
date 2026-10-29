//! A railroad diagram laid out as a scene, as Pele's renderRailroad in render.ts draws it: a
//! track for each rule, one under another, and tighter, with sequences folded into rows,
//! where that does not fit the width the host has.

use std::collections::HashMap;

use super::common::fold;
use super::db::{RailroadDb, RailroadNode};
use crate::common::{line, setting, tighten, title_label, title_room};
use crate::js::math::{max, min, round};
use crate::js::number::to_string;
use crate::layout::Options;
use crate::scene::style::{Paint, Style};
use crate::scene::text::{Face, Label, Measurer};
use crate::scene::{Align, Item, Kind, Painting, Part, Path, Placed, Point, Rect, Scene, Shape};
use crate::text::decode_entities;
use crate::value::{Map, Value};

/// The roles of the items of a railroad diagram. The title is above the rules. For each rule
/// in turn: its name above its track, aligned at its start, which has the rule's name as its
/// ID, as have the rule's track, arrows and dots; the track, one path of lines and quarter
/// turns; the arrows on it where a long sequence folds and on the loops of repetitions; the
/// dots at the track's ends; and, in the order Pele draws them, a terminal, a pill; a
/// nonterminal, a box with the theme's corners, with the name it refers to as its ID; special
/// text, a dashed box, with the question marks about its text as items of their own; and how
/// often a loop repeats.
pub(crate) const ROLE_TITLE: &str = "railroad.title";
pub(crate) const ROLE_NAME: &str = "railroad.name";
pub(crate) const ROLE_TRACK: &str = "railroad.track";
pub(crate) const ROLE_ARROWS: &str = "railroad.arrows";
pub(crate) const ROLE_START: &str = "railroad.start";
pub(crate) const ROLE_END: &str = "railroad.end";
pub(crate) const ROLE_TERMINAL: &str = "railroad.terminal";
pub(crate) const ROLE_NONTERMINAL: &str = "railroad.nonterminal";
pub(crate) const ROLE_SPECIAL: &str = "railroad.special";
pub(crate) const ROLE_SPECIAL_MARK: &str = "railroad.special-mark";
pub(crate) const ROLE_COUNT: &str = "railroad.count";

/// The kind of a laid-out element, Pele's Kind.
#[derive(Clone, Copy, PartialEq, Eq)]
enum BoxKind {
    Terminal,
    NonTerminal,
    Special,
    Sequence,
    Choice,
    Bypass,
    Loop,
    Folded,
}

/// A laid-out element, Pele's Box. The track enters at the left edge and leaves at the right
/// edge on one baseline, with up above it and down below it. Its parts are the boxes at
/// their indices in the layout's boxes.
struct LaidOut {
    kind: BoxKind,
    width: f64,
    up: f64,
    down: f64,
    kids: Vec<usize>,
    /// A leaf's text, or a loop's count.
    text: String,
    /// The name a nonterminal refers to.
    id: String,
    /// How far a bypass rises above the baseline, or a loop drops below it.
    reach: f64,
}

impl LaidOut {
    fn new(kind: BoxKind, width: f64, up: f64, down: f64, kids: Vec<usize>) -> LaidOut {
        LaidOut {
            kind,
            width,
            up,
            down,
            kids,
            text: String::new(),
            id: String::new(),
            reach: 0.0,
        }
    }
}

/// The measures of a diagram's elements, Pele's Metrics. gap is the space between elements
/// in a sequence, and sep between stacked branches.
struct Metrics<'a> {
    measurer: &'a dyn Measurer,
    size: f64,
    padding: f64,
    height: f64,
    gap: f64,
    sep: f64,
    arc: f64,
}

const LEAD: f64 = 12.0;
const HEAD_GAP: f64 = 6.0;
const RULE_GAP: f64 = 18.0;
const COUNT_SIZE: f64 = 12.0;
const ARROW: f64 = 7.0;

/// A number of the railroad configuration, where it is one of at least 0, and at most max,
/// as Pele's option reads it.
fn option(config: &Map, key: &str, fallback: f64, most: f64) -> f64 {
    match section(config).and_then(|s| s.get(key)) {
        Some(&Value::Number(v)) if v >= 0.0 => min(v, most),
        _ => fallback,
    }
}

fn section(config: &Map) -> Option<&Map> {
    config.get("railroad").and_then(Value::as_object)
}

/// Text as the diagram shows it, as Pele's shown makes it: literal, so that a terminal such
/// as "<br>" or "*" is itself, not markup. Mermaid's `#35;` codes still stand for
/// characters, and the escapes of characters that would not show are written out.
fn shown(text: &str) -> String {
    let s = if text.contains("¶ß") {
        decode_entities(&text.replace('&', "&amp;"))
    } else {
        text.to_string()
    };
    if !s.contains(['\n', '\r', '\t']) {
        return s;
    }
    s.replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Reverses a list from an index on, as Pele's reverse does.
fn reverse<T>(list: &mut [T], from: usize) {
    list[from..].reverse();
}

/// A value rounded up to an even number, Pele's even.
fn even(value: f64) -> f64 {
    (value / 2.0).ceil() * 2.0
}

fn leaf(kind: BoxKind, text: String, id: String, face: Face, m: &Metrics) -> LaidOut {
    let half = m.height / 2.0;
    let width = max(
        even(m.measurer.width(&text, m.size, face) + 2.0 * m.padding),
        m.height,
    );
    LaidOut {
        text,
        id,
        ..LaidOut::new(kind, width, half, half, Vec::new())
    }
}

/// What a loop says of how often it repeats, as Pele's count writes it, or nothing when that
/// is once or more, or not at all.
fn count(min: f64, max: f64) -> String {
    if max == f64::INFINITY && min <= 1.0 {
        return String::new();
    }
    let text = |v: f64| {
        if v.is_finite() {
            to_string(v)
        } else {
            "∞".to_string()
        }
    };
    if min == max {
        return text(min) + "×";
    }
    if max == f64::INFINITY {
        return "≥".to_string() + &text(min) + "×";
    }
    if min == 0.0 {
        "≤".to_string() + &text(max) + "×"
    } else {
        text(min) + "–" + &text(max) + "×"
    }
}

/// Where the next branch of a choice sits below the baseline, given the one above it, as
/// Pele's below gives it.
fn below(offset: f64, above: &LaidOut, branch: &LaidOut, m: &Metrics) -> f64 {
    max(offset + above.down + m.sep + branch.up, 2.0 * m.arc)
}

fn bypass(boxes: &[LaidOut], kid: usize, m: &Metrics) -> LaidOut {
    let k = &boxes[kid];
    let reach = max(k.up + m.sep, 2.0 * m.arc);
    LaidOut {
        reach,
        ..LaidOut::new(
            BoxKind::Bypass,
            k.width + 4.0 * m.arc,
            reach,
            k.down,
            vec![kid],
        )
    }
}

fn push(boxes: &mut Vec<LaidOut>, b: LaidOut) -> usize {
    boxes.push(b);
    boxes.len() - 1
}

/// A row of boxes, measured, as a sequence.
fn row(boxes: &[LaidOut], kids: Vec<usize>, width: f64) -> LaidOut {
    let (mut up, mut down) = (0.0, 0.0);
    for &kid in &kids {
        up = max(up, boxes[kid].up);
        down = max(down, boxes[kid].down);
    }
    LaidOut::new(BoxKind::Sequence, width, up, down, kids)
}

/// Lays a definition out as boxes, added to boxes, and gives the index of its own, as Pele's
/// measure does. With room, a sequence wider than it has folds into rows, and its parts are
/// sized for the rows inside its rails.
fn measure(
    db: &RailroadDb,
    root: usize,
    m: &Metrics,
    room: f64,
    boxes: &mut Vec<LaidOut>,
) -> usize {
    let nodes = &db.nodes;
    let mut budgets: HashMap<usize, f64> = HashMap::new();
    let mut pending = vec![(root, room)];
    let mut i = 0;
    while room != f64::INFINITY && i < pending.len() {
        let (node, width) = pending[i];
        budgets.insert(node, width);
        let around = match nodes[node] {
            RailroadNode::Choice(_) | RailroadNode::Optional(_) => 4.0 * m.arc,
            RailroadNode::Repetition { min, .. } => (if min == 0.0 { 6.0 } else { 2.0 }) * m.arc,
            RailroadNode::Sequence(_) => 6.0 * m.arc,
            _ => 0.0,
        };
        for child in nodes[node].parts() {
            pending.push((child, max(40.0, width - around)));
        }
        i += 1;
    }
    fold(
        root,
        |n| nodes[n].parts(),
        |n, kids: Vec<usize>| {
            let room = budgets.get(&n).copied().unwrap_or(f64::INFINITY);
            let b = match &nodes[n] {
                RailroadNode::Terminal(value) => {
                    let text = shown(value);
                    let text = if !text.is_empty() && text.bytes().all(|c| c == b' ') {
                        "␣".repeat(text.len())
                    } else {
                        text
                    };
                    leaf(BoxKind::Terminal, text, String::new(), Face::MONO, m)
                }
                RailroadNode::Nonterminal(name) => leaf(
                    BoxKind::NonTerminal,
                    shown(name),
                    name.clone(),
                    Face::REGULAR,
                    m,
                ),
                RailroadNode::Special(text) => {
                    let text = format!("? {} ?", shown(text));
                    leaf(BoxKind::Special, text, String::new(), Face::ITALIC, m)
                }
                RailroadNode::Sequence(_) => sequence(boxes, kids, m, room),
                RailroadNode::Choice(_) => {
                    let (mut inner, mut offset) = (0.0, 0.0);
                    for (i, &kid) in kids.iter().enumerate() {
                        inner = max(inner, boxes[kid].width);
                        if i > 0 {
                            offset = below(offset, &boxes[kids[i - 1]], &boxes[kid], m);
                        }
                    }
                    let up = kids.first().map_or(0.0, |&k| boxes[k].up);
                    let down = kids.last().map_or(0.0, |&k| offset + boxes[k].down);
                    LaidOut::new(BoxKind::Choice, inner + 4.0 * m.arc, up, down, kids)
                }
                RailroadNode::Optional(_) => bypass(boxes, kids[0], m),
                &RailroadNode::Repetition { min, max: most, .. } => {
                    let kid = kids[0];
                    if min == 1.0 && most == 1.0 {
                        return kid;
                    }
                    let text = count(min, most);
                    let k = &boxes[kid];
                    let reach = max(k.down + m.sep, 2.0 * m.arc);
                    let label = if text.is_empty() {
                        0.0
                    } else {
                        even(m.measurer.width(&text, COUNT_SIZE, Face::REGULAR))
                    };
                    let extra = if text.is_empty() {
                        ARROW / 2.0 + 1.0
                    } else {
                        COUNT_SIZE + 6.0
                    };
                    let looped = LaidOut {
                        text,
                        reach,
                        ..LaidOut::new(
                            BoxKind::Loop,
                            max(k.width, label) + 2.0 * m.arc,
                            k.up,
                            reach + extra,
                            vec![kid],
                        )
                    };
                    if min != 0.0 {
                        return push(boxes, looped);
                    }
                    let looped = push(boxes, looped);
                    bypass(boxes, looped, m)
                }
            };
            push(boxes, b)
        },
    )
}

/// A sequence of boxes, folded into rows inside rails where it is wider than the room it
/// has, as Pele's measure lays one out.
fn sequence(boxes: &mut Vec<LaidOut>, kids: Vec<usize>, m: &Metrics, room: f64) -> LaidOut {
    let mut width = kids.len().saturating_sub(1) as f64 * m.gap;
    for &kid in &kids {
        width += boxes[kid].width;
    }
    if width <= room || kids.len() < 2 {
        return row(boxes, kids, width);
    }
    let mut rows: Vec<LaidOut> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut row_w = 0.0;
    for &kid in &kids {
        if !current.is_empty() && row_w + m.gap + boxes[kid].width > max(40.0, room - 6.0 * m.arc) {
            rows.push(row(boxes, std::mem::take(&mut current), row_w));
            row_w = 0.0;
        }
        row_w += (if current.is_empty() { 0.0 } else { m.gap }) + boxes[kid].width;
        current.push(kid);
    }
    rows.push(row(boxes, current, row_w));
    if rows.len() < 2 {
        return row(boxes, kids, width);
    }
    let (mut folded_width, mut offset) = (0.0, 0.0);
    for i in 0..rows.len() {
        folded_width = max(folded_width, rows[i].width);
        if i > 0 {
            offset += rows[i - 1].down + max(m.sep, 2.0 * m.arc) + rows[i].up;
        }
    }
    let up = rows[0].up;
    let down = offset + rows[rows.len() - 1].down;
    let rows = rows.into_iter().map(|r| push(boxes, r)).collect();
    LaidOut::new(BoxKind::Folded, folded_width + 6.0 * m.arc, up, down, rows)
}

/// Lays a railroad diagram out as a scene, as Pele's renderRailroad does.
pub(crate) fn layout(db: &RailroadDb, config: &Map, o: &Options) -> Scene {
    tighten(o, |tight| draw(db, config, o, tight))
}

/// A number of Pele's inside the group of its rules, where the scene has it. Pele moves the
/// group by half a unit each way, so that lines one unit wide fall on whole pixels.
fn off(v: f64) -> f64 {
    v + 0.5
}

/// Draws the track as Pele writes its path, on a path in the scene's places: its places
/// moved as off moves them, and its relative arcs and lines taken from where the path is.
/// Each figure is drawn on a path of its own, which arc_to reads where it is from, and the
/// figures are joined when the next starts, so that a long track is drawn in time in
/// proportion to it.
struct Pen {
    whole: Path,
    p: Path,
    x: f64,
    y: f64,
}

impl Pen {
    fn new() -> Pen {
        Pen {
            whole: Path::default(),
            p: Path::default(),
            x: 0.0,
            y: 0.0,
        }
    }

    /// The whole path drawn.
    fn path(&mut self) -> Path {
        let p = std::mem::take(&mut self.p);
        self.whole.ops.extend(p.ops);
        self.whole.args.extend(p.args);
        self.whole.clone()
    }

    fn join(&mut self) {
        let p = std::mem::take(&mut self.p);
        self.whole.ops.extend(p.ops);
        self.whole.args.extend(p.args);
    }

    fn move_to(&mut self, x: f64, y: f64) {
        self.join();
        (self.x, self.y) = (off(x), off(y));
        self.p.move_to(self.x, self.y);
    }

    fn across(&mut self, x: f64) {
        self.x = off(x);
        self.p.line_to(self.x, self.y);
    }

    fn down(&mut self, y: f64) {
        self.y = off(y);
        self.p.line_to(self.x, self.y);
    }

    /// A line by dx, dy from where the path is, as l draws one.
    fn line(&mut self, dx: f64, dy: f64) {
        (self.x, self.y) = (self.x + dx, self.y + dy);
        self.p.line_to(self.x, self.y);
    }

    /// A quarter of a circle of radius r by dx, dy from where the path is, as Pele's turn
    /// writes one with a: clockwise where sweep is set.
    fn turn(&mut self, r: f64, sweep: bool, dx: f64, dy: f64) {
        (self.x, self.y) = (self.x + dx, self.y + dy);
        self.p.arc_to(r, r, 0.0, false, sweep, self.x, self.y);
    }
}

/// A label of one line of text, measured.
fn text_line(s: &str, face: Face, size: f64, m: &dyn Measurer) -> Label {
    line(s, m.width(s, size, face), size, face)
}

/// What a rule's place puts in the scene: its track, its arrows and the items of its
/// elements.
struct Drawn {
    track: Pen,
    arrows: Pen,
    nodes: Vec<Item>,
}

/// Places a rule's boxes and draws them, from the root's left edge on a baseline, as Pele's
/// place does.
#[allow(clippy::too_many_arguments)]
fn place(
    boxes: &[LaidOut],
    root: usize,
    left: f64,
    baseline: f64,
    m: &Metrics,
    stroke: Option<f64>,
    out: &mut Drawn,
) {
    let arc = m.arc;
    let box_top = m.height / 2.0;
    let (track, arrows) = (&mut out.track, &mut out.arrows);
    let straight = |track: &mut Pen, x1: f64, y: f64, x2: f64| {
        if x2 > x1 {
            track.move_to(x1, y);
            track.across(x2);
        }
    };
    let east_to_south = |t: &mut Pen| t.turn(arc, true, arc, arc);
    let south_to_east = |t: &mut Pen| t.turn(arc, false, arc, arc);
    let east_to_north = |t: &mut Pen| t.turn(arc, false, arc, -arc);
    let north_to_east = |t: &mut Pen| t.turn(arc, true, arc, -arc);
    let loop_down = |t: &mut Pen| t.turn(arc, false, -arc, arc);
    let loop_up = |t: &mut Pen| t.turn(arc, false, -arc, -arc);

    let mut stack = vec![root];
    let mut at = vec![left, baseline];
    while let Some(b) = stack.pop() {
        let y = at.pop().expect("a place");
        let x = at.pop().expect("a place");
        let b = &boxes[b];
        let kids = &b.kids;
        let right = x + b.width;
        match b.kind {
            BoxKind::Sequence => {
                let mut kx = right;
                for i in (0..kids.len()).rev() {
                    kx -= boxes[kids[i]].width;
                    stack.push(kids[i]);
                    at.extend([kx, y]);
                    if i > 0 {
                        straight(track, kx - m.gap, y, kx);
                    }
                    kx -= m.gap;
                }
            }
            BoxKind::Folded => {
                let start = x + 2.0 * arc;
                let from = stack.len();
                let mut offset = 0.0;
                straight(track, x, y, start);
                for (i, &kid) in kids.iter().enumerate() {
                    let k = &boxes[kid];
                    let ky = y + offset;
                    stack.push(kid);
                    at.extend([ky, start]);
                    if i + 1 < kids.len() {
                        let next = &boxes[kids[i + 1]];
                        let next_offset = offset + k.down + max(m.sep, 2.0 * arc) + next.up;
                        let mid = y + offset + k.down + max(m.sep, 2.0 * arc) / 2.0;
                        track.move_to(start + k.width, ky);
                        track.across(right - 2.0 * arc);
                        track.down(mid);
                        track.across(x + arc);
                        track.down(y + next_offset);
                        track.across(start);
                        arrows.move_to(start - ARROW, y + next_offset - ARROW / 2.0);
                        arrows.line(ARROW, ARROW / 2.0);
                        arrows.line(-ARROW, ARROW / 2.0);
                        arrows.p.close();
                        offset = next_offset;
                    } else {
                        track.move_to(start + k.width, ky);
                        track.across(right - arc);
                        track.down(y);
                        track.across(right);
                    }
                }
                reverse(&mut stack, from);
                reverse(&mut at, 2 * from);
            }
            BoxKind::Choice => {
                let inner = b.width - 4.0 * arc;
                let rail_in = x + arc;
                let rail_out = right - arc;
                let from = stack.len();
                let mut offset = 0.0;
                for (i, &kid) in kids.iter().enumerate() {
                    let k = &boxes[kid];
                    if i > 0 {
                        offset = below(offset, &boxes[kids[i - 1]], k, m);
                    }
                    let ky = y + offset;
                    let kx = x + 2.0 * arc + (inner - k.width) / 2.0;
                    let last = i == kids.len() - 1;
                    if i == 0 {
                        straight(track, x, y, kx);
                        straight(track, kx + k.width, y, right);
                    } else {
                        if last {
                            track.move_to(x, y);
                            east_to_south(track);
                            track.down(ky - arc);
                        } else {
                            track.move_to(rail_in, ky - arc);
                        }
                        south_to_east(track);
                        if kx > rail_in + arc {
                            track.across(kx);
                        }
                        track.move_to(kx + k.width, ky);
                        if kx > rail_in + arc {
                            track.across(rail_out - arc);
                        }
                        east_to_north(track);
                        if last {
                            track.down(y + arc);
                            north_to_east(track);
                        }
                    }
                    stack.push(kid);
                    at.extend([ky, kx]);
                }
                // Reversed, so that the branches come off the stack top to bottom.
                reverse(&mut stack, from);
                reverse(&mut at, 2 * from);
            }
            BoxKind::Bypass => {
                let kid = kids[0];
                let kx = x + 2.0 * arc;
                straight(track, x, y, kx);
                straight(track, kx + boxes[kid].width, y, right);
                track.move_to(x, y);
                east_to_north(track);
                track.down(y - b.reach + arc);
                north_to_east(track);
                track.across(right - 2.0 * arc);
                east_to_south(track);
                track.down(y - arc);
                south_to_east(track);
                stack.push(kid);
                at.extend([kx, y]);
            }
            BoxKind::Loop => {
                let kid = kids[0];
                let k = &boxes[kid];
                let kx = x + (b.width - k.width) / 2.0;
                let loop_y = y + b.reach;
                let middle = x + b.width / 2.0;
                straight(track, x, y, kx);
                straight(track, kx + k.width, y, right);
                track.move_to(x + arc, y);
                loop_down(track);
                track.down(loop_y - arc);
                south_to_east(track);
                track.across(right - arc);
                east_to_north(track);
                track.down(y + arc);
                loop_up(track);
                arrows.move_to(middle - ARROW / 2.0, loop_y);
                arrows.line(ARROW, -ARROW / 2.0);
                arrows.line(0.0, ARROW);
                arrows.p.close();
                if !b.text.is_empty() {
                    let mut it = Item::new(Kind::Text, ROLE_COUNT);
                    // The count's baseline is 3 below its size under the loop.
                    let at = Point::new(
                        off(middle),
                        off(loop_y + COUNT_SIZE + 3.0 - COUNT_SIZE * 0.35),
                    );
                    let label = text_line(&b.text, Face::REGULAR, COUNT_SIZE, m.measurer);
                    it.label = Some(Placed::new(label, at));
                    it.style = Style {
                        text: Some(Paint::MUTED),
                        ..Style::default()
                    };
                    out.nodes.push(it);
                }
                stack.push(kid);
                at.extend([kx, y]);
            }
            BoxKind::Terminal | BoxKind::NonTerminal | BoxKind::Special => {
                let r = Rect::new(off(x), off(y - box_top), b.width, m.height);
                let center = Point::new(off(x + b.width / 2.0), off(y));
                let mut it = Item::new(Kind::Node, ROLE_TERMINAL);
                it.bounds = r;
                it.parts = vec![Part::rect(r, true, Painting::FillAndStroke)];
                it.style = Style {
                    fill: Some(Paint::BG),
                    stroke: Some(Paint::BORDER),
                    text: Some(Paint::FG),
                    stroke_width: stroke,
                    ..Style::default()
                };
                match b.kind {
                    BoxKind::Terminal => {
                        it.shape = Some(Shape::new("stadium"));
                        it.style.fill = Some(Paint::SURFACE);
                        it.style.radius = Some(box_top);
                        let label = text_line(&b.text, Face::MONO, m.size, m.measurer);
                        it.label = Some(Placed::new(label, center));
                        out.nodes.push(it);
                    }
                    BoxKind::NonTerminal => {
                        it.role = ROLE_NONTERMINAL.into();
                        it.shape = Some(Shape::new("rounded"));
                        it.id = Some(b.id.clone());
                        let label = text_line(&b.text, Face::REGULAR, m.size, m.measurer);
                        it.label = Some(Placed::new(label, center));
                        out.nodes.push(it);
                    }
                    _ => {
                        // Pele writes the question marks about special text in spans of the
                        // muted color, in one text centered on the box. A label is of one
                        // color, so the marks are items of their own: the first starts where
                        // the whole starts, and the last ends where it ends.
                        it.role = ROLE_SPECIAL.into();
                        it.shape = Some(Shape::new("rounded"));
                        it.style.dash = Some(vec![4.0, 3.0]);
                        let text = &b.text[2..b.text.len() - 2];
                        let label = text_line(text, Face::ITALIC, m.size, m.measurer);
                        it.label = Some(Placed::new(label, center));
                        let half = m.measurer.width(&b.text, m.size, Face::ITALIC) / 2.0;
                        let mark = |s: &str, x: f64, align: Align| {
                            let mut mark = Item::new(Kind::Text, ROLE_SPECIAL_MARK);
                            let label = text_line(s, Face::ITALIC, m.size, m.measurer);
                            let mut placed = Placed::new(label, Point::new(x, center.y));
                            placed.align = align;
                            mark.label = Some(placed);
                            mark.style = Style {
                                text: Some(Paint::MUTED),
                                ..Style::default()
                            };
                            mark
                        };
                        out.nodes.push(it);
                        out.nodes.push(mark("? ", center.x - half, Align::Start));
                        out.nodes.push(mark(" ?", center.x + half, Align::End));
                    }
                }
            }
        }
    }
}

/// Lays the diagram out, with room to spare or tightly, as Pele's draw does.
fn draw(db: &RailroadDb, config: &Map, o: &Options, tight: bool) -> Scene {
    let base = o.font_size;
    let font_size = option(config, "fontSize", 0.0, 200.0);
    let size = if font_size != 0.0 {
        font_size
    } else {
        round(base * 0.875)
    };
    let head_size = if font_size != 0.0 { font_size } else { base };
    let padding = option(config, "padding", -1.0, 200.0);
    let line_height = round(size * 1.5);
    let set = setting(o);
    let m = Metrics {
        measurer: set.measurer,
        size,
        padding: if padding < 0.0 { 12.0 } else { padding },
        height: even(line_height + if padding < 0.0 { 9.0 } else { 2.0 * padding }),
        gap: min(
            option(config, "horizontalSeparation", 16.0, 500.0),
            if tight { 12.0 } else { f64::INFINITY },
        ),
        sep: option(config, "verticalSeparation", 10.0, 500.0),
        arc: min(
            option(config, "arcRadius", 10.0, 200.0),
            if tight { 6.0 } else { f64::INFINITY },
        ),
    };
    let markers = section(config).and_then(|s| s.get("showMarkers")) != Some(&Value::Bool(false));
    let radius = if markers {
        option(config, "markerRadius", 4.0, 50.0)
    } else {
        0.0
    };
    let stroke_width = option(config, "strokeWidth", 0.0, 20.0);
    let stroke = (stroke_width > 0.0).then_some(stroke_width);
    let pad = o.pad();

    let room = if tight { title_room(o) } else { 4000.0 };
    let title = title_label(db.title.as_deref(), &set, base, room);
    let title_height = if title.height > 0.0 {
        title.height + 16.0
    } else {
        0.0
    };
    let head_height = round(head_size * 1.5);

    let mut items: Vec<Item> = Vec::new();
    let mut width = title.width;
    let mut top = pad + title_height;
    let mut bottom = pad + title.height;
    for rule in &db.rules {
        let room = if tight {
            max(
                80.0,
                o.max_width - 2.0 * pad - 4.0 * radius - 2.0 * LEAD - 1.0,
            )
        } else {
            f64::INFINITY
        };
        let mut boxes = Vec::new();
        let root = measure(db, rule.definition, &m, room, &mut boxes);
        let b = &boxes[root];
        let name = shown(&rule.name);
        let baseline = top + head_height + HEAD_GAP + b.up;
        let start = pad + 2.0 * radius;
        let left = start + LEAD;
        let end = left + b.width + LEAD;
        let mut drawn = Drawn {
            track: Pen::new(),
            arrows: Pen::new(),
            nodes: Vec::new(),
        };
        if left > start {
            drawn.track.move_to(start, baseline);
            drawn.track.across(left);
        }
        if end > left + b.width {
            drawn.track.move_to(left + b.width, baseline);
            drawn.track.across(end);
        }
        place(&boxes, root, left, baseline, &m, stroke, &mut drawn);

        let mut it = Item::new(Kind::Text, ROLE_NAME);
        it.id = Some(rule.name.clone());
        let label = text_line(&name, Face::BOLD, head_size, m.measurer);
        let mut placed = Placed::new(label, Point::new(off(pad), off(top + head_height / 2.0)));
        placed.align = Align::Start;
        it.label = Some(placed);
        it.style = Style {
            text: Some(Paint::FG),
            ..Style::default()
        };
        items.push(it);

        let track = drawn.track.path();
        let mut it = Item::new(Kind::Mark, ROLE_TRACK);
        it.id = Some(rule.name.clone());
        it.bounds = track.bounds();
        it.parts = vec![Part::path(track, Painting::StrokeOnly)];
        it.style = Style {
            stroke: Some(Paint::LINE),
            stroke_width: stroke,
            ..Style::default()
        };
        items.push(it);
        let arrows = drawn.arrows.path();
        if !arrows.is_empty() {
            let mut it = Item::new(Kind::Mark, ROLE_ARROWS);
            it.id = Some(rule.name.clone());
            it.bounds = arrows.bounds();
            it.parts = vec![Part::path(arrows, Painting::FillOnly)];
            it.style = Style {
                fill: Some(Paint::LINE),
                ..Style::default()
            };
            items.push(it);
        }
        if radius > 0.0 {
            let dot = |role: &'static str, cx: f64| {
                let (x, y) = (off(cx), off(baseline));
                let mut it = Item::new(Kind::Mark, role);
                it.shape = Some(Shape::new("circle"));
                it.id = Some(rule.name.clone());
                it.bounds = Rect::new(x - radius, y - radius, 2.0 * radius, 2.0 * radius);
                let mut circle = Path::default();
                circle.ellipse(x, y, radius, radius);
                it.parts = vec![Part::path(circle, Painting::FillOnly)];
                it.style = Style {
                    fill: Some(Paint::LINE),
                    ..Style::default()
                };
                it
            };
            items.push(dot(ROLE_START, pad + radius));
            items.push(dot(ROLE_END, end + radius));
        }
        items.append(&mut drawn.nodes);
        width = max(
            max(width, end + 2.0 * radius - pad),
            m.measurer.width(&name, head_size, Face::BOLD),
        );
        bottom = baseline + b.down;
        top = bottom + m.sep + RULE_GAP;
    }

    let total_width = (width + 2.0 * pad).ceil();
    let total_height = (bottom + pad).ceil();
    if !title.is_empty() {
        let at = Point::new(total_width / 2.0, pad + title.height / 2.0);
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        it.label = Some(Placed::new(title, at));
        it.style = Style {
            text: Some(Paint::FG),
            ..Style::default()
        };
        items.insert(0, it);
    }
    Scene {
        diagram: "railroad".into(),
        width: total_width,
        height: total_height,
        font_size: base,
        title: db.acc_title.clone(),
        desc: db.acc_descr.clone(),
        items,
    }
}
