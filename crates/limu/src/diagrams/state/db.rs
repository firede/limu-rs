//! What a state diagram's parser fills, as Pele's StateDb in db.ts has it, with the
//! statements of types.ts: Mermaid's StateDB in its version 2 form, which Mermaid uses for
//! both `stateDiagram` and `stateDiagram-v2`. The legacy version, which numbers `[*]` states,
//! is not ported.

use super::graph::{StateGraph, build_state_graph};
use crate::errors::Error;
use crate::js::chars::{drop_indents, trim, trim_start};
use crate::js::map::Ordered;

/// A note beside a state, Pele's StateNote.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StateNote {
    /// "left of" or "right of".
    pub(crate) position: String,
    pub(crate) text: String,
}

/// What a statement says a state is described as: one text, or, for `state "d" as a:extra`,
/// a list, as Pele's StateStmt holds a string or an array.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Description {
    One(String),
    Many(Vec<String>),
}

/// A state as a statement gives it, Pele's StateStmt. What Pele leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct StateStmt {
    pub(crate) id: String,
    /// "default", "fork", "join", "choice" or "divider", or none, as a note's statement gives.
    pub(crate) kind: Option<&'static str>,
    pub(crate) description: Option<Description>,
    /// The document of a composite: none where the state is none, and empty for a composite
    /// with nothing in it.
    pub(crate) doc: Option<Vec<Stmt>>,
    pub(crate) note: Option<StateNote>,
    /// Set on a `[*]` once it is named: true where it starts, false where it ends.
    pub(crate) start: Option<bool>,
    pub(crate) classes: Option<Vec<String>>,
}

impl StateStmt {
    /// A state of an id and a type, with nothing else.
    pub(crate) fn new(id: impl Into<String>, kind: &'static str) -> StateStmt {
        StateStmt {
            id: id.into(),
            kind: Some(kind),
            ..StateStmt::default()
        }
    }
}

/// A statement of a document, Pele's Stmt. Text is what Mermaid's grammar leaves of a
/// statement that has no effect in a document: `hide empty description`, `scale`, a bare
/// `state name`, a floating note, and the values of accTitle and accDescr.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Stmt {
    Text(String),
    State(StateStmt),
    Relation {
        state1: StateStmt,
        state2: StateStmt,
        description: Option<String>,
    },
    ClassDef {
        id: String,
        classes: String,
    },
    Style {
        id: String,
        style_class: String,
    },
    ApplyClass {
        id: String,
        style_class: String,
    },
    Dir(String),
    Click {
        id: StateStmt,
        url: String,
        tooltip: String,
    },
}

/// A state as the DB keeps it, Pele's StateStmt as addState makes it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct State {
    pub(crate) id: String,
    pub(crate) kind: &'static str,
    pub(crate) descriptions: Vec<String>,
    /// The length of its document, where it has one. The DB shares the document with the
    /// statement it comes from, and reads nothing of it but whether there is one.
    pub(crate) doc: Option<usize>,
    pub(crate) note: Option<StateNote>,
    pub(crate) classes: Vec<String>,
    pub(crate) styles: Vec<String>,
    pub(crate) text_styles: Vec<String>,
}

/// A transition, Pele's StateRelation.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StateRelation {
    pub(crate) id1: String,
    pub(crate) id2: String,
    pub(crate) relation_title: String,
}

/// A class of styles, from classDef: Pele's StateClassDef.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct StateClassDef {
    pub(crate) id: String,
    pub(crate) styles: Vec<String>,
    pub(crate) text_styles: Vec<String>,
}

/// Where a click statement makes a state lead, Pele's StateLink. Its URL and tooltip keep the
/// quotes they were written with.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StateLink {
    pub(crate) id: String,
    pub(crate) url: String,
    pub(crate) tooltip: String,
}

/// A state diagram as its parser fills it, Pele's StateDb.
#[derive(Clone, Debug, Default)]
pub(crate) struct StateDb {
    pub(crate) root_doc: Vec<Stmt>,
    pub(crate) states: Ordered<State>,
    pub(crate) relations: Vec<StateRelation>,
    pub(crate) classes: Ordered<StateClassDef>,
    pub(crate) links: Ordered<StateLink>,
    pub(crate) graph: StateGraph,
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    divider_count: usize,
}

/// The direction a document is laid out in: its last `direction` statement, or TB.
pub(crate) fn doc_direction(doc: &[Stmt]) -> &str {
    let mut dir = "TB";
    for item in doc {
        if let Stmt::Dir(value) = item {
            dir = value;
        }
    }
    dir
}

impl StateDb {
    /// The direction the root document is laid out in: its last `direction` statement, as
    /// Pele's direction gives it.
    pub(crate) fn direction(&self) -> &str {
        doc_direction(&self.root_doc)
    }

