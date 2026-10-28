//! A tree view's entries nested by their indentation, as Pele's model.ts nests them, and the
//! reading of its text, drawn with box-drawing characters or indented.

use super::boxdrawing::{line_start, preprocess_box_drawing, remap_error_lines};
use super::parser::{TreeViewAst, parse_tree_view};
use crate::errors::{Error, locate};
use crate::js::chars::{drop_indents, substring, trim_start};

/// Whether an entry is a file or a directory, Pele's NodeType.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NodeType {
    File,
    Directory,
}

/// An entry of the tree, Pele's TreeNode. Its id is its index in the model's nodes. What
/// Pele leaves undefined is none.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TreeNode {
    pub(crate) level: i64,
    /// The name as written, without the / that makes a directory.
    pub(crate) name: String,
    pub(crate) node_type: NodeType,
    /// An icon name as written (`pack:name`, `file`, `folder`), or `none` to hide the icon.
    pub(crate) icon: Option<String>,
    pub(crate) css_class: Option<String>,
    pub(crate) description: Option<String>,
    /// The indexes of its entries.
    pub(crate) children: Vec<usize>,
}

/// A parsed tree view, Pele's TreeViewModel. Its first node is a directory named `/` at level
/// -1 that holds the entries at the top, and Pele's count is the number of nodes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TreeViewModel {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) nodes: Vec<TreeNode>,
}

impl TreeViewModel {
    /// The directory that holds the entries at the top.
    pub(crate) fn root(&self) -> &TreeNode {
        &self.nodes[0]
    }
}

/// Mermaid's tree view database, Pele's TreeViewDb: entries arrive in the order of the text
/// with their indentation.
struct TreeViewDb {
    nodes: Vec<TreeNode>,
    /// The indexes of the entries an entry may be put under, the root first.
    stack: Vec<usize>,
}

impl TreeViewDb {
    fn new() -> TreeViewDb {
        let root = TreeNode {
            level: -1,
            name: "/".into(),
            node_type: NodeType::Directory,
            icon: None,
            css_class: None,
            description: None,
            children: Vec::new(),
        };
        TreeViewDb {
            nodes: vec![root],
            stack: vec![0],
        }
    }

    /// Puts an entry under the last entry less indented than it, as Pele's addNode does.
    fn add_node(
        &mut self,
        level: i64,
        name: String,
        node_type: NodeType,
        css_class: Option<String>,
        icon: Option<String>,
        description: Option<String>,
    ) {
        while level <= self.nodes[*self.stack.last().expect("the root")].level {
            self.stack.pop();
        }
        let id = self.nodes.len();
        self.nodes.push(TreeNode {
            level,
            name,
            node_type,
            icon,
            css_class,
            description,
            children: Vec::new(),
        });
        let parent = *self.stack.last().expect("the root");
        self.nodes[parent].children.push(id);
        self.stack.push(id);
    }
}

/// Adds the entries of a tree to the database, as Pele's populate does: a name that ends in /
/// is a directory's, and an empty icon() hides the icon.
fn populate(ast: &TreeViewAst, db: &mut TreeViewDb) {
    let some = |s: &Option<String>| s.clone().filter(|s| !s.is_empty());
    for node in &ast.nodes {
        let level = node.indent.unwrap_or(0) as i64;
        let (name, node_type) = match node.name.strip_suffix('/') {
            Some(name) => (name, NodeType::Directory),
            None => (node.name.as_str(), NodeType::File),
        };
        let icon = node.icon_annotation.as_ref().map(|icon| {
            if icon.is_empty() {
                "none".to_string()
            } else {
                icon.clone()
            }
        });
        db.add_node(
            level,
            name.to_string(),
            node_type,
            some(&node.class_annotation),
            icon,
            some(&node.desc_annotation),
        );
    }
}

/// Reads the source to a syntax tree, as Pele's readTreeView does, taking a tree drawn with
/// box-drawing characters too. Errors name the lines of the original text.
pub(crate) fn read_tree_view(source: &str) -> Result<TreeViewAst, Error> {
    let pre = preprocess_box_drawing(source)?;
    let error = match parse_tree_view(&pre.text) {
        Ok(ast) => return Ok(ast),
        Err(e) => e,
    };
    // The parser already located errors in unchanged text.
    if pre.text == source {
        return Err(error);
    }
    let Some(out) = error.line.checked_sub(1) else {
        return Err(error);
    };
    let Some(&line) = pre.line_map.get(out) else {
        return Err(error);
    };
    let start = line_start(source, line);
    let at = match (error.column.checked_sub(1), pre.columns[out]) {
        (None, _) => 0,
        (Some(at), Some(column)) => column.at(at),
        (Some(at), None) => {
            let line = &source[start..];
            line.len() - substring(line, at).len()
        }
    };
    let written = locate(source, start + at);
    let head = error.message.find('\n').unwrap_or(error.message.len());
    let rest = &error.message[head..];
    let mut message = remap_error_lines(&error.message[..head], &[(error.line, written.line)]);
    if error.snippet.is_empty() {
        message.push_str(rest);
    } else {
        message.push_str(&rest.replacen(&error.snippet, &written.snippet, 1));
    }
    Err(Error {
        message,
        line: written.line,
        column: if error.column > 0 { written.column } else { 0 },
        snippet: if error.snippet.is_empty() {
            String::new()
        } else {
            written.snippet
        },
        ..error
    })
}

/// Makes the model of a tree, as Pele's buildTreeView does. title is the one the front
/// matter gives.
pub(crate) fn build_tree_view(ast: &TreeViewAst, title: Option<&str>) -> TreeViewModel {
    let mut db = TreeViewDb::new();
    populate(ast, &mut db);
    let title = match ast.title.as_deref() {
        Some(t) if !t.is_empty() => Some(t.to_string()),
        _ => title.map(str::to_string),
    };
    TreeViewModel {
        title,
        acc_title: ast
            .acc_title
            .as_deref()
            .filter(|t| !t.is_empty())
            .map(|t| trim_start(t).to_string()),
        acc_descr: ast
            .acc_descr
            .as_deref()
            .filter(|d| !d.is_empty())
            .map(drop_indents),
        nodes: db.nodes,
    }
}
