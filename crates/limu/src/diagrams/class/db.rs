//! What a class diagram's parser fills, as Pele's ClassDb in db.ts has it: its classes,
//! relations, notes, namespaces, interfaces and the classes of styles classDef defines.

use std::collections::HashMap;

use super::members::{ClassMember, MemberType};
use crate::js::chars::{drop_indents, is_digit, trim, trim_start, unit};
use crate::text::decode_entities;
use crate::url::sanitize_url;

/// The ends of a relation with a marker, as Pele's RELATION_TYPE numbers them. An end with
/// no marker, Pele's 'none', is none.
pub(crate) const AGGREGATION: u8 = 0;
pub(crate) const EXTENSION: u8 = 1;
pub(crate) const COMPOSITION: u8 = 2;
pub(crate) const DEPENDENCY: u8 = 3;
pub(crate) const LOLLIPOP: u8 = 4;

/// The lines of a relation, as Pele's LINE_TYPE numbers them.
pub(crate) const LINE: u8 = 0;
pub(crate) const DOTTED_LINE: u8 = 1;

/// The kind of marker at an end of a relation, Pele's RelationEnd: one of RELATION_TYPE, or
/// none for 'none'.
pub(crate) type RelationEnd = Option<u8>;

/// A class, Pele's ClassNode. What Pele leaves undefined is none.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClassNode {
    pub(crate) id: String,
    /// The generic type its name carries, as `T` of `List~T~`.
    pub(crate) kind: String,
    pub(crate) label: String,
    pub(crate) css_classes: String,
    pub(crate) methods: Vec<ClassMember>,
    pub(crate) members: Vec<ClassMember>,
    pub(crate) annotations: Vec<String>,
    pub(crate) styles: Vec<String>,
    /// The namespace it is declared in.
    pub(crate) parent: Option<String>,
    pub(crate) link: Option<String>,
    pub(crate) link_target: Option<String>,
    /// Set by a click with a callback, which is never run. The class is only marked
    /// clickable, as in Mermaid's strict mode.
    pub(crate) have_callback: bool,
    pub(crate) tooltip: Option<String>,
}

/// A line between two classes, Pele's ClassRelation.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClassRelation {
    pub(crate) id1: String,
    pub(crate) id2: String,
    /// The texts at its ends, "none" where there is none.
    pub(crate) relation_title1: String,
    pub(crate) relation_title2: String,
    pub(crate) title: Option<String>,
    pub(crate) type1: RelationEnd,
    pub(crate) type2: RelationEnd,
    pub(crate) line_type: u8,
}

/// A note, on its own or for a class, Pele's ClassNote.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClassNote {
    pub(crate) id: String,
    /// The class it is for, as written.
    pub(crate) class: Option<String>,
    pub(crate) text: String,
    pub(crate) index: usize,
    /// The namespace it is declared in.
    pub(crate) parent: Option<String>,
}

/// The interface at the lollipop end of a relation, Pele's ClassInterface.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClassInterface {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) class_id: String,
}

/// A namespace, Pele's NamespaceNode. It holds the keys of its classes, notes and children,
/// in the order they were added, as its Maps do.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NamespaceNode {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) classes: Vec<String>,
    pub(crate) notes: Vec<String>,
    pub(crate) children: Vec<String>,
    pub(crate) parent: Option<String>,
    /// False for the ancestors made on the way to a dotted name such as `A.B.C`.
    pub(crate) explicit: bool,
}

/// A class of styles classDef defines, Pele's StyleClass.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StyleClass {
    pub(crate) id: String,
    pub(crate) styles: Vec<String>,
    pub(crate) text_styles: Vec<String>,
}

/// Items kept in the order they were first set, and found by their keys, as a JavaScript
/// Map keeps them.
#[derive(Clone, Debug)]
pub(crate) struct Keyed<T> {
    items: Vec<T>,
    at: HashMap<String, usize>,
}

impl<T> Default for Keyed<T> {
    fn default() -> Self {
        Keyed {
            items: Vec::new(),
            at: HashMap::new(),
        }
    }
}

