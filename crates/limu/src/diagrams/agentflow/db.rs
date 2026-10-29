//! What an agent flow's parser fills, as Pele's AgentflowDb in db.ts has it, with the types
//! of types.ts: its nodes, edges, flows, connectors, classes and tooltips, where each
//! statement sits in the text, and what is found wrong with it.

use std::collections::{HashMap, HashSet};

use super::diagnostics::DiagnosticId;
use super::graph::AgentGraph;
use super::shapes::{is_valid_shape, resolve_shape_alias};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode, locate};
use crate::js::chars::{drop_indents, space_run, to_lower, trim, trim_start};
use crate::js::json::stringify;
use crate::url::safe_url;
use crate::value::{Map, Value};
use crate::yaml::parse_yaml;

/// How a label's text is read, Pele's LabelType.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LabelType {
    #[default]
    Text,
    String,
    Markdown,
}

impl LabelType {
    #[cfg(test)]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            LabelType::Text => "text",
            LabelType::String => "string",
            LabelType::Markdown => "markdown",
        }
    }

    /// The type a value names, or markdown for any other, as Pele's labelType reads it.
    fn of(v: Option<&Value>) -> LabelType {
        match v.and_then(Value::as_str) {
            Some("text") => LabelType::Text,
            Some("string") => LabelType::String,
            _ => LabelType::Markdown,
        }
    }
}

/// A label as written, with how it is read, Pele's FlowText.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FlowText {
    pub(crate) text: String,
    pub(crate) kind: LabelType,
}

/// A place in the text as Jison reports it, Pele's Loc: the first line and column and the
/// last line and column. Lines start at 1, and columns at 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Loc {
    pub(crate) first_line: usize,
    pub(crate) first_column: usize,
    pub(crate) last_line: usize,
    pub(crate) last_column: usize,
}

/// A node, or a connector, Pele's AgentNode. What Pele leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct AgentNode {
    pub(crate) id: String,
    pub(crate) text: Option<String>,
    pub(crate) label_type: LabelType,
    /// The shape as the text gives it: a bracket's name, as "square", or the name of a shape
    /// the node's metadata gives.
    pub(crate) kind: Option<String>,
    pub(crate) styles: Vec<String>,
    pub(crate) classes: Vec<String>,
    pub(crate) dir: Option<String>,
    /// The properties `id[|name:value|label]` gives, in the order JavaScript gives an
    /// object's keys, or none for a connector no statement gave any.
    pub(crate) props: Option<Map>,
    pub(crate) link: Option<String>,
    pub(crate) link_target: Option<String>,
    pub(crate) metadata: Option<Map>,
    pub(crate) is_connector: bool,
}

/// An edge, Pele's AgentEdge.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct AgentEdge {
    pub(crate) start: String,
    pub(crate) end: String,
    pub(crate) kind: Option<String>,
    pub(crate) text: String,
    pub(crate) label_type: LabelType,
    pub(crate) stroke: Option<String>,
    pub(crate) edge_semantic: Option<&'static str>,
    /// The edge's length in ranks, at most 10, or none for an edge whose start and end do not
    /// agree.
    pub(crate) length: Option<f64>,
    /// What linkStyle sets, or none where it sets nothing.
    pub(crate) style: Option<Vec<String>>,
    pub(crate) classes: Vec<String>,
    pub(crate) id: String,
    pub(crate) is_user_defined_id: bool,
    /// The edge's curve: a name linkStyle gives, or whatever value the edge's metadata gives
    /// its curve. Animate and animation are what its metadata gives them.
    pub(crate) interpolate: Option<Value>,
    pub(crate) animate: Option<Value>,
    pub(crate) animation: Option<Value>,
    pub(crate) metadata: Option<Map>,
}

/// A class classDef defines, Pele's AgentClassDef.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct AgentClassDef {
    pub(crate) id: String,
    pub(crate) styles: Vec<String>,
    pub(crate) text_styles: Vec<String>,
}

/// A flow, Pele's AgentSubgraph. Its type is always "flow".
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct AgentSubgraph {
    pub(crate) id: String,
    pub(crate) nodes: Vec<String>,
    pub(crate) title: String,
    pub(crate) classes: Vec<String>,
    pub(crate) dir: Option<String>,
    pub(crate) label_type: LabelType,
    pub(crate) metadata: Option<Map>,
}

/// What a link's text says it is, Pele's LinkInfo.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct LinkInfo {
    pub(crate) kind: String,
    pub(crate) stroke: String,
    pub(crate) length: Option<f64>,
    pub(crate) edge_semantic: Option<&'static str>,
    pub(crate) text: Option<FlowText>,
    pub(crate) id: Option<String>,
}

/// What a flow's body lists, Pele's DocItem: a separator or a statement's text, the nodes of
/// a statement, or a direction.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DocItem {
    Text(String),
    Nodes(Vec<String>),
    Dir(&'static str),
}

/// Where a statement sits in the text, Pele's ElementPosition. Mermaid builds its parser
/// without ranges, so the indices Pele gives are always 0, and are left out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ElementPosition {
    pub(crate) start_line: usize,
    pub(crate) start_column: usize,
    pub(crate) end_line: usize,
    pub(crate) end_column: usize,
}

/// What kind of statement a mapping is of, Pele's StatementType.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatementType {
    Vertex,
    Edge,
    Subgraph,
    Connector,
    Attachment,
}

impl StatementType {
    #[cfg(test)]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            StatementType::Vertex => "vertex",
            StatementType::Edge => "edge",
            StatementType::Subgraph => "subgraph",
            StatementType::Connector => "connector",
            StatementType::Attachment => "attachment",
        }
    }
}

/// Where a statement of an element sits, Pele's ElementMapping.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ElementMapping {
    pub(crate) id: String,
    pub(crate) kind: StatementType,
    pub(crate) position: ElementPosition,
}

/// How bad what a diagnostic finds is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Severity {
    Warning,
    Error,
}

