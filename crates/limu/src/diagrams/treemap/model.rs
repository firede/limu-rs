//! The model of a treemap, as Pele's model.ts makes it.

use std::collections::HashMap;

use super::parser::{RowAst, TreemapAst};
use crate::js::chars::{drop_indents, trim_start};

/// A section or a leaf of a treemap, Pele's TreemapNode. A section has a list of children,
/// perhaps empty, by their places among the model's nodes, and a leaf has none, and a value,
/// NaN where its text gives no number. css is the declarations of the node's class, Pele's
/// cssCompiledStyles, where the class is defined with some.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TreemapNode {
    pub(crate) name: String,
    pub(crate) children: Option<Vec<usize>>,
    pub(crate) value: Option<f64>,
    pub(crate) class_selector: Option<String>,
    pub(crate) css: Option<Vec<String>>,
}

/// A parsed treemap, Pele's TreemapModel. Its nodes are kept in one list, each before its
/// children, so that a tree however deep is walked without recursion. classes are the
/// declarations of each class, in the order the classes are first defined.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TreemapModel {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) nodes: Vec<TreemapNode>,
    pub(crate) roots: Vec<usize>,
    pub(crate) classes: Vec<(String, Vec<String>)>,
}

/// A row of the treemap, Pele's TreemapItem: a leaf has a value.
struct TreemapItem {
    level: usize,
    name: String,
    value: Option<f64>,
    leaf: bool,
    class_selector: Option<String>,
    css: Option<Vec<String>>,
}

/// Nests the rows, as Pele's buildHierarchy does: a row belongs to the nearest section above
/// it that is indented less. It gives the nodes and the roots.
fn build_hierarchy(items: Vec<TreemapItem>) -> (Vec<TreemapNode>, Vec<usize>) {
    let mut nodes: Vec<TreemapNode> = Vec::with_capacity(items.len());
    let mut roots = Vec::new();
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for item in items {
        let i = nodes.len();
        nodes.push(TreemapNode {
            name: item.name,
            children: (!item.leaf).then(Vec::new),
            value: if item.leaf { item.value } else { None },
            class_selector: item.class_selector,
            css: item.css,
        });
        while stack.last().is_some_and(|&(_, level)| level >= item.level) {
            stack.pop();
        }
        match stack.last() {
            None => roots.push(i),
            Some(&(parent, _)) => {
                if let Some(children) = &mut nodes[parent].children {
                    children.push(i);
                }
            }
        }
        if !item.leaf {
            stack.push((i, item.level));
        }
    }
    (nodes, roots)
}

/// Splits the text of a classDef into declarations, as Pele's buildTreemap does: an escaped
/// comma stays in its value, and other commas and semicolons separate them. What stands for
/// the escaped comma meanwhile, §§§, is a comma after, wherever it was.
fn split_styles(text: &str) -> Vec<String> {
    text.replace("\\,", "§§§")
        .replace(',', ";")
        .replace("§§§", ",")
        .split(';')
        .map(str::to_string)
        .collect()
}

/// Makes the model of a treemap's tree, as Pele's buildTreemap does: the classes first, from
/// every classDef wherever it is, then the rows nested into a tree. A title the text gives
/// takes the place of the one its front matter gives, unless it is empty, as `||` has it.
pub(crate) fn build_treemap(ast: TreemapAst, title: Option<&str>) -> TreemapModel {
    let mut classes: Vec<(String, Vec<String>)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for row in &ast.rows {
        let RowAst::ClassDef {
            class_name,
            style_text,
            ..
        } = row
        else {
            continue;
        };
        let k = *index.entry(class_name.clone()).or_insert_with(|| {
            classes.push((class_name.clone(), Vec::new()));
            classes.len() - 1
        });
        classes[k]
            .1
            .extend(split_styles(style_text.as_deref().unwrap_or("")));
    }

    let mut items = Vec::new();
    for row in ast.rows {
        let RowAst::Row { indent, item } = row else {
            continue;
        };
        let css = item
            .class_selector
            .as_ref()
            .filter(|c| !c.is_empty())
            .and_then(|c| index.get(c))
            .map(|&k| &classes[k].1)
            .filter(|styles| !styles.is_empty())
            .cloned();
        items.push(TreemapItem {
            level: indent.unwrap_or(0),
            leaf: item.value.is_some(),
            name: item.name,
            value: item.value,
            class_selector: item.class_selector,
            css,
        });
    }

    let (nodes, roots) = build_hierarchy(items);
    let given = |s: Option<String>| s.filter(|s| !s.is_empty());
    TreemapModel {
        title: given(ast.title).or_else(|| title.map(str::to_string)),
        acc_title: given(ast.acc_title).map(|s| trim_start(&s).to_string()),
        acc_descr: given(ast.acc_descr).map(|s| drop_indents(&s)),
        nodes,
        roots,
        classes,
    }
}
