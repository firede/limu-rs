//! What a flowchart's parser fills, as Pele's FlowDb in db.ts has it: its nodes, edges,
//! subgraphs, classes and tooltips, as Mermaid's flowDb keeps them.

use std::collections::{HashMap, HashSet};

use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::{drop_indents, is_space_char, to_lower, trim, trim_start};
use crate::url::safe_url;
use crate::value::{Map, Value};
use crate::yaml::parse_yaml;

use super::shapes::is_valid_shape;

/// How a label's text is read, Pele's LabelType.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LabelType {
    #[default]
    Text,
    String,
    Markdown,
}

impl LabelType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            LabelType::Text => "text",
            LabelType::String => "string",
            LabelType::Markdown => "markdown",
        }
    }

    /// The type of a name, or markdown for any other, as Pele's labelType reads it.
    fn of(name: Option<&str>) -> LabelType {
        match name {
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

/// A node, Pele's FlowNode. What Pele leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FlowNode {
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
    /// object's keys.
    pub(crate) props: Map,
    pub(crate) link: Option<String>,
    pub(crate) link_target: Option<String>,
    pub(crate) icon: Option<String>,
    pub(crate) form: Option<String>,
    pub(crate) pos: Option<String>,
    pub(crate) img: Option<String>,
    pub(crate) constraint: Option<String>,
    pub(crate) asset_width: Option<f64>,
    pub(crate) asset_height: Option<f64>,
}

/// An edge, Pele's FlowEdge.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FlowEdge {
    pub(crate) start: String,
    pub(crate) end: String,
    pub(crate) kind: Option<String>,
    pub(crate) text: String,
    pub(crate) label_type: LabelType,
    pub(crate) stroke: Option<String>,
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
}

/// A class classDef defines, Pele's FlowClassDef.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FlowClassDef {
    pub(crate) id: String,
    pub(crate) styles: Vec<String>,
    pub(crate) text_styles: Vec<String>,
}

/// A subgraph, Pele's FlowSubgraph.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FlowSubgraph {
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
    pub(crate) text: Option<FlowText>,
    pub(crate) id: Option<String>,
}

/// What a subgraph's body lists, Pele's DocItem: a separator or a statement's text, the
/// nodes of a statement, or a direction.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DocItem {
    Text(String),
    Nodes(Vec<String>),
    Dir(&'static str),
}

/// What a FlowDb is made with, Pele's FlowDbOptions.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FlowDbOptions {
    /// The most edges, or none for no limit.
    pub(crate) max_edges: Option<usize>,
    pub(crate) inherit_dir: bool,
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

/// A flowchart as its parser fills it, Pele's FlowDb.
#[derive(Clone, Debug, Default)]
pub(crate) struct FlowDb {
    pub(crate) direction: Option<String>,
    pub(crate) nodes: Ordered<FlowNode>,
    pub(crate) edges: Vec<FlowEdge>,
    pub(crate) subgraphs: Vec<FlowSubgraph>,
    pub(crate) classes: Ordered<FlowClassDef>,
    /// The tooltips, by the ids they are for, as (id, text).
    pub(crate) tooltips: Ordered<(String, String)>,
    pub(crate) default_edge_style: Option<Vec<String>>,
    pub(crate) default_interpolate: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,

    /// The subgraphs by their ids, as indices into subgraphs.
    subgraph_lookup: HashMap<String, usize>,
    members: HashSet<String>,
    /// The first edge of each id, as an index into edges.
    edge_by_id: HashMap<String, usize>,
    pair_count: HashMap<String, usize>,
    sub_count: usize,
    options: FlowDbOptions,
}