    pub(crate) fn set_root_doc(&mut self, doc: Vec<Stmt>) -> Result<(), Error> {
        self.root_doc = doc;
        self.translate();
        self.extract()
    }

    /// Reads the root document's statements into states, relations and classes, as Pele's
    /// extract does, and builds the graph of the whole. The statements inside composites are
    /// read when the graph is built.
    fn extract(&mut self) -> Result<(), Error> {
        self.states = Ordered::default();
        self.relations = Vec::new();
        self.classes = Ordered::default();
        let doc = std::mem::take(&mut self.root_doc);
        for item in &doc {
            match item {
                Stmt::Text(_) | Stmt::Dir(_) => {}
                Stmt::State(s) => self.add_state(
                    trim(&s.id),
                    s.kind,
                    s.doc.as_ref().map(Vec::len),
                    s.description.as_ref(),
                    s.note.as_ref(),
                    None,
                ),
                Stmt::Relation {
                    state1,
                    state2,
                    description,
                } => self.add_relation(state1, state2, description.as_deref()),
                Stmt::ClassDef { id, classes } => self.add_style_class(trim(id), classes),
                Stmt::Style { id, style_class } => {
                    let styles: Vec<String> = style_class
                        .split(',')
                        .map(|s| trim(&s.replace(';', "")).to_string())
                        .collect();
                    for at in self.named(trim(id)) {
                        self.states.at_mut(at).styles = styles.clone();
                    }
                }
                Stmt::ApplyClass { id, style_class } => self.set_css_class(trim(id), style_class),
                Stmt::Click { id, url, tooltip } => self.add_link(&id.id, url, tooltip),
            }
        }
        self.root_doc = doc;
        self.graph = build_state_graph(&self.root_doc, &self.states, &self.classes)?;
        Ok(())
    }

    /// Mermaid's docTranslator, as Pele's translate does it: gives each `[*]` an id scoped to
    /// its parent, and turns a document split by `--` into one region per part. It walks the
    /// documents without recursion, in the order Pele's stack takes them, which is the order
    /// the regions after the last divider are numbered in.
    fn translate(&mut self) {
        let mut root = std::mem::take(&mut self.root_doc);
        {
            let mut pending: Vec<(String, std::slice::IterMut<Stmt>)> =
                vec![("root".to_string(), root.iter_mut())];
            while let Some((parent_id, items)) = pending.last_mut() {
                let Some(item) = items.next() else {
                    pending.pop();
                    continue;
                };
                let parent_id = parent_id.clone();
                let node = match item {
                    Stmt::Relation { state1, state2, .. } => {
                        name_state(state1, &parent_id, true);
                        name_state(state2, &parent_id, false);
                        continue;
                    }
                    Stmt::State(node) => node,
                    _ => continue,
                };
                name_state(node, &parent_id, true);
                let Some(doc) = node.doc.as_mut() else {
                    continue;
                };
                let mut regions: Vec<Stmt> = Vec::new();
                let mut current: Vec<Stmt> = Vec::new();
                for item in std::mem::take(doc) {
                    match item {
                        Stmt::State(mut divider)
                            if divider.kind == Some("divider") && divider.doc.is_none() =>
                        {
                            divider.doc = Some(std::mem::take(&mut current));
                            regions.push(Stmt::State(divider));
                        }
                        item => current.push(item),
                    }
                }
                // Mermaid names the region after the last divider with a random id, and Pele
                // continues the divider numbering. Mermaid also leaves a document that ends
                // with a divider ungrouped, and then fails to draw it. Pele keeps the regions
                // before that divider.
                if regions.is_empty() {
                    *doc = current;
                } else {
                    if !current.is_empty() {
                        let mut last = StateStmt::new(self.get_divider_id(), "divider");
                        last.doc = Some(current);
                        regions.push(Stmt::State(last));
                    }
                    *doc = regions;
                }
                let id = node.id.clone();
                let doc = node.doc.as_mut().expect("a document");
                pending.push((id, doc.iter_mut()));
            }
        }
        // The root document holds no divider, since `--` is a token only inside a composite,
        // so Pele's regions of the root, which it would not keep, are never made.
        self.root_doc = root;
    }

    #[allow(clippy::too_many_arguments)]
    fn add_state(
        &mut self,
        id: &str,
        kind: Option<&'static str>,
        doc: Option<usize>,
        descr: Option<&Description>,
        note: Option<&StateNote>,
        classes: Option<&[String]>,
    ) {
        let trimmed = trim(id);
        let kind = kind.unwrap_or("default");
        match self.states.get_mut(trimmed) {
            None => {
                self.states.set(
                    trimmed,
                    State {
                        id: trimmed.to_string(),
                        kind,
                        descriptions: Vec::new(),
                        doc,
                        note: note.cloned(),
                        classes: Vec::new(),
                        styles: Vec::new(),
                        text_styles: Vec::new(),
                    },
                );
            }
            Some(state) => {
                if state.doc.is_none() {
                    state.doc = doc;
                }
            }
        }
        match descr {
            Some(Description::One(text)) if !text.is_empty() => {
                self.add_description(trimmed, trim(text));
            }
            Some(Description::Many(texts)) => {
                for text in texts {
                    self.add_description(trimmed, trim(text));
                }
            }
            _ => {}
        }
        if let (Some(note), Some(state)) = (note, self.states.get_mut(trimmed)) {
            state.note = Some(note.clone());
        }
        for name in classes.into_iter().flatten() {
            self.set_css_class(trimmed, trim(name));
        }
    }

