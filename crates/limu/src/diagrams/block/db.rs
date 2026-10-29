//! The model of a block diagram, as Pele's types.ts and db.ts have it: the tree of blocks
//! under its root, its edges, the classes classDef statements define, and every block by its
//! id.

use std::collections::{HashMap, HashSet};

use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::trim;
use crate::js::map::Ordered;
use crate::js::number::to_string;

/// The most space blocks a diagram may hold, Pele's MAX_SPACES.
const MAX_SPACES: f64 = 100000.0;

/// A block, an edge, or a statement that sets columns, classes or styles, in one shape, as
/// Pele's Block is, after Mermaid. A field Pele may leave undefined is none here. The blocks
/// a composite holds, and those it is made of before the model reads them, are the blocks at
/// their indices in the model's blocks, so that one block is found both in the tree and by
/// its id.
#[derive(Clone, Debug, Default)]
pub(crate) struct Block {
    /// None only for a composite whose header was a statement with links, until the model
    /// gives it one.
    pub(crate) id: Option<String>,
    pub(crate) r#type: String,
    pub(crate) label: Option<String>,
    pub(crate) children: Option<Vec<usize>>,
    /// The number of columns of a composite, -1 for as many as it has blocks.
    pub(crate) columns: Option<f64>,
    /// The number of columns a block asked to span.
    pub(crate) width_in_columns: Option<f64>,
    /// The number of cells a `space` statement asked for.
    pub(crate) width: Option<f64>,
    pub(crate) directions: Option<Vec<String>>,
    pub(crate) classes: Option<Vec<String>>,
    pub(crate) styles: Option<Vec<String>>,
    pub(crate) color_index: Option<usize>,
    // What an edge has: the ids of the blocks it joins, its markers, and how its line is
    // drawn.
    pub(crate) start: Option<String>,
    pub(crate) end: Option<String>,
    pub(crate) arrow_type_start: Option<String>,
    pub(crate) arrow_type_end: Option<String>,
    pub(crate) thickness: Option<String>,
    pub(crate) pattern: Option<String>,
    // What the statements that set classes and styles have.
    pub(crate) css: Option<String>,
    pub(crate) style_class: Option<String>,
    pub(crate) styles_str: Option<String>,
    /// What a composite holds as the parser leaves it, before the model reads it into
    /// children.
    pub(crate) statements: Vec<Statement>,
}

/// A statement of a diagram, Pele's Statement: a block, or a statement with links, which is
/// a list of blocks and the edges between them.
#[derive(Clone, Debug)]
pub(crate) enum Statement {
    One(usize),
    Chain(Vec<usize>),
}

/// A class of styles a classDef statement defines, Pele's BlockClassDef.
#[derive(Clone, Debug)]
pub(crate) struct BlockClassDef {
    pub(crate) id: String,
    pub(crate) styles: Vec<String>,
    pub(crate) text_styles: Vec<String>,
}

/// The type of a block whose label is in the brackets type_str, or none, as Pele's
/// typeStr2Type gives it.
pub(crate) fn type_str2_type(type_str: Option<&str>) -> &'static str {
    match type_str.unwrap_or("") {
        "[]" => "square",
        "()" => "round",
        "(())" => "circle",
        ">]" => "rect_left_inv_arrow",
        "{}" => "diamond",
        "{{}}" => "hexagon",
        "([])" => "stadium",
        "[[]]" => "subroutine",
        "[()]" => "cylinder",
        "((()))" => "doublecircle",
        "[//]" => "lean_right",
        "[\\\\]" => "lean_left",
        "[/\\]" => "trapezoid",
        "[\\/]" => "inv_trapezoid",
        "<[]>" => "block_arrow",
        _ => "na",
    }
}

/// The marker at the end of a link, as Pele's edgeStrToEdgeData reads it.
pub(crate) fn edge_str_to_edge_data(type_str: &str) -> &'static str {
    match trim(type_str).bytes().last() {
        Some(b'x') => "arrow_cross",
        Some(b'o') => "arrow_circle",
        Some(b'>') => "arrow_point",
        _ => "",
    }
}

/// The marker at the start of a link, as Pele's edgeStrToEdgeStartData reads it.
pub(crate) fn edge_str_to_edge_start_data(type_str: &str) -> &'static str {
    match trim(type_str).bytes().next() {
        Some(b'x') => "arrow_cross",
        Some(b'o') => "arrow_circle",
        Some(b'<') => "arrow_point",
        _ => "arrow_open",
    }
}

/// How thick a link is drawn, as Pele's edgeStrToThickness says.
pub(crate) fn edge_str_to_thickness(type_str: &str) -> &'static str {
    if type_str.contains("==") {
        "thick"
    } else {
        "normal"
    }
}

/// How a link's line is drawn, as Pele's edgeStrToPattern says.
pub(crate) fn edge_str_to_pattern(type_str: &str) -> &'static str {
    if type_str.contains(".-") {
        "dotted"
    } else {
        "solid"
    }
}

/// A statement list being read into a block's children, Pele's Frame.
struct Frame {
    list: Vec<usize>,
    at: usize,
    parent: usize,
    children: Vec<usize>,
    columns: f64,
}

