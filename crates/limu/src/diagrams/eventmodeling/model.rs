//! An event modeling diagram's model, as Pele's model.ts builds it from the tree: its frames
//! in lanes, the relations between them, and its notes and specifications.

use std::collections::{HashMap, HashSet};

use super::parser::{EmFrame, EventModelAst};
use crate::common::tokens::{acc_descr_value, acc_title_value, title_value};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode, locate};
use crate::js::chars::{drop_indents, len16, trim, trim_end, trim_start};

/// What a frame is, Pele's EntityKind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntityKind {
    Ui,
    Processor,
    Command,
    ReadModel,
    Event,
}

impl EntityKind {
    /// The kind as Pele names it.
    #[cfg(test)]
    pub(crate) fn name(self) -> &'static str {
        match self {
            EntityKind::Ui => "ui",
            EntityKind::Processor => "processor",
            EntityKind::Command => "command",
            EntityKind::ReadModel => "readmodel",
            EntityKind::Event => "event",
        }
    }
}

/// Which group of lanes a frame is in, Pele's LaneGroup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum LaneGroup {
    Trigger,
    State,
    Stream,
}

impl LaneGroup {
    /// The group as Pele names it.
    #[cfg(test)]
    pub(crate) fn name(self) -> &'static str {
        match self {
            LaneGroup::Trigger => "trigger",
            LaneGroup::State => "state",
            LaneGroup::Stream => "stream",
        }
    }

    /// The group's first index, its label, and the prefix of the label of a lane of a
    /// namespace in it, as Pele's GROUPS has them.
    fn of(self) -> (usize, &'static str, &'static str) {
        match self {
            LaneGroup::Trigger => (0, "UI/Automation", "UI/A: "),
            LaneGroup::State => (100, "Command/Read Model", "C/RM: "),
            LaneGroup::Stream => (200, "Events", "Stream: "),
        }
    }
}

/// A lane, Pele's EmLane. Its index is Mermaid's key of order: 0 for UI/Automation, 100 for
/// Command/Read Model, 200 for Events, and the next free number after those for each
/// namespace.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EmLane {
    pub(crate) index: usize,
    pub(crate) label: String,
    pub(crate) group: LaneGroup,
    pub(crate) namespace: Option<String>,
}

/// A frame drawn as a box, Pele's EmBox. Its lane is its place in the model's lanes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EmBox {
    /// The frame's number as written, such as "01".
    pub(crate) id: String,
    pub(crate) kind: EntityKind,
    pub(crate) entity: String,
    pub(crate) name: String,
    pub(crate) namespace: Option<String>,
    pub(crate) reset: bool,
    pub(crate) lane: usize,
    /// Example data from the frame, or from the data block it refers to.
    pub(crate) data: Option<String>,
    pub(crate) data_type: Option<String>,
}

/// A relation from one box to another, by their places in the model's boxes, Pele's
/// EmRelation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EmRelation {
    pub(crate) source: usize,
    pub(crate) target: usize,
}

/// A note on a frame, Pele's EmNote.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EmNote {
    pub(crate) frame: String,
    pub(crate) text: String,
    pub(crate) data_type: Option<String>,
}

/// A given/when/then specification of a frame, Pele's EmSpecification.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EmSpecification {
    pub(crate) frame: String,
    pub(crate) given: Vec<String>,
    pub(crate) when: Vec<String>,
    pub(crate) then: Vec<String>,
}

/// An event modeling diagram, Pele's EventModelingModel. Its lanes are sorted from top to
/// bottom.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EventModelingModel {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) boxes: Vec<EmBox>,
    pub(crate) lanes: Vec<EmLane>,
    pub(crate) relations: Vec<EmRelation>,
    pub(crate) notes: Vec<EmNote>,
    pub(crate) specifications: Vec<EmSpecification>,
    pub(crate) entities: Vec<String>,
}

/// The kind of a frame of an entity type, as Pele's KINDS gives it.
fn kind_of(entity_type: &str) -> EntityKind {
    match entity_type {
        "ui" => EntityKind::Ui,
        "pcr" | "processor" => EntityKind::Processor,
        "rmo" | "readmodel" => EntityKind::ReadModel,
        "cmd" | "command" => EntityKind::Command,
        _ => EntityKind::Event,
    }
}

/// The group of lanes a kind of frame is in, as Pele's groupOf gives it.
fn group_of(kind: EntityKind) -> LaneGroup {
    match kind {
        EntityKind::Ui | EntityKind::Processor => LaneGroup::Trigger,
        EntityKind::Event => LaneGroup::Stream,
        _ => LaneGroup::State,
    }
}