impl<T> Keyed<T> {
    pub(crate) fn get(&self, key: &str) -> Option<&T> {
        self.at.get(key).map(|&i| &self.items[i])
    }

    fn get_mut(&mut self, key: &str) -> Option<&mut T> {
        self.at.get(key).map(|&i| &mut self.items[i])
    }

    /// The place of a key, in the order the keys were set.
    pub(crate) fn index(&self, key: &str) -> Option<usize> {
        self.at.get(key).copied()
    }

    /// Adds an item of a key that has none.
    fn add(&mut self, key: &str, item: T) -> usize {
        let i = self.items.len();
        self.at.insert(key.to_string(), i);
        self.items.push(item);
        i
    }

    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }

    pub(crate) fn values(&self) -> std::slice::Iter<'_, T> {
        self.items.iter()
    }
}

impl<T> std::ops::Index<usize> for Keyed<T> {
    type Output = T;

    fn index(&self, i: usize) -> &T {
        &self.items[i]
    }
}

/// Sets a key in a list of keys, where it has no place yet, as a Map's set of a key does.
fn set_key(keys: &mut Vec<String>, key: &str) {
    if !keys.iter().any(|k| k == key) {
        keys.push(key.to_string());
    }
}

/// A class diagram as its parser fills it, Pele's ClassDb.
#[derive(Clone, Debug)]
pub(crate) struct ClassDb {
    pub(crate) direction: String,
    pub(crate) classes: Keyed<ClassNode>,
    pub(crate) relations: Vec<ClassRelation>,
    pub(crate) notes: Keyed<ClassNote>,
    pub(crate) namespaces: Keyed<NamespaceNode>,
    pub(crate) interfaces: Vec<ClassInterface>,
    pub(crate) style_classes: Keyed<StyleClass>,
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,

    namespace_stack: Vec<String>,
    /// The classes of each CSS class name, by their places in classes, in the order they were
    /// tagged, as the Sets of Pele's withCssClass hold them.
    with_css_class: HashMap<String, Vec<usize>>,
}

impl Default for ClassDb {
    fn default() -> Self {
        ClassDb {
            direction: "TB".to_string(),
            classes: Keyed::default(),
            relations: Vec::new(),
            notes: Keyed::default(),
            namespaces: Keyed::default(),
            interfaces: Vec::new(),
            style_classes: Keyed::default(),
            title: None,
            acc_title: None,
            acc_descr: None,
            namespace_stack: Vec::new(),
            with_css_class: HashMap::new(),
        }
    }
}

/// Mermaid looks ids that start with a digit up under this prefix, so they never match a
/// class, Pele's DOM_ID_PREFIX.
const DOM_ID_PREFIX: &str = "classId-";

/// A name such as `List~T~` split into the class's name and its generic type, as Pele's
/// splitName splits it.
pub(crate) fn split_name(id: &str) -> (&str, &str) {
    match id.find('~') {
        Some(at) if at > 0 => {
            let rest = &id[at + 1..];
            let end = rest.find('~').unwrap_or(rest.len());
            (&id[..at], &rest[..end])
        }
        _ => (id, ""),
    }
}

/// The class's name of a name that may carry a generic type.
fn class_name(id: &str) -> &str {
    split_name(id).0
}

/// The class's name an id given to cssClass or a link is looked up by, under the prefix of
/// DOM_ID_PREFIX where it starts with a digit, as Pele's lookupId gives it.
fn lookup_id(id: &str) -> String {
    if is_digit(unit(id, 0)) {
        class_name(&format!("{DOM_ID_PREFIX}{id}")).to_string()
    } else {
        class_name(id).to_string()
    }
}

/// A label after a colon, as Pele's cleanupLabel takes it.
pub(crate) fn cleanup_label(label: &str) -> String {
    trim(label.strip_prefix(':').unwrap_or(label)).to_string()
}

