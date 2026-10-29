//! The commits and branches of a git graph, as Pele's db.ts replays the statements of its
//! tree into them, as Mermaid's gitGraphDb does.

use super::parser::{GitGraphAst, StatementAst};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::{drop_indents, trim_start};
use crate::js::map::Ordered;
use crate::js::math::by;
use crate::value::{Map, Value};

// The types of commit, Pele's commitType.
pub(crate) const NORMAL: u8 = 0;
pub(crate) const REVERSE: u8 = 1;
pub(crate) const HIGHLIGHT: u8 = 2;
pub(crate) const MERGE: u8 = 3;
pub(crate) const CHERRY_PICK: u8 = 4;

/// A commit, Pele's GitCommit. What Pele leaves undefined is none: a commit's custom_type
/// and custom_id are a merge's.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GitCommit {
    pub(crate) id: String,
    pub(crate) message: String,
    pub(crate) seq: usize,
    pub(crate) kind: u8,
    pub(crate) tags: Vec<String>,
    pub(crate) parents: Vec<String>,
    pub(crate) branch: String,
    pub(crate) custom_type: Option<u8>,
    pub(crate) custom_id: Option<bool>,
}

/// A branch, and the id of its latest commit, or none while it has none, as Pele's map of
/// branches holds it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Branch {
    pub(crate) name: String,
    pub(crate) head: Option<String>,
}

/// A git graph, Pele's GitModel. Its branches, current branch, head and warnings are read
/// only by the tests that compare it with Pele's.
#[derive(Clone, Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct GitModel {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    /// "LR", "TB" or "BT".
    pub(crate) direction: &'static str,
    /// Keyed by commit id, in the order the ids first appeared.
    pub(crate) commits: Ordered<GitCommit>,
    /// In the order they were made, the main branch first.
    pub(crate) branches: Ordered<Branch>,
    /// Branch names in drawing order: by order, then by creation.
    pub(crate) lanes: Vec<String>,
    pub(crate) current_branch: String,
    /// The id of the commit at the head, Pele's head.
    pub(crate) head: Option<String>,
    pub(crate) warnings: Vec<String>,
}

/// A semantic error of a git graph, as Pele's fail throws it.
fn fail(message: String) -> Error {
    let mut e = Error::new(message, ErrorCode::Semantic);
    e.diagram = Some(DiagramType::GitGraph);
    e
}

/// The type of a commit of a name, as Pele's typeCode gives it.
fn type_code(name: &str) -> u8 {
    match name {
        "REVERSE" => REVERSE,
        "HIGHLIGHT" => HIGHLIGHT,
        _ => NORMAL,
    }
}

/// What buildGit keeps as it replays the statements.
struct Build {
    commits: Ordered<GitCommit>,
    branches: Ordered<Branch>,
    orders: Vec<(String, Option<f64>)>,
    warnings: Vec<String>,
    head: Option<String>,
    current: String,
    direction: &'static str,
    seq: usize,
}

impl Build {
    /// A new id, as Pele's nextId makes it. Mermaid appends seven random hex digits to the
    /// sequence number. Pele derives them from the sequence number instead, so the same text
    /// always gives the same ids.
    fn next_id(&self) -> String {
        // The salt is ToInt32'd by Math.imul, so it is kept modulo 2^32.
        let mut salt = (self.seq as u32).wrapping_add(1);
        loop {
            let hex = format!("{:08x}", salt.wrapping_mul(0x9e37_79b1));
            let id = format!("{}-{}", self.seq, &hex[..7]);
            if !self.commits.contains(&id) {
                return id;
            }
            salt = salt.wrapping_add(0x9e37_79b1);
        }
    }

    /// The parents of a commit made at the head, as Pele lists them: none without a head,
    /// else the head and then the others.
    fn at_head(&self, others: &[String]) -> Vec<String> {
        match &self.head {
            None => Vec::new(),
            Some(h) => std::iter::once(h.clone())
                .chain(others.iter().cloned())
                .collect(),
        }
    }

    fn add(&mut self, commit: GitCommit) {
        let id = commit.id.clone();
        self.head = Some(id.clone());
        self.commits.set(&id, commit);
        let current = self.current.clone();
        if let Some(b) = self.branches.get_mut(&current) {
            b.head = Some(id);
        }
    }

    /// The commit a branch's head is, or none.
    fn head_of(&self, branch: &str) -> Option<&GitCommit> {
        let id = self.branches.get(branch)?.head.as_deref()?;
        self.commits.get(id)
    }

