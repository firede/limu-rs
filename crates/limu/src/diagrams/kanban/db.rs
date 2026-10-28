//! What a kanban board's parser fills, as Pele's KanbanDb in db.ts has it: its columns, each
//! with its cards.

use crate::common::outline::{Decoration, OutlineDb};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::to_lower;
use crate::value::Value;
use crate::yaml::parse_yaml;

/// A column or a card, Pele's KanbanNode. A field Pele leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct KanbanNode {
    pub(crate) id: String,
    /// The indentation of its line, in UTF-16 code units.
    pub(crate) level: usize,
    pub(crate) label: String,
    pub(crate) icon: Option<String>,
    pub(crate) assigned: Option<String>,
    pub(crate) ticket: Option<String>,
    pub(crate) priority: Option<String>,
    /// The classes its author gave it, as written after `:::`.
    pub(crate) css_classes: Option<String>,
}

/// A column and its cards, Pele's KanbanSection.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct KanbanSection {
    pub(crate) node: KanbanNode,
    pub(crate) items: Vec<KanbanNode>,
}

/// The node a decoration goes to: a column, or a card of one.
#[derive(Clone, Copy, Debug)]
enum Last {
    Section(usize),
    Item(usize, usize),
}

/// A kanban board as its parser fills it, Pele's KanbanDb.
#[derive(Clone, Debug, Default)]
pub(crate) struct KanbanDb {
    pub(crate) title: Option<String>,
    pub(crate) sections: Vec<KanbanSection>,
    last: Option<Last>,
    section_level: usize,
    /// The label of a card indented less than the first column.
    misplaced: Option<String>,
}

fn kanban_error(message: String, code: ErrorCode) -> Error {
    let mut e = Error::new(message, code);
    e.diagram = Some(DiagramType::Kanban);
    e
}

impl OutlineDb for KanbanDb {
    /// Adds a column, or a card to the last column, as Pele's addNode does.
    fn add_node(
        &mut self,
        level: usize,
        id: &str,
        descr: &str,
        _kind: usize,
        shape_data: Option<&str>,
    ) -> Result<(), Error> {
        let mut node = KanbanNode {
            id: id.to_string(),
            level,
            label: descr.to_string(),
            ..KanbanNode::default()
        };

        if let Some(data) = shape_data {
            let yaml = if data.contains('\n') {
                format!("{data}\n")
            } else {
                format!("{{\n{data}\n}}")
            };
            let doc = parse_yaml(&yaml).map_err(|e| {
                kanban_error(
                    format!("Metadata of \"{}\" is not valid YAML. {}", node.id, e.0),
                    ErrorCode::Syntax,
                )
            })?;
            // Mermaid throws a TypeError on metadata that is empty or not a mapping. Here it
            // is ignored.
            if let Value::Object(doc) = doc {
                let field =
                    |key: &str| doc.get(key).filter(|v| v.truthy()).map(Value::to_js_string);
                let shape = field("shape").unwrap_or_default();
                if shape != to_lower(&shape) || shape.contains('_') {
                    return Err(kanban_error(
                        format!("No such shape: {shape}. Shape names should be lowercase."),
                        ErrorCode::Semantic,
                    ));
                }
                if let Some(label) = field("label") {
                    node.label = label;
                }
                node.icon = field("icon");
                node.assigned = field("assigned");
                node.ticket = field("ticket");
                node.priority = field("priority");
            }
        }

        // A node indented less than the first column becomes a card, and Mermaid rejects
        // whatever follows it.
        if let Some(label) = &self.misplaced {
            return Err(kanban_error(
                format!("Items without section detected, found section (\"{label}\")"),
                ErrorCode::Semantic,
            ));
        }
        if self.sections.is_empty() {
            self.section_level = level;
        }
        let s = self.sections.len();
        if level == self.section_level {
            self.last = Some(Last::Section(s));
            self.sections.push(KanbanSection {
                node,
                items: Vec::new(),
            });
        } else {
            if level < self.section_level {
                self.misplaced = Some(node.label.clone());
            }
            let items = &mut self.sections[s - 1].items;
            self.last = Some(Last::Item(s - 1, items.len()));
            items.push(node);
        }
        Ok(())
    }

    /// Gives the last node an icon or classes, as Pele's decorateNode does. Mermaid throws a
    /// TypeError when a decoration comes before any node. Here it is ignored.
    fn decorate_node(&mut self, decoration: Decoration) {
        let node = match self.last {
            Some(Last::Section(s)) => &mut self.sections[s].node,
            Some(Last::Item(s, i)) => &mut self.sections[s].items[i],
            None => return,
        };
        match decoration {
            Decoration::Icon(icon) if !icon.is_empty() => node.icon = Some(icon.to_string()),
            Decoration::Class(class) if !class.is_empty() => {
                node.css_classes = Some(class.to_string());
            }
            _ => {}
        }
    }
}
