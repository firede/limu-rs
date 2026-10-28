//! The graph a state diagram is drawn of, as Pele's buildStateGraph in graph.ts resolves it:
//! Mermaid's dataFetcher.

use std::collections::HashMap;

use super::db::{Description, State, StateClassDef, StateStmt, Stmt, doc_direction};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::{len16, trim};
use crate::js::map::Ordered;

/// A node as it is drawn, Pele's StateNode. What Pele leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct StateNode {
    pub(crate) id: String,
    /// rect, rectWithTitle, stateStart, stateEnd, choice, fork, join, divider,
    /// roundedWithTitle, note or noteGroup, or none for a state a note names, which has none.
    pub(crate) shape: Option<&'static str>,
    pub(crate) label: String,
    pub(crate) description: Vec<String>,
    pub(crate) css_classes: String,
    pub(crate) css_compiled_styles: Vec<String>,
    pub(crate) css_styles: Vec<String>,
    /// "group", "node" or none.
    pub(crate) kind: Option<&'static str>,
    pub(crate) is_group: bool,
    pub(crate) parent_id: Option<String>,
    /// The direction inside a composite: its last `direction` statement, or TB.
    pub(crate) dir: Option<String>,
    pub(crate) position: Option<String>,
    pub(crate) note_for: Option<String>,
    pub(crate) color_index: Option<usize>,
}

/// An edge as it is drawn, Pele's StateEdge. Labels are markdown throughout, as in Mermaid.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StateEdge {
    pub(crate) id: String,
    pub(crate) start: String,
    pub(crate) end: String,
    pub(crate) label: String,
    pub(crate) classes: &'static str,
    /// "dashed" for the line from a note to its state.
    pub(crate) pattern: Option<&'static str>,
}

/// The nodes and edges of a state diagram, Pele's StateGraph.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct StateGraph {
    pub(crate) nodes: Vec<StateNode>,
    pub(crate) edges: Vec<StateEdge>,
}

/// The fields a node carries, which differ between the nodes Pele makes, so that one merged
/// into a node of the same id by Object.assign leaves those it does not carry as they were.
#[derive(Clone, Copy, PartialEq)]
enum Carries {
    /// A state's: its color index, and no position.
    State,
    /// A note's group: a position.
    NoteGroup,
    /// A note: a position and the state it is for.
    Note,
}

/// A node as the graph gathers it across the statements that name it, Pele's Draft.
struct Draft {
    shape: Option<&'static str>,
    description: Description,
    css_classes: String,
    css_styles: Vec<String>,
    kind: Option<&'static str>,
    dir: Option<String>,
    color_index: Option<usize>,
}

impl Description {
    /// Its length, as JavaScript's length gives it: of the list, or of the text in UTF-16
    /// code units.
    fn length(&self) -> usize {
        match self {
            Description::One(s) => len16(s),
            Description::Many(v) => v.len(),
        }
    }

    /// Whether it is truthy, as `if (item.description)` tests it: a text that is not empty,
    /// or any list.
    fn truthy(&self) -> bool {
        match self {
            Description::One(s) => !s.is_empty(),
            Description::Many(_) => true,
        }
    }

    /// It as a list, as `[d].flat()` makes one.
    fn flat(&self) -> Vec<String> {
        match self {
            Description::One(s) => vec![s.clone()],
            Description::Many(v) => v.clone(),
        }
    }
}

/// A document being walked, Pele's Frame: the state it is the document of, its statements,
/// how far it has been read, and whether its clusters alternate.
struct Frame<'a> {
    item: Option<&'a StateStmt>,
    doc: &'a [Stmt],
    at: usize,
    alt: bool,
}