impl ClassDb {
    /// Adds a class of a name that may carry a generic type, unless there is one, as Pele's
    /// addClass does.
    pub(crate) fn add_class(&mut self, id: &str) {
        let (name, kind) = split_name(id);
        if self.classes.get(name).is_some() {
            return;
        }
        let node = ClassNode {
            id: name.to_string(),
            kind: kind.to_string(),
            label: name.to_string(),
            css_classes: "default".to_string(),
            methods: Vec::new(),
            members: Vec::new(),
            annotations: Vec::new(),
            styles: Vec::new(),
            parent: None,
            link: None,
            link_target: None,
            have_callback: false,
            tooltip: None,
        };
        let at = self.classes.add(name, node);
        self.tag(at, "default");
    }

    /// Records that a class has a CSS class name, for defineClass to style it, as Pele's tag
    /// does.
    fn tag(&mut self, node: usize, css_class: &str) {
        let tagged = self
            .with_css_class
            .entry(css_class.to_string())
            .or_default();
        if !tagged.contains(&node) {
            tagged.push(node);
        }
    }

    /// The class of a name that may carry a generic type, made where there is none, as
    /// Pele's ensure gives it. Mermaid throws a TypeError where this makes one, as for an
    /// annotation on a class not yet declared.
    fn ensure(&mut self, id: &str) -> &mut ClassNode {
        self.add_class(id);
        self.classes
            .get_mut(class_name(id))
            .expect("a class just added")
    }

    /// Sets the label a class is shown with, as Pele's setClassLabel does.
    pub(crate) fn set_class_label(&mut self, id: &str, label: &str) {
        self.ensure(id).label = label.to_string();
    }

    /// Adds a relation, and the classes at its ends, as Pele's addRelation does. A lollipop
    /// end with no marker at the other is an interface of its own.
    pub(crate) fn add_relation(&mut self, mut relation: ClassRelation) {
        let (type1, type2) = (relation.type1, relation.type2);
        if type1 == Some(LOLLIPOP) && type2.is_none() {
            self.add_class(&relation.id2);
            relation.id1 = self.add_interface(&relation.id1, &relation.id2);
        } else if type2 == Some(LOLLIPOP) && type1.is_none() {
            self.add_class(&relation.id1);
            relation.id2 = self.add_interface(&relation.id2, &relation.id1);
        } else {
            self.add_class(&relation.id1);
            self.add_class(&relation.id2);
        }
        relation.id1 = class_name(&relation.id1).to_string();
        relation.id2 = class_name(&relation.id2).to_string();
        relation.relation_title1 = trim(&relation.relation_title1).to_string();
        relation.relation_title2 = trim(&relation.relation_title2).to_string();
        self.relations.push(relation);
    }

    /// Adds the interface of a lollipop end, labeled label and on the class of class_id, and
    /// gives its id, as Pele's addInterface does.
    fn add_interface(&mut self, label: &str, class_id: &str) -> String {
        let id = format!("interface{}", self.interfaces.len());
        self.interfaces.push(ClassInterface {
            id: id.clone(),
            label: label.to_string(),
            class_id: class_id.to_string(),
        });
        id
    }

    /// Adds an annotation, as `interface` of `<<interface>>`, to a class, as Pele's
    /// addAnnotation does.
    pub(crate) fn add_annotation(&mut self, class: &str, annotation: &str) {
        self.ensure(class).annotations.push(annotation.to_string());
    }

    /// Adds a member to a class, an annotation, a method or an attribute, as Pele's
    /// addMember reads it.
    pub(crate) fn add_member(&mut self, class: &str, member: &str) {
        let node = self.ensure(class);
        let text = trim(member);
        if text.starts_with("<<") && text.ends_with(">>") {
            node.annotations.push(text[2..text.len() - 2].to_string());
        } else if text.find(')').is_some_and(|i| i > 0) {
            let parsed = ClassMember::new(text, MemberType::Method);
            if parsed.member_type == MemberType::Method {
                node.methods.push(parsed);
            } else {
                node.members.push(parsed);
            }
        } else if !text.is_empty() {
            node.members
                .push(ClassMember::new(text, MemberType::Attribute));
        }
    }