impl Severity {
    #[cfg(test)]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

/// Something found wrong with a diagram that does not stop it being drawn, Pele's Diagnostic.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Diagnostic {
    pub(crate) id: DiagnosticId,
    pub(crate) severity: Severity,
    pub(crate) message: String,
    pub(crate) node_id: Option<String>,
    pub(crate) edge_id: Option<String>,
    pub(crate) position: Option<ElementPosition>,
}

/// A node of the semantic model, Pele's SemanticVertex.
#[derive(Clone, Debug, PartialEq)]
#[cfg(test)]
pub(crate) struct SemanticVertex {
    pub(crate) id: String,
    pub(crate) label: Option<String>,
    pub(crate) shape: Option<String>,
    pub(crate) vertex_kind: &'static str,
    pub(crate) metadata: Option<Map>,
}

/// An edge of the semantic model, Pele's SemanticEdge.
#[derive(Clone, Debug, PartialEq)]
#[cfg(test)]
pub(crate) struct SemanticEdge {
    pub(crate) start: String,
    pub(crate) end: String,
    pub(crate) id: String,
    pub(crate) label: Option<String>,
    pub(crate) kind: Option<String>,
    pub(crate) stroke: Option<String>,
    pub(crate) edge_semantic: Option<&'static str>,
    pub(crate) length: Option<f64>,
    pub(crate) metadata: Option<Map>,
}

/// A flow of the semantic model, Pele's SemanticSubgraph.
#[derive(Clone, Debug, PartialEq)]
#[cfg(test)]
pub(crate) struct SemanticSubgraph {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) nodes: Vec<String>,
    pub(crate) metadata: Option<Map>,
    pub(crate) direction: Option<String>,
}

/// A connector of the semantic model, Pele's SemanticConnector.
#[derive(Clone, Debug, PartialEq)]
#[cfg(test)]
pub(crate) struct SemanticConnector {
    pub(crate) id: String,
    pub(crate) title: Option<String>,
    pub(crate) metadata: Option<Map>,
}

/// The diagram with what only says how it is drawn left out, for tools that read what it
/// means, Pele's SemanticModel.
#[derive(Clone, Debug, PartialEq)]
#[cfg(test)]
pub(crate) struct SemanticModel<'a> {
    pub(crate) direction: Option<&'a str>,
    pub(crate) vertices: Vec<SemanticVertex>,
    pub(crate) edges: Vec<SemanticEdge>,
    pub(crate) sub_graphs: Vec<SemanticSubgraph>,
    pub(crate) connectors: Vec<SemanticConnector>,
    pub(crate) diagnostics: &'a [Diagnostic],
}

/// What an AgentflowDb is made with, Pele's AgentflowDbOptions.
#[derive(Clone, Debug, Default)]
pub(crate) struct AgentflowDbOptions {
    /// The most edges, or none for no limit.
    pub(crate) max_edges: Option<usize>,
    pub(crate) inherit_dir: bool,
    /// The direction the configuration gives, which a flow takes where it inherits one and
    /// the diagram gives none.
    pub(crate) direction: Option<String>,
    /// The lines of front matter before the text the parser sees, added to every line
    /// reported.
    pub(crate) line_offset: usize,
    /// The text the parser sees, where errors are located.
    pub(crate) source: Option<String>,
}

/// Items kept in the order they were added, and found by their ids, as a JavaScript Map
/// keeps them.
#[derive(Clone, Debug)]
pub(crate) struct Ordered<T> {
    items: Vec<T>,
    at: HashMap<String, usize>,
}

impl<T> Default for Ordered<T> {
    fn default() -> Self {
        Ordered {
            items: Vec::new(),
            at: HashMap::new(),
        }
    }
}

impl<T> Ordered<T> {
    pub(crate) fn get(&self, id: &str) -> Option<&T> {
        self.at.get(id).map(|&i| &self.items[i])
    }

    fn get_mut(&mut self, id: &str) -> Option<&mut T> {
        self.at.get(id).map(|&i| &mut self.items[i])
    }

    pub(crate) fn has(&self, id: &str) -> bool {
        self.at.contains_key(id)
    }

    /// Sets the item of an id, in its place where it has one and last where it does not.
    fn set(&mut self, id: &str, item: T) {
        match self.at.get(id) {
            Some(&i) => self.items[i] = item,
            None => {
                self.at.insert(id.to_string(), self.items.len());
                self.items.push(item);
            }
        }
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &T> {
        self.items.iter()
    }
}

/// An agent flow as its parser fills it, Pele's AgentflowDb, and the graph that is drawn of
/// it, which Pele keeps beside it.
#[derive(Clone, Debug, Default)]
pub(crate) struct AgentflowDb {
    pub(crate) direction: Option<String>,
    pub(crate) nodes: Ordered<AgentNode>,
    pub(crate) edges: Vec<AgentEdge>,
    pub(crate) subgraphs: Vec<AgentSubgraph>,
    /// The ids of the connectors, which are nodes, in the order they were added. A
    /// connector is the node of its id, as Pele's connectors and nodes hold the one object.
    pub(crate) connectors: Vec<String>,
    pub(crate) classes: Ordered<AgentClassDef>,
    /// The tooltips, by the ids they are for, as (id, text).
    pub(crate) tooltips: Ordered<(String, String)>,
    pub(crate) default_edge_style: Option<Vec<String>>,
    pub(crate) default_interpolate: Option<String>,
    /// Where each statement sits in the text, in the order the statements were read.
    pub(crate) mappings: Vec<ElementMapping>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) line_offset: usize,
    /// The graph that is drawn, once it is built.
    pub(crate) graph: Option<AgentGraph>,

    /// The flows by their ids, as indices into subgraphs.
    subgraph_lookup: HashMap<String, usize>,
    connector_set: HashSet<String>,
    global_nodes: HashSet<String>,
    members: HashSet<String>,
    /// The first edge of each id, as an index into edges.
    edge_by_id: HashMap<String, usize>,
    pair_count: HashMap<String, usize>,
    sub_count: usize,
    /// The mappings of bare references, which become attachments where metadata follows
    /// them, as indices into mappings.
    bare_mappings: HashSet<usize>,
    first_mapping: HashMap<String, usize>,
    /// The diagnostics reported of each id, as indices into diagnostics.
    reported: HashMap<String, Vec<usize>>,
    options: AgentflowDbOptions,
}