/// The diagram of a model, as Pele's BlockDb is: the tree of blocks under its root, the
/// edges, the classes classDef statements define, and every block by its id.
#[derive(Clone, Debug)]
pub(crate) struct BlockDb {
    pub(crate) title: Option<String>,
    /// Every block the parser and the model made, the root first.
    pub(crate) arena: Vec<Block>,
    pub(crate) blocks: Vec<usize>,
    pub(crate) edges: Vec<usize>,
    /// The classes by name, in the order they were first defined.
    pub(crate) classes: Ordered<(String, BlockClassDef)>,
    /// Every block by its id, in the order the ids were first set. Mermaid keeps the root in
    /// here too, which makes a block called `root` vanish. Here it stays out.
    pub(crate) by_id: Ordered<(String, usize)>,
    /// Ids that a class or style statement named before any block had them.
    pending: HashSet<String>,
    edge_count: HashMap<String, usize>,
    ids: usize,
    colors: usize,
    spaces: f64,
}

/// The root of a model's tree, the first of its blocks.
pub(crate) const ROOT: usize = 0;

impl Default for BlockDb {
    fn default() -> BlockDb {
        BlockDb {
            title: None,
            arena: vec![Block {
                id: Some("root".into()),
                r#type: "composite".into(),
                children: Some(Vec::new()),
                columns: Some(-1.0),
                ..Block::default()
            }],
            blocks: Vec::new(),
            edges: Vec::new(),
            classes: Ordered::default(),
            by_id: Ordered::default(),
            pending: HashSet::new(),
            edge_count: HashMap::new(),
            ids: 0,
            colors: 0,
            spaces: 0.0,
        }
    }
}

/// The blocks of statements in a row, Pele's flat.
fn flatten(statements: &[Statement]) -> Vec<usize> {
    let mut out = Vec::new();
    for s in statements {
        match s {
            Statement::One(b) => out.push(*b),
            Statement::Chain(list) => out.extend(list),
        }
    }
    out
}

impl BlockDb {
    /// Adds a block and gives its index.
    pub(crate) fn add(&mut self, block: Block) -> usize {
        self.arena.push(block);
        self.arena.len() - 1
    }

    /// The block of an id.
    pub(crate) fn get(&self, id: &str) -> Option<&Block> {
        self.by_id.get(id).map(|&(_, b)| &self.arena[b])
    }

    /// The next id of a block that has none of its own, as Pele's generateId gives it.
    /// Mermaid's ids are random. These are counted, so the same text always gives the same
    /// output.
    pub(crate) fn generate_id(&mut self) -> String {
        self.ids += 1;
        format!("id-{}", self.ids)
    }

    fn set_by_id(&mut self, id: &str, b: usize) {
        self.by_id.set(id, (id.to_string(), b));
    }

    /// Adds the declarations of a classDef statement to a class, as Pele's addStyleClass
    /// does.
    fn add_style_class(&mut self, id: &str, style_attributes: &str) {
        if !self.classes.contains(id) {
            let def = BlockClassDef {
                id: id.to_string(),
                styles: Vec::new(),
                text_styles: Vec::new(),
            };
            self.classes.set(id, (id.to_string(), def));
        }
        let (_, found) = self.classes.get_mut(id).expect("a class");
        for attribute in style_attributes.split(',') {
            let fixed = match attribute.find(';') {
                Some(semi) => format!("{}{}", &attribute[..semi], &attribute[semi + 1..]),
                None => attribute.to_string(),
            };
            let fixed = trim(&fixed);
            if attribute.contains("color") {
                found.text_styles.push(
                    fixed
                        .replacen("fill", "bgFill", 1)
                        .replacen("color", "fill", 1),
                );
            }
            found.styles.push(fixed.to_string());
        }
    }

    /// Gives a block the declarations of a style statement, as Pele's addStyle2Node does.
    fn add_style2_node(&mut self, id: &str, styles: &str) {
        let b = self.named(id);
        self.arena[b].styles = Some(styles.split(',').map(str::to_string).collect());
    }

    /// Gives blocks a class, as Pele's setCssClass does. Mermaid looks each id up before
    /// trimming it, so `class a, b hot` misses b. Here it is trimmed first.
    fn set_css_class(&mut self, item_ids: &str, css_class_name: &str) {
        for id in item_ids.split(',') {
            let b = self.named(trim(id));
            self.arena[b]
                .classes
                .get_or_insert_with(Vec::new)
                .push(css_class_name.to_string());
        }
    }

    /// The block of an id, or one that waits for it, as Pele's named gives it. Mermaid throws
    /// on a style for an unknown id, and a class for one makes the block of that id disappear
    /// when it is declared later. Here both wait for the block.
    fn named(&mut self, id: &str) -> usize {
        if let Some(&(_, b)) = self.by_id.get(id) {
            return b;
        }
        let b = self.add(Block {
            id: Some(id.to_string()),
            r#type: "na".into(),
            children: Some(Vec::new()),
            ..Block::default()
        });
        self.set_by_id(id, b);
        self.pending.insert(id.to_string());
        b
    }