/// The text between the outer braces, without the indentation its lines share, as Pele's
/// blockText takes it. A quoted value has no braces, and its quotes are taken off.
pub(crate) fn block_text(raw: &str) -> String {
    let open = raw.find('{');
    let close = raw.rfind('}');
    let inner = match (open, close) {
        (Some(open), Some(close)) if close > open => &raw[open + 1..close],
        // The quotes are ASCII, as the token takes them.
        (None, _) if len16(raw) >= 2 => &raw[1..raw.len() - 1],
        _ => "",
    };
    let inner = inner.replace('\r', "");
    let mut lines: Vec<&str> = inner.split('\n').collect();
    while lines.first().is_some_and(|l| trim(l).is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| trim(l).is_empty()) {
        lines.pop();
    }
    // The indentation is white space, each character of it one code unit.
    let indent = lines
        .iter()
        .filter(|l| !trim(l).is_empty())
        .map(|l| l[..l.len() - trim_start(l).len()].chars().count())
        .min();
    lines
        .iter()
        .map(|line| {
            let cut = match indent {
                Some(n) => line.char_indices().nth(n).map_or(line.len(), |(i, _)| i),
                None => 0,
            };
            trim_end(&line[cut..])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Builds the model of an event modeling diagram from its tree, as Pele's buildEventModel
/// does. title is the one its front matter gives. A frame number given twice is refused.
pub(crate) fn build_event_model(
    ast: &EventModelAst,
    source: &str,
    title: Option<&str>,
) -> Result<EventModelingModel, Error> {
    let mut ids = HashSet::new();
    for frame in &ast.frames {
        if !ids.insert(frame.name.as_str()) {
            let line = locate(source, frame.offset).line;
            let mut e = Error::new(
                format!(
                    "Duplicate event modeling frame ID \"{}\" on line {line}",
                    frame.name
                ),
                ErrorCode::Semantic,
            );
            e.diagram = Some(DiagramType::EventModeling);
            e.line = line;
            return Err(e);
        }
    }

    let mut data: HashMap<&str, &str> = HashMap::new();
    for entity in &ast.data_entities {
        data.entry(&entity.name).or_insert(&entity.data_block_value);
    }

    // One lane per group, and one more per namespace within a group, in the order they are
    // first met.
    let mut lanes: Vec<EmLane> = Vec::new();
    let mut lane_of: HashMap<(LaneGroup, Option<String>), usize> = HashMap::new();
    let mut next: HashMap<LaneGroup, usize> = HashMap::new();
    let mut lane = |kind: EntityKind, namespace: Option<&str>| -> usize {
        let group = group_of(kind);
        let key = (group, namespace.map(str::to_string));
        if let Some(&found) = lane_of.get(&key) {
            return found;
        }
        let (first, label, prefix) = group.of();
        let mut index = first;
        if namespace.is_some() {
            index = next.get(&group).copied().unwrap_or(first + 1);
            next.insert(group, index + 1);
        }
        lanes.push(EmLane {
            index,
            label: match namespace {
                Some(ns) => format!("{prefix}{ns}"),
                None => label.to_string(),
            },
            group,
            namespace: namespace.map(str::to_string),
        });
        lane_of.insert(key, lanes.len() - 1);
        lanes.len() - 1
    };

    let mut boxes: Vec<EmBox> = Vec::new();
    let mut relations = Vec::new();
    for (index, frame) in ast.frames.iter().enumerate() {
        let b = frame_box(frame, &data, &mut lane);
        boxes.push(b);
        let at = boxes.len() - 1;
        if !frame.source_frames.is_empty() {
            // Sources connect in frame order, and only frames that already have a box.
            let wanted: HashSet<&str> = frame.source_frames.iter().map(String::as_str).collect();
            for (source, b) in boxes.iter().enumerate() {
                if wanted.contains(b.id.as_str()) {
                    relations.push(EmRelation { source, target: at });
                }
            }
        } else if index > 0 && !boxes[at].reset {
            // Otherwise the nearest earlier frame in another lane feeds this one.
            if let Some(source) = (0..at).rev().find(|&i| boxes[i].lane != boxes[at].lane) {
                relations.push(EmRelation { source, target: at });
            }
        }
    }

    // The lanes sorted from top to bottom, stably, and each box given its lane's new place.
    let mut order: Vec<usize> = (0..lanes.len()).collect();
    order.sort_by_key(|&i| lanes[i].index);
    let mut place = vec![0; lanes.len()];
    for (to, &from) in order.iter().enumerate() {
        place[from] = to;
    }
    for b in &mut boxes {
        b.lane = place[b.lane];
    }
    let lanes = order.iter().map(|&i| lanes[i].clone()).collect();

    Ok(EventModelingModel {
        title: ast
            .title
            .as_deref()
            .map(title_value)
            .filter(|t| !t.is_empty())
            .or_else(|| title.map(str::to_string)),
        acc_title: ast
            .acc_title
            .as_deref()
            .map(|t| trim_start(&acc_title_value(t)).to_string()),
        acc_descr: ast
            .acc_descr
            .as_deref()
            .map(|d| drop_indents(&acc_descr_value(d))),
        boxes,
        lanes,
        relations,
        notes: ast
            .note_entities
            .iter()
            .map(|note| EmNote {
                frame: note.source_frame.clone(),
                text: block_text(&note.data_block_value),
                data_type: note.data_type.clone(),
            })
            .collect(),
        specifications: ast
            .gwt_entities
            .iter()
            .map(|gwt| EmSpecification {
                frame: gwt.source_frame.clone(),
                given: gwt.given_statements.clone(),
                when: gwt.when_statements.clone(),
                then: gwt.then_statements.clone(),
            })
            .collect(),
        entities: ast.model_entities.clone(),
    })
}

/// The box of a frame, in the lane lane gives it.
fn frame_box(
    frame: &EmFrame,
    data: &HashMap<&str, &str>,
    lane: &mut impl FnMut(EntityKind, Option<&str>) -> usize,
) -> EmBox {
    // Only a two-part name has a namespace.
    let parts: Vec<&str> = frame.entity_identifier.split('.').collect();
    let namespace = (parts.len() == 2).then(|| parts[0]);
    let kind = kind_of(&frame.model_entity_type);
    let mut text = frame.data_inline_value.as_deref().map(block_text);
    if let Some(reference) = &frame.data_reference
        && let Some(block) = data.get(reference.as_str())
    {
        text = Some(block_text(block));
    }
    EmBox {
        id: frame.name.clone(),
        kind,
        entity: frame.entity_identifier.clone(),
        name: if parts.len() == 2 {
            parts[1].to_string()
        } else {
            frame.entity_identifier.clone()
        },
        namespace: namespace.map(str::to_string),
        reset: frame.reset,
        lane: lane(kind, namespace),
        data: text,
        data_type: frame.data_type.clone(),
    }
}