/// The shapes Pele takes a node for a tool by, Pele's TOOL_SHAPES.
const TOOL_SHAPES: [&str; 5] = [
    "subroutine",
    "subprocess",
    "subproc",
    "framed-rectangle",
    "tool",
];

/// The keys of metadata that say only how a thing is drawn, left out of the semantic model,
/// Pele's PRESENTATION_KEYS.
#[cfg(test)]
const PRESENTATION_KEYS: [&str; 11] = [
    "shape",
    "view",
    "icon",
    "img",
    "form",
    "pos",
    "w",
    "h",
    "class",
    "style",
    "labelType",
];

/// Whether a node is a tool's definition, as Pele's isToolDefinition says.
pub(crate) fn is_tool_definition(vertex: &AgentNode) -> bool {
    vertex
        .kind
        .as_deref()
        .is_some_and(|k| TOOL_SHAPES.contains(&k))
}

fn agentflow_error(message: String, code: ErrorCode) -> Error {
    let mut e = Error::new(message, code);
    e.diagram = Some(DiagramType::AgentFlow);
    e
}

/// text without the double quotes about it, as Pele's unquote takes them off. A text that
/// is one double quote keeps it, as substring(1, 0) is substring(0, 1).
fn unquote(text: &str) -> &str {
    if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
        &text[1..text.len() - 1]
    } else {
        text
    }
}

/// A value read from YAML without its `__proto__`, `constructor` and `prototype` keys, at
/// every level, as Pele's stripPrototypeKeys copies it, so that a reader merging metadata
/// into its own objects cannot be made to pollute a prototype.
fn strip_prototype_keys(v: &Value) -> Value {
    match v {
        Value::Array(items) => Value::Array(items.iter().map(strip_prototype_keys).collect()),
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(k, _)| !matches!(*k, "__proto__" | "constructor" | "prototype"))
                .map(|(k, v)| (k, strip_prototype_keys(v)))
                .collect(),
        ),
        _ => v.clone(),
    }
}

/// Adds what a metadata block holds to an element's metadata, as Pele's merge does with
/// Object.assign. Mermaid spreads the parsed YAML whatever it is, so a list adds its indexes
/// as keys and a string its code units. A code unit that is half of a surrogate pair, which a
/// Rust string cannot hold alone, is U+FFFD, as JSON reads one back.
fn merge(into: Option<Map>, doc: &Value) -> Map {
    let mut into = into.unwrap_or_default();
    match doc {
        Value::Object(m) => {
            for (k, v) in m {
                into.insert(k, v.clone());
            }
        }
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                into.insert(i.to_string(), v.clone());
            }
        }
        Value::String(s) => {
            let mut i = 0;
            for c in s.chars() {
                if c.len_utf16() == 2 {
                    into.insert(i.to_string(), Value::String("\u{fffd}".into()));
                    into.insert((i + 1).to_string(), Value::String("\u{fffd}".into()));
                    i += 2;
                } else {
                    into.insert(i.to_string(), Value::String(c.to_string()));
                    i += 1;
                }
            }
        }
        _ => {}
    }
    into
}

/// The number of code units of white space a line starts with, Pele's
/// line.length - line.trimStart().length.
fn indent_of(line: &str) -> usize {
    line.len() - trim_start(line).len()
}

/// Whether code ends with the header of a block scalar, Pele's RE_BLOCK_SCALAR,
/// /:\s*[>|][\d+-]*\s*$/.
fn block_scalar(code: &str) -> bool {
    let b = code.as_bytes();
    (0..b.len()).filter(|&i| b[i] == b':').any(|i| {
        let j = space_run(code, i + 1);
        if !matches!(b.get(j), Some(b'>' | b'|')) {
            return false;
        }
        let k = j
            + 1
            + b[j + 1..]
                .iter()
                .take_while(|&&c| c.is_ascii_digit() || c == b'+' || c == b'-')
                .count();
        space_run(code, k) == b.len()
    })
}

/// Removes the comma that ends a line of a metadata body of more than one line, which block
/// YAML does not allow, as Pele's stripLineTrailingCommas does. Commas inside quoted scalars,
/// flow collections and block scalars are content and stay.
fn strip_line_trailing_commas(body: &str) -> String {
    let (mut in_single, mut in_double) = (false, false);
    let mut flow_depth = 0usize;
    let mut block_indent: Option<usize> = None;
    let mut out: Vec<String> = Vec::new();
    for line in body.split('\n') {
        if let Some(indent) = block_indent {
            if trim(line).is_empty() || indent_of(line) > indent {
                out.push(line.to_string());
                continue;
            }
            block_indent = None;
        }
        // The characters looked for are ASCII, so the bytes stand for the code units.
        let b = line.as_bytes();
        let mut comment_start = None;
        let mut i = 0;
        while i < b.len() {
            let ch = b[i];
            if in_double {
                if ch == b'\\' {
                    i += 1;
                } else if ch == b'"' {
                    in_double = false;
                }
            } else if in_single {
                if ch == b'\'' {
                    in_single = false;
                }
            } else if ch == b'"' {
                in_double = true;
            } else if ch == b'\'' {
                in_single = true;
            } else if ch == b'#' && (i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\t') {
                comment_start = Some(i);
                break;
            } else if ch == b'[' || ch == b'{' {
                flow_depth += 1;
            } else if ch == b']' || ch == b'}' {
                flow_depth = flow_depth.saturating_sub(1);
            }
            i += 1;
        }
        if in_single || in_double || flow_depth > 0 {
            out.push(line.to_string());
            continue;
        }
        let (code, comment) = line.split_at(comment_start.unwrap_or(line.len()));
        if block_scalar(code) {
            block_indent = Some(indent_of(line));
            out.push(line.to_string());
            continue;
        }
        // code.replace(/,([\t ]*)$/, '$1')
        let kept = code.trim_end_matches([' ', '\t']);
        let code = match kept.strip_suffix(',') {
            Some(head) => format!("{head}{}", &code[kept.len()..]),
            None => code.to_string(),
        };
        out.push(code + comment);
    }
    out.join("\n")
}

