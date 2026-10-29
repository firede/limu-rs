//! What a sequence diagram's parser fills, as Pele's SeqDb in db.ts has it: its participants,
//! the boxes about them, and its messages, notes, activations and blocks in the order they
//! come, as Mermaid's sequenceDb keeps them.

use std::collections::HashMap;

use super::colors::is_css_color;
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::{
    drop_indents, is_line_end, is_space, is_word, len16, trim, trim_start, unit_at,
};
use crate::js::json::parse_json;
use crate::value::Value;
use crate::yaml::parse_yaml;

// The kinds of message, as Mermaid numbers them, Pele's LINETYPE.
pub(crate) const SOLID: i32 = 0;
pub(crate) const DOTTED: i32 = 1;
pub(crate) const NOTE: i32 = 2;
pub(crate) const SOLID_CROSS: i32 = 3;
pub(crate) const DOTTED_CROSS: i32 = 4;
pub(crate) const SOLID_OPEN: i32 = 5;
pub(crate) const DOTTED_OPEN: i32 = 6;
pub(crate) const LOOP_START: i32 = 10;
pub(crate) const LOOP_END: i32 = 11;
pub(crate) const ALT_START: i32 = 12;
pub(crate) const ALT_ELSE: i32 = 13;
pub(crate) const ALT_END: i32 = 14;
pub(crate) const OPT_START: i32 = 15;
pub(crate) const OPT_END: i32 = 16;
pub(crate) const ACTIVE_START: i32 = 17;
pub(crate) const ACTIVE_END: i32 = 18;
pub(crate) const PAR_START: i32 = 19;
pub(crate) const PAR_AND: i32 = 20;
pub(crate) const PAR_END: i32 = 21;
pub(crate) const RECT_START: i32 = 22;
pub(crate) const RECT_END: i32 = 23;
pub(crate) const SOLID_POINT: i32 = 24;
pub(crate) const DOTTED_POINT: i32 = 25;
pub(crate) const AUTONUMBER: i32 = 26;
pub(crate) const CRITICAL_START: i32 = 27;
pub(crate) const CRITICAL_OPTION: i32 = 28;
pub(crate) const CRITICAL_END: i32 = 29;
pub(crate) const BREAK_START: i32 = 30;
pub(crate) const BREAK_END: i32 = 31;
pub(crate) const PAR_OVER_START: i32 = 32;
pub(crate) const BIDIRECTIONAL_SOLID: i32 = 33;
pub(crate) const BIDIRECTIONAL_DOTTED: i32 = 34;
pub(crate) const SOLID_TOP: i32 = 41;
pub(crate) const SOLID_BOTTOM: i32 = 42;
pub(crate) const STICK_TOP: i32 = 43;
pub(crate) const STICK_BOTTOM: i32 = 44;
pub(crate) const SOLID_ARROW_TOP_REVERSE: i32 = 45;
pub(crate) const SOLID_ARROW_BOTTOM_REVERSE: i32 = 46;
pub(crate) const STICK_ARROW_TOP_REVERSE: i32 = 47;
pub(crate) const STICK_ARROW_BOTTOM_REVERSE: i32 = 48;
pub(crate) const SOLID_TOP_DOTTED: i32 = 51;
pub(crate) const SOLID_BOTTOM_DOTTED: i32 = 52;
pub(crate) const STICK_TOP_DOTTED: i32 = 53;
pub(crate) const STICK_BOTTOM_DOTTED: i32 = 54;
pub(crate) const SOLID_ARROW_TOP_REVERSE_DOTTED: i32 = 55;
pub(crate) const SOLID_ARROW_BOTTOM_REVERSE_DOTTED: i32 = 56;
pub(crate) const STICK_ARROW_TOP_REVERSE_DOTTED: i32 = 57;
pub(crate) const STICK_ARROW_BOTTOM_REVERSE_DOTTED: i32 = 58;
pub(crate) const CENTRAL_CONNECTION: i32 = 59;
pub(crate) const CENTRAL_CONNECTION_REVERSE: i32 = 60;
pub(crate) const CENTRAL_CONNECTION_DUAL: i32 = 61;

