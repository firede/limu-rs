//! The layout of a sequence diagram, as Pele's render.ts draws one: the participants in a row
//! with their lifelines, the messages, notes and blocks down the page in their order, the
//! boxes about the participants they hold, and the participants again at the foot.
//!
//! The scene's items have the roles sequence.title, sequence.box (a group of participants),
//! sequence.rect (a shaded block), sequence.lifeline, sequence.activation, sequence.frame (a
//! loop, alt, opt, par, critical or break block, its kind its shape), sequence.divider (the
//! dashed line between the sections of a frame), sequence.frame-kind (a frame's tab, with its
//! kind), sequence.condition (the text of a frame's section), sequence.message,
//! sequence.number (a message's number), sequence.destroy (the cross a lifeline ends in),
//! sequence.message-label, sequence.note and sequence.actor (a participant, at the head of its
//! lifeline and, mirrored, at its foot, its kind its shape).

use std::cell::RefCell;
use std::collections::HashMap;

use super::actors::{PAD_X, actor_kind, actor_size, circle, draw_actor, is_figure};
use super::db::{
    ACTIVE_END, ACTIVE_START, ALT_ELSE, ALT_END, ALT_START, AUTONUMBER, BIDIRECTIONAL_DOTTED,
    BIDIRECTIONAL_SOLID, BREAK_END, BREAK_START, CENTRAL_CONNECTION, CENTRAL_CONNECTION_DUAL,
    CENTRAL_CONNECTION_REVERSE, CRITICAL_END, CRITICAL_OPTION, CRITICAL_START, DOTTED,
    DOTTED_CROSS, DOTTED_OPEN, DOTTED_POINT, LEFTOF, LOOP_END, LOOP_START, MessageText, NOTE,
    OPT_END, OPT_START, PAR_AND, PAR_END, PAR_OVER_START, PAR_START, RECT_END, RECT_START, RIGHTOF,
    SOLID, SOLID_ARROW_BOTTOM_REVERSE, SOLID_ARROW_TOP_REVERSE, SOLID_BOTTOM, SOLID_CROSS,
    SOLID_OPEN, SOLID_POINT, SOLID_TOP, STICK_ARROW_BOTTOM_REVERSE, STICK_ARROW_TOP_REVERSE,
    STICK_BOTTOM, STICK_TOP, SeqActor, SeqDb, SeqMessage,
};
use crate::common::{break_label, class_names, setting, tighten, title_label, title_room};
use crate::css::{declarations, resolve_style};
use crate::js::math::{max, min, round};
use crate::js::number::to_string;
use crate::layout::{Monospace, Options};
use crate::scene::style::{Cap, Paint, Style};
use crate::scene::text::{Face, Label, Measurer};
use crate::scene::{
    Edge, Item, Kind, MARKER_SIZE, Marker, MarkerType, Painting, Part, Path, Placed, Point, Rect,
    Scene,
};
use crate::text::{Setting, layout_label};
use crate::value::{Map, Value};

// The roles of the items of a sequence diagram.
pub(crate) const ROLE_ACTIVATION: &str = "sequence.activation";
pub(crate) const ROLE_ACTOR: &str = "sequence.actor";
pub(crate) const ROLE_BOX: &str = "sequence.box";
pub(crate) const ROLE_CONDITION: &str = "sequence.condition";
pub(crate) const ROLE_DESTROY: &str = "sequence.destroy";
pub(crate) const ROLE_DIVIDER: &str = "sequence.divider";
pub(crate) const ROLE_FRAME: &str = "sequence.frame";
pub(crate) const ROLE_FRAME_KIND: &str = "sequence.frame-kind";
pub(crate) const ROLE_LIFELINE: &str = "sequence.lifeline";
pub(crate) const ROLE_MESSAGE: &str = "sequence.message";
pub(crate) const ROLE_MESSAGE_LABEL: &str = "sequence.message-label";
pub(crate) const ROLE_NOTE: &str = "sequence.note";
pub(crate) const ROLE_NUMBER: &str = "sequence.number";
pub(crate) const ROLE_RECT: &str = "sequence.rect";
pub(crate) const ROLE_TITLE: &str = "sequence.title";

/// The heads of messages, Pele's Head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Head {
    None,
    Arrow,
    Cross,
    Open,
    HalfSolid,
    HalfStick,
}

/// How a message is drawn, Pele's LineStyle: whether it is dotted, its head, and where the
/// heads are: at the receiver, at the sender only (the reverse arrows), or at both.
#[derive(Clone, Copy, Debug)]
struct LineStyle {
    dotted: bool,
    head: Head,
    at_start: bool,
    at_end: bool,
    bottom: bool,
}

/// How messages of a type are drawn, where they are drawn as lines, Pele's STYLES.
fn line_style(kind: i32) -> Option<LineStyle> {
    // Pele's style: ends is 1 for a head at the receiver, 0 for one at the sender only, and 2
    // for both.
    let style = |dotted, head, ends: u8, bottom| LineStyle {
        dotted,
        head,
        at_start: ends != 1,
        at_end: ends != 0,
        bottom,
    };
    let dotted = (41..=58).contains(&kind) && kind > 50;
    let base = if dotted { kind - 10 } else { kind };
    Some(match kind {
        SOLID => style(false, Head::Arrow, 1, false),
        DOTTED => style(true, Head::Arrow, 1, false),
        SOLID_CROSS => style(false, Head::Cross, 1, false),
        DOTTED_CROSS => style(true, Head::Cross, 1, false),
        SOLID_OPEN => style(false, Head::None, 1, false),
        DOTTED_OPEN => style(true, Head::None, 1, false),
        SOLID_POINT => style(false, Head::Open, 1, false),
        DOTTED_POINT => style(true, Head::Open, 1, false),
        BIDIRECTIONAL_SOLID => style(false, Head::Arrow, 2, false),
        BIDIRECTIONAL_DOTTED => style(true, Head::Arrow, 2, false),
        41..=48 | 51..=58 => match base {
            SOLID_TOP => style(dotted, Head::HalfSolid, 1, false),
            SOLID_BOTTOM => style(dotted, Head::HalfSolid, 1, true),
            STICK_TOP => style(dotted, Head::HalfStick, 1, false),
            STICK_BOTTOM => style(dotted, Head::HalfStick, 1, true),
            SOLID_ARROW_TOP_REVERSE => style(dotted, Head::HalfSolid, 0, false),
            SOLID_ARROW_BOTTOM_REVERSE => style(dotted, Head::HalfSolid, 0, true),
            STICK_ARROW_TOP_REVERSE => style(dotted, Head::HalfStick, 0, false),
            STICK_ARROW_BOTTOM_REVERSE => style(dotted, Head::HalfStick, 0, true),
            _ => return None,
        },
        _ => return None,
    })
}

/// The kind of block a message starts, Pele's FRAME_KIND.
fn frame_kind(kind: i32) -> Option<&'static str> {
    Some(match kind {
        LOOP_START => "loop",
        ALT_START => "alt",
        OPT_START => "opt",
        PAR_START | PAR_OVER_START => "par",
        CRITICAL_START => "critical",
        BREAK_START => "break",
        RECT_START => "rect",
        _ => return None,
    })
}

// The measures of a sequence diagram, as Pele's render.ts has them.
const MSG_PAD: f64 = 12.0;
const SELF_W: f64 = 28.0;
const SELF_H: f64 = 22.0;
const ACT_W: f64 = 10.0;
const ACT_STEP: f64 = 4.0;
const ACT_MIN: f64 = 14.0;
const NOTE_GAP: f64 = 10.0;
const NOTE_OVER: f64 = 16.0;
const FRAME_PAD: f64 = 20.0;
const FRAME_NEST: f64 = 10.0;
const TAB_H: f64 = 20.0;
const FRAME_TEXT: f64 = 240.0;
const BADGE_H: f64 = 16.0;
const CENTRAL_R: f64 = 4.0;
const ARROW: f64 = MARKER_SIZE;
const INF: f64 = f64::INFINITY;

/// The numbers of the messages, Pele's Numbering: the number each message line carries,
/// whether or not it is shown, NaN for everything else, and whether numbers are on after the
/// last statement.
pub(crate) struct Numbering {
    pub(crate) index: Vec<f64>,
    pub(crate) shown: Vec<bool>,
    pub(crate) visible: bool,
}

/// Whether a message of a type is drawn as a line, as Pele's isLineMessage says.
pub(crate) fn is_line_message(kind: i32) -> bool {
    line_style(kind).is_some()
}