fn flowchart_error(message: String, code: ErrorCode) -> Error {
    let mut e = Error::new(message, code);
    e.diagram = Some(DiagramType::Flowchart);
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

/// What the start of a link with a label in it says, Pele's destructStartLink.
fn destruct_start_link(raw: &str) -> (String, &'static str) {
    let mut s = trim(raw);
    let kind = match s.as_bytes().first() {
        Some(b'<') => "arrow_point",
        Some(b'x') => "arrow_cross",
        Some(b'o') => "arrow_circle",
        _ => "arrow_open",
    };
    if kind != "arrow_open" {
        s = &s[1..];
    }
    let mut stroke = "normal";
    if s.contains('=') {
        stroke = "thick";
    }
    if s.contains('.') {
        stroke = "dotted";
    }
    (kind.to_string(), stroke)
}

/// What a link, or the end of one with a label in it, says, Pele's destructEndLink.
fn destruct_end_link(raw: &str) -> LinkInfo {
    let s = trim(raw);
    // The links are ASCII, as their lexer reads them.
    let mut line = &s[..s.len().saturating_sub(1)];
    let (single, double) = match s.as_bytes().last() {
        Some(b'x') => ("arrow_cross", s.starts_with('x')),
        Some(b'>') => ("arrow_point", s.starts_with('<')),
        Some(b'o') => ("arrow_circle", s.starts_with('o')),
        _ => ("arrow_open", false),
    };
    let kind = if double {
        line = &line[1..];
        format!("double_{single}")
    } else {
        single.to_string()
    };
    let mut stroke = "normal";
    let mut length = line.len() as f64 - 1.0;
    if line.starts_with('=') {
        stroke = "thick";
    }
    if line.starts_with('~') {
        stroke = "invisible";
    }
    let dots = line.bytes().filter(|&c| c == b'.').count();
    if dots > 0 {
        stroke = "dotted";
        length = dots as f64;
    }
    LinkInfo {
        kind,
        stroke: stroke.to_string(),
        length: Some(length),
        text: None,
        id: None,
    }
}

/// Whether s has a white space character in it, /\s/.test(s).
fn has_space(s: &str) -> bool {
    s.chars().any(is_space_char)
}

impl FlowDb {
    pub(crate) fn new(options: FlowDbOptions) -> FlowDb {
        FlowDb {
            options,
            ..FlowDb::default()
        }
    }

    /// Adds a node, or adds to one, as Pele's addVertex does. metadata is the text of the
    /// node's `@{ … }`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn add_vertex(
        &mut self,
        id: &str,
        text: Option<&FlowText>,
        kind: Option<&str>,
        style: Option<Vec<String>>,
        props: Option<Map>,
        metadata: Option<&str>,
    ) -> Result<(), Error> {
        if trim(id).is_empty() {
            return Ok(());
        }

        let mut doc: Option<Map> = None;
        if let Some(metadata) = metadata {
            let yaml = if metadata.contains('\n') {
                format!("{metadata}\n")
            } else {
                format!("{{\n{metadata}\n}}")
            };
            let parsed = parse_yaml(&yaml).map_err(|e| {
                flowchart_error(
                    format!("Metadata of \"{id}\" is not valid YAML. {}", e.0),
                    ErrorCode::Syntax,
                )
            })?;
            if let Value::Object(map) = parsed {
                doc = Some(map);
            }
        }

        if let Some(&at) = self.subgraph_lookup.get(id)
            && let Some(doc) = &doc
        {
            let subgraph = &mut self.subgraphs[at];
            let mut merged = subgraph.metadata.take().unwrap_or_default();
            for (k, v) in doc {
                merged.insert(k, v.clone());
            }
            subgraph.metadata = Some(merged);
            return Ok(());
        }

        if let Some(&at) = self.edge_by_id.get(id) {
            let edge = &mut self.edges[at];
            if let Some(doc) = &doc {
                if let Some(v) = doc.get("animate") {
                    edge.animate = Some(v.clone());
                }
                if let Some(v) = doc.get("animation") {
                    edge.animation = Some(v.clone());
                }
                if let Some(v) = doc.get("curve") {
                    edge.interpolate = Some(v.clone());
                }
            }
            return Ok(());
        }

        let created = self.nodes.get(id).is_none();
        if created {
            let vertex = FlowNode {
                id: id.to_string(),
                ..FlowNode::default()
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
        if created {
            vertex.props = props.unwrap_or_default();
        } else if let Some(props) = props {
            // Object.assign sets each property as an assignment does, and an assignment to a
            // __proto__ the object does not hold as its own sets its prototype instead, which
            // a string cannot be.
            for (k, v) in &props {
                if k != "__proto__" || vertex.props.contains_key(k) {
                    vertex.props.insert(k, v.clone());
                }
            }
        }

        let Some(doc) = doc else { return Ok(()) };

        if let Some(shape) = doc.get("shape").filter(|v| v.truthy()) {
            let shape = shape.to_js_string();
            if shape != to_lower(&shape) || shape.contains('_') {
                return Err(flowchart_error(
                    format!("No such shape: {shape}. Shape names should be lowercase."),
                    ErrorCode::Semantic,
                ));
            }
            if !is_valid_shape(&shape) {
                return Err(flowchart_error(
                    format!("No such shape: {shape}."),
                    ErrorCode::Semantic,
                ));
            }
            vertex.kind = Some(shape);
        }
        let label = doc.get("label").and_then(Value::as_str);
        // A label that is no text, or is white space, leaves the text an icon or an image
        // takes the place of.
        let no_label = label.is_none_or(|l| trim(l).is_empty());
        if let Some(v) = doc.get("label").filter(|v| v.truthy()) {
            vertex.text = Some(v.to_js_string());
            vertex.label_type = LabelType::of(doc.get("labelType").and_then(Value::as_str));
        }
        let string = |key: &str| doc.get(key).filter(|v| v.truthy()).map(Value::to_js_string);
        if let Some(icon) = string("icon") {
            vertex.icon = Some(icon);
            if no_label && vertex.text.as_deref() == Some(id) {
                vertex.text = Some(String::new());
            }
        }
        if let Some(form) = string("form") {
            vertex.form = Some(form);
        }
        if let Some(pos) = string("pos") {
            vertex.pos = Some(pos);
        }
        if let Some(img) = string("img") {
            vertex.img = Some(img);
            if no_label && vertex.text.as_deref() == Some(id) {
                vertex.text = Some(String::new());
            }
        }
        if let Some(constraint) = string("constraint") {
            vertex.constraint = Some(constraint);
        }
        if let Some(w) = doc.get("w").filter(|v| v.truthy()) {
            vertex.asset_width = Some(w.to_number());
        }
        if let Some(h) = doc.get("h").filter(|v| v.truthy()) {
            vertex.asset_height = Some(h.to_number());
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
        let mut edge = FlowEdge {
            start: start.to_string(),
            end: end.to_string(),
            interpolate: self.default_interpolate.clone().map(Value::String),
            ..FlowEdge::default()
        };
        if let Some(text) = &info.text {
            edge.text = unquote(trim(&text.text)).to_string();
            edge.label_type = LabelType::of(Some(text.kind.as_str()));
        }
        edge.kind = Some(info.kind.clone());
        edge.stroke = Some(info.stroke.clone());
        edge.length = info.length.map(|l| if l > 10.0 { 10.0 } else { l });
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
            return Err(flowchart_error(
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

    /// Sets the style of the edges at positions, as Pele's updateLink does. The edges share
    /// the one array in Pele, which takes fill:none once for each edge that has no fill.
    pub(crate) fn update_link(
        &mut self,
        positions: &[String],
        mut style: Vec<String>,
    ) -> Result<(), Error> {
        let mut at = Vec::new();
        for pos in positions {
            if pos == "default" {
                self.default_edge_style = Some(style.clone());
                continue;
            }
            let i = self.edge_index(pos)?;
            at.push(i);
            if !style.is_empty() && !style.iter().any(|s| s.starts_with("fill")) {
                style.push("fill:none".into());
            }
        }
        // Each edge holds the array as it is at the end, as it is one array in Pele.
        for i in at {
            self.edges[i].style = Some(style.clone());
        }
        Ok(())
    }

    fn edge_index(&self, pos: &str) -> Result<usize, Error> {
        let n = crate::js::number::number(pos);
        let i = (n >= 0.0 && n.fract() == 0.0 && n < self.edges.len() as f64).then_some(n as usize);
        i.ok_or_else(|| {
            flowchart_error(
                format!(
                    "The index {pos} for linkStyle is out of bounds. Valid indices for linkStyle are between 0 and {}. (Help: Ensure that the index is within the range of existing edges.)",
                    self.edges.len() as i64 - 1
                ),
                ErrorCode::Semantic,
            )
        })
    }

    fn edge_at(&mut self, pos: &str) -> Result<&mut FlowEdge, Error> {
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
            if self.classes.get(id).is_none() {
                self.classes.set(
                    id,
                    FlowClassDef {
                        id: id.to_string(),
                        ..FlowClassDef::default()
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

    /// Gives the nodes, edges and subgraphs of ids a class, as Pele's setClass does.
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

    /// Adds a subgraph of what a body lists, or adds to one of the same id, as Pele's
    /// addSubGraph does. title_is_id says the title is the id itself, as no [title] was
    /// written. It gives the subgraph's id.
    pub(crate) fn add_sub_graph(
        &mut self,
        id_text: Option<&FlowText>,
        list: &[DocItem],
        title_text: Option<&FlowText>,
        title_is_id: bool,
    ) -> String {
        let mut id = id_text.map(|t| trim(&t.text).to_string());
        let title = title_text.map_or("", |t| t.text.as_str());
        if id_text.is_some() && title_is_id && has_space(title) {
            id = None;
        }

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
            dir = self.direction.clone();
        }

        let id = id.unwrap_or_else(|| format!("subGraph{}", self.sub_count));
        self.sub_count += 1;

        // A node belongs to the first subgraph that lists it, and a subgraph never contains
        // itself.
        let nodes: Vec<String> = node_list
            .into_iter()
            .filter(|n| !self.members.contains(n) && *n != id)
            .collect();
        self.members.extend(nodes.iter().cloned());

        match self.subgraph_lookup.get(&id) {
            Some(&at) => self.subgraphs[at].nodes.extend(nodes),
            None => {
                self.subgraph_lookup
                    .insert(id.clone(), self.subgraphs.len());
                self.subgraphs.push(FlowSubgraph {
                    id: id.clone(),
                    nodes,
                    title: trim(title).to_string(),
                    classes: Vec::new(),
                    dir,
                    label_type: LabelType::of(title_text.map(|t| t.kind.as_str())),
                    metadata: None,
                });
            }
        }
        id
    }

    /// What a link's text says, or what the end and the start of a link with a label in it
    /// say together, as Pele's destructLink reads them.
    pub(crate) fn destruct_link(&self, end: &str, start: Option<&str>) -> LinkInfo {
        let info = destruct_end_link(end);
        let Some(start) = start.filter(|s| !s.is_empty()) else {
            return info;
        };
        let invalid = || LinkInfo {
            kind: "INVALID".into(),
            stroke: "INVALID".into(),
            ..LinkInfo::default()
        };
        let (mut kind, stroke) = destruct_start_link(start);
        if stroke != info.stroke {
            return invalid();
        }
        if kind == "arrow_open" {
            kind = info.kind;
        } else {
            if kind != info.kind {
                return invalid();
            }
            kind = format!("double_{kind}");
        }
        if kind == "double_arrow" {
            kind = "double_arrow_point".into();
        }
        LinkInfo {
            kind,
            stroke: stroke.to_string(),
            length: info.length,
            text: None,
            id: None,
        }
    }

    /// The subgraph of an id, as Pele's subgraph gives it.
    pub(crate) fn subgraph(&self, id: &str) -> Option<&FlowSubgraph> {
        self.subgraph_lookup.get(id).map(|&at| &self.subgraphs[at])
    }
}