    /// The frame of the statements of parent, Pele's frame: the statements flattened, and
    /// the columns the first column setting among them gives.
    fn frame(&self, statements: &[Statement], parent: usize) -> Frame {
        let list = flatten(statements);
        let columns = list
            .iter()
            .map(|&b| &self.arena[b])
            .find(|b| b.r#type == "column-setting")
            .map_or(-1.0, |b| b.columns.unwrap_or(-1.0));
        Frame {
            list,
            at: 0,
            parent,
            children: Vec::new(),
            columns,
        }
    }

    /// Reads the statements of a diagram into its tree of blocks, its edges and its classes,
    /// as Pele's setHierarchy does.
    pub(crate) fn set_hierarchy(&mut self, statements: &[Statement]) -> Result<(), Error> {
        let mut stack = vec![self.frame(statements, ROOT)];
        while let Some(f) = stack.last_mut() {
            if f.at == f.list.len() {
                let f = stack.pop().expect("a frame");
                self.arena[f.parent].children = Some(f.children);
                continue;
            }
            let b = f.list[f.at];
            f.at += 1;
            let parent = f.parent;
            // Pele warns of a block wider than its composite's columns where it is asked to.
            // A host is not told.
            let block = &mut self.arena[b];
            match block.r#type.as_str() {
                "classDef" => {
                    let (id, css) = (block.id.clone(), block.css.clone());
                    self.add_style_class(&id.unwrap_or_default(), &css.unwrap_or_default());
                    continue;
                }
                "applyClass" => {
                    let (id, class) = (block.id.clone(), block.style_class.clone());
                    self.set_css_class(&id.unwrap_or_default(), &class.unwrap_or_default());
                    continue;
                }
                "applyStyles" => {
                    if let Some(styles) = block.styles_str.clone().filter(|s| !s.is_empty()) {
                        let id = block.id.clone().unwrap_or_default();
                        self.add_style2_node(&id, &styles);
                    }
                    continue;
                }
                "column-setting" => {
                    let columns = block.columns.unwrap_or(-1.0);
                    self.arena[parent].columns = Some(columns);
                    continue;
                }
                "edge" => {
                    let id = block.id.clone().unwrap_or_default();
                    let count = self.edge_count.get(&id).copied().unwrap_or(0) + 1;
                    self.edge_count.insert(id.clone(), count);
                    self.arena[b].id = Some(format!("{count}-{id}"));
                    self.edges.push(b);
                    continue;
                }
                _ => {}
            }

            // A composite whose header was a link statement has no id of its own.
            if self.arena[b].id.is_none() {
                let id = self.generate_id();
                self.arena[b].id = Some(id);
            }
            let block = &mut self.arena[b];
            let id = block.id.clone().expect("an id");
            if block.label.as_deref().is_none_or(str::is_empty) {
                block.label = Some(if block.r#type == "composite" {
                    String::new()
                } else {
                    id.clone()
                });
            }
            let known = self.by_id.get(&id).map(|&(_, k)| k);
            let fresh = known.is_none() || self.pending.remove(&id);
            if !fresh {
                let known = known.expect("a block");
                let (kind, label) = (block.r#type.clone(), block.label.clone());
                if kind != "na" {
                    self.arena[known].r#type = kind;
                }
                if label.as_deref() != Some(id.as_str()) {
                    self.arena[known].label = label;
                }
            } else {
                if let Some(known) = known {
                    let (classes, styles) = (
                        self.arena[known].classes.clone(),
                        self.arena[known].styles.clone(),
                    );
                    self.arena[b].classes = classes;
                    self.arena[b].styles = styles;
                }
                if self.arena[b].r#type == "composite" {
                    self.arena[b].color_index = Some(self.colors);
                    self.colors += 1;
                }
                self.set_by_id(&id, b);
            }

            let inner = std::mem::take(&mut self.arena[b].statements);
            self.arena[b].children = Some(Vec::new());
            let f = stack.last_mut().expect("a frame");
            if self.arena[b].r#type == "space" {
                let width = self.arena[b].width.unwrap_or(1.0);
                if !(width <= MAX_SPACES - self.spaces) {
                    let message = format!(
                        "A block diagram can hold at most {} space blocks.",
                        to_string(MAX_SPACES)
                    );
                    let mut e = Error::new(message, ErrorCode::Limit);
                    e.diagram = Some(DiagramType::Block);
                    return Err(e);
                }
                self.spaces += width.max(0.0);
                let mut j = 0;
                while (j as f64) < width {
                    let copy = Block {
                        id: Some(format!("{id}-{j}")),
                        children: Some(Vec::new()),
                        ..self.arena[b].clone()
                    };
                    let copy_id = copy.id.clone().expect("an id");
                    self.arena.push(copy);
                    let c = self.arena.len() - 1;
                    self.set_by_id(&copy_id, c);
                    f.children.push(c);
                    j += 1;
                }
            } else if fresh {
                f.children.push(b);
            }
            if !inner.is_empty() {
                let next = self.frame(&inner, b);
                stack.push(next);
            }
        }
        self.blocks = self.arena[ROOT].children.clone().unwrap_or_default();
        Ok(())
    }
}