/// The graph a state diagram is drawn of, as Pele's buildStateGraph resolves it: the
/// documents walked, without recursion, into the nodes and edges that are drawn.
pub(crate) fn build_state_graph(
    root_doc: &[Stmt],
    states: &Ordered<State>,
    classes: &Ordered<StateClassDef>,
) -> Result<StateGraph, Error> {
    let mut b = Builder {
        states,
        classes,
        g: StateGraph::default(),
        by_id: HashMap::new(),
        drafts: HashMap::new(),
        slots: HashMap::new(),
        counter: 0,
        next_slot: 0,
    };
    let mut stack = vec![Frame {
        item: None,
        doc: root_doc,
        at: 0,
        alt: true,
    }];
    while let Some(frame) = stack.last_mut() {
        if frame.at >= frame.doc.len() {
            stack.pop();
            continue;
        }
        let item = &frame.doc[frame.at];
        frame.at += 1;
        let (parent, alt) = (frame.item, frame.alt);
        match item {
            Stmt::State(s) => {
                b.visit(parent, s, alt);
                if let Some(doc) = &s.doc {
                    stack.push(Frame {
                        item: Some(s),
                        doc,
                        at: 0,
                        alt: !alt,
                    });
                }
            }
            Stmt::Relation {
                state1,
                state2,
                description,
            } => {
                b.visit(parent, state1, alt);
                b.visit(parent, state2, alt);
                let id = format!("edge{}", b.counter);
                b.counter += 1;
                b.g.edges.push(StateEdge {
                    id,
                    start: state1.id.clone(),
                    end: state2.id.clone(),
                    label: description.clone().unwrap_or_default(),
                    classes: "transition",
                    pattern: None,
                });
            }
            _ => {}
        }
    }

    for node in &b.g.nodes {
        if node.is_group && !node.description.is_empty() {
            let mut e = Error::new(
                format!(
                    "Group nodes can only have label. Remove the additional description for node [{}]",
                    node.id
                ),
                ErrorCode::Semantic,
            );
            e.diagram = Some(DiagramType::State);
            return Err(e);
        }
    }
    Ok(b.g)
}

struct Builder<'a> {
    states: &'a Ordered<State>,
    classes: &'a Ordered<StateClassDef>,
    g: StateGraph,
    by_id: HashMap<String, usize>,
    drafts: HashMap<String, Draft>,
    slots: HashMap<String, Option<usize>>,
    counter: usize,
    next_slot: usize,
}