    fn checkout(&mut self, branch: &str) -> Result<(), Error> {
        let Some(b) = self.branches.get(branch) else {
            return Err(fail(format!(
                "Trying to checkout branch which is not yet created. (Help try using \"branch {branch}\")"
            )));
        };
        self.head = b
            .head
            .as_deref()
            .filter(|id| self.commits.contains(id))
            .map(str::to_string);
        self.current = branch.to_string();
        Ok(())
    }

    fn commit(&mut self, msg: String, id: Option<String>, kind: u8, tags: Vec<String>) {
        let id = match id.filter(|id| !id.is_empty()) {
            Some(id) => id,
            None => self.next_id(),
        };
        let commit = GitCommit {
            id,
            message: msg,
            seq: self.seq,
            kind,
            tags,
            parents: self.at_head(&[]),
            branch: self.current.clone(),
            custom_type: None,
            custom_id: None,
        };
        self.seq += 1;
        if self.commits.contains(&commit.id) {
            self.warnings
                .push(format!("Commit ID {} already exists", commit.id));
        }
        self.add(commit);
    }

    fn branch(&mut self, name: &str, order: Option<f64>) -> Result<(), Error> {
        if self.branches.contains(name) {
            return Err(fail(format!(
                "Trying to create an existing branch. (Help: Either use a new name if you want create a new branch or try using \"checkout {name}\")"
            )));
        }
        let branch = Branch {
            name: name.to_string(),
            head: self.head.clone(),
        };
        self.branches.set(name, branch);
        self.orders.push((name.to_string(), order));
        self.checkout(name)
    }

    fn merge(
        &mut self,
        other: &str,
        custom_id: &str,
        kind: Option<u8>,
        tags: Vec<String>,
    ) -> Result<(), Error> {
        let current = self.current.clone();
        let other_head = self.branches.get(other).and_then(|b| b.head.clone());
        let current_commit = self.head_of(&current);
        let other_commit = self.head_of(other);
        if let (Some(c), Some(_)) = (current_commit, other_commit)
            && c.branch == other
        {
            return Err(fail(format!("Cannot merge branch '{other}' into itself.")));
        }
        if current == other {
            return Err(fail(
                "Incorrect usage of \"merge\". Cannot merge a branch to itself".into(),
            ));
        }
        let Some(current_commit) = current_commit else {
            return Err(fail(format!(
                "Incorrect usage of \"merge\". Current branch ({current})has no commits"
            )));
        };
        if !self.branches.contains(other) {
            return Err(fail(format!(
                "Incorrect usage of \"merge\". Branch to be merged ({other}) does not exist"
            )));
        }
        let Some(other_commit) = other_commit else {
            return Err(fail(format!(
                "Incorrect usage of \"merge\". Branch to be merged ({other}) has no commits"
            )));
        };
        if current_commit.id == other_commit.id {
            return Err(fail(
                "Incorrect usage of \"merge\". Both branches have same head".into(),
            ));
        }
        if !custom_id.is_empty() && self.commits.contains(custom_id) {
            return Err(fail(format!(
                "Incorrect usage of \"merge\". Commit with id:{custom_id} already exists, use different custom id"
            )));
        }
        let id = if custom_id.is_empty() {
            self.next_id()
        } else {
            custom_id.to_string()
        };
        let commit = GitCommit {
            id,
            message: format!("merged branch {other} into {current}"),
            seq: self.seq,
            parents: self.at_head(&[other_head.unwrap_or_default()]),
            branch: current,
            kind: MERGE,
            custom_type: kind,
            custom_id: Some(!custom_id.is_empty()),
            tags,
        };
        self.seq += 1;
        self.add(commit);
        Ok(())
    }

