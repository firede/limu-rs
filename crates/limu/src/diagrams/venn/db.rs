//! What a venn diagram's parser fills, as Pele's VennDb in db.ts has it: its sets and their
//! overlaps, the texts written in them, and their styles.

use std::collections::HashSet;

use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::trim;

/// A set (one id) or an overlap of sets (two or more ids, sorted), with its relative area,
/// Pele's VennSubset. A label Pele leaves undefined is none.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VennSubset {
    pub(crate) sets: Vec<String>,
    pub(crate) size: f64,
    pub(crate) label: Option<String>,
}

/// A line of text shown inside the region of the given sets, Pele's VennText.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VennText {
    pub(crate) sets: Vec<String>,
    pub(crate) id: String,
    pub(crate) label: Option<String>,
}

/// A style statement, Pele's VennStyle. Its targets are the sorted ids of a set or an
/// overlap, or the id of a text, and its declarations each property once, in the order the
/// properties first came, with the last value given it, as a Map keeps them.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VennStyle {
    pub(crate) targets: Vec<String>,
    pub(crate) styles: Vec<(String, String)>,
}

/// A venn diagram as its parser fills it, Pele's VennDb.
#[derive(Clone, Debug, Default)]
pub(crate) struct VennDb {
    pub(crate) title: Option<String>,
    pub(crate) subsets: Vec<VennSubset>,
    pub(crate) text_nodes: Vec<VennText>,
    pub(crate) styles: Vec<VennStyle>,

    known: HashSet<String>,
    current: Option<Vec<String>>,
    /// Whether an indented `text` line belongs to the set or union before it. The lexer
    /// reads it.
    indent: bool,
}

/// What the lexer asks of the db, Pele's IndentHost: an indented `text` line is a token of
/// its own after a set or a union, and the parser is what knows which came before.
pub(crate) trait IndentHost {
    fn indent_mode(&self) -> bool;
    fn set_indent_mode(&mut self, enabled: bool);
}

impl IndentHost for VennDb {
    fn indent_mode(&self) -> bool {
        self.indent
    }

    fn set_indent_mode(&mut self, enabled: bool) {
        self.indent = enabled;
    }
}

/// Takes one pair of quotes off an id, label or style value, as Mermaid does and as Pele's
/// normalizeText does.
pub(crate) fn normalize_text(text: &str) -> String {
    let trimmed = trim(text);
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        return trimmed[1..trimmed.len() - 1].to_string();
    }
    trimmed.to_string()
}

/// Sorts strings as JavaScript's sort orders them, by their UTF-16 code units, which puts
/// characters above the Basic Multilingual Plane before those from U+E000 up.
pub(crate) fn sort16(ids: &mut [String]) {
    ids.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
}

/// The ids without their quotes, sorted, as Pele's normalizeIds gives them.
fn normalize_ids(ids: &[String]) -> Vec<String> {
    let mut out: Vec<String> = ids.iter().map(|id| normalize_text(id)).collect();
    sort16(&mut out);
    out
}

/// A label without its quotes, or none for one that is missing or empty, as Pele's
/// `label ? normalizeText(label) : undefined` gives it.
fn normalized(label: Option<&str>) -> Option<String> {
    label.filter(|l| !l.is_empty()).map(normalize_text)
}

/// An error of a venn diagram's meaning.
fn semantic(message: String) -> Error {
    let mut e = Error::new(message, ErrorCode::Semantic);
    e.diagram = Some(DiagramType::Venn);
    e
}

impl VennDb {
    /// An empty db, of the title its front matter gives, where it gives one that is not
    /// empty.
    pub(crate) fn new(title: Option<&str>) -> VennDb {
        VennDb {
            title: title.filter(|t| !t.is_empty()).map(str::to_string),
            ..VennDb::default()
        }
    }

    pub(crate) fn set_diagram_title(&mut self, title: &str) {
        self.title = Some(title.to_string());
    }

    /// Adds a set or an overlap, as Pele's addSubsetData does. A set without a size gets 10,
    /// and an overlap of n sets 10 / n².
    pub(crate) fn add_subset_data(
        &mut self,
        ids: &[String],
        label: Option<&str>,
        size: Option<f64>,
    ) {
        let sets = normalize_ids(ids);
        if sets.len() == 1 {
            self.known.insert(sets[0].clone());
        }
        self.current = Some(sets.clone());
        // V8 works out ids.length ** 2 as a product.
        let n = ids.len() as f64;
        self.subsets.push(VennSubset {
            sets,
            size: size.unwrap_or(10.0 / (n * n)),
            label: normalized(label),
        });
    }

    pub(crate) fn add_text_data(&mut self, ids: &[String], id: &str, label: Option<&str>) {
        self.text_nodes.push(VennText {
            sets: normalize_ids(ids),
            id: normalize_text(id),
            label: normalized(label),
        });
    }

    pub(crate) fn add_style_data(&mut self, ids: &[String], data: &[(String, String)]) {
        let mut styles: Vec<(String, String)> = Vec::new();
        for (key, value) in data {
            let value = if value.is_empty() {
                value.clone()
            } else {
                normalize_text(value)
            };
            set_declaration(&mut styles, key, value);
        }
        self.styles.push(VennStyle {
            targets: normalize_ids(ids),
            styles,
        });
    }

    pub(crate) fn validate_union_identifiers(&self, ids: &[String]) -> Result<(), Error> {
        let unknown: Vec<String> = ids
            .iter()
            .map(|id| normalize_text(id))
            .filter(|id| !self.known.contains(id))
            .collect();
        if unknown.is_empty() {
            return Ok(());
        }
        Err(semantic(format!(
            "unknown set identifier: {}",
            unknown.join(", ")
        )))
    }

    pub(crate) fn current_sets(&self) -> Option<&[String]> {
        self.current.as_deref()
    }
}

/// Sets a property in a list of declarations as Map's set does: in its place if it is there,
/// at the end if it is not.
pub(crate) fn set_declaration(list: &mut Vec<(String, String)>, property: &str, value: String) {
    match list.iter_mut().find(|(p, _)| p == property) {
        Some((_, v)) => *v = value,
        None => list.push((property.to_string(), value)),
    }
}

/// The error of a union of fewer than two sets.
pub(crate) fn union_of_one() -> Error {
    semantic("union requires multiple identifiers".into())
}

/// The error of an indented text before any set.
pub(crate) fn text_without_set() -> Error {
    semantic("text requires set".into())
}