/// What an arrow of a type and stroke means, as Pele's edgeSemantic says.
fn edge_semantic(kind: &str, stroke: &str) -> Option<&'static str> {
    match (kind, stroke) {
        ("arrow_point", "normal") => Some("sequence"),
        ("arrow_cross", "normal") => Some("failure"),
        ("arrow_open", "dotted") => Some("reference"),
        _ => None,
    }
}

/// What a link, or the end of one with a label in it, says, as Pele's destructEndLink reads
/// it.
fn destruct_end_link(raw: &str) -> LinkInfo {
    let s = trim(raw);
    // The links are ASCII, as their lexer reads them.
    let mut line = &s[..s.len().saturating_sub(1)];
    let kind = match s.as_bytes().last() {
        Some(b'x') => "arrow_cross",
        Some(b'>') => "arrow_point",
        Some(b'-' | b'.') => {
            line = s;
            "arrow_open"
        }
        _ => "arrow_open",
    };
    let mut stroke = "normal";
    let mut length = line.len() as f64 - 1.0;
    let dots = line.bytes().filter(|&c| c == b'.').count();
    if dots > 0 {
        stroke = "dotted";
        length = dots as f64;
    }
    LinkInfo {
        kind: kind.to_string(),
        stroke: stroke.to_string(),
        length: Some(length),
        ..LinkInfo::default()
    }
}

/// Metadata without the keys that only say how a thing is drawn, or none where nothing is
/// left, as Pele's semanticMetadata gives it.
#[cfg(test)]
fn semantic_metadata(metadata: Option<&Map>) -> Option<Map> {
    let out: Map = metadata?
        .iter()
        .filter(|(k, _)| !PRESENTATION_KEYS.contains(k))
        .map(|(k, v)| (k, v.clone()))
        .collect();
    (!out.is_empty()).then_some(out)
}

/// The byte of s that is units code units on from byte from, or the end. A character above
/// the Basic Multilingual Plane that the count ends inside is stepped over whole.
fn units_on(s: &str, from: usize, units: usize) -> usize {
    let mut at = from;
    let mut left = units;
    for c in s[from..].chars() {
        if left == 0 {
            break;
        }
        left = left.saturating_sub(c.len_utf16());
        at += c.len_utf8();
    }
    at
}

impl AgentflowDb {
    pub(crate) fn new(options: AgentflowDbOptions) -> AgentflowDb {
        AgentflowDb {
            line_offset: options.line_offset,
            options,
            ..AgentflowDb::default()
        }
    }

    /// The node of a connector's id.
    pub(crate) fn connector(&self, id: &str) -> Option<&AgentNode> {
        if self.connector_set.contains(id) {
            self.nodes.get(id)
        } else {
            None
        }
    }

    /// Reads a metadata block, as Pele's metadata does: a block of one line as a flow
    /// mapping, and one of more lines as block YAML, without the commas that end its lines
    /// if it does not read with them.
    fn metadata(&self, id: &str, source: &str, loc: Option<Loc>) -> Result<Value, Error> {
        let multiline = source.contains('\n');
        let yaml = if multiline {
            format!("{source}\n")
        } else {
            format!("{{\n{source}\n}}")
        };
        let parsed = match parse_yaml(&yaml) {
            Ok(v) => v,
            Err(error) => {
                // Block YAML rejects the comma-separated style of the single-line form, so
                // try once more without the commas.
                let stripped = if multiline {
                    strip_line_trailing_commas(source)
                } else {
                    source.to_string()
                };
                let retried = if stripped != source {
                    parse_yaml(&format!("{stripped}\n")).ok()
                } else {
                    None
                };
                match retried {
                    Some(v) => v,
                    None => {
                        let (line, column) = self.error_at(loc);
                        let at = if loc.is_some() {
                            format!(" ({line}:{column})")
                        } else {
                            String::new()
                        };
                        let mut e = agentflow_error(
                            format!("Metadata of \"{id}\" is not valid YAML. {}{at}", error.0),
                            ErrorCode::Syntax,
                        );
                        (e.line, e.column) = (line, column);
                        return Err(e);
                    }
                }
            }
        };
        Ok(strip_prototype_keys(&match parsed {
            Value::Null => Value::Object(Map::new()),
            v => v,
        }))
    }

    /// The line and column of an error at loc, as Pele's errorAt gives them: located in the
    /// text the parser sees, so that they are those of the text as its author wrote it.
    /// Without that text, the line counts the lines of front matter before it.
    fn error_at(&self, loc: Option<Loc>) -> (usize, usize) {
        let Some(loc) = loc else { return (0, 0) };
        let Some(source) = &self.options.source else {
            return (loc.first_line + self.line_offset, loc.first_column + 1);
        };
        let mut start = 0;
        for _ in 1..loc.first_line {
            // indexOf's -1 for a line past the last starts over at 0.
            start = source[start..].find('\n').map_or(0, |i| start + i + 1);
        }
        let l = locate(source, units_on(source, start, loc.first_column));
        (l.line, l.column)
    }

    fn shape_error(&self, message: String, loc: Option<Loc>) -> Error {
        let mut e = agentflow_error(message, ErrorCode::Semantic);
        (e.line, e.column) = self.error_at(loc);
        e
    }