/// Counts message lines the way Mermaid's renderer does while it draws, as Pele's
/// numberMessages does.
pub(crate) fn number_messages(messages: &[SeqMessage], always: bool) -> Numbering {
    let mut n = Numbering {
        index: vec![f64::NAN; messages.len()],
        shown: vec![false; messages.len()],
        visible: false,
    };
    let (mut current, mut step) = (1.0, 1.0);
    for (i, m) in messages.iter().enumerate() {
        if m.kind == AUTONUMBER {
            if let MessageText::Numbering(spec) = &m.message {
                // spec.start || current, and the same of the step.
                current = spec
                    .start
                    .filter(|&s| s != 0.0 && !s.is_nan())
                    .unwrap_or(current);
                step = spec
                    .step
                    .filter(|&s| s != 0.0 && !s.is_nan())
                    .unwrap_or(step);
                n.visible = spec.visible;
            }
        } else if is_line_message(m.kind) {
            n.index[i] = current;
            n.shown[i] = n.visible || always;
            current = round((current + step) * 100.0) / 100.0;
        }
    }
    n
}

/// A participant as it is laid out, Pele's Column.
struct Column<'a> {
    id: &'a str,
    actor: &'a SeqActor,
    kind: &'a str,
    label: Label,
    w: f64,
    h: f64,
    x: f64,
    create_at: Option<usize>,
    destroy_at: Option<usize>,
    top: f64,
    life_start: f64,
    life_end: f64,
    activations: Vec<f64>,
}

/// The kinds of view, Pele's V.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum V {
    Line,
    SelfLoop,
    Note,
}

/// A message or a note as it is laid out, Pele's View.
struct View {
    kind: V,
    a: usize,
    b: usize,
    label: Label,
    x: f64,
    w: f64,
}

/// A block as it is laid out, Pele's Frame.
struct Frame {
    kind: &'static str,
    x0: f64,
    x1: f64,
    top: f64,
    title: Label,
    tab_w: f64,
    fill: String,
    dividers: Vec<usize>,
    divider_y: Vec<f64>,
    divider_labels: Vec<Label>,
}

/// A run of participants side by side in one box, Pele's BoxRun.
struct BoxRun {
    r#box: usize,
    first: usize,
    last: usize,
    title: Label,
    grow: f64,
}

/// A number of the sequence section of the config from 0 to 2000, or fallback, as Pele's
/// option reads it.
fn option(conf: Option<&Map>, key: &str, fallback: f64) -> f64 {
    match conf.and_then(|c| c.get(key)) {
        Some(&Value::Number(v)) if (0.0..=2000.0).contains(&v) => v,
        _ => fallback,
    }
}

/// The marker of a head whose tip is at x, y and points along dir across, as Pele's head
/// draws it. up is the side a half head sits on, down the page for 1.
fn head(kind: Head, x: f64, y: f64, dir: f64, up: f64) -> Option<Marker> {
    // The side of a half head as seen along its direction: left of it is down the page for a
    // head pointing right.
    let left = up * dir > 0.0;
    let kind = match kind {
        Head::Arrow => MarkerType::ArrowPoint,
        Head::Cross => MarkerType::ArrowCross,
        Head::Open => MarkerType::SequenceOpen,
        Head::HalfSolid if left => MarkerType::SequenceHalfLeft,
        Head::HalfSolid => MarkerType::SequenceHalfRight,
        Head::HalfStick if left => MarkerType::SequenceStickLeft,
        Head::HalfStick => MarkerType::SequenceStickRight,
        Head::None => return None,
    };
    Some(Marker {
        kind,
        at: Point::new(x, y),
        dx: dir,
        dy: 0.0,
    })
}

/// How far a line stops short of its head's tip, Pele's trim.
fn trim(kind: Head) -> f64 {
    match kind {
        Head::Arrow => ARROW - 1.0,
        Head::Cross => 4.0,
        _ => 0.0,
    }
}

/// How a box or a shaded block is painted, by the fill its author gave it, where Pele writes
/// one: in the theme's alternate surface, at half its opacity where no fill is written, and
/// the author's fill over it. It gives the diagram's style and the author's.
fn fill_style(fill: &str, written: bool) -> (Style, Style) {
    if !written {
        let own = Style {
            fill: Some(Paint::SURFACE_ALT),
            fill_opacity: Some(0.5),
            ..Style::default()
        };
        return (own, Style::default());
    }
    let own = Style {
        fill: Some(Paint::SURFACE_ALT),
        ..Style::default()
    };
    (own, declarations(&[format!("fill:{fill}")]).style)
}

/// Whether Pele writes a fill an author gave: whether its resolveStyle keeps it.
fn writes_fill(fill: &str) -> bool {
    !resolve_style(&[format!("fill:{fill}")]).shape.is_empty()
}

/// Measures text in the monospace face whatever face it is asked of, as the messageMeasurer
/// of Pele's draw does where monospace lists messages.
struct MonoMeasurer<'a>(&'a dyn Measurer);

impl Measurer for MonoMeasurer<'_> {
    fn width(&self, s: &str, size: f64, f: Face) -> f64 {
        self.0.width(s, size, f | Face::MONO)
    }
}

/// A label measured by a MonoMeasurer, with each of its spans in the monospace face, so
/// that it is drawn in the face it was measured in. Pele draws the labels of messages in a
/// group that sets the monospace font instead.
fn mono_label(mut label: Label) -> Label {
    for line in &mut label.lines {
        for span in line {
            span.face |= Face::MONO;
        }
    }
    label
}

