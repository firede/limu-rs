//! What an Ishikawa diagram's parser fills, as Pele's IshikawaDb in db.ts has it: the effect
//! and its causes, as a tree read from their indentation.

/// The effect or a cause, Pele's IshikawaNode. Its children are indices into the db's nodes.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct IshikawaNode {
    pub(crate) text: String,
    pub(crate) children: Vec<usize>,
}

/// An Ishikawa diagram as its parser fills it, Pele's IshikawaDb. The root is the effect,
/// the first node. Its children are the categories of causes, theirs the causes, and so on.
/// A diagram with no lines after its keyword has no nodes.
#[derive(Clone, Debug, Default)]
pub(crate) struct IshikawaDb {
    pub(crate) title: Option<String>,
    pub(crate) nodes: Vec<IshikawaNode>,

    /// The line of ancestors of the last node, each with its level.
    stack: Vec<(usize, usize)>,
    base_level: Option<usize>,
}

impl IshikawaDb {
    /// An empty db, of the title its front matter gives.
    pub(crate) fn new(title: Option<&str>) -> IshikawaDb {
        IshikawaDb {
            title: title.map(str::to_string),
            ..IshikawaDb::default()
        }
    }

    /// The effect, if the diagram has one.
    pub(crate) fn root(&self) -> Option<&IshikawaNode> {
        self.nodes.first()
    }

    /// Adds a line of the outline, as Pele's addNode does. The first line is the effect.
    /// Every later line is a cause, nested by its indentation relative to the first cause,
    /// so the effect itself may be indented more than the causes.
    pub(crate) fn add_node(&mut self, raw_level: usize, text: &str) {
        let index = self.nodes.len();
        self.nodes.push(IshikawaNode {
            text: text.to_string(),
            children: Vec::new(),
        });
        if index == 0 {
            self.stack = vec![(0, 0)];
            return;
        }
        let base = *self.base_level.get_or_insert(raw_level);
        // Math.max(rawLevel - baseLevel + 1, 1), where the level may be under the base.
        let level = (raw_level + 1).saturating_sub(base).max(1);
        while self.stack.len() > 1 && self.stack[self.stack.len() - 1].0 >= level {
            self.stack.pop();
        }
        let parent = self.stack[self.stack.len() - 1].1;
        self.nodes[parent].children.push(index);
        self.stack.push((level, index));
    }
}