    /// Adds the members of a class's body, in order, as Pele's addMembers does. Pele's parser
    /// gives it them last first, and it turns them back. Here they are in order throughout.
    pub(crate) fn add_members(&mut self, class: &str, members: &[&str]) {
        for m in members {
            self.add_member(class, m);
        }
    }

    /// Adds a note, for a class or for none, and gives its id, as Pele's addNote does.
    pub(crate) fn add_note(&mut self, text: &str, class: Option<&str>) -> String {
        let index = self.notes.len();
        let id = format!("note{index}");
        let note = ClassNote {
            id: id.clone(),
            class: class.map(str::to_string),
            text: text.to_string(),
            index,
            parent: None,
        };
        self.notes.add(&id, note);
        id
    }

    /// Adds a class name to each class of a list of ids, as Pele's setCssClass does.
    pub(crate) fn set_css_class(&mut self, ids: &str, css_class: &str) {
        for id in ids.split(',') {
            let Some(at) = self.classes.index(&lookup_id(id)) else {
                continue;
            };
            let node = &mut self.classes.items[at];
            node.css_classes.push(' ');
            node.css_classes.push_str(css_class);
            for word in css_class.split(' ') {
                self.tag(at, word);
            }
        }
    }

    /// Defines a class of styles for each of ids, and styles the classes that already have
    /// its name, as Pele's defineClass does. Mermaid tests `cssClasses.includes(id)`, which
    /// also matches part of a longer name, so that a classDef named `def` styles every class
    /// through `default`. Pele compares whole names, and so does this.
    pub(crate) fn define_class(&mut self, ids: &[&str], style: &[String]) {
        for &id in ids {
            let at = match self.style_classes.index(id) {
                Some(at) => at,
                None => self.style_classes.add(
                    id,
                    StyleClass {
                        id: id.to_string(),
                        styles: Vec::new(),
                        text_styles: Vec::new(),
                    },
                ),
            };
            let class = &mut self.style_classes.items[at];
            for s in style {
                if s.contains("color") {
                    class.text_styles.push(s.replacen("fill", "bgFill", 1));
                }
                class.styles.push(s.clone());
            }
            let Some(tagged) = self.with_css_class.get(id) else {
                continue;
            };
            let parts: Vec<&str> = style.iter().flat_map(|s| s.split(',')).collect();
            for &node in tagged {
                let styles = &mut self.classes.items[node].styles;
                styles.extend(parts.iter().map(|p| p.to_string()));
            }
        }
    }

    /// Sets the tooltip of each class of a list of ids, as Pele's setTooltip does.
    pub(crate) fn set_tooltip(&mut self, ids: &str, tooltip: &str) {
        for id in ids.split(',') {
            if let Some(node) = self.classes.get_mut(class_name(id)) {
                node.tooltip = Some(tooltip.to_string());
            }
        }
    }

    /// Makes each class of a list of ids a link, as Pele's setLink does. Entity codes are
    /// resolved first, so that what is checked is what a browser would follow. A target not
    /// given is _blank.
    pub(crate) fn set_link(&mut self, ids: &str, link: &str, target: Option<&str>) {
        let url = decode_entities(link);
        let url = trim(&url);
        for id in ids.split(',') {
            if let Some(node) = self.classes.get_mut(&lookup_id(id)) {
                node.link = (!url.is_empty()).then(|| sanitize_url(url));
                node.link_target = Some(target.unwrap_or("_blank").to_string());
            }
        }
        self.set_css_class(ids, "clickable");
    }

    /// Marks each class of a list of ids as having a callback, as Pele's setClickEvent does.
    /// Callbacks are never run. The class is only marked clickable, as in Mermaid's strict
    /// mode.
    pub(crate) fn set_click_event(&mut self, ids: &str) {
        for id in ids.split(',') {
            if let Some(node) = self.classes.get_mut(class_name(id)) {
                node.have_callback = true;
            }
        }
        self.set_css_class(ids, "clickable");
    }

