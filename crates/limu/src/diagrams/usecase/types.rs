//! The model of a use case diagram, as Pele's types.ts has it. Each map of Pele's model is a
//! list here, in the order the map iterates.

use crate::value::Map;

/// How a label's text is read, Pele's LabelType: as text, or as markdown, for a label written
/// in backquotes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LabelType {
    #[default]
    Text,
    Markdown,
}

impl LabelType {
    #[cfg(test)]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            LabelType::Text => "text",
            LabelType::Markdown => "markdown",
        }
    }
}

/// Where a part of a statement is in the source: the byte it starts at and the byte after its
/// end. Pele counts them in UTF-16 code units.
pub(crate) type Span = [usize; 2];

/// An actor, Pele's Actor. Its kind is "normal", "hollow", "awesome" or "icon".
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Actor {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) label_type: LabelType,
    pub(crate) kind: &'static str,
    pub(crate) icon: Option<String>,
    pub(crate) business: bool,
    pub(crate) stereotype: Option<String>,
    pub(crate) parent_id: Option<String>,
    pub(crate) classes: Vec<String>,
    pub(crate) styles: Vec<String>,
}

/// A use case, Pele's UseCase. Its shape is "ellipse" or "rect".
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UseCase {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) label_type: LabelType,
    pub(crate) shape: &'static str,
    pub(crate) business: bool,
    pub(crate) stereotype: Option<String>,
    pub(crate) parent_id: Option<String>,
    pub(crate) classes: Vec<String>,
    pub(crate) styles: Vec<String>,
}

/// A system boundary, Pele's SystemBoundary. Its kind is "rect" or "package".
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SystemBoundary {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) label_type: LabelType,
    pub(crate) kind: &'static str,
    pub(crate) members: Vec<String>,
    pub(crate) classes: Vec<String>,
    pub(crate) styles: Vec<String>,
}

// The arrow types of an association, Pele's Arrow.
pub(crate) const SOLID_ARROW: usize = 0;
pub(crate) const BACK_ARROW: usize = 1;
pub(crate) const LINE_SOLID: usize = 2;
pub(crate) const CIRCLE_ARROW: usize = 3;
pub(crate) const CROSS_ARROW: usize = 4;
pub(crate) const CIRCLE_ARROW_REVERSED: usize = 5;
pub(crate) const CROSS_ARROW_REVERSED: usize = 6;

/// A relationship between two elements, Pele's Relationship. Its kind is "association",
/// "include", "extend" or "generalization", and its animation "fast" or "slow".
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Relationship {
    pub(crate) id: String,
    pub(crate) explicit_id: bool,
    pub(crate) source: String,
    pub(crate) target: String,
    pub(crate) kind: &'static str,
    pub(crate) arrow_type: usize,
    /// The label and how it is read, where the relationship has one.
    pub(crate) label: Option<(String, LabelType)>,
    pub(crate) minlen: f64,
    pub(crate) classes: Vec<String>,
    pub(crate) styles: Vec<String>,
    pub(crate) animate: bool,
    pub(crate) animation: Option<&'static str>,
}

/// A note about an actor or a use case, Pele's UsecaseNote.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UsecaseNote {
    pub(crate) id: String,
    pub(crate) target: String,
    pub(crate) label: String,
    pub(crate) label_type: LabelType,
}

/// A table of JSON, Pele's UsecaseJsonNode: the object JSON.parse gives, and the keys of each
/// object in it in the order they were written, by the object's JSON Pointer, in the order
/// Pele's Map iterates them.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UsecaseJsonNode {
    pub(crate) id: String,
    pub(crate) value: Map,
    pub(crate) property_order: Vec<(String, Vec<String>)>,
    pub(crate) classes: Vec<String>,
    pub(crate) styles: Vec<String>,
}

/// A class defined with classDef, Pele's ClassDef.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClassDef {
    pub(crate) id: String,
    pub(crate) styles: Vec<String>,
}

/// Where a property of a metadata block is, Pele's MetadataOccurrence.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MetadataOccurrence {
    pub(crate) key: String,
    pub(crate) span: Span,
    pub(crate) key_span: Span,
    pub(crate) value_span: Span,
}

/// Where an element is mentioned in a statement, Pele's NodeOccurrence. What Pele leaves
/// undefined is none.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NodeOccurrence {
    pub(crate) id: String,
    pub(crate) span: Span,
    pub(crate) id_span: Span,
    pub(crate) label_span: Option<Span>,
    pub(crate) defines: bool,
    pub(crate) stereotype_span: Option<Span>,
    pub(crate) metadata: Option<Vec<MetadataOccurrence>>,
    pub(crate) class_spans: Option<Vec<Span>>,
}

/// Where a relationship is in a statement, Pele's EdgeOccurrence.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EdgeOccurrence {
    pub(crate) id: String,
    pub(crate) span: Span,
    pub(crate) label_span: Option<Span>,
    pub(crate) id_span: Option<Span>,
    pub(crate) metadata: Option<Vec<MetadataOccurrence>>,
}

/// One statement of the source with the positions of its parts, as in Mermaid's use case
/// AST: Pele's GraphStatement. Its kind is "node", "edge", "group", "note", "json",
/// "metadata", "edgeMetadata", "classDef", "classAssign", "style", "direction", "accTitle",
/// "accDescr", "comment" or "blank". What Pele leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct GraphStatement {
    pub(crate) kind: &'static str,
    pub(crate) span: Span,
    pub(crate) nodes: Option<Vec<NodeOccurrence>>,
    pub(crate) edges: Option<Vec<EdgeOccurrence>>,
    pub(crate) group: Option<String>,
    pub(crate) id_span: Option<Span>,
    pub(crate) title_span: Option<Span>,
    pub(crate) end_span: Option<Span>,
    pub(crate) reference: Option<String>,
    pub(crate) ref_span: Option<Span>,
    pub(crate) children: Option<Vec<GraphStatement>>,
    pub(crate) metadata: Option<Vec<MetadataOccurrence>>,
    pub(crate) class_spans: Option<Vec<Span>>,
}

impl GraphStatement {
    pub(crate) fn new(kind: &'static str) -> GraphStatement {
        GraphStatement {
            kind,
            ..GraphStatement::default()
        }
    }
}

/// A parsed use case diagram, Pele's UsecaseModel. Its direction is "TB", "BT", "LR" or
/// "RL".
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UsecaseModel {
    pub(crate) actors: Vec<Actor>,
    pub(crate) use_cases: Vec<UseCase>,
    pub(crate) system_boundaries: Vec<SystemBoundary>,
    pub(crate) relationships: Vec<Relationship>,
    pub(crate) notes: Vec<UsecaseNote>,
    pub(crate) json_nodes: Vec<UsecaseJsonNode>,
    pub(crate) class_defs: Vec<ClassDef>,
    pub(crate) direction: &'static str,
    pub(crate) title: Option<String>,
    pub(crate) acc_title: String,
    pub(crate) acc_descr: String,
    /// The accessible description as written, before the indentation of its lines is
    /// dropped.
    pub(crate) acc_description: String,
    pub(crate) source: String,
    pub(crate) header_span: Span,
    pub(crate) statements: Vec<GraphStatement>,
}