    fn add_link(&mut self, state_id: &str, url: &str, tooltip: &str) {
        self.links.set(
            state_id,
            StateLink {
                id: state_id.to_string(),
                url: url.to_string(),
                tooltip: tooltip.to_string(),
            },
        );
    }

    fn add_relation(&mut self, item1: &StateStmt, item2: &StateStmt, title: Option<&str>) {
        for item in [item1, item2] {
            self.add_state(
                &item.id,
                item.kind,
                item.doc.as_ref().map(Vec::len),
                item.description.as_ref(),
                item.note.as_ref(),
                item.classes.as_deref(),
            );
        }
        self.relations.push(StateRelation {
            id1: trim(&item1.id).to_string(),
            id2: trim(&item2.id).to_string(),
            relation_title: title.unwrap_or("").to_string(),
        });
    }

    fn add_description(&mut self, id: &str, descr: &str) {
        let Some(state) = self.states.get_mut(id) else {
            return;
        };
        let text = match descr.strip_prefix(':') {
            Some(rest) => trim(rest).to_string(),
            None => descr.to_string(),
        };
        state.descriptions.push(text);
    }

    pub(crate) fn get_divider_id(&mut self) -> String {
        self.divider_count += 1;
        format!("divider-id-{}", self.divider_count)
    }

    fn add_style_class(&mut self, id: &str, style_attributes: &str) {
        if self.classes.get(id).is_none() {
            self.classes.set(
                id,
                StateClassDef {
                    id: id.to_string(),
                    ..StateClassDef::default()
                },
            );
        }
        if style_attributes.is_empty() {
            return;
        }
        let found = self.classes.get_mut(id).expect("a class");
        for attrib in style_attributes.split(',') {
            let fixed = trim(&attrib.replacen(';', "", 1)).to_string();
            if attrib.contains("color") {
                found.text_styles.push(
                    fixed
                        .replacen("fill", "bgFill", 1)
                        .replacen("color", "fill", 1),
                );
            }
            found.styles.push(fixed);
        }
    }

    fn set_css_class(&mut self, item_ids: &str, css_class_name: &str) {
        for at in self.named(item_ids) {
            self.states
                .at_mut(at)
                .classes
                .push(css_class_name.to_string());
        }
    }

    /// The grammar calls this while it is still building the document, so the statement lands
    /// in the root document before the one it parses, and the direction is read from the
    /// parsed `dir` statements instead, as Pele's setDirection has it.
    pub(crate) fn set_direction(&mut self, dir: &str) {
        for item in &mut self.root_doc {
            if let Stmt::Dir(value) = item {
                *value = dir.to_string();
                return;
            }
        }
        self.root_doc.insert(0, Stmt::Dir(dir.to_string()));
    }

    /// The text without a colon it starts with, trimmed, as Pele's trimColon gives it.
    pub(crate) fn trim_colon(text: &str) -> String {
        trim(text.strip_prefix(':').unwrap_or(text)).to_string()
    }

    pub(crate) fn set_acc_title(&mut self, text: &str) {
        self.acc_title = Some(trim_start(text).to_string());
    }

    pub(crate) fn set_acc_description(&mut self, text: &str) {
        self.acc_descr = Some(drop_indents(text));
    }

    /// The states a comma-separated list of ids names, by their places, created where they do
    /// not exist yet, as Pele's named finds them. As in Mermaid, an id is looked up as written
    /// and trimmed only when it has to be created.
    fn named(&mut self, ids: &str) -> Vec<usize> {
        let mut out = Vec::new();
        for id in ids.split(',') {
            let mut at = self.states.index_of(id);
            if at.is_none() {
                self.add_state(id, None, None, None, None, None);
                at = self.states.index_of(trim(id));
            }
            out.extend(at);
        }
        out
    }
}

/// Names a state of a document whose parent is parent_id, as Pele's translate does: a `[*]`
/// takes the parent's id and _start where it is first, or _end, and any other id is trimmed.
fn name_state(node: &mut StateStmt, parent_id: &str, first: bool) {
    if node.id == "[*]" {
        node.id = format!("{parent_id}{}", if first { "_start" } else { "_end" });
        node.start = Some(first);
    } else {
        node.id = trim(&node.id).to_string();
    }
}