// Where a note is, Pele's PLACEMENT.
pub(crate) const LEFTOF: i32 = 0;
pub(crate) const RIGHTOF: i32 = 1;
pub(crate) const OVER: i32 = 2;

/// A text, and whether it is to wrap where it says, Pele's SeqText.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SeqText {
    pub(crate) text: String,
    pub(crate) wrap: Option<bool>,
}

/// What a box statement's line says, Pele's SeqBoxData.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SeqBoxData {
    pub(crate) text: Option<String>,
    pub(crate) color: String,
    pub(crate) wrap: Option<bool>,
}

/// A box drawn about participants, Pele's SeqBox.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SeqBox {
    pub(crate) name: Option<String>,
    pub(crate) wrap: bool,
    pub(crate) fill: String,
    pub(crate) actor_keys: Vec<String>,
}

/// The keys and values of a JavaScript Map, in the order the keys were first set, as the
/// links and properties of Pele's SeqActor keep them.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Entries(pub(crate) Vec<(String, Value)>);

impl Entries {
    /// Map.set: a key set again keeps its place.
    fn set(&mut self, key: String, value: Value) {
        match self.0.iter_mut().find(|(k, _)| *k == key) {
            Some(entry) => entry.1 = value,
            None => self.0.push((key, value)),
        }
    }

    /// Map.get.
    pub(crate) fn get(&self, key: &str) -> Option<&Value> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

/// A participant, Pele's SeqActor. Its box is its place among the diagram's boxes.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SeqActor {
    pub(crate) id: String,
    pub(crate) r#box: Option<usize>,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) wrap: bool,
    pub(crate) prev_actor: Option<String>,
    pub(crate) next_actor: Option<String>,
    pub(crate) links: Entries,
    pub(crate) properties: Entries,
    /// The id of the page element named by a `details` statement. Pele has no page to read it
    /// from.
    pub(crate) details: Option<String>,
    pub(crate) kind: String,
}

/// What an autonumber statement says, Pele's SeqNumbering.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct SeqNumbering {
    pub(crate) start: Option<f64>,
    pub(crate) step: Option<f64>,
    pub(crate) visible: bool,
}

/// What a message holds: its text, or what an autonumber statement says.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MessageText {
    Text(String),
    Numbering(SeqNumbering),
}

/// An entry of the diagram's list of messages, Pele's SeqMessage: a message between
/// participants, a note, the start or end of an activation or a block, or an autonumber
/// statement, by its kind.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SeqMessage {
    pub(crate) id: String,
    pub(crate) from: Option<String>,
    pub(crate) to: Option<String>,
    pub(crate) message: MessageText,
    pub(crate) wrap: bool,
    pub(crate) kind: i32,
    pub(crate) activate: Option<bool>,
    pub(crate) placement: Option<i32>,
    pub(crate) central_connection: Option<i32>,
}

impl SeqMessage {
    /// The message's text, where it is one, as `typeof m.message === 'string'` tells.
    pub(crate) fn text(&self) -> Option<&str> {
        match &self.message {
            MessageText::Text(t) => Some(t),
            MessageText::Numbering(_) => None,
        }
    }
}

/// Whom a note is on: one participant, or the two of a note over them, Pele's
/// `string | string[]`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NoteActor {
    One(String),
    Pair(String, String),
}

/// A note, Pele's SeqNote.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SeqNote {
    pub(crate) actor: NoteActor,
    pub(crate) placement: i32,
    pub(crate) message: String,
    pub(crate) wrap: bool,
}

/// A statement the parser hands to apply, Pele's SeqStatement. Of its texts, text holds
/// whichever the statement's type has: a message's, a note's, a menu's, or a block's.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SeqStatement {
    pub(crate) kind: &'static str,
    pub(crate) actor: String,
    pub(crate) note_actor: Option<NoteActor>,
    pub(crate) draw: Option<&'static str>,
    pub(crate) description: Option<SeqText>,
    pub(crate) config: Option<String>,
    pub(crate) signal_type: i32,
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) text: Option<SeqText>,
    pub(crate) activate: bool,
    pub(crate) central_connection: i32,
    pub(crate) placement: i32,
    pub(crate) box_data: SeqBoxData,
    pub(crate) sequence_index: Option<f64>,
    pub(crate) sequence_index_step: Option<f64>,
    pub(crate) sequence_visible: bool,
}