    pub(crate) fn set_direction(&mut self, dir: &str) {
        self.direction = dir.to_string();
    }

    /// Sets the title for assistive technology, as Pele's setAccTitle does.
    pub(crate) fn set_acc_title(&mut self, text: &str) {
        self.acc_title = Some(trim_start(text).to_string());
    }

    /// Sets the description for assistive technology, as Pele's setAccDescription does, with
    /// the white space at the start of each of its lines taken out.
    pub(crate) fn set_acc_description(&mut self, text: &str) {
        self.acc_descr = Some(drop_indents(text));
    }

    /// Opens a namespace in the one open, and its ancestors where its name is dotted, and
    /// gives its full name, as Pele's addNamespace does.
    pub(crate) fn add_namespace(&mut self, id: &str, label: Option<&str>) -> String {
        let label = label.filter(|l| !l.is_empty());
        let prefix = self.namespace_stack.last().cloned();
        let qualified = match &prefix {
            Some(p) => format!("{p}.{id}"),
            None => id.to_string(),
        };
        self.namespace_stack.push(qualified.clone());

        if let Some(existing) = self.namespaces.get_mut(&qualified) {
            existing.explicit = true;
            if let Some(label) = label {
                existing.label = label.to_string();
            }
            return qualified;
        }

        // The enclosing namespace and its ancestors exist already, so only the parts of id
        // are walked.
        let mut parent_id = prefix.clone();
        let mut from = prefix.map_or(0, |p| p.len() + 1);
        loop {
            let dot = qualified[from..].find('.').map(|d| from + d);
            let leaf = dot.is_none();
            let current = &qualified[..dot.unwrap_or(qualified.len())];
            match self.namespaces.get_mut(current) {
                None => {
                    let part = &qualified[from..dot.unwrap_or(qualified.len())];
                    let node = NamespaceNode {
                        id: current.to_string(),
                        label: match label {
                            Some(l) if leaf => l.to_string(),
                            _ => part.to_string(),
                        },
                        classes: Vec::new(),
                        notes: Vec::new(),
                        children: Vec::new(),
                        parent: parent_id.clone(),
                        explicit: leaf,
                    };
                    self.namespaces.add(current, node);
                }
                Some(node) if leaf => node.explicit = true,
                Some(_) => {}
            }
            if let Some(p) = &parent_id
                && let Some(parent) = self.namespaces.get_mut(p)
            {
                set_key(&mut parent.children, current);
                let node = self
                    .namespaces
                    .get_mut(current)
                    .expect("a namespace just set");
                if node.parent.is_none() {
                    node.parent = parent_id.clone();
                }
            }
            let Some(dot) = dot else {
                return qualified;
            };
            parent_id = Some(current.to_string());
            from = dot + 1;
        }
    }

    /// Closes the namespace open, as Pele's popNamespace does.
    pub(crate) fn pop_namespace(&mut self) {
        self.namespace_stack.pop();
    }

    /// Puts classes and notes, by name, in a namespace, as Pele's addClassesToNamespace does.
    pub(crate) fn add_classes_to_namespace(
        &mut self,
        id: &str,
        class_names: &[String],
        note_names: &[String],
    ) {
        let Some(at) = self.namespaces.index(id) else {
            return;
        };
        for name in class_names {
            let name = class_name(name);
            let Some(node) = self.classes.get_mut(name) else {
                continue;
            };
            node.parent = Some(id.to_string());
            set_key(&mut self.namespaces.items[at].classes, name);
        }
        for name in note_names {
            let Some(note) = self.notes.get_mut(name) else {
                continue;
            };
            note.parent = Some(id.to_string());
            set_key(&mut self.namespaces.items[at].notes, name);
        }
    }

    /// Adds style declarations to a class, by its id as written, as Pele's setCssStyle does.
    pub(crate) fn set_css_style(&mut self, id: &str, styles: &[String]) {
        let Some(node) = self.classes.get_mut(id) else {
            return;
        };
        for s in styles {
            node.styles.extend(s.split(',').map(str::to_string));
        }
    }
}