impl Builder<'_> {
    /// Adds a node, or merges it into the node of its id, as Pele's insert does.
    fn insert(&mut self, mut node: StateNode, carries: Carries) {
        // The two literal ids are what DOMPurify leaves of `<<fork>>` and `<<choice>>` in
        // Mermaid, kept for parity.
        if node.id.is_empty() || node.id == "</join></fork>" || node.id == "</choice>" {
            return;
        }
        if !node.css_classes.is_empty() {
            // One copy, however many classes: a state named in a class statement many times
            // has a long list.
            let mut compiled: Option<Vec<String>> = None;
            for name in node.css_classes.split(' ') {
                let Some(def) = self.classes.get(name) else {
                    continue;
                };
                compiled
                    .get_or_insert_with(|| node.css_compiled_styles.clone())
                    .extend(def.styles.iter().cloned());
            }
            if let Some(compiled) = compiled {
                node.css_compiled_styles = compiled;
            }
        }
        let Some(&at) = self.by_id.get(&node.id) else {
            self.by_id.insert(node.id.clone(), self.g.nodes.len());
            self.g.nodes.push(node);
            return;
        };
        let existing = &mut self.g.nodes[at];
        let parent_id = node.parent_id.take().or_else(|| existing.parent_id.take());
        // Object.assign: each field the new node carries replaces the existing one's.
        let (position, note_for, color_index) = (
            existing.position.take(),
            existing.note_for.take(),
            existing.color_index,
        );
        *existing = StateNode {
            position: if carries == Carries::State {
                position
            } else {
                node.position
            },
            note_for: if carries == Carries::Note {
                node.note_for
            } else {
                note_for
            },
            color_index: if carries == Carries::State {
                node.color_index
            } else {
                color_index
            },
            parent_id,
            ..node
        };
    }

    /// Resolves a state a statement gives into its node, and the notes beside it, as Pele's
    /// visit does.
    fn visit(&mut self, parent: Option<&StateStmt>, item: &StateStmt, alt: bool) {
        let id = item.id.as_str();
        if id == "root" {
            return;
        }
        let known = self.states.get(id);
        let class_str = known.map_or(String::new(), |s| s.classes.join(" "));
        let styles = known.map_or(Vec::new(), |s| s.styles.clone());

        let draft = self.drafts.entry(id.to_string()).or_insert_with(|| {
            let mut shape = Some("rect");
            match item.start {
                Some(true) => shape = Some("stateStart"),
                Some(false) => shape = Some("stateEnd"),
                None => {}
            }
            if item.kind != Some("default") {
                shape = item.kind;
            }
            Draft {
                shape,
                description: Description::One(id.to_string()),
                css_classes: format!("{class_str} statediagram-state"),
                css_styles: styles.clone(),
                kind: None,
                dir: None,
                color_index: None,
            }
        });

        if let Some(descr) = item.description.as_ref().filter(|d| d.truthy()) {
            match &draft.description {
                Description::Many(list) => {
                    draft.shape = Some("rectWithTitle");
                    let mut list = list.clone();
                    list.extend(descr.flat());
                    draft.description = Description::Many(list);
                }
                Description::One(text) if !text.is_empty() => {
                    draft.shape = Some("rectWithTitle");
                    let list = if text == id {
                        descr.flat()
                    } else {
                        let mut list = vec![text.clone()];
                        list.extend(descr.flat());
                        list
                    };
                    draft.description = Description::Many(list);
                }
                Description::One(_) => {
                    draft.shape = Some("rect");
                    draft.description = descr.clone();
                }
            }
        }
        if draft.description.length() == 1 && draft.shape == Some("rectWithTitle") {
            draft.shape = Some(if draft.kind == Some("group") {
                "roundedWithTitle"
            } else {
                "rect"
            });
        }

        if draft.kind.is_none()
            && let Some(doc) = &item.doc
        {
            draft.kind = Some("group");
            draft.dir = Some(doc_direction(doc).to_string());
            draft.shape = Some(if item.kind == Some("divider") {
                "divider"
            } else {
                "roundedWithTitle"
            });
            // A container takes the next palette slot unless it is a region, which shares its
            // composite's.
            match parent.and_then(|p| self.slots.get(&p.id)) {
                Some(&slot) if draft.shape == Some("divider") => draft.color_index = slot,
                _ => {
                    let slot = self.next_slot;
                    self.next_slot += 1;
                    draft.color_index = if !trim(&class_str).is_empty() || !styles.is_empty() {
                        None
                    } else {
                        Some(slot)
                    };
                }
            }
            self.slots.insert(id.to_string(), draft.color_index);
            draft.css_classes = format!(
                "{} statediagram-cluster {}",
                draft.css_classes,
                if alt { "statediagram-cluster-alt" } else { "" }
            );
        }

        let (label, description) = match &draft.description {
            Description::Many(full) => (
                full.first().cloned().unwrap_or_default(),
                full[1.min(full.len())..].to_vec(),
            ),
            Description::One(full) => (full.clone(), Vec::new()),
        };
        let mut node = StateNode {
            id: id.to_string(),
            shape: draft.shape,
            label,
            description,
            css_classes: draft.css_classes.clone(),
            css_compiled_styles: Vec::new(),
            css_styles: draft.css_styles.clone(),
            kind: draft.kind,
            is_group: draft.kind == Some("group"),
            dir: draft.dir.clone(),
            color_index: draft.color_index,
            ..StateNode::default()
        };
        if node.shape == Some("divider") {
            node.label = String::new();
        }
        if let Some(p) = parent.filter(|p| p.id != "root") {
            node.parent_id = Some(p.id.clone());
        }

        let Some(note) = &item.note else {
            self.insert(node, Carries::State);
            return;
        };
        // Mermaid puts each note in a group of its own, named after the state. Pele keeps that
        // group in the model and lays the note out beside its state instead.
        let aside = |aside_id: String, shape, css_classes: String, is_group| StateNode {
            id: aside_id,
            shape: Some(shape),
            label: note.text.clone(),
            css_classes,
            kind: Some(if is_group { "group" } else { "node" }),
            is_group,
            position: Some(note.position.clone()),
            ..StateNode::default()
        };
        let mut note_node = aside(
            format!("{id}----note-{}", self.counter),
            "note",
            "statediagram-note".to_string(),
            false,
        );
        self.counter += 1;
        let group_id = format!("{id}----parent");
        note_node.parent_id = Some(group_id.clone());
        note_node.note_for = Some(id.to_string());
        let note_id = note_node.id.clone();
        let group = aside(group_id, "noteGroup", node.css_classes.clone(), true);
        self.insert(group, Carries::NoteGroup);
        self.insert(note_node, Carries::Note);
        self.insert(node, Carries::State);
        let left = note.position == "left of";
        let (from, to) = if left {
            (note_id, id.to_string())
        } else {
            (id.to_string(), note_id)
        };
        self.g.edges.push(StateEdge {
            id: format!("{from}-{to}"),
            start: from,
            end: to,
            label: String::new(),
            classes: "transition note-edge",
            pattern: Some("dashed"),
        });
    }
}