/// An entry of the statement list, as Mermaid's grammar builds it, Pele's SeqItem.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SeqItem {
    Statement(Box<SeqStatement>),
    Text(String),
    List(Vec<SeqItem>),
}

/// Drops nested statement lists without recursion, so that a list nested to any depth is
/// dropped with no more stack than a shallow one.
pub(crate) fn release(items: Vec<SeqItem>) {
    let mut stack = vec![items];
    while let Some(mut list) = stack.pop() {
        for item in list.drain(..) {
            if let SeqItem::List(inner) = item {
                stack.push(inner);
            }
        }
    }
}

/// A sequence diagram as its parser fills it, Pele's SeqDb.
#[derive(Clone, Debug, Default)]
pub(crate) struct SeqDb {
    /// The participants, in the order a JavaScript Map keeps them.
    pub(crate) actors: Vec<SeqActor>,
    actor_index: HashMap<String, usize>,
    /// The index of the message each participant is created and destroyed at, in the order a
    /// JavaScript Map keeps them.
    pub(crate) created_actors: Vec<(String, usize)>,
    pub(crate) destroyed_actors: Vec<(String, usize)>,
    pub(crate) boxes: Vec<SeqBox>,
    pub(crate) messages: Vec<SeqMessage>,
    pub(crate) notes: Vec<SeqNote>,
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    prev_actor: Option<String>,
    current_box: Option<usize>,
    last_created: Option<String>,
    last_destroyed: Option<String>,
    wrap_enabled: Option<bool>,
    sequence_wrap: bool,
    active: HashMap<String, i64>,
}

/// A semantic error of a sequence diagram, as Pele's fail throws it.
fn fail(message: impl Into<String>) -> Error {
    let mut e = Error::new(message, ErrorCode::Semantic);
    e.diagram = Some(DiagramType::Sequence);
    e
}

/// A text with a leading wrap: or nowrap: taken off, and whether it says to wrap, as Pele's
/// extractWrap reads it with /^:?wrap:/, /^:?nowrap:/ and /^:?(?:no)?wrap:/.
fn extract_wrap(raw: &str) -> SeqText {
    let text = trim(raw);
    let colon = usize::from(text.starts_with(':'));
    let (cut, wrap) = if text[colon..].starts_with("wrap:") {
        (colon + 5, Some(true))
    } else if text[colon..].starts_with("nowrap:") {
        (colon + 7, Some(false))
    } else {
        (0, None)
    };
    SeqText {
        text: trim(&text[cut..]).to_string(),
        wrap,
    }
}

/// The length of what /^(?:rgba?|hsla?)\s*\(/ matches at the start of s, Pele's
/// RE_BOX_FUNCTION.
fn box_function(s: &str) -> Option<usize> {
    if !s.starts_with("rgb") && !s.starts_with("hsl") {
        return None;
    }
    let mut i = 3;
    if s.as_bytes().get(i) == Some(&b'a') {
        i += 1;
    }
    loop {
        let (c, size) = unit_at(s, i);
        if !is_space(c) {
            break;
        }
        i += size;
    }
    (s.as_bytes().get(i) == Some(&b'(')).then_some(i + 1)
}

/// Whether s holds a line terminator other than a line feed, Pele's RE_LINE_TERMINATOR.
fn has_line_terminator(s: &str) -> bool {
    s.chars().any(|c| c != '\n' && is_line_end(c as i32))
}