    /// Adds a node, or adds to one, as Pele's addVertex does. metadata is the text of the
    /// node's `@{ … }`, and metadata_loc where it is. Metadata on the id of a flow, a
    /// connector or an edge is theirs.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn add_vertex(
        &mut self,
        id: &str,
        text: Option<&FlowText>,
        kind: Option<&str>,
        style: Option<Vec<String>>,
        props: Option<Map>,
        metadata: Option<&str>,
        metadata_loc: Option<Loc>,
    ) -> Result<(), Error> {
        if trim(id).is_empty() {
            return Ok(());
        }

        let mut doc: Option<Value> = None;
        let mut authored_shape: Option<String> = None;
        if let Some(metadata) = metadata {
            let mut d = self.metadata(id, metadata, metadata_loc)?;
            if let Value::Object(fields) = &mut d
                && let Some(Value::String(shape)) = fields.get("shape").cloned()
            {
                let resolved = resolve_shape_alias(Some(&shape)).unwrap_or("").to_string();
                authored_shape = Some(shape);
                fields.insert("shape", Value::String(resolved));
            }
            doc = Some(d);
        }
        let truthy = doc.as_ref().is_some_and(Value::truthy);

        if let Some(&at) = self.subgraph_lookup.get(id)
            && truthy
        {
            let subgraph = &mut self.subgraphs[at];
            subgraph.metadata = Some(merge(subgraph.metadata.take(), doc.as_ref().unwrap()));
            return Ok(());
        }

        if self.connector_set.contains(id) && truthy {
            let connector = self.nodes.get_mut(id).expect("a connector is a node");
            connector.metadata = Some(merge(connector.metadata.take(), doc.as_ref().unwrap()));
            return Ok(());
        }

        // Mermaid reserves this id and creates no node for it.
        if id == "connectors" {
            return Ok(());
        }

        let fields = match &doc {
            Some(Value::Object(m)) => Some(m),
            _ => None,
        };

        if let Some(&at) = self.edge_by_id.get(id) {
            let edge = &mut self.edges[at];
            if let Some(fields) = fields {
                if let Some(v) = fields.get("animate") {
                    edge.animate = Some(v.clone());
                }
                if let Some(v) = fields.get("animation") {
                    edge.animation = Some(v.clone());
                }
                if let Some(v) = fields.get("curve") {
                    edge.interpolate = Some(v.clone());
                }
            }
            if let Some(doc) = doc.as_ref().filter(|d| d.truthy()) {
                edge.metadata = Some(merge(edge.metadata.take(), doc));
            }
            return Ok(());
        }

        if !self.nodes.has(id) {
            let vertex = AgentNode {
                id: id.to_string(),
                ..AgentNode::default()
            };
            self.nodes.set(id, vertex);
        }
        let vertex = self.nodes.get_mut(id).expect("added above");

        if let Some(text) = text {
            vertex.label_type = text.kind;
            vertex.text = Some(unquote(trim(&text.text)).to_string());
        } else if vertex.text.is_none() {
            vertex.text = Some(id.to_string());
        }
        if let Some(kind) = kind {
            vertex.kind = Some(kind.to_string());
        }
        if let Some(style) = style {
            vertex.styles.extend(style);
        }
        let props = props.unwrap_or_default();
        match &mut vertex.props {
            None => vertex.props = Some(props),
            Some(own) => {
                // Object.assign sets each property as an assignment does, and an assignment
                // to a __proto__ the object does not hold as its own sets its prototype
                // instead, which a string cannot be.
                for (k, v) in &props {
                    if k != "__proto__" || own.contains_key(k) {
                        own.insert(k, v.clone());
                    }
                }
            }
        }

        let Some(doc) = doc.filter(Value::truthy) else {
            return Ok(());
        };
        vertex.metadata = Some(merge(vertex.metadata.take(), &doc));
        let Value::Object(fields) = doc else {
            return Ok(());
        };

        if let Some(shape) = fields.get("shape").filter(|v| v.truthy()) {
            let authored = match &authored_shape {
                Some(s) => s.clone(),
                None => {
                    let message = format!("No such shape: {}.", stringify(shape));
                    return Err(self.shape_error(message, metadata_loc));
                }
            };
            if authored != to_lower(&authored) || authored.contains('_') {
                let message =
                    format!("No such shape: {authored}. Shape names should be lowercase.");
                return Err(self.shape_error(message, metadata_loc));
            }
            let shape = shape.to_js_string();
            if !is_valid_shape(&shape) {
                return Err(self.shape_error(format!("No such shape: {shape}."), metadata_loc));
            }
            let vertex = self.nodes.get_mut(id).expect("added above");
            vertex.kind = Some(shape);
        }
        if let Some(label) = fields.get("label").filter(|v| v.truthy()) {
            let vertex = self.nodes.get_mut(id).expect("added above");
            vertex.text = Some(label.to_js_string());
            vertex.label_type = LabelType::of(fields.get("labelType"));
        }
        Ok(())
    }

    /// Adds an edge, as Pele's addSingleLink does.
    fn add_single_link(
        &mut self,
        start: &str,
        end: &str,
        info: &LinkInfo,
        id: Option<&str>,
    ) -> Result<(), Error> {
        let mut edge = AgentEdge {
            start: start.to_string(),
            end: end.to_string(),
            interpolate: self.default_interpolate.clone().map(Value::String),
            ..AgentEdge::default()
        };
        if let Some(text) = &info.text {
            edge.text = unquote(trim(&text.text)).to_string();
            edge.label_type = text.kind;
        }
        edge.kind = Some(info.kind.clone());
        edge.stroke = Some(info.stroke.clone());
        edge.length = info.length.map(|l| if l > 10.0 { 10.0 } else { l });
        edge.edge_semantic = info.edge_semantic;
        let pair = format!("{start}\0{end}");
        let count = self.pair_count.get(&pair).copied().unwrap_or(0);
        match id {
            Some(id) if !id.is_empty() && !self.edge_by_id.contains_key(id) => {
                edge.id = id.to_string();
                edge.is_user_defined_id = true;
            }
            _ => edge.id = format!("L_{start}_{end}_{}", if count == 0 { 0 } else { count + 1 }),
        }

        if let Some(max) = self.options.max_edges
            && self.edges.len() >= max
        {
            return Err(agentflow_error(
                format!("Edge limit exceeded. The limit is {max}."),
                ErrorCode::Limit,
            ));
        }
        if !self.edge_by_id.contains_key(&edge.id) {
            self.edge_by_id.insert(edge.id.clone(), self.edges.len());
        }
        self.edges.push(edge);
        self.pair_count.insert(pair, count + 1);
        Ok(())
    }

    /// Adds the edges from each of starts to each of ends, as Pele's addLink does. The id
    /// the link gives goes to the edge from the last start to the first end.
    pub(crate) fn add_link(
        &mut self,
        starts: &[String],
        ends: &[String],
        info: &LinkInfo,
    ) -> Result<(), Error> {
        let id = info.id.as_deref().map(|id| id.replacen('@', "", 1));
        let last_start = &starts[starts.len() - 1];
        let first_end = &ends[0];
        for start in starts {
            for end in ends {
                let id = if start == last_start && end == first_end {
                    id.as_deref()
                } else {
                    None
                };
                self.add_single_link(start, end, info, id)?;
            }
        }
        Ok(())
    }

    /// Sets the curve of the edges at positions, as Pele's updateLinkInterpolate does.
    pub(crate) fn update_link_interpolate(
        &mut self,
        positions: &[String],
        interpolate: &str,
    ) -> Result<(), Error> {
        for pos in positions {
            if pos == "default" {
                self.default_interpolate = Some(interpolate.to_string());
            } else {
                self.edge_at(pos)?.interpolate = Some(Value::String(interpolate.to_string()));
            }
        }
        Ok(())
    }

    /// Sets the style of the edges at positions, and of the edges with none, as Pele's
    /// updateLink does. They all hold the one array in Pele, which takes fill:none at the
    /// first edge where it has no fill, so each holds it as it is at the end.
    pub(crate) fn update_link(
        &mut self,
        positions: &[String],
        mut style: Vec<String>,
    ) -> Result<(), Error> {
        let mut at = Vec::new();
        let mut default = false;
        for pos in positions {
            if pos == "default" {
                default = true;
                continue;
            }
            at.push(self.edge_index(pos)?);
            if !style.is_empty() && !style.iter().any(|s| s.starts_with("fill")) {
                style.push("fill:none".into());
            }
        }
        if default {
            self.default_edge_style = Some(style.clone());
        }
        for i in at {
            self.edges[i].style = Some(style.clone());
        }
        Ok(())
    }

    /// The index of the edge at a position, as Pele's edgeAt finds it. Mermaid checks the
    /// bounds only for a numeric index.
    fn edge_index(&self, pos: &str) -> Result<usize, Error> {
        let n = crate::js::number::number(pos);
        let i = (n >= 0.0 && n.fract() == 0.0 && n < self.edges.len() as f64).then_some(n as usize);
        i.ok_or_else(|| {
            agentflow_error(
                format!(
                    "The index {pos} for linkStyle is out of bounds. Valid indices for linkStyle are between 0 and {}. (Help: Ensure that the index is within the range of existing edges.)",
                    self.edges.len() as i64 - 1
                ),
                ErrorCode::Semantic,
            )
        })
    }

    fn edge_at(&mut self, pos: &str) -> Result<&mut AgentEdge, Error> {
        let i = self.edge_index(pos)?;
        Ok(&mut self.edges[i])
    }

    /// Defines the classes ids names, as Pele's addClass does.
    pub(crate) fn add_class(&mut self, ids: &str, styles: &[String]) {
        let style: Vec<String> = styles
            .join(",")
            .replace("\\,", "§§§")
            .replace(',', ";")
            .replace("§§§", ",")
            .split(';')
            .map(str::to_string)
            .collect();
        for id in ids.split(',') {
            if !self.classes.has(id) {
                self.classes.set(
                    id,
                    AgentClassDef {
                        id: id.to_string(),
                        ..AgentClassDef::default()
                    },
                );
            }
            let class = self.classes.get_mut(id).expect("added above");
            for s in &style {
                if s.contains("color") {
                    class.text_styles.push(s.replacen("fill", "bgFill", 1));
                }
                class.styles.push(s.clone());
            }
        }
    }

    /// Sets the direction, as Pele's setDirection does.
    pub(crate) fn set_direction(&mut self, dir: &str) {
        let mut d = trim(dir);
        if d.contains('<') {
            d = "RL";
        }
        if d.contains('^') {
            d = "BT";
        }
        if d.contains('>') {
            d = "LR";
        }
        if d.contains('v') {
            d = "TB";
        }
        if d == "TD" {
            d = "TB";
        }
        self.direction = Some(d.to_string());
    }

    /// Gives the nodes, edges and flows of ids a class, as Pele's setClass does.
    pub(crate) fn set_class(&mut self, ids: &str, class: &str) {
        for id in ids.split(',') {
            if let Some(v) = self.nodes.get_mut(id) {
                v.classes.push(class.to_string());
            }
            if let Some(&at) = self.edge_by_id.get(id) {
                self.edges[at].classes.push(class.to_string());
            }
            if let Some(&at) = self.subgraph_lookup.get(id) {
                self.subgraphs[at].classes.push(class.to_string());
            }
        }
    }

    pub(crate) fn set_tooltip(&mut self, ids: &str, tooltip: &str) {
        for id in ids.split(',') {
            self.tooltips.set(id, (id.to_string(), tooltip.to_string()));
        }
    }

    /// Links the nodes of ids, as Pele's setLink does.
    pub(crate) fn set_link(&mut self, ids: &str, link: &str, target: Option<&str>) {
        let url = trim(link);
        for id in ids.split(',') {
            if let Some(v) = self.nodes.get_mut(id) {
                v.link = (!url.is_empty()).then(|| safe_url(url));
                v.link_target = target.map(str::to_string);
            }
        }
        self.set_class(ids, "clickable");
    }

    /// Marks the nodes of ids clickable, as Pele's setClickEvent does. Callbacks are never
    /// run, as in Mermaid's strict mode.
    pub(crate) fn set_click_event(&mut self, ids: &str) {
        self.set_class(ids, "clickable");
    }

    pub(crate) fn set_acc_title(&mut self, text: &str) {
        self.acc_title = Some(trim_start(text).to_string());
    }

    pub(crate) fn set_acc_description(&mut self, text: &str) {
        self.acc_descr = Some(drop_indents(text));
    }

    /// Adds a flow of what a body lists, or adds to one of the same id, as Pele's addSubGraph
    /// does. It gives the flow's id.
    pub(crate) fn add_sub_graph(
        &mut self,
        id_text: Option<&FlowText>,
        list: &[DocItem],
        title_text: Option<&FlowText>,
    ) -> String {
        let id = id_text.map(|t| trim(&t.text).to_string());
        let title = title_text.map_or("", |t| t.text.as_str());

        let mut seen = HashSet::new();
        let mut node_list: Vec<String> = Vec::new();
        let mut dir = None;
        let mut take = |item: &String, node_list: &mut Vec<String>| {
            if !trim(item).is_empty() && seen.insert(item.clone()) {
                node_list.push(item.clone());
            }
        };
        for entry in list {
            match entry {
                DocItem::Text(s) => take(s, &mut node_list),
                DocItem::Nodes(items) => items.iter().for_each(|s| take(s, &mut node_list)),
                DocItem::Dir(d) => dir = Some(d.to_string()),
            }
        }
        if dir.is_none() && self.options.inherit_dir {
            dir = self
                .direction
                .clone()
                .or_else(|| self.options.direction.clone());
        }

        let id = id.unwrap_or_else(|| format!("subGraph{}", self.sub_count));
        self.sub_count += 1;

        // A flow never contains itself, a node of global scope, or a node another flow
        // already holds.
        let nodes: Vec<String> = node_list
            .into_iter()
            .filter(|n| *n != id && !self.global_nodes.contains(n) && !self.members.contains(n))
            .collect();
        self.members.extend(nodes.iter().cloned());

        if let Some(&at) = self.subgraph_lookup.get(&id) {
            // A repeated id adds its members to the first flow, and its title and direction
            // win.
            let existing = &mut self.subgraphs[at];
            existing.nodes.extend(nodes);
            if !trim(title).is_empty() {
                existing.title = trim(title).to_string();
            }
            if let Some(dir) = dir.filter(|d| !d.is_empty()) {
                existing.dir = Some(dir);
            }
            return id;
        }
        self.subgraph_lookup
            .insert(id.clone(), self.subgraphs.len());
        self.subgraphs.push(AgentSubgraph {
            id: id.clone(),
            nodes,
            title: trim(title).to_string(),
            classes: Vec::new(),
            dir,
            label_type: match title_text {
                Some(t) => t.kind,
                None => LabelType::Markdown,
            },
            metadata: None,
        });
        id
    }

    /// Keeps every node named inside a `global` block out of the flows that name it, as
    /// Pele's addGlobal does.
    pub(crate) fn add_global(&mut self, list: &[DocItem]) {
        let mut release = false;
        let mut add = |item: &str| {
            let id = trim(item);
            if id.is_empty() {
                return;
            }
            self.global_nodes.insert(id.to_string());
            if self.members.remove(id) {
                release = true;
            }
        };
        for entry in list {
            match entry {
                DocItem::Text(s) => add(s),
                DocItem::Nodes(items) => items.iter().for_each(|s| add(s)),
                DocItem::Dir(_) => {}
            }
        }
        if !release {
            return;
        }
        for subgraph in &mut self.subgraphs {
            subgraph.nodes.retain(|n| !self.global_nodes.contains(n));
        }
    }

    /// Adds a connector, or titles one, as Pele's addConnector does. It gives the
    /// connector's id, or nothing for one with no id.
    pub(crate) fn add_connector(&mut self, text: &FlowText, title: &FlowText) -> String {
        let id = trim(&text.text).to_string();
        if id.is_empty() {
            return id;
        }
        let title = trim(&title.text);
        if !self.connector_set.contains(&id) {
            let created = AgentNode {
                id: id.clone(),
                is_connector: true,
                text: Some(if title.is_empty() {
                    id.clone()
                } else {
                    title.to_string()
                }),
                ..AgentNode::default()
            };
            self.connector_set.insert(id.clone());
            self.connectors.push(id.clone());
            self.nodes.set(&id, created);
        } else if !title.is_empty() {
            let connector = self.nodes.get_mut(&id).expect("a connector is a node");
            connector.text = Some(title.to_string());
        }
        id
    }

    /// What a link's text says, or what the end and the start of a link with a label in it
    /// say together, as Pele's destructLink reads them.
    pub(crate) fn destruct_link(&self, end: &str, start: Option<&str>) -> LinkInfo {
        let info = destruct_end_link(end);
        if let Some(start) = start.filter(|s| !s.is_empty()) {
            // The first half of a split arrow only says whether the line is dotted.
            let stroke = if start.contains('.') {
                "dotted"
            } else {
                "normal"
            };
            if stroke != info.stroke {
                return LinkInfo {
                    kind: "INVALID".into(),
                    stroke: "INVALID".into(),
                    length: None,
                    ..LinkInfo::default()
                };
            }
        }
        LinkInfo {
            edge_semantic: edge_semantic(&info.kind, &info.stroke),
            ..info
        }
    }

    /// The flow of an id, as Pele's subgraph gives it.
    pub(crate) fn subgraph(&self, id: &str) -> Option<&AgentSubgraph> {
        self.subgraph_lookup.get(id).map(|&at| &self.subgraphs[at])
    }

    /// What a node is, as Pele's vertexKind says.
    pub(crate) fn vertex_kind(&self, vertex: &AgentNode) -> &'static str {
        if is_tool_definition(vertex) {
            return "tool";
        }
        match resolve_shape_alias(vertex.kind.as_deref()) {
            Some("hexagon" | "hex") => "action",
            Some("lean-right" | "lean_right") => "input",
            Some("lin-doc" | "lined-document") => "refdoc",
            Some("diamond") => "decision",
            _ => "task",
        }
    }

    /// The diagram as tools that read what it means take it, as Pele's semanticModel gives
    /// it.
    #[cfg(test)]
    pub(crate) fn semantic_model(&self) -> SemanticModel<'_> {
        let mut vertices = Vec::new();
        let mut connectors = Vec::new();
        for v in self.nodes.values() {
            if self.subgraph_lookup.contains_key(&v.id) {
                continue;
            }
            let metadata = semantic_metadata(v.metadata.as_ref());
            if v.is_connector {
                connectors.push(SemanticConnector {
                    id: v.id.clone(),
                    title: v.text.clone().filter(|t| *t != v.id),
                    metadata,
                });
                continue;
            }
            vertices.push(SemanticVertex {
                id: v.id.clone(),
                label: v.text.clone(),
                shape: resolve_shape_alias(v.kind.as_deref()).map(str::to_string),
                vertex_kind: self.vertex_kind(v),
                metadata,
            });
        }
        let edges = self
            .edges
            .iter()
            .map(|e| SemanticEdge {
                start: e.start.clone(),
                end: e.end.clone(),
                id: e.id.clone(),
                label: (!e.text.is_empty()).then(|| e.text.clone()),
                kind: e.kind.clone(),
                stroke: e.stroke.clone(),
                edge_semantic: e.edge_semantic,
                length: e.length,
                metadata: e.metadata.clone().filter(|m| !m.is_empty()),
            })
            .collect();
        let sub_graphs = self
            .subgraphs
            .iter()
            .map(|sg| SemanticSubgraph {
                id: sg.id.clone(),
                title: sg.title.clone(),
                nodes: sg.nodes.clone(),
                metadata: semantic_metadata(sg.metadata.as_ref()),
                direction: sg.dir.clone(),
            })
            .collect();
        SemanticModel {
            direction: self.direction.as_deref(),
            vertices,
            edges,
            sub_graphs,
            connectors,
            diagnostics: &self.diagnostics,
        }
    }

    /// Where loc is, with the lines of front matter added, as Pele's position gives it.
    fn position(&self, loc: Loc) -> ElementPosition {
        ElementPosition {
            start_line: loc.first_line + self.line_offset,
            start_column: loc.first_column,
            end_line: loc.last_line + self.line_offset,
            end_column: loc.last_column,
        }
    }

    fn push_mapping(&mut self, id: &str, kind: StatementType, position: ElementPosition) -> usize {
        let at = self.mappings.len();
        self.mappings.push(ElementMapping {
            id: id.to_string(),
            kind,
            position,
        });
        self.first_mapping.entry(id.to_string()).or_insert(at);
        at
    }

    /// Records where a node is written, as Pele's addVertexMapping does. A bare reference,
    /// with no shape, becomes an attachment if a metadata block follows it.
    pub(crate) fn add_vertex_mapping(&mut self, id: &str, shaped: bool, loc: Loc) {
        if id.is_empty() {
            return;
        }
        let at = self.push_mapping(id, StatementType::Vertex, self.position(loc));
        if !shaped {
            self.bare_mappings.insert(at);
        }
    }

    /// Widens a node's span over the metadata block that follows it, as Pele's
    /// extendVertexMapping does.
    pub(crate) fn extend_vertex_mapping(&mut self, id: &str, loc: Loc) {
        if id.is_empty() {
            return;
        }
        let end = self.position(loc);
        for i in (0..self.mappings.len()).rev() {
            let m = &self.mappings[i];
            if m.kind != StatementType::Vertex || m.id != id {
                continue;
            }
            if self.bare_mappings.remove(&i) {
                self.mappings[i].kind = StatementType::Attachment;
            }
            let p = &mut self.mappings[i].position;
            if end.end_line > p.end_line
                || (end.end_line == p.end_line && end.end_column > p.end_column)
            {
                p.end_line = end.end_line;
                p.end_column = end.end_column;
            }
            return;
        }
        self.push_mapping(id, StatementType::Attachment, end);
    }

    /// Records where an edge statement is written, as Pele's addEdgeMapping does: by the ids
    /// of the nodes it runs to.
    pub(crate) fn add_edge_mapping(&mut self, to_nodes: &[String], loc: Loc) {
        let ids: Vec<&str> = to_nodes
            .iter()
            .map(String::as_str)
            .filter(|n| !n.is_empty())
            .collect();
        let id = if ids.is_empty() {
            "edge".to_string()
        } else {
            ids.join(">")
        };
        let position = self.position(loc);
        self.push_mapping(&id, StatementType::Edge, position);
    }

    fn block_mapping(&mut self, id: &str, kind: StatementType, start: Loc, end: Loc) {
        if id.is_empty() {
            return;
        }
        let (start, end) = (self.position(start), self.position(end));
        self.push_mapping(
            id,
            kind,
            ElementPosition {
                start_line: start.start_line,
                start_column: start.start_column,
                end_line: end.end_line,
                end_column: end.end_column,
            },
        );
    }

    /// Records where a flow is written, as Pele's addSubgraphMapping does.
    pub(crate) fn add_subgraph_mapping(&mut self, id: Option<&FlowText>, start: Loc, end: Loc) {
        let id = id.map_or("", |t| t.text.as_str()).to_string();
        self.block_mapping(&id, StatementType::Subgraph, start, end);
    }

    /// Records where a connector is written, as Pele's addConnectorMapping does.
    pub(crate) fn add_connector_mapping(&mut self, id: &FlowText, start: Loc, end: Loc) {
        let id = trim(&id.text).to_string();
        self.block_mapping(&id, StatementType::Connector, start, end);
    }

    /// Records a finding about a node, once, as Pele's emit does: the graph may be built more
    /// than once.
    fn emit(&mut self, id: DiagnosticId, severity: Severity, message: String, node_id: &str) {
        let diagnostic = Diagnostic {
            id,
            severity,
            message,
            node_id: (!node_id.is_empty()).then(|| node_id.to_string()),
            edge_id: None,
            position: (!node_id.is_empty())
                .then(|| self.first_mapping.get(node_id))
                .flatten()
                .map(|&at| self.mappings[at].position),
        };
        let same = self.reported.entry(node_id.to_string()).or_default();
        if same.iter().any(|&k| {
            let d = &self.diagnostics[k];
            d.id == id
                && d.severity == severity
                && d.message == diagnostic.message
                && d.node_id == diagnostic.node_id
                && d.edge_id == diagnostic.edge_id
        }) {
            return;
        }
        same.push(self.diagnostics.len());
        self.diagnostics.push(diagnostic);
    }

    pub(crate) fn emit_warning(&mut self, id: DiagnosticId, message: String, node_id: &str) {
        self.emit(id, Severity::Warning, message, node_id);
    }

    pub(crate) fn emit_error(&mut self, id: DiagnosticId, message: String, node_id: &str) {
        self.emit(id, Severity::Error, message, node_id);
    }
}
