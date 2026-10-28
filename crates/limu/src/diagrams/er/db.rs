//! What an entity relationship diagram's parser fills, as Pele's ErDb in db.ts has it: its
//! entities, relationships, classes and subgraphs, as Mermaid's erDb keeps them.

use std::collections::{HashMap, HashSet};

use crate::js::chars::{drop_indents, trim, trim_start};

/// An attribute of an entity, Pele's ErAttribute: its type, its name, its keys (PK, FK, UK)
/// and its comment, the last two empty where none is written.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ErAttribute {
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) keys: Vec<String>,
    pub(crate) comment: String,
}

/// An entity, Pele's ErEntity.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ErEntity {
    /// "entity-<name>-<n>", n counting the entities before it.
    pub(crate) id: String,
    pub(crate) label: String,
    /// The name it is shown by, from `name[alias]`, or "".
    pub(crate) alias: String,
    pub(crate) attributes: Vec<ErAttribute>,
    /// Its classes, "default" first, separated by spaces.
    pub(crate) css_classes: String,
    pub(crate) css_styles: Vec<String>,
}

/// A relationship's ends and kind, Pele's RelSpec. card_b sits at entity A's end of the line
/// and card_a at entity B's, as in Mermaid. Each is a cardinality as Mermaid names it, as
/// "ZERO_OR_ONE", and the kind is "IDENTIFYING" or "NON_IDENTIFYING".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RelSpec {
    pub(crate) card_a: &'static str,
    pub(crate) rel_type: &'static str,
    pub(crate) card_b: &'static str,
}

/// A relationship, Pele's ErRelationship. entity_a and entity_b hold an entity's id, or a
/// subgraph's id when the name was a subgraph at the time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ErRelationship {
    pub(crate) entity_a: String,
    pub(crate) role_a: String,
    pub(crate) entity_b: String,
    pub(crate) rel_spec: RelSpec,
}

/// A class of styles, from classDef, Pele's ErClassDef.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ErClassDef {
    pub(crate) id: String,
    pub(crate) styles: Vec<String>,
    pub(crate) text_styles: Vec<String>,
}

/// A subgraph, Pele's ErSubgraph. Its label type is always Pele's "markdown", since the
/// parser gives addSubGraph no type.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ErSubgraph {
    pub(crate) id: String,
    pub(crate) nodes: Vec<String>,
    pub(crate) title: String,
    pub(crate) classes: Vec<String>,
    pub(crate) css_styles: Vec<String>,
    pub(crate) dir: Option<&'static str>,
}

/// What a subgraph's body lists, Pele's DocItem: a statement's text, or a direction.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DocItem {
    Text(String),
    Dir(&'static str),
}

/// An attribute as the parser reads it, Pele's ParsedAttribute: its keys and comment are
/// none where they are not written.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ParsedAttribute {
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) keys: Option<Vec<String>>,
    pub(crate) comment: Option<String>,
}

/// An entity relationship diagram as its parser fills it, Pele's ErDb.
#[derive(Clone, Debug)]
pub(crate) struct ErDb {
    pub(crate) direction: &'static str,
    /// The entities, in the order they were first named, as Pele's Map keeps them.
    pub(crate) entities: Vec<ErEntity>,
    pub(crate) relationships: Vec<ErRelationship>,
    /// The classes, in the order they were first defined.
    pub(crate) classes: Vec<ErClassDef>,
    pub(crate) subgraphs: Vec<ErSubgraph>,
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,

    /// The entities by their names and the classes by their ids, as indices.
    entity_at: HashMap<String, usize>,
    class_at: HashMap<String, usize>,
    /// The subgraphs by their ids, as indices into subgraphs. A later one of an id takes its
    /// place.
    subgraph_lookup: HashMap<String, usize>,
    claimed: HashSet<String>,
}

impl Default for ErDb {
    fn default() -> ErDb {
        ErDb {
            direction: "TB",
            entities: Vec::new(),
            relationships: Vec::new(),
            classes: Vec::new(),
            subgraphs: Vec::new(),
            title: None,
            acc_title: None,
            acc_descr: None,
            entity_at: HashMap::new(),
            class_at: HashMap::new(),
            subgraph_lookup: HashMap::new(),
            claimed: HashSet::new(),
        }
    }
}

impl ErDb {
    /// The entity of a name.
    pub(crate) fn entity(&self, name: &str) -> Option<&ErEntity> {
        self.entity_at.get(name).map(|&i| &self.entities[i])
    }

    /// The class of an id.
    pub(crate) fn class(&self, id: &str) -> Option<&ErClassDef> {
        self.class_at.get(id).map(|&i| &self.classes[i])
    }

    /// The entity of a name, added with an alias where there is none, or given the alias
    /// where it has none, as Pele's addEntity does. It gives the entity's index.
    pub(crate) fn add_entity(&mut self, name: &str, alias: &str) -> usize {
        if let Some(&i) = self.entity_at.get(name) {
            let entity = &mut self.entities[i];
            if entity.alias.is_empty() && !alias.is_empty() {
                entity.alias = alias.to_string();
            }
            return i;
        }
        let i = self.entities.len();
        self.entities.push(ErEntity {
            id: format!("entity-{name}-{i}"),
            label: name.to_string(),
            alias: alias.to_string(),
            attributes: Vec::new(),
            css_classes: "default".into(),
            css_styles: Vec::new(),
        });
        self.entity_at.insert(name.to_string(), i);
        i
    }