/// Splits a box line into a leading color and the rest, as Mermaid's
/// /^((?:rgba?|hsla?)\s*\(.*\)|\w*)(.*)$/ does, as Pele's splitBox does. `.` stops at a line
/// terminator, so a line that holds one outside the gap before the parenthesis does not match
/// at all.
fn split_box(s: &str) -> Option<(&str, &str)> {
    if let Some(open) = box_function(s)
        && let Some(close) = s.rfind(')')
        && close >= open
    {
        if has_line_terminator(&s[open..]) {
            return None;
        }
        return Some((&s[..close + 1], &s[close + 1..]));
    }
    if has_line_terminator(s) {
        return None;
    }
    let word = s
        .bytes()
        .position(|c| !is_word(i32::from(c)))
        .unwrap_or(s.len());
    Some((&s[..word], &s[word..]))
}

/// s.slice(from, to) in code units, with JavaScript's negative ends counted from the end. A
/// cut through a surrogate pair leaves U+FFFD, which a Rust string can hold, for the half
/// JavaScript would keep.
fn slice16(s: &str, from: isize, to: isize) -> String {
    let units: Vec<u16> = s.encode_utf16().collect();
    let n = units.len() as isize;
    let end = |i: isize| if i < 0 { (n + i).max(0) } else { i.min(n) } as usize;
    let (from, to) = (end(from), end(to));
    if from >= to {
        return String::new();
    }
    String::from_utf16_lossy(&units[from..to])
}

/// The entries of a value JSON.parse gave, as Pele's entries gives them: a string's
/// characters by index, an object's or an array's entries in JavaScript's order, or nothing.
fn entries(value: Value) -> Vec<(String, Value)> {
    match value {
        Value::String(s) => s
            .chars()
            .enumerate()
            .map(|(i, c)| (i.to_string(), Value::String(c.to_string())))
            .collect(),
        Value::Array(items) => items
            .into_iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v))
            .collect(),
        Value::Object(m) => m.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
        _ => Vec::new(),
    }
}

/// A box's name as a template string writes it, "undefined" for none.
fn box_name(b: &SeqBox) -> &str {
    b.name.as_deref().unwrap_or("undefined")
}

impl SeqDb {
    /// An empty diagram whose text wraps as the top-level wrap setting and sequence.wrap say,
    /// as Pele's SeqDb constructor takes its options.
    pub(crate) fn new(wrap: Option<bool>, sequence_wrap: bool) -> SeqDb {
        SeqDb {
            wrap_enabled: wrap,
            sequence_wrap,
            ..SeqDb::default()
        }
    }

    fn actor_mut(&mut self, id: &str) -> Option<&mut SeqActor> {
        self.actor_index.get(id).map(|&i| &mut self.actors[i])
    }

    /// Whether text wraps where its statement does not say, as Pele's autoWrap gives it.
    pub(crate) fn auto_wrap(&self) -> bool {
        self.wrap_enabled.unwrap_or(self.sequence_wrap)
    }

    pub(crate) fn set_acc_title(&mut self, text: &str) {
        self.acc_title = Some(trim_start(text).to_string());
    }

    pub(crate) fn set_acc_description(&mut self, text: &str) {
        self.acc_descr = Some(drop_indents(text));
    }

    pub(crate) fn set_diagram_title(&mut self, text: &str) {
        self.title = Some(text.to_string());
    }

    pub(crate) fn parse_message(&self, s: &str) -> SeqText {
        extract_wrap(s)
    }

    pub(crate) fn parse_box_data(&self, s: &str) -> SeqBoxData {
        let found = split_box(s);
        let mut color = match found {
            Some((first, _)) if !first.is_empty() => trim(first).to_string(),
            _ => "transparent".to_string(),
        };
        let mut title = match found {
            Some((_, rest)) if !rest.is_empty() => Some(trim(rest).to_string()),
            _ => None,
        };
        if !is_css_color(&color) {
            color = "transparent".into();
            title = Some(trim(s).to_string());
        }
        let Some(title) = title else {
            return SeqBoxData {
                text: None,
                color,
                wrap: None,
            };
        };
        let SeqText { text, wrap } = extract_wrap(&title);
        SeqBoxData {
            text: (!text.is_empty()).then_some(text),
            color,
            wrap,
        }
    }