    fn cherry_pick(
        &mut self,
        source_id: Option<&str>,
        target_id: &str,
        tags: Option<Vec<String>>,
        parent: Option<&str>,
    ) -> Result<(), Error> {
        let source = source_id
            .filter(|id| !id.is_empty())
            .and_then(|id| self.commits.get(id));
        let Some(source) = source else {
            return Err(fail(
                "Incorrect usage of \"cherryPick\". Source commit id should exist and provided"
                    .into(),
            ));
        };
        let parent = parent.filter(|p| !p.is_empty());
        if let Some(p) = parent
            && !source.parents.iter().any(|q| q == p)
        {
            return Err(fail(
                "Invalid operation: The specified parent commit is not an immediate parent of the cherry-picked commit.".into(),
            ));
        }
        if source.kind == MERGE && parent.is_none() {
            return Err(fail(
                "Incorrect usage of cherry-pick: If the source commit is a merge commit, an immediate parent commit must be specified.".into(),
            ));
        }
        if !target_id.is_empty() && self.commits.contains(target_id) {
            return Ok(());
        }
        let current = self.current.clone();
        if source.branch == current {
            return Err(fail(
                "Incorrect usage of \"cherryPick\". Source commit is already on current branch"
                    .into(),
            ));
        }
        let current_head = self.branches.get(&current).and_then(|b| b.head.as_deref());
        if !current_head.is_some_and(|h| self.commits.contains(h)) {
            return Err(fail(format!(
                "Incorrect usage of \"cherry-pick\". Current branch ({current})has no commits"
            )));
        }
        let (source_id, message, merged) = (
            source.id.clone(),
            source.message.clone(),
            source.kind == MERGE,
        );
        let tags = match tags {
            Some(tags) => tags.into_iter().filter(|t| !t.is_empty()).collect(),
            None => {
                let mut tag = format!("cherry-pick:{source_id}");
                if merged {
                    tag += &format!("|parent:{}", parent.unwrap_or("undefined"));
                }
                vec![tag]
            }
        };
        let commit = GitCommit {
            id: self.next_id(),
            message: format!("cherry-picked {message} into {current}"),
            seq: self.seq,
            parents: self.at_head(std::slice::from_ref(&source_id)),
            branch: current,
            kind: CHERRY_PICK,
            tags,
            custom_type: None,
            custom_id: None,
        };
        self.seq += 1;
        self.add(commit);
        Ok(())
    }

    /// Replays a statement, as Pele's parseStatement does.
    fn statement(&mut self, statement: &StatementAst) -> Result<(), Error> {
        match statement {
            StatementAst::Commit {
                id,
                message,
                tags,
                kind,
            } => {
                let kind = kind.map_or(NORMAL, type_code);
                let msg = message.clone().unwrap_or_default();
                self.commit(msg, id.clone(), kind, tags.clone());
                Ok(())
            }
            StatementAst::Branch { name, order } => self.branch(name, Some(order.unwrap_or(0.0))),
            StatementAst::Merge {
                branch,
                id,
                tags,
                kind,
            } => self.merge(
                branch,
                id.as_deref().unwrap_or(""),
                kind.map(type_code),
                tags.clone(),
            ),
            StatementAst::Checkout { branch } => self.checkout(branch),
            StatementAst::CherryPicking { id, tags, parent } => self.cherry_pick(
                id.as_deref(),
                "",
                (!tags.is_empty()).then(|| tags.clone()),
                parent.as_deref(),
            ),
        }
    }
}

/// Replays a git graph's tree into its model, as Pele's buildGit does. title is the one its
/// front matter gives.
pub(crate) fn build_git(
    ast: GitGraphAst,
    config: &Map,
    title: Option<&str>,
) -> Result<GitModel, Error> {
    let git = config.get("gitGraph").and_then(Value::as_object);
    let main_name = match git.and_then(|g| g.get("mainBranchName")) {
        Some(Value::String(s)) => s.clone(),
        _ => "main".to_string(),
    };
    let main_order = match git.and_then(|g| g.get("mainBranchOrder")) {
        Some(&Value::Number(n)) => n,
        _ => 0.0,
    };

    let mut branches = Ordered::default();
    branches.set(
        &main_name,
        Branch {
            name: main_name.clone(),
            head: None,
        },
    );
    let mut b = Build {
        commits: Ordered::default(),
        branches,
        orders: vec![(main_name.clone(), Some(main_order))],
        warnings: Vec::new(),
        head: None,
        current: main_name,
        direction: "LR",
        seq: 0,
    };
    if let Some(dir) = ast.dir {
        b.direction = dir;
    }
    for statement in &ast.statements {
        b.statement(statement)?;
    }

    // A branch made without an order sorts by creation among those of order zero, as in
    // Mermaid.
    let mut lanes: Vec<(String, f64)> = b
        .orders
        .iter()
        .enumerate()
        .map(|(i, (name, order))| {
            let order = order.unwrap_or_else(|| format!("0.{i}").parse().unwrap_or(f64::NAN));
            (name.clone(), order)
        })
        .collect();
    lanes.sort_by(|a, b| by(a.1 - b.1));

    Ok(GitModel {
        title: ast
            .title
            .filter(|t| !t.is_empty())
            .or_else(|| title.map(str::to_string)),
        acc_title: ast
            .acc_title
            .filter(|t| !t.is_empty())
            .map(|t| trim_start(&t).to_string()),
        acc_descr: ast
            .acc_descr
            .filter(|t| !t.is_empty())
            .map(|t| drop_indents(&t)),
        direction: b.direction,
        commits: b.commits,
        branches: b.branches,
        lanes: lanes.into_iter().map(|(name, _)| name).collect(),
        current_branch: b.current,
        head: b.head,
        warnings: b.warnings,
    })
}