    /// Adds attributes to the entity of a name, as Pele's addAttributes does. The grammar
    /// builds the list back to front, so the last attribute written comes first.
    pub(crate) fn add_attributes(&mut self, name: &str, attributes: Vec<ParsedAttribute>) {
        let i = self.add_entity(name, "");
        for a in attributes.into_iter().rev() {
            self.entities[i].attributes.push(ErAttribute {
                kind: a.kind,
                name: a.name,
                keys: a.keys.unwrap_or_default(),
                comment: a.comment.unwrap_or_default(),
            });
        }
    }

    /// Adds a relationship, as Pele's addRelationship does: its ends are the ids of the
    /// entities named, or the names themselves where they are subgraphs.
    pub(crate) fn add_relationship(
        &mut self,
        ent_a: &str,
        role_a: &str,
        ent_b: &str,
        rel_spec: RelSpec,
    ) {
        let mut end = |name: &str| {
            if self.subgraph_lookup.contains_key(name) {
                name.to_string()
            } else {
                let i = self.add_entity(name, "");
                self.entities[i].id.clone()
            }
        };
        let entity_a = end(ent_a);
        let entity_b = end(ent_b);
        self.relationships.push(ErRelationship {
            entity_a,
            role_a: role_a.to_string(),
            entity_b,
            rel_spec,
        });
    }

    pub(crate) fn set_direction(&mut self, dir: &'static str) {
        self.direction = dir;
    }

    /// Adds styles to the entities and subgraphs of ids, as Pele's addCssStyles does.
    pub(crate) fn add_css_styles(&mut self, ids: &[String], styles: &[String]) {
        for id in ids {
            if let Some(&i) = self.entity_at.get(id.as_str()) {
                self.entities[i].css_styles.extend(styles.iter().cloned());
            }
            if let Some(&i) = self.subgraph_lookup.get(id.as_str()) {
                self.subgraphs[i].css_styles.extend(styles.iter().cloned());
            }
        }
    }

    /// Defines classes of styles, or adds to them, as Pele's addClass does. A style that
    /// sets a color is a style of text too, with its first fill read as bgFill.
    pub(crate) fn add_class(&mut self, ids: &[String], styles: &[String]) {
        for id in ids {
            let i = match self.class_at.get(id.as_str()) {
                Some(&i) => i,
                None => {
                    self.classes.push(ErClassDef {
                        id: id.clone(),
                        ..ErClassDef::default()
                    });
                    self.class_at.insert(id.clone(), self.classes.len() - 1);
                    self.classes.len() - 1
                }
            };
            let def = &mut self.classes[i];
            for style in styles {
                if style.contains("color") {
                    def.text_styles.push(style.replacen("fill", "bgFill", 1));
                }
                def.styles.push(style.clone());
            }
        }
    }

    /// Gives classes to the entities and subgraphs of ids, as Pele's setClass does.
    pub(crate) fn set_class(&mut self, ids: &[String], class_names: &[String]) {
        for id in ids {
            if let Some(&i) = self.entity_at.get(id.as_str()) {
                for name in class_names {
                    let classes = &mut self.entities[i].css_classes;
                    classes.push(' ');
                    classes.push_str(name);
                }
            }
            if let Some(&i) = self.subgraph_lookup.get(id.as_str()) {
                self.subgraphs[i]
                    .classes
                    .extend(class_names.iter().cloned());
            }
        }
    }

    /// Adds a subgraph of what its body lists, as Pele's addSubGraph does, and gives its id.
    /// A node belongs to the first subgraph that lists it.
    pub(crate) fn add_sub_graph(&mut self, id: &str, list: &[DocItem], title: &str) -> String {
        let id = trim(id).to_string();
        let mut seen = HashSet::new();
        let mut nodes: Vec<String> = Vec::new();
        let mut dir = None;
        for item in list {
            let item = match item {
                DocItem::Dir(d) => {
                    dir = Some(*d);
                    continue;
                }
                DocItem::Text(s) => s,
            };
            let trimmed = trim(item);
            if trimmed.is_empty() || !seen.insert(trimmed) {
                continue;
            }
            if !self.claimed.contains(item.as_str()) {
                nodes.push(item.clone());
            }
        }
        self.claimed.extend(nodes.iter().cloned());
        self.subgraphs.push(ErSubgraph {
            id: id.clone(),
            nodes,
            title: trim(title).to_string(),
            classes: Vec::new(),
            css_styles: Vec::new(),
            dir,
        });
        self.subgraph_lookup
            .insert(id.clone(), self.subgraphs.len() - 1);
        id
    }

    pub(crate) fn set_acc_title(&mut self, text: &str) {
        self.acc_title = Some(trim_start(text).to_string());
    }

    pub(crate) fn set_acc_description(&mut self, text: &str) {
        self.acc_descr = Some(drop_indents(text));
    }
}