    pub(crate) fn add_box(&mut self, data: &SeqBoxData) {
        self.boxes.push(SeqBox {
            name: data.text.clone(),
            wrap: data.wrap.unwrap_or_else(|| self.auto_wrap()),
            fill: data.color.clone(),
            actor_keys: Vec::new(),
        });
        self.current_box = Some(self.boxes.len() - 1);
    }

    pub(crate) fn add_actor(
        &mut self,
        id: &str,
        name: &str,
        description: Option<SeqText>,
        kind: Option<&str>,
        metadata: Option<&str>,
    ) -> Result<(), Error> {
        let mut description = description;
        let mut kind = kind.map(str::to_string);
        let mut assigned_box = self.current_box;
        let mut doc = None;
        if let Some(metadata) = metadata {
            let src = if metadata.contains('\n') {
                format!("{metadata}\n")
            } else {
                format!("{{\n{metadata}\n}}")
            };
            let parsed = parse_yaml(&src).map_err(|e| {
                let mut err = Error::new(
                    format!("Config of participant \"{id}\" is not valid YAML. {}", e.0),
                    ErrorCode::Syntax,
                );
                err.diagram = Some(DiagramType::Sequence);
                err
            })?;
            if let Value::Object(m) = parsed {
                doc = Some(m);
            }
        }
        let doc_type = doc.as_ref().and_then(|d| d.get("type"));
        let doc_alias = doc.as_ref().and_then(|d| d.get("alias"));
        if let Some(t) = doc_type
            && !t.is_null()
        {
            kind = Some(t.to_js_string());
        }
        if let Some(alias) = doc_alias
            && alias.truthy()
            && description.as_ref().is_none_or(|d| d.text == name)
        {
            description = Some(SeqText {
                text: alias.to_js_string(),
                wrap: description.and_then(|d| d.wrap),
            });
        }

        if let Some(&old) = self.actor_index.get(id) {
            let old_box = self.actors[old].r#box;
            if let (Some(current), Some(was)) = (self.current_box, old_box)
                && current != was
            {
                return Err(fail(format!(
                    "A same participant should only be defined in one Box: {} can't be in '{}' and in '{}' at the same time.",
                    self.actors[old].name,
                    box_name(&self.boxes[was]),
                    box_name(&self.boxes[current])
                )));
            }
            assigned_box = old_box.or(self.current_box);
            self.actors[old].r#box = assigned_box;
            if name == self.actors[old].name && description.is_none() {
                return Ok(());
            }
        }
        // A participant that is only mentioned in a message has no kind and takes its name
        // as its label.
        if description.is_none() || kind.is_none() {
            description = Some(SeqText {
                text: name.to_string(),
                wrap: None,
            });
        }
        let description = description.unwrap_or_default();
        let actor = SeqActor {
            id: id.to_string(),
            r#box: assigned_box,
            name: name.to_string(),
            description: description.text,
            wrap: description.wrap.unwrap_or_else(|| self.auto_wrap()),
            prev_actor: self.prev_actor.clone(),
            next_actor: None,
            links: Entries::default(),
            properties: Entries::default(),
            details: None,
            kind: kind.unwrap_or_else(|| "participant".into()),
        };
        match self.actor_index.get(id) {
            // Map.set keeps the key where it was.
            Some(&i) => self.actors[i] = actor,
            None => {
                self.actor_index.insert(id.to_string(), self.actors.len());
                self.actors.push(actor);
            }
        }
        if let Some(prev) = self.prev_actor.clone()
            && let Some(prev) = self.actor_mut(&prev)
        {
            prev.next_actor = Some(id.to_string());
        }
        if let Some(b) = self.current_box {
            self.boxes[b].actor_keys.push(id.to_string());
        }
        self.prev_actor = Some(id.to_string());
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn add_signal(
        &mut self,
        from: Option<&str>,
        to: Option<&str>,
        message: Option<&SeqText>,
        kind: i32,
        activate: bool,
        central_connection: i32,
    ) -> Result<(), Error> {
        // Mermaid recounts every earlier message here. A running count per participant says
        // the same.
        if kind == ACTIVE_END {
            let count = from.map_or(0, |f| self.active.get(f).copied().unwrap_or(0));
            if count < 1 {
                return Err(fail(format!(
                    "Trying to inactivate an inactive participant ({})",
                    from.unwrap_or("undefined")
                )));
            }
            if let Some(f) = from {
                self.active.insert(f.to_string(), count - 1);
            }
        } else if kind == ACTIVE_START
            && let Some(f) = from
        {
            *self.active.entry(f.to_string()).or_insert(0) += 1;
        }
        self.messages.push(SeqMessage {
            id: self.messages.len().to_string(),
            from: from.map(str::to_string),
            to: to.map(str::to_string),
            message: MessageText::Text(message.map_or(String::new(), |m| m.text.clone())),
            wrap: message
                .and_then(|m| m.wrap)
                .unwrap_or_else(|| self.auto_wrap()),
            kind,
            activate: Some(activate),
            placement: None,
            central_connection: Some(central_connection),
        });
        Ok(())
    }

    pub(crate) fn add_note(&mut self, actor: &NoteActor, placement: i32, message: &SeqText) {
        let wrap = message.wrap.unwrap_or_else(|| self.auto_wrap());
        self.notes.push(SeqNote {
            actor: actor.clone(),
            placement,
            message: message.text.clone(),
            wrap,
        });
        let (from, to) = match actor {
            NoteActor::One(a) => (a, a),
            NoteActor::Pair(a, b) => (a, b),
        };
        self.messages.push(SeqMessage {
            id: self.messages.len().to_string(),
            from: Some(from.clone()),
            to: Some(to.clone()),
            message: MessageText::Text(message.text.clone()),
            wrap,
            kind: NOTE,
            activate: None,
            placement: Some(placement),
            central_connection: None,
        });
    }

    /// Mermaid logs and ignores menu text that is not valid JSON. It also undoes the `&amp;`
    /// and `&equals;` its sanitizer writes. Pele has no sanitizer, and a `;` cannot occur in
    /// the text.
    pub(crate) fn add_links(&mut self, actor_id: &str, text: &SeqText) {
        let Some(actor) = self.actor_mut(actor_id) else {
            return;
        };
        if let Some(parsed) = parse_json(&text.text) {
            for (k, v) in entries(parsed) {
                actor.links.set(k, v);
            }
        }
    }

    pub(crate) fn add_a_link(&mut self, actor_id: &str, text: &SeqText) {
        let Some(actor) = self.actor_mut(actor_id) else {
            return;
        };
        let s = &text.text;
        let sep = s.find('@').map_or(-1, |i| len16(&s[..i]) as isize);
        let key = slice16(s, 0, sep - 1);
        let value = slice16(s, sep + 1, len16(s) as isize);
        actor.links.set(
            trim(&key).to_string(),
            Value::String(trim(&value).to_string()),
        );
    }

    pub(crate) fn add_properties(&mut self, actor_id: &str, text: &SeqText) {
        let Some(actor) = self.actor_mut(actor_id) else {
            return;
        };
        if let Some(parsed) = parse_json(&text.text) {
            for (k, v) in entries(parsed) {
                actor.properties.set(k, v);
            }
        }
    }

    /// Mermaid reads the properties and links from a page element with this id. Pele keeps
    /// the id.
    pub(crate) fn add_details(&mut self, actor_id: &str, text: &SeqText) {
        if let Some(actor) = self.actor_mut(actor_id) {
            actor.details = Some(text.text.clone());
        }
    }

    /// Applies the statements of nested lists in order. Mermaid walks them by recursion. An
    /// explicit stack takes any depth, as Pele's apply does.
    pub(crate) fn apply<'r>(&mut self, root: &'r [SeqItem]) -> Result<(), Error> {
        let mut lists: Vec<(&'r [SeqItem], usize)> = vec![(root, 0)];
        while let Some(top) = lists.last_mut() {
            let (list, at) = *top;
            let Some(item) = list.get(at) else {
                lists.pop();
                continue;
            };
            top.1 += 1;
            match item {
                SeqItem::List(inner) => lists.push((inner, 0)),
                SeqItem::Statement(s) => self.apply_one(s)?,
                SeqItem::Text(_) => {}
            }
        }
        Ok(())
    }

    fn apply_one(&mut self, param: &SeqStatement) -> Result<(), Error> {
        let actor = param.actor.as_str();
        let text = param.text.as_ref();
        match param.kind {
            "sequenceIndex" => self.messages.push(SeqMessage {
                id: self.messages.len().to_string(),
                from: None,
                to: None,
                message: MessageText::Numbering(SeqNumbering {
                    start: param.sequence_index,
                    step: param.sequence_index_step,
                    visible: param.sequence_visible,
                }),
                wrap: false,
                kind: param.signal_type,
                activate: None,
                placement: None,
                central_connection: None,
            }),
            "addParticipant" => self.add_actor(
                actor,
                actor,
                param.description.clone(),
                param.draw,
                param.config.as_deref(),
            )?,
            "createParticipant" => {
                if self.actor_index.contains_key(actor) {
                    return Err(fail(
                        "It is not possible to have actors with the same id, even if one is destroyed before the next is created. Use 'AS' aliases to simulate the behavior",
                    ));
                }
                self.last_created = Some(actor.to_string());
                self.add_actor(
                    actor,
                    actor,
                    param.description.clone(),
                    param.draw,
                    param.config.as_deref(),
                )?;
                set_position(&mut self.created_actors, actor, self.messages.len());
            }
            "destroyParticipant" => {
                self.last_destroyed = Some(actor.to_string());
                set_position(&mut self.destroyed_actors, actor, self.messages.len());
            }
            "activeStart" | "activeEnd" | "centralConnection" | "centralConnectionReverse" => {
                self.add_signal(Some(actor), None, None, param.signal_type, false, 0)?;
            }
            "addNote" => {
                if let (Some(who), Some(text)) = (&param.note_actor, text) {
                    self.add_note(who, param.placement, text);
                }
            }
            "addLinks" => self.add_links(actor, text.unwrap_or(&SeqText::default())),
            "addALink" => self.add_a_link(actor, text.unwrap_or(&SeqText::default())),
            "addProperties" => self.add_properties(actor, text.unwrap_or(&SeqText::default())),
            "addDetails" => self.add_details(actor, text.unwrap_or(&SeqText::default())),
            "addMessage" => {
                if let Some(created) = &self.last_created {
                    if param.to != *created {
                        return Err(fail(format!(
                            "The created participant {created} does not have an associated creating message after its declaration. Please check the sequence diagram."
                        )));
                    }
                    self.last_created = None;
                } else if let Some(destroyed) = &self.last_destroyed {
                    if param.to != *destroyed && param.from != *destroyed {
                        return Err(fail(format!(
                            "The destroyed participant {destroyed} does not have an associated destroying message after its declaration. Please check the sequence diagram."
                        )));
                    }
                    self.last_destroyed = None;
                }
                self.add_signal(
                    Some(&param.from),
                    Some(&param.to),
                    text,
                    param.signal_type,
                    param.activate,
                    param.central_connection,
                )?;
            }
            "boxStart" => self.add_box(&param.box_data),
            "boxEnd" => self.current_box = None,
            "loopStart" | "rectStart" | "optStart" | "altStart" | "else" | "parStart" | "and"
            | "criticalStart" | "option" | "breakStart" => {
                self.add_signal(None, None, text, param.signal_type, false, 0)?;
            }
            "loopEnd" | "rectEnd" | "optEnd" | "altEnd" | "parEnd" | "criticalEnd" | "breakEnd" => {
                self.add_signal(None, None, None, param.signal_type, false, 0)?
            }
            _ => {}
        }
        Ok(())
    }
}

/// Map.set on a list of positions, as on Pele's createdActors and destroyedActors.
fn set_position(list: &mut Vec<(String, usize)>, id: &str, index: usize) {
    match list.iter_mut().find(|(k, _)| k == id) {
        Some(entry) => entry.1 = index,
        None => list.push((id.to_string(), index)),
    }
}