/// The box a route's corners lie in, the box of a message's item.
fn route_bounds(route: &[Point]) -> Rect {
    let Some(first) = route.first() else {
        return Rect::default();
    };
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
    for p in &route[1..] {
        (x0, y0) = (x0.min(p.x), y0.min(p.y));
        (x1, y1) = (x1.max(p.x), y1.max(p.y));
    }
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// A number as a key of a JavaScript Map, which takes 0 and -0 to be the same key.
fn key(x: f64) -> u64 {
    if x == 0.0 { 0 } else { x.to_bits() }
}

/// Lays a sequence diagram out, as Pele's renderSequence does: loosely, and tightly when that
/// does not fit the width the host has and is narrower.
pub(crate) fn layout(db: &SeqDb, config: &Map, o: &Options) -> Scene {
    tighten(o, |tight| draw(db, config, o, tight, 0.0, true))
}

/// Lays the diagram out, as Pele's draw does. Laid out tightly in a host of a set width, it
/// widens the gaps between lifelines into the width to spare where that wraps messages into
/// fewer lines. hold is width it keeps from that spare width, and by_need lets it share the
/// spare width out by what each gap needs rather than evenly. When what it widened comes out
/// too wide, it lays the diagram out again with other values of them.
#[allow(clippy::too_many_lines)]
fn draw(db: &SeqDb, config: &Map, o: &Options, tight: bool, hold: f64, by_need: bool) -> Scene {
    let conf = config.get("sequence").and_then(Value::as_object);
    let flag = |key: &str| conf.and_then(|c| c.get(key));
    let size = o.font_size;
    let small = round(size * 0.875);
    let tiny = round(size * 0.75);
    let set = setting(o);
    let monospaced = o.monospace.contains(Monospace::MESSAGES);
    let mono = MonoMeasurer(set.measurer);
    let message_set = if monospaced {
        Setting {
            measurer: &mono,
            icons: set.icons,
        }
    } else {
        set
    };
    let pad = o.padding;
    let mirror = flag("mirrorActors") != Some(&Value::Bool(false));
    let right_angles = flag("rightAngles") == Some(&Value::Bool(true));
    let pick = |tight_value: f64| if tight { tight_value } else { INF };
    let actor_margin = min(option(conf, "actorMargin", 40.0), pick(8.0));
    let min_w = min(option(conf, "width", 64.0), pick(48.0));
    let min_h = option(conf, "height", round(size * 1.5) + 20.0);
    let max_width = if o.max_width.is_finite() {
        o.max_width
    } else {
        320.0
    };
    let wrap_w = min(
        option(conf, "width", 160.0),
        if tight {
            max(
                32.0,
                (max_width - 2.0 * pad) / max(1.0, db.actors.len() as f64) - actor_margin - 24.0,
            )
        } else {
            INF
        },
    );
    // Mermaid's default of 35 corresponds to Pele's default gap. Other values move it by the
    // difference.
    let msg_gap = max(4.0, 14.0 + option(conf, "messageMargin", 35.0) - 35.0);
    let note_pad = min(option(conf, "noteMargin", 10.0), pick(6.0));
    let box_pad = min(option(conf, "boxMargin", 10.0), pick(6.0));
    let align = match flag("messageAlign").and_then(Value::as_str) {
        Some("left") => -1,
        Some("right") => 1,
        _ => 0,
    };

    let msg_pad = if tight { 6.0 } else { MSG_PAD };
    let note_over = if tight { 8.0 } else { NOTE_OVER };
    let frame_pad = if tight { 12.0 } else { FRAME_PAD };
    let frame_nest = if tight { 6.0 } else { FRAME_NEST };
    let messages = &db.messages;
    let count = messages.len();
    let numbering = number_messages(
        messages,
        flag("showSequenceNumbers") == Some(&Value::Bool(true)),
    );
    let text_by = |raw: &str, font_size: f64, wrap: bool, width: f64, by: &Setting| -> Label {
        let label = layout_label(
            raw,
            false,
            by,
            font_size,
            if tight || wrap { width } else { INF },
            Face::REGULAR,
        );
        if tight {
            break_label(label, by, max(120.0, width))
        } else {
            label
        }
    };
    let text = |raw: &str, font_size: f64, wrap: bool| text_by(raw, font_size, wrap, wrap_w, &set);
    let message_text = |raw: &str, wrap: bool, width: f64| -> Label {
        let label = text_by(raw, small, wrap, width, &message_set);
        if monospaced { mono_label(label) } else { label }
    };

    let used: Option<Vec<Option<&str>>> =
        (flag("hideUnusedParticipants") == Some(&Value::Bool(true))).then(|| {
            messages
                .iter()
                .flat_map(|m| [m.from.as_deref(), m.to.as_deref()])
                .collect()
        });

    let mut next_line = vec![None; count + 1];
    for i in (0..count).rev() {
        next_line[i] = if is_line_message(messages[i].kind) {
            Some(i)
        } else {
            next_line[i + 1]
        };
    }

    let position =
        |list: &[(String, usize)], id: &str| list.iter().find(|(k, _)| k == id).map(|&(_, i)| i);
    let mut cols: Vec<Column> = Vec::new();
    let mut col_of: HashMap<&str, usize> = HashMap::new();
    for actor in &db.actors {
        let id = actor.id.as_str();
        if let Some(used) = &used
            && !used.contains(&Some(id))
        {
            continue;
        }
        let kind = actor_kind(&actor.kind);
        let label = text(&actor.description, size, actor.wrap);
        let mut create_at = position(&db.created_actors, id).and_then(|c| next_line[c.min(count)]);
        if create_at.is_some_and(|c| messages[c].to.as_deref() != Some(id)) {
            create_at = None;
        }
        let mut destroy_at =
            position(&db.destroyed_actors, id).and_then(|d| next_line[d.min(count)]);
        if destroy_at.is_some_and(|d| {
            messages[d].to.as_deref() != Some(id) && messages[d].from.as_deref() != Some(id)
        }) {
            destroy_at = None;
        }
        let (w, h) = actor_size(kind, &label, min_w, min_h, if tight { 8.0 } else { PAD_X });
        col_of.insert(id, cols.len());
        cols.push(Column {
            id,
            actor,
            kind,
            label,
            w,
            h,
            x: 0.0,
            create_at,
            destroy_at,
            top: 0.0,
            life_start: 0.0,
            life_end: -1.0,
            activations: Vec::new(),
        });
    }
    let n_cols = cols.len();
    let last = n_cols as isize - 1;

    // Widths between lifelines. Every message and note says how far apart the lifelines it
    // touches must be.
    let mut needs: HashMap<usize, f64> = HashMap::new();
    let mut need_keys: Vec<usize> = Vec::new();
    let mut need = |lo: isize, hi: isize, width: f64| {
        if lo < 0 || hi > last {
            return;
        }
        let key = lo as usize * n_cols + hi as usize;
        let was = needs.get(&key).copied();
        if was.unwrap_or(0.0) < width {
            if was.is_none() {
                need_keys.push(key);
            }
            needs.insert(key, width);
        }
    };
    let badge_width = |i: usize| -> f64 {
        if numbering.shown[i] {
            max(
                BADGE_H,
                set.width(&to_string(numbering.index[i]), tiny, Face::REGULAR) + 8.0,
            )
        } else {
            0.0
        }
    };

    let mut views: Vec<Option<View>> = (0..count).map(|_| None).collect();
    let mut sized: Vec<Option<HashMap<u64, Label>>> = (0..count).map(|_| None).collect();
    for (i, m) in messages.iter().enumerate() {
        let a = m.from.as_deref().and_then(|f| col_of.get(f).copied());
        let b = m.to.as_deref().and_then(|t| col_of.get(t).copied());
        let (Some(a), Some(b), Some(raw)) = (a, b, m.text()) else {
            continue;
        };
        let (ai, bi) = (a as isize, b as isize);
        if m.kind == NOTE {
            let label = text(raw, small, m.wrap);
            let w = label.width + 2.0 * note_pad;
            let placement = m.placement.unwrap_or(-1);
            if placement == LEFTOF {
                need(ai - 1, ai, w + 2.0 * NOTE_GAP + ACT_W);
            } else if placement == RIGHTOF {
                need(ai, ai + 1, w + 2.0 * NOTE_GAP + ACT_W);
            } else if a == b {
                need(ai - 1, ai, w / 2.0 + NOTE_GAP);
                need(ai, ai + 1, w / 2.0 + NOTE_GAP);
            } else {
                need(ai.min(bi), ai.max(bi), w - 2.0 * note_over);
            }
            views[i] = Some(View {
                kind: V::Note,
                a,
                b,
                label,
                x: 0.0,
                w,
            });
        } else if is_line_message(m.kind) {
            let label = message_text(raw, m.wrap, wrap_w);
            let badge = badge_width(i);
            if a == b {
                need(
                    ai,
                    ai + 1,
                    SELF_W + 8.0 + label.width + 16.0 + ACT_W + badge,
                );
                views[i] = Some(View {
                    kind: V::SelfLoop,
                    a,
                    b,
                    label,
                    x: 0.0,
                    w: 0.0,
                });
            } else {
                let created = if cols[b].create_at == Some(i) {
                    cols[b].w / 2.0
                } else {
                    0.0
                };
                let central = if m.central_connection.unwrap_or(0) != 0 {
                    4.0 * CENTRAL_R
                } else {
                    0.0
                };
                let lw = if tight {
                    min(label.width, max(16.0, wrap_w - 16.0))
                } else {
                    label.width
                };
                need(
                    ai.min(bi),
                    ai.max(bi),
                    lw + 2.0 * msg_pad + ACT_W + badge + created + central,
                );
                views[i] = Some(View {
                    kind: V::Line,
                    a,
                    b,
                    label,
                    x: 0.0,
                    w: 0.0,
                });
            }
        }
    }

    let mut runs: Vec<BoxRun> = Vec::new();
    for (i, col) in cols.iter().enumerate() {
        let Some(b) = col.actor.r#box else {
            continue;
        };
        match runs.last_mut() {
            Some(run) if run.r#box == b && run.last + 1 == i => run.last = i,
            _ => {
                let bx = &db.boxes[b];
                runs.push(BoxRun {
                    r#box: b,
                    first: i,
                    last: i,
                    title: text(bx.name.as_deref().unwrap_or(""), small, bx.wrap),
                    grow: 0.0,
                });
            }
        }
    }

    let mut gaps = vec![0.0; n_cols.saturating_sub(1)];
    for (i, gap) in gaps.iter_mut().enumerate() {
        let (left, right) = (&cols[i], &cols[i + 1]);
        let boxed = if left.actor.r#box != right.actor.r#box {
            (if left.actor.r#box.is_some() {
                box_pad
            } else {
                0.0
            }) + (if right.actor.r#box.is_some() {
                box_pad
            } else {
                0.0
            })
        } else {
            0.0
        };
        *gap = max(
            left.w / 2.0 + right.w / 2.0 + actor_margin + boxed,
            needs.get(&(i * n_cols + i + 1)).copied().unwrap_or(0.0),
        );
    }
    // A message across several lifelines widens each gap it crosses by an equal share of what
    // is missing.
    let mut spans: Vec<(usize, usize, f64)> = need_keys
        .iter()
        .map(|&k| (k / n_cols, k % n_cols, needs[&k]))
        .filter(|&(lo, hi, _)| hi > lo + 1)
        .collect();
    spans.sort_by(|p, q| (p.1 - p.0).cmp(&(q.1 - q.0)).then(p.0.cmp(&q.0)));
    for &(lo, hi, width) in &spans {
        let mut total = 0.0;
        for gap in &gaps[lo..hi] {
            total += gap;
        }
        if total >= width {
            continue;
        }
        let share = (width - total) / (hi - lo) as f64;
        for gap in &mut gaps[lo..hi] {
            *gap += share;
        }
    }
    let place = |cols: &mut [Column], gaps: &[f64]| {
        for i in 1..cols.len() {
            cols[i].x = cols[i - 1].x + gaps[i - 1];
        }
    };
    place(&mut cols, &gaps);
    for run in &mut runs {
        let (first, end) = (&cols[run.first], &cols[run.last]);
        let inner = end.x + end.w / 2.0 - (first.x - first.w / 2.0);
        run.grow = max(0.0, (run.title.width - inner) / 2.0);
        if run.grow == 0.0 {
            continue;
        }
        if run.first > 0 {
            gaps[run.first - 1] += run.grow;
        }
        if (run.last as isize) < last {
            gaps[run.last] += run.grow;
        }
    }

    let (mut min_x, mut max_x) = (INF, -INF);
    let extend = |min_x: &mut f64, max_x: &mut f64, x0: f64, x1: f64| {
        if x0 < *min_x {
            *min_x = x0;
        }
        if x1 > *max_x {
            *max_x = x1;
        }
    };
    let box_edges = |cols: &[Column], run: &BoxRun| -> (f64, f64) {
        let (first, end) = (&cols[run.first], &cols[run.last]);
        (
            first.x - first.w / 2.0 - box_pad - run.grow,
            end.x + end.w / 2.0 + box_pad + run.grow,
        )
    };
    let mut frames: Vec<Frame> = Vec::new();
    let mut frame_at: Vec<Option<usize>> = vec![None; count];
    // Places the participants from the gaps between their lifelines and works out the extents
    // across of what is drawn, of every block and of every note, and the text of the messages
    // that wrap to the room between their lifelines, as the layout of Pele's draw does. It is
    // laid out again for each share of spare width tried.
    let lay_across = |cols: &mut Vec<Column>,
                      gaps: &[f64],
                      views: &mut Vec<Option<View>>,
                      sized: &mut Vec<Option<HashMap<u64, Label>>>,
                      frames: &mut Vec<Frame>,
                      frame_at: &mut Vec<Option<usize>>,
                      min_x: &mut f64,
                      max_x: &mut f64| {
        place(cols, gaps);
        (*min_x, *max_x) = (INF, -INF);
        for col in cols.iter() {
            extend(min_x, max_x, col.x - col.w / 2.0, col.x + col.w / 2.0);
        }
        for run in &runs {
            let (x0, x1) = box_edges(cols, run);
            extend(min_x, max_x, x0, x1);
        }

        // Horizontal extents of every block, from what it holds.
        frames.clear();
        frame_at.iter_mut().for_each(|f| *f = None);
        let mut open: Vec<usize> = Vec::new();
        let span_all = || -> (f64, f64) {
            if last < 0 {
                (0.0, 120.0)
            } else {
                (cols[0].x - frame_pad, cols[last as usize].x + frame_pad)
            }
        };
        let span_all = span_all();
        for (i, m) in messages.iter().enumerate() {
            let kind = m.kind;
            if let Some(fk) = frame_kind(kind) {
                frames.push(Frame {
                    kind: fk,
                    x0: INF,
                    x1: -INF,
                    top: 0.0,
                    title: Label::default(),
                    tab_w: 0.0,
                    fill: if fk == "rect" {
                        m.text().unwrap_or("").to_string()
                    } else {
                        String::new()
                    },
                    dividers: vec![i],
                    divider_y: Vec::new(),
                    divider_labels: Vec::new(),
                });
                frame_at[i] = Some(frames.len() - 1);
                open.push(frames.len() - 1);
            } else if kind == ALT_ELSE || kind == PAR_AND || kind == CRITICAL_OPTION {
                if let Some(&f) = open.last() {
                    frames[f].dividers.push(i);
                }
            } else if matches!(
                kind,
                LOOP_END | ALT_END | OPT_END | PAR_END | CRITICAL_END | BREAK_END | RECT_END
            ) {
                let Some(f) = open.pop() else {
                    continue;
                };
                frame_at[i] = Some(f);
                let frame = &mut frames[f];
                if frame.x0 > frame.x1 {
                    (frame.x0, frame.x1) = span_all;
                }
                if frame.kind != "rect" {
                    frame.tab_w = set.width(frame.kind, tiny, Face::BOLD) + 14.0;
                    // A block grows to the right to give its texts up to FRAME_TEXT of width
                    // before they wrap.
                    for k in 0..frame.dividers.len() {
                        let raw = messages[frame.dividers[k]].text().unwrap_or("");
                        let tab = if k == 0 { frame.tab_w + 8.0 } else { 0.0 };
                        let room = frame.x1 - frame.x0 - 16.0 - tab;
                        let label = if raw.is_empty() {
                            Label::default()
                        } else {
                            let floor = if tight {
                                min(FRAME_TEXT, wrap_w)
                            } else {
                                FRAME_TEXT
                            };
                            text_by(&format!("[{raw}]"), small, true, max(room, floor), &set)
                        };
                        if label.width > room {
                            frame.x1 += label.width - room;
                        }
                        if k == 0 {
                            frame.title = label;
                        } else {
                            frame.divider_labels.push(label);
                        }
                    }
                }
                let (x0, x1) = (frame.x0, frame.x1);
                extend(min_x, max_x, x0, x1);
                if let Some(&parent) = open.last() {
                    let parent = &mut frames[parent];
                    parent.x0 = min(parent.x0, x0 - frame_nest);
                    parent.x1 = max(parent.x1, x1 + frame_nest);
                }
            } else if let Some(view) = &mut views[i] {
                let (a, b) = (&cols[view.a], &cols[view.b]);
                let (x0, x1);
                match view.kind {
                    V::Note => {
                        let placement = m.placement.unwrap_or(-1);
                        if placement == LEFTOF {
                            view.x = a.x - NOTE_GAP - view.w;
                        } else if placement == RIGHTOF {
                            view.x = a.x + NOTE_GAP;
                        } else if view.a == view.b {
                            view.x = a.x - view.w / 2.0;
                        } else {
                            let lo = min(a.x, b.x) - note_over;
                            let hi = max(a.x, b.x) + note_over;
                            if tight || m.wrap {
                                view.label = text_by(
                                    m.text().unwrap_or(""),
                                    small,
                                    true,
                                    max(wrap_w, hi - lo - 2.0 * note_pad),
                                    &set,
                                );
                            }
                            view.w = max(hi - lo, view.label.width + 2.0 * note_pad);
                            view.x = (lo + hi - view.w) / 2.0;
                        }
                        x0 = view.x;
                        x1 = view.x + view.w;
                    }
                    V::SelfLoop => {
                        x0 = a.x;
                        x1 = a.x + ACT_W + SELF_W + 8.0 + view.label.width + badge_width(i);
                    }
                    V::Line => {
                        let mut l = min(a.x, b.x);
                        let mut r = max(a.x, b.x);
                        if tight || m.wrap {
                            // The text of message i wrapped to room, as the sizedText of
                            // Pele's draw gives it.
                            let room = max(wrap_w, r - l - 2.0 * msg_pad - ACT_W);
                            let known = &mut sized[i];
                            if known.is_some() || room != wrap_w {
                                let known = known.get_or_insert_with(|| {
                                    HashMap::from([(key(wrap_w), view.label.clone())])
                                });
                                let label = known.entry(key(room)).or_insert_with(|| {
                                    message_text(m.text().unwrap_or(""), true, room)
                                });
                                view.label = label.clone();
                            }
                        }
                        if b.create_at == Some(i) {
                            if b.x > a.x {
                                r += b.w / 2.0;
                            } else {
                                l -= b.w / 2.0;
                            }
                        }
                        x0 = l;
                        x1 = r;
                    }
                }
                extend(min_x, max_x, x0, x1);
                if let Some(&f) = open.last() {
                    let frame = &mut frames[f];
                    frame.x0 = min(frame.x0, x0 - frame_pad);
                    frame.x1 = max(frame.x1, x1 + frame_pad);
                }
            }
        }
    };
    lay_across(
        &mut cols,
        &gaps,
        &mut views,
        &mut sized,
        &mut frames,
        &mut frame_at,
        &mut min_x,
        &mut max_x,
    );

    // Widen gaps only if it reduces message wrapping.
    let mut grown = false;
    let mut uneven = false;
    if tight && o.max_width.is_finite() {
        let spare = o.max_width.floor() - (max_x - min_x + 2.0 * pad).ceil() - hold;
        let mut stretch = vec![0i32; n_cols];
        let mut want = vec![0.0; gaps.len()];
        let mut wrapped: Vec<usize> = Vec::new();
        let mut lines = 0usize;
        let mut i = 0;
        while spare > 0.0 && i < views.len() {
            let Some(view) = views[i].as_ref().filter(|v| v.kind == V::Line) else {
                i += 1;
                continue;
            };
            let natural = message_text(messages[i].text().unwrap_or(""), false, INF);
            if view.label.lines.len() <= natural.lines.len() {
                i += 1;
                continue;
            }
            wrapped.push(i);
            lines += view.label.lines.len();
            let (lo, hi) = (view.a.min(view.b), view.a.max(view.b));
            stretch[lo] += 1;
            stretch[hi] -= 1;
            let short = (natural.width - (cols[hi].x - cols[lo].x - 2.0 * msg_pad - ACT_W))
                / (hi - lo) as f64;
            for w in &mut want[lo..hi] {
                *w = max(*w, short);
            }
            i += 1;
        }
        let mut active = 0;
        let mut crossed = 0.0;
        for s in stretch.iter_mut().take(gaps.len()) {
            active += *s;
            *s = i32::from(active > 0);
            crossed += f64::from(*s);
        }
        if crossed > 0.0 {
            let before = gaps.clone();
            let even = |g: usize| f64::from(stretch[g]) * spare / crossed;
            let spread = |gaps: &mut Vec<f64>,
                          share: &dyn Fn(usize) -> f64,
                          cols: &mut Vec<Column>,
                          views: &mut Vec<Option<View>>,
                          sized: &mut Vec<Option<HashMap<u64, Label>>>,
                          frames: &mut Vec<Frame>,
                          frame_at: &mut Vec<Option<usize>>,
                          min_x: &mut f64,
                          max_x: &mut f64|
             -> Vec<usize> {
                for (g, gap) in gaps.iter_mut().enumerate() {
                    *gap = before[g] + share(g);
                }
                lay_across(cols, gaps, views, sized, frames, frame_at, min_x, max_x);
                wrapped
                    .iter()
                    .map(|&i| views[i].as_ref().map_or(0, |v| v.label.lines.len()))
                    .collect()
            };
            let sum = |counts: &[usize]| counts.iter().sum::<usize>();
            let mut counts = spread(
                &mut gaps,
                &even,
                &mut cols,
                &mut views,
                &mut sized,
                &mut frames,
                &mut frame_at,
                &mut min_x,
                &mut max_x,
            );
            let mut missing = 0.0;
            for w in &want {
                missing += w;
            }
            // Allow for rounding error.
            let by_needs = |g: usize| want[g] * (spare - 0.01) / missing;
            // Reducing wrapping requires at least one gap wider than the even split.
            if by_need
                && crossed > 1.0
                && missing > 0.0
                && (0..gaps.len()).any(|g| by_needs(g) > even(g))
            {
                let first = counts;
                counts = spread(
                    &mut gaps,
                    &by_needs,
                    &mut cols,
                    &mut views,
                    &mut sized,
                    &mut frames,
                    &mut frame_at,
                    &mut min_x,
                    &mut max_x,
                );
                uneven =
                    sum(&counts) < sum(&first) && counts.iter().zip(&first).all(|(n, f)| n <= f);
                if !uneven {
                    counts = spread(
                        &mut gaps,
                        &even,
                        &mut cols,
                        &mut views,
                        &mut sized,
                        &mut frames,
                        &mut frame_at,
                        &mut min_x,
                        &mut max_x,
                    );
                }
            }
            grown = sum(&counts) < lines;
            if !grown {
                gaps.copy_from_slice(&before);
                lay_across(
                    &mut cols,
                    &gaps,
                    &mut views,
                    &mut sized,
                    &mut frames,
                    &mut frame_at,
                    &mut min_x,
                    &mut max_x,
                );
            }
        }
    }

    // Vertical placement, top to bottom, drawing as it goes.
    let mut box_title_h = 0.0;
    for run in &runs {
        box_title_h = max(box_title_h, run.title.height);
    }
    let box_top = if runs.is_empty() {
        0.0
    } else {
        box_pad
            + if box_title_h > 0.0 {
                box_title_h + 4.0
            } else {
                0.0
            }
    };
    let mut row_h = 0.0;
    for col in &cols {
        if col.create_at.is_none() {
            row_h = max(row_h, col.h);
        }
    }
    let head_bottom = box_top + row_h;
    for col in &mut cols {
        col.top = head_bottom - col.h;
        col.life_start = head_bottom;
    }

    let xs: Vec<f64> = cols.iter().map(|c| c.x).collect();
    // Whether a lifeline runs between two x positions, where text would be struck through.
    let crosses = |x0: f64, x1: f64| -> bool {
        let (mut lo, mut hi) = (0, xs.len());
        while lo < hi {
            let mid = (lo + hi) >> 1;
            if xs[mid] <= x0 {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo < xs.len() && xs[lo] < x1
    };
    let mut stack: Vec<usize> = Vec::new();
    let inks: RefCell<HashMap<String, Option<Paint>>> = RefCell::new(HashMap::new());
    // The ground a fill an author gave a shaded block or a box makes for what lies inside
    // it, read once for each fill, as the inkOf of Pele's draw reads its ink. It is none where
    // there is no fill or it is not one ink picks a color on, and what lies inside it then
    // keeps the theme's colors.
    let ink_of = |fill: &str| -> Option<Paint> {
        if fill.is_empty() {
            return None;
        }
        *inks
            .borrow_mut()
            .entry(fill.to_string())
            .or_insert_with(|| {
                let decls = [format!("fill:{fill}")];
                if resolve_style(&decls).inks() {
                    declarations(&decls).style.fill
                } else {
                    None
                }
            })
    };
    // Prefer the innermost rect's fill, then a containing box's.
    let fill_under =
        |stack: &[usize], frames: &[Frame], cols: &[Column], l: f64, r: f64| -> String {
            for &k in stack.iter().rev() {
                if frames[k].kind == "rect" {
                    return frames[k].fill.clone();
                }
            }
            for run in &runs {
                let (x0, x1) = box_edges(cols, run);
                if l >= x0 && r <= x1 {
                    return db.boxes[run.r#box].fill.clone();
                }
            }
            String::new()
        };
    // A label on a patch of the background where a lifeline runs under it, and on the ground
    // of a fill it lies inside otherwise, as Pele's backed draws it. It is none where there is
    // nothing to draw.
    let backed =
        |mut it: Item, label: &Label, cx: f64, cy: f64, ground: Option<Paint>| -> Option<Item> {
            if label.width == 0.0 {
                return None;
            }
            let l = cx - label.width / 2.0;
            if crosses(l, l + label.width) {
                it.parts = vec![Part::rect(
                    Rect::new(
                        l - 3.0,
                        cy - label.height / 2.0 + 2.0,
                        label.width + 6.0,
                        label.height - 4.0,
                    ),
                    true,
                    Painting::FillOnly,
                )];
                it.style.fill = Some(Paint::BG);
                it.style.radius = Some(3.0);
            } else {
                it.style.ground = ground;
            }
            it.bounds = Rect::new(l, cy - label.height / 2.0, label.width, label.height);
            it.label = Some(Placed::new(label.clone(), Point::new(cx, cy)));
            Some(it)
        };
    let left_edge = |col: &Column, extra: usize| -> f64 {
        if col.activations.len() + extra > 0 {
            col.x - ACT_W / 2.0
        } else {
            col.x
        }
    };
    let right_edge = |col: &Column, extra: usize| -> f64 {
        let depth = col.activations.len() + extra;
        if depth > 0 {
            col.x + ACT_W / 2.0 + (depth - 1) as f64 * ACT_STEP
        } else {
            col.x
        }
    };

    // Shaded blocks and activations close from the inside out. They are listed in reverse, so
    // that the inner ones are drawn over the outer.
    let mut rects_out: Vec<Item> = Vec::new();
    let mut activations_out: Vec<Item> = Vec::new();
    let mut frames_out: Vec<Item> = Vec::new();
    let mut edges_out: Vec<Item> = Vec::new();
    let mut destroys_out: Vec<Item> = Vec::new();
    let mut labels_out: Vec<Item> = Vec::new();
    let mut notes_out: Vec<Item> = Vec::new();
    let mut actors_out: Vec<Item> = Vec::new();
    // Closes a participant's inner activation at end, at least ACT_MIN below where it opened,
    // and gives its bottom, as the endActivation of Pele's draw does.
    let end_activation = |col: &mut Column,
                          end: f64,
                          out: &mut Vec<Item>,
                          min_x: &mut f64,
                          max_x: &mut f64|
     -> f64 {
        let start = col.activations.pop().unwrap_or(end);
        let bottom = max(end, start + ACT_MIN);
        let x = col.x - ACT_W / 2.0 + col.activations.len() as f64 * ACT_STEP;
        extend(min_x, max_x, x, x + ACT_W);
        let r = Rect::new(x, start, ACT_W, bottom - start);
        let mut it = Item::new(Kind::Mark, ROLE_ACTIVATION);
        it.id = Some(col.id.to_string());
        it.bounds = r;
        it.parts = vec![Part::rect(r, true, Painting::FillAndStroke)];
        it.style = Style {
            fill: Some(Paint::SURFACE),
            stroke: Some(Paint::BORDER),
            radius: Some(2.0),
            ..Style::default()
        };
        out.push(it);
        bottom
    };
    // The lifeline of a destroyed participant ends in a cross a little below the message that
    // ends it, with round ends, as Pele draws its crosses in a group of them. It is the
    // destroy of Pele's draw.
    let destroy = |col: &mut Column, line_y: f64, out: &mut Vec<Item>| -> f64 {
        let (cx, cy) = (col.x, line_y + 16.0);
        col.life_end = cy;
        let mut p = Path::new();
        p.move_to(cx - 6.0, cy - 6.0)
            .line_to(cx + 6.0, cy + 6.0)
            .move_to(cx + 6.0, cy - 6.0)
            .line_to(cx - 6.0, cy + 6.0);
        let mut it = Item::new(Kind::Mark, ROLE_DESTROY);
        it.id = Some(col.id.to_string());
        it.bounds = p.bounds();
        it.parts = vec![Part::path(p, Painting::StrokeOnly)];
        it.style = Style {
            stroke: Some(Paint::LINE),
            stroke_width: Some(1.5),
            cap: Some(Cap::Round),
            ..Style::default()
        };
        out.push(it);
        cy + 6.0
    };
    // A participant drawn with its box's top at top, with the classes its properties name, as
    // the actorSvg of Pele's draw draws it.
    let actor_item = |col: &Column, top: f64| -> Item {
        let (parts, at) = draw_actor(col.kind, col.x, top, col.w, col.h, &col.label);
        let mut it = Item::new(Kind::Node, ROLE_ACTOR);
        it.shape = Some(if col.kind == "participant" {
            "rect".into()
        } else {
            col.kind.to_string().into()
        });
        it.id = Some(col.id.to_string());
        it.bounds = Rect::new(col.x - col.w / 2.0, top, col.w, col.h);
        it.parts = parts;
        it.style = Style {
            fill: Some(Paint::SURFACE),
            stroke: Some(Paint::BORDER),
            text: Some(Paint::FG),
            ..Style::default()
        };
        // A figure's name sits on whatever is behind the figure: the fill of its box, where it
        // is in one.
        if is_figure(col.kind)
            && let Some(b) = col.actor.r#box
        {
            it.style.text_ground = ink_of(&db.boxes[b].fill);
        }
        if let Some(Value::String(cls)) = col.actor.properties.get("class") {
            it.class = class_names(cls);
        }
        if !col.label.is_empty() {
            it.label = Some(Placed::new(col.label.clone(), at));
        }
        it
    };
    // The number of message i in a badge of width w about cx, cy, as the badge of Pele's draw
    // draws it.
    let badge = |i: usize, cx: f64, cy: f64, w: f64| -> Item {
        let label = layout_label(
            &to_string(numbering.index[i]),
            false,
            &set,
            tiny,
            INF,
            Face::REGULAR,
        );
        let r = Rect::new(cx - w / 2.0, cy - BADGE_H / 2.0, w, BADGE_H);
        let mut it = Item::new(Kind::Node, ROLE_NUMBER);
        it.shape = Some("rounded".into());
        it.bounds = r;
        it.parts = vec![Part::rect(r, true, Painting::FillAndStroke)];
        it.label = Some(Placed::new(label, Point::new(cx, cy)));
        it.style = Style {
            fill: Some(Paint::LINE),
            stroke: Some(Paint::NONE),
            text: Some(Paint::BG),
            radius: Some(BADGE_H / 2.0),
            ..Style::default()
        };
        it
    };

    let mut y = head_bottom + 6.0;
    for (i, m) in messages.iter().enumerate() {
        let kind = m.kind;
        let col_from = m.from.as_deref().and_then(|f| col_of.get(f).copied());
        if kind == ACTIVE_START {
            if let Some(c) = col_from {
                cols[c].activations.push(y);
            }
        } else if kind == ACTIVE_END {
            if let Some(c) = col_from
                && !cols[c].activations.is_empty()
            {
                y = max(
                    y,
                    end_activation(
                        &mut cols[c],
                        y,
                        &mut activations_out,
                        &mut min_x,
                        &mut max_x,
                    ),
                );
            }
        } else if let Some(f) = frame_at[i]
            && frames[f].dividers[0] == i
        {
            stack.push(f);
            let frame = &mut frames[f];
            if frame.kind == "rect" {
                frame.top = y + 8.0;
                y = frame.top;
            } else {
                frame.top = y + 12.0;
                y = frame.top + max(TAB_H, frame.title.height + 2.0);
            }
        } else if let Some(f) = frame_at[i] {
            stack.pop();
            let bottom = y + 12.0;
            let frame = &frames[f];
            let r = Rect::new(frame.x0, frame.top, frame.x1 - frame.x0, bottom - frame.top);
            if frame.kind == "rect" {
                let written = !frame.fill.is_empty() && writes_fill(&frame.fill);
                let (own, author) = fill_style(&frame.fill, written);
                let mut it = Item::new(Kind::Group, ROLE_RECT);
                it.shape = Some("rect".into());
                it.bounds = r;
                it.parts = vec![Part::rect(r, true, Painting::FillOnly)];
                it.style = own;
                it.author = author;
                rects_out.push(it);
            } else {
                let mut it = Item::new(Kind::Group, ROLE_FRAME);
                it.shape = Some(frame.kind.into());
                it.bounds = r;
                it.parts = vec![Part::rect(r, true, Painting::StrokeOnly)];
                it.style = Style {
                    stroke: Some(Paint::BORDER),
                    ..Style::default()
                };
                frames_out.push(it);
                let condition = || {
                    let mut it = Item::new(Kind::Text, ROLE_CONDITION);
                    it.style.text = Some(Paint::MUTED);
                    it
                };
                for (k, &dy) in frame.divider_y.iter().enumerate() {
                    let label = &frame.divider_labels[k];
                    let mut p = Path::new();
                    p.move_to(frame.x0, dy).line_to(frame.x1, dy);
                    let mut it = Item::new(Kind::Mark, ROLE_DIVIDER);
                    it.bounds = p.bounds();
                    it.parts = vec![Part::path(p, Painting::StrokeOnly)];
                    it.style = Style {
                        stroke: Some(Paint::BORDER),
                        dash: Some(vec![4.0, 4.0]),
                        ..Style::default()
                    };
                    frames_out.push(it);
                    let cx = frame.x0 + 8.0 + label.width / 2.0;
                    let l = cx - label.width / 2.0;
                    let ground = ink_of(&fill_under(&stack, &frames, &cols, l, l + label.width));
                    if let Some(it) = backed(
                        condition(),
                        label,
                        cx,
                        dy + 2.0 + label.height / 2.0,
                        ground,
                    ) {
                        frames_out.push(it);
                    }
                }
                let tab = Rect::new(frame.x0, frame.top, frame.tab_w, TAB_H);
                let kind_label = layout_label(frame.kind, false, &set, tiny, INF, Face::BOLD);
                let mut it = Item::new(Kind::Text, ROLE_FRAME_KIND);
                it.shape = Some(frame.kind.into());
                it.bounds = tab;
                it.parts = vec![Part::rect(tab, true, Painting::FillAndStroke)];
                it.label = Some(Placed::new(
                    kind_label,
                    Point::new(frame.x0 + frame.tab_w / 2.0, frame.top + TAB_H / 2.0),
                ));
                it.style = Style {
                    fill: Some(Paint::SURFACE),
                    stroke: Some(Paint::BORDER),
                    text: Some(Paint::MUTED),
                    ..Style::default()
                };
                frames_out.push(it);
                let title = &frame.title;
                let cx = frame.x0 + frame.tab_w + 8.0 + title.width / 2.0;
                let l = cx - title.width / 2.0;
                let ground = ink_of(&fill_under(&stack, &frames, &cols, l, l + title.width));
                if let Some(it) = backed(
                    condition(),
                    title,
                    cx,
                    frame.top + max(TAB_H, title.height) / 2.0,
                    ground,
                ) {
                    frames_out.push(it);
                }
            }
            y = bottom;
        } else if kind == ALT_ELSE || kind == PAR_AND || kind == CRITICAL_OPTION {
            if let Some(&f) = stack.last() {
                let current = &mut frames[f];
                let height = current
                    .divider_labels
                    .get(current.divider_y.len())
                    .map_or(0.0, |l| l.height);
                current.divider_y.push(y + 12.0);
                y += 12.0 + if height > 0.0 { height } else { 0.0 };
            }
        } else if let Some(view) = views[i].as_ref().filter(|v| v.kind == V::Note) {
            let a = &cols[view.a];
            let h = view.label.height + 2.0 * max(4.0, note_pad - 4.0);
            let top = y + 10.0;
            let mut x = view.x;
            let placement = m.placement.unwrap_or(-1);
            if placement == RIGHTOF {
                x += right_edge(a, 0) - a.x;
            } else if placement == LEFTOF {
                x += left_edge(a, 0) - a.x;
            }
            extend(&mut min_x, &mut max_x, x, x + view.w);
            let r = Rect::new(x, top, view.w, h);
            let mut it = Item::new(Kind::Node, ROLE_NOTE);
            it.shape = Some("rect".into());
            it.id = Some(format!("i{}", m.id));
            it.bounds = r;
            it.parts = vec![Part::rect(r, true, Painting::FillAndStroke)];
            it.style = Style {
                fill: Some(Paint::SURFACE_ALT),
                stroke: Some(Paint::BORDER),
                text: Some(Paint::FG),
                ..Style::default()
            };
            if !view.label.is_empty() {
                it.label = Some(Placed::new(
                    view.label.clone(),
                    Point::new(x + view.w / 2.0, top + h / 2.0),
                ));
            }
            notes_out.push(it);
            y = top + h;
        } else if let Some(view) = views[i].as_ref() {
            let Some(st) = line_style(kind) else {
                continue;
            };
            let (ai, bi) = (view.a, view.b);
            let label = &view.label;
            let bw = badge_width(i);
            // The activation a `+` starts belongs to this message, so the arrow stops at its
            // edge.
            let mut starting = 0;
            for next in &messages[i + 1..] {
                let t = next.kind;
                if t == ACTIVE_START {
                    if next.from == m.to {
                        starting += 1;
                    }
                } else if t != ACTIVE_END
                    && t != CENTRAL_CONNECTION
                    && t != CENTRAL_CONNECTION_REVERSE
                {
                    break;
                }
            }
            let up = if st.bottom { 1.0 } else { -1.0 };
            let cc = m.central_connection.unwrap_or(0);
            let circle_to = cc == CENTRAL_CONNECTION || cc == CENTRAL_CONNECTION_DUAL;
            let circle_from = cc == CENTRAL_CONNECTION_REVERSE || cc == CENTRAL_CONNECTION_DUAL;
            let tail_at_end = st.at_start && !st.at_end;
            let head_trim = trim(st.head);
            // A message's line and heads end round, as Pele draws them in the group of edges,
            // which sets round caps.
            let mut msg = Item::new(Kind::Edge(Edge::default()), ROLE_MESSAGE);
            msg.id = Some(format!("i{}", m.id));
            msg.style = Style {
                stroke: Some(Paint::LINE),
                fill: Some(Paint::LINE),
                cap: Some(Cap::Round),
                ..Style::default()
            };
            if st.dotted {
                msg.style.dash = Some(vec![4.0, 4.0]);
            }
            let mut circles: Vec<Part> = Vec::new();
            let mut number: Option<Item> = None;
            let mut ends: [Option<Marker>; 2] = [None, None];
            let route: Vec<Point>;
            let mut p = Path::new();
            let line_y;

            if view.kind == V::SelfLoop {
                let a = &cols[ai];
                let top = y + msg_gap + max(0.0, (label.height - SELF_H) / 2.0);
                let bottom = top + SELF_H;
                let mut x0 = if circle_from {
                    a.x + CENTRAL_R
                } else {
                    right_edge(a, 0)
                };
                let mut x1 = if circle_to {
                    a.x + CENTRAL_R
                } else {
                    right_edge(a, starting)
                };
                let out = max(x0, x1) + SELF_W + bw / 2.0;
                msg.style.ground = ink_of(&fill_under(&stack, &frames, &cols, a.x, out));
                if circle_from {
                    circles.push(circle(a.x, top, CENTRAL_R, Painting::Dot));
                }
                if circle_to {
                    circles.push(circle(a.x, bottom, CENTRAL_R, Painting::Dot));
                }
                if bw > 0.0 {
                    if tail_at_end {
                        number = Some(badge(i, x1, bottom, bw));
                        x1 += bw / 2.0;
                    } else {
                        number = Some(badge(i, x0, top, bw));
                        x0 += bw / 2.0;
                    }
                }
                // The loop leaves to the right and comes back heading left, which turns a half
                // head over.
                if st.at_start {
                    ends[0] = head(st.head, x0, top, -1.0, up);
                }
                if st.at_end {
                    ends[1] = head(st.head, x1, bottom, -1.0, -up);
                }
                let from = x0 + if st.at_start { head_trim } else { 0.0 };
                let to = x1 + if st.at_end { head_trim } else { 0.0 };
                p.move_to(from, top);
                if right_angles {
                    p.line_to(out, top).line_to(out, bottom).line_to(to, bottom);
                } else {
                    p.line_to(out - 6.0, top)
                        .arc_to(6.0, 6.0, 0.0, false, true, out, top + 6.0)
                        .line_to(out, bottom - 6.0)
                        .arc_to(6.0, 6.0, 0.0, false, true, out - 6.0, bottom)
                        .line_to(to, bottom);
                }
                route = vec![
                    Point::new(x0, top),
                    Point::new(out, top),
                    Point::new(out, bottom),
                    Point::new(x1, bottom),
                ];
                extend(&mut min_x, &mut max_x, a.x, out + 8.0 + label.width);
                if label.width > 0.0 {
                    let mut it = Item::new(Kind::Text, ROLE_MESSAGE_LABEL);
                    it.id = Some(format!("i{}", m.id));
                    it.bounds = Rect::new(
                        out + 8.0,
                        (top + bottom) / 2.0 - label.height / 2.0,
                        label.width,
                        label.height,
                    );
                    it.label = Some(Placed::new(
                        label.clone(),
                        Point::new(out + 8.0 + label.width / 2.0, (top + bottom) / 2.0),
                    ));
                    it.style = Style {
                        text: Some(Paint::FG),
                        ground: ink_of(&fill_under(
                            &stack,
                            &frames,
                            &cols,
                            out + 8.0,
                            out + 8.0 + label.width,
                        )),
                        ..Style::default()
                    };
                    labels_out.push(it);
                }
                // A label taller than the loop reaches below it, so the next row starts under
                // the label.
                line_y = max(bottom, (top + bottom) / 2.0 + label.height / 2.0);
            } else {
                let (a, b) = (&cols[ai], &cols[bi]);
                let dir = if b.x > a.x { 1.0 } else { -1.0 };
                let creating = b.create_at == Some(i);
                msg.style.ground = ink_of(&fill_under(
                    &stack,
                    &frames,
                    &cols,
                    min(a.x, b.x),
                    max(a.x, b.x),
                ));
                line_y = y
                    + msg_gap
                    + max(
                        if label.height > 0.0 {
                            label.height + 2.0
                        } else {
                            6.0
                        },
                        if creating {
                            b.h / 2.0 - msg_gap + 6.0
                        } else {
                            0.0
                        },
                    );
                let mut x0 = if dir > 0.0 {
                    right_edge(a, 0)
                } else {
                    left_edge(a, 0)
                };
                let mut x1 = if dir > 0.0 {
                    left_edge(b, starting)
                } else {
                    right_edge(b, starting)
                };
                let (a_x, b_x, b_w, b_h, b_figure) = (a.x, b.x, b.w, b.h, is_figure(b.kind));
                if creating {
                    let b = &mut cols[bi];
                    b.top = line_y - b_h / 2.0;
                    b.life_start = b.top + b_h;
                    x1 = b_x - dir * if b_figure { 18.0 } else { b_w / 2.0 + 1.0 };
                }
                if circle_from {
                    circles.push(circle(a_x, line_y, CENTRAL_R, Painting::Dot));
                    x0 = a_x + dir * CENTRAL_R;
                }
                if circle_to {
                    circles.push(circle(b_x, line_y, CENTRAL_R, Painting::Dot));
                    x1 = b_x - dir * CENTRAL_R;
                }
                if bw > 0.0 {
                    // The number sits on the lifeline at the tail of the arrow, past a
                    // connection circle if there is one.
                    if tail_at_end {
                        let cx = if circle_to {
                            x1 - dir * (bw / 2.0 + 2.0)
                        } else {
                            x1
                        };
                        number = Some(badge(i, cx, line_y, bw));
                        x1 = cx - dir * bw / 2.0;
                    } else {
                        let cx = if circle_from {
                            x0 + dir * (bw / 2.0 + 2.0)
                        } else {
                            x0
                        };
                        number = Some(badge(i, cx, line_y, bw));
                        x0 = cx + dir * bw / 2.0;
                    }
                }
                if st.at_start {
                    ends[0] = head(st.head, x0, line_y, -dir, up * dir);
                }
                if st.at_end {
                    ends[1] = head(st.head, x1, line_y, dir, up * dir);
                }
                let from = x0 + if st.at_start { dir * head_trim } else { 0.0 };
                let to = x1 - if st.at_end { dir * head_trim } else { 0.0 };
                p.move_to(from, line_y).line_to(to, line_y);
                route = vec![Point::new(x0, line_y), Point::new(x1, line_y)];
                if label.width > 0.0 {
                    let (l, r) = (min(x0, x1), max(x0, x1));
                    let cx = if align < 0 {
                        l + msg_pad + label.width / 2.0
                    } else if align > 0 {
                        r - msg_pad - label.width / 2.0
                    } else {
                        (l + r) / 2.0
                    };
                    extend(
                        &mut min_x,
                        &mut max_x,
                        cx - label.width / 2.0,
                        cx + label.width / 2.0,
                    );
                    let mut it = Item::new(Kind::Text, ROLE_MESSAGE_LABEL);
                    it.id = Some(format!("i{}", m.id));
                    it.style.text = Some(Paint::FG);
                    let lx = cx - label.width / 2.0;
                    let ground = ink_of(&fill_under(&stack, &frames, &cols, lx, lx + label.width));
                    if let Some(it) =
                        backed(it, label, cx, line_y - 4.0 - label.height / 2.0, ground)
                    {
                        labels_out.push(it);
                    }
                }
                if creating {
                    y = line_y + b_h / 2.0 - 6.0;
                }
            }
            msg.bounds = route_bounds(&route);
            msg.kind = Kind::Edge(Edge { route, ends });
            msg.parts = vec![Part::path(p, Painting::StrokeOnly)];
            msg.parts.extend(circles);
            edges_out.push(msg);
            if let Some(number) = number {
                edges_out.push(number);
            }
            y = max(y, line_y);
            if cols[ai].destroy_at == Some(i) {
                y = max(y, destroy(&mut cols[ai], line_y, &mut destroys_out));
            }
            if cols[bi].destroy_at == Some(i) && bi != ai {
                y = max(y, destroy(&mut cols[bi], line_y, &mut destroys_out));
            }
        }
    }

    y += 20.0;
    for col in &mut cols {
        while !col.activations.is_empty() {
            end_activation(col, y, &mut activations_out, &mut min_x, &mut max_x);
        }
    }
    let mut bottom = y;
    if mirror {
        let mut foot_h = 0.0;
        for col in &cols {
            if col.life_end >= 0.0 {
                continue;
            }
            foot_h = max(foot_h, col.h);
            actors_out.push(actor_item(col, y));
        }
        bottom = y + foot_h;
    }
    let mut lifelines_out: Vec<Item> = Vec::new();
    for col in &cols {
        actors_out.push(actor_item(col, col.top));
        let end = if col.life_end >= 0.0 { col.life_end } else { y };
        if end > col.life_start {
            let mut p = Path::new();
            p.move_to(col.x, col.life_start).line_to(col.x, end);
            let mut it = Item::new(Kind::Mark, ROLE_LIFELINE);
            it.id = Some(col.id.to_string());
            it.bounds = p.bounds();
            it.parts = vec![Part::path(p, Painting::StrokeOnly)];
            it.style.stroke = Some(Paint::BORDER);
            lifelines_out.push(it);
        }
    }

    let mut boxes_out: Vec<Item> = Vec::new();
    for run in &runs {
        let (x0, x1) = box_edges(&cols, run);
        let bx = &db.boxes[run.r#box];
        let written = bx.fill != "transparent" && writes_fill(&bx.fill);
        let (mut own, author) = fill_style(&bx.fill, written);
        // The title is set on the box's fill, in the color that stands out on a fill the
        // author gives it, as Pele's inkOn picks it.
        own.stroke = Some(Paint::BORDER);
        own.text = Some(Paint::MUTED);
        own.on_fill = true;
        let r = Rect::new(x0, 0.0, x1 - x0, bottom + box_pad);
        let mut it = Item::new(Kind::Group, ROLE_BOX);
        it.shape = Some("rect".into());
        it.id = bx.name.clone();
        it.bounds = r;
        it.parts = vec![Part::rect(r, true, Painting::FillAndStroke)];
        it.style = own;
        it.author = author;
        if !run.title.is_empty() {
            it.label = Some(Placed::new(
                run.title.clone(),
                Point::new(
                    (x0 + x1) / 2.0,
                    box_pad / 2.0 + 2.0 + run.title.height / 2.0,
                ),
            ));
        }
        boxes_out.push(it);
    }
    if !runs.is_empty() {
        bottom += box_pad;
    }

    let mut min_y = 0.0;
    let mut titles_out: Vec<Item> = Vec::new();
    let room = title_room(o);
    let plain_title = title_label(
        db.title.as_deref(),
        &set,
        size,
        if tight { room } else { INF },
    );
    let title = if tight {
        break_label(plain_title, &set, max(120.0, room))
    } else {
        plain_title
    };
    if min_x > max_x {
        extend(&mut min_x, &mut max_x, 0.0, max(title.width, 40.0));
    }
    if title.width > 0.0 {
        let cx = (min_x + max_x) / 2.0;
        extend(
            &mut min_x,
            &mut max_x,
            cx - title.width / 2.0,
            cx + title.width / 2.0,
        );
        min_y = -title.height - 12.0;
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        it.bounds = Rect::new(cx - title.width / 2.0, min_y, title.width, title.height);
        it.label = Some(Placed::new(
            title.clone(),
            Point::new(cx, min_y + title.height / 2.0),
        ));
        it.style.text = Some(Paint::FG);
        titles_out.push(it);
    }

    let width = (max_x - min_x + 2.0 * pad).ceil();
    // Retry with equal shares or room for activation notes and rounding.
    if grown && width > o.max_width {
        let hold = if uneven {
            hold
        } else if hold > 0.0 {
            INF
        } else {
            width - o.max_width.floor()
        };
        return draw(db, config, o, tight, hold, false);
    }
    let height = (bottom - min_y + 2.0 * pad).ceil();
    let mut sc = Scene {
        diagram: "sequence".into(),
        width,
        height,
        font_size: size,
        title: db.acc_title.clone(),
        desc: db.acc_descr.clone(),
        items: Vec::new(),
    };
    rects_out.reverse();
    activations_out.reverse();
    for layer in [
        titles_out,
        boxes_out,
        rects_out,
        lifelines_out,
        activations_out,
        frames_out,
        edges_out,
        destroys_out,
        labels_out,
        notes_out,
        actors_out,
    ] {
        sc.items.extend(layer);
    }
    // Pele draws all of it in a group moved by the padding past the extents. The scene holds
    // where it is on the page.
    let (dx, dy) = (pad - min_x, pad - min_y);
    for it in &mut sc.items {
        it.move_by(dx, dy);
    }
    sc
}
