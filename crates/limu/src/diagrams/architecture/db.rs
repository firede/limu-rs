//! What an architecture diagram's parser fills, as Pele's ArchitectureDb in db.ts has it: its
//! services, junctions, groups, edges and align directives, checked as Mermaid checks them.

use std::collections::{HashMap, HashSet};

use super::parser::{ArchitectureAst, EdgeAst, GroupAst, JunctionAst, ServiceAst};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::{drop_indents, trim_start};
use crate::js::map::Ordered;

/// A service or a junction, Pele's ArchNode. What Pele leaves undefined is none. parent is
/// Pele's in, the group the node is in.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ArchNode {
    pub(crate) id: String,
    pub(crate) junction: bool,
    /// The name of a service's icon.
    pub(crate) icon: Option<String>,
    /// The text a service shows in place of an icon.
    pub(crate) icon_text: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) parent: Option<String>,
}

/// A group, Pele's ArchGroup.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ArchGroup {
    pub(crate) id: String,
    pub(crate) icon: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) parent: Option<String>,
}

/// An edge, Pele's ArchEdge: from the side lhs_dir of lhs_id to the side rhs_dir of rhs_id.
/// into is set at an end with an arrowhead, and group where the edge leaves the node's group
/// at its border.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ArchEdge {
    pub(crate) lhs_id: String,
    pub(crate) lhs_dir: char,
    pub(crate) lhs_into: bool,
    pub(crate) lhs_group: bool,
    pub(crate) rhs_id: String,
    pub(crate) rhs_dir: char,
    pub(crate) rhs_into: bool,
    pub(crate) rhs_group: bool,
    pub(crate) title: Option<String>,
}

/// An align directive, Pele's ArchLayoutHint: its members in a "row" or a "column", in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ArchLayoutHint {
    pub(crate) direction: String,
    pub(crate) members: Vec<String>,
}

/// Whether an id names a node or a group, as Pele's registered Map holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Registered {
    Node,
    Group,
}

impl Registered {
    fn name(self) -> &'static str {
        match self {
            Registered::Node => "node",
            Registered::Group => "group",
        }
    }
}

/// An architecture diagram as its parser fills it, Pele's ArchitectureDb. Its nodes and
/// groups are in the order they were declared, as Pele's Maps keep them.
#[derive(Clone, Debug, Default)]
pub(crate) struct ArchitectureDb {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) nodes: Ordered<ArchNode>,
    pub(crate) groups: Ordered<ArchGroup>,
    pub(crate) edges: Vec<ArchEdge>,
    pub(crate) layout_hints: Vec<ArchLayoutHint>,
    registered: HashMap<String, Registered>,
}

/// A semantic error of an architecture diagram, as Pele's fail makes it.
fn fail(message: String) -> Error {
    let mut e = Error::new(message, ErrorCode::Semantic);
    e.diagram = Some(DiagramType::Architecture);
    e
}

impl ArchitectureDb {
    fn register(&mut self, kind: &str, id: &str, parent: Option<&str>) -> Result<(), Error> {
        if let Some(was) = self.registered.get(id) {
            return Err(fail(format!(
                "The {kind} id [{id}] is already in use by another {}",
                was.name()
            )));
        }
        if let Some(parent) = parent {
            if id == parent {
                return Err(fail(format!(
                    "The {kind} [{id}] cannot be placed within itself"
                )));
            }
            match self.registered.get(parent) {
                None => {
                    return Err(fail(format!(
                        "The {kind} [{id}]'s parent does not exist. Please make sure the parent \
                         is created before this {kind}"
                    )));
                }
                Some(Registered::Node) => {
                    return Err(fail(format!("The {kind} [{id}]'s parent is not a group")));
                }
                Some(Registered::Group) => {}
            }
        }
        let was = if kind == "group" {
            Registered::Group
        } else {
            Registered::Node
        };
        self.registered.insert(id.to_string(), was);
        Ok(())
    }

    fn add_service(&mut self, s: ServiceAst) -> Result<(), Error> {
        self.register("service", &s.id, s.parent.as_deref())?;
        let node = ArchNode {
            id: s.id.clone(),
            junction: false,
            icon: s.icon,
            icon_text: s.icon_text,
            title: s.title,
            parent: s.parent,
        };
        self.nodes.set(&s.id, node);
        Ok(())
    }

    fn add_junction(&mut self, j: JunctionAst) -> Result<(), Error> {
        self.register("junction", &j.id, j.parent.as_deref())?;
        let node = ArchNode {
            id: j.id.clone(),
            junction: true,
            parent: j.parent,
            ..ArchNode::default()
        };
        self.nodes.set(&j.id, node);
        Ok(())
    }

    fn add_group(&mut self, g: GroupAst) -> Result<(), Error> {
        self.register("group", &g.id, g.parent.as_deref())?;
        let group = ArchGroup {
            id: g.id.clone(),
            icon: g.icon,
            title: g.title,
            parent: g.parent,
        };
        self.groups.set(&g.id, group);
        Ok(())
    }

    fn add_edge(&mut self, e: EdgeAst) -> Result<(), Error> {
        let ends = [
            ("left", &e.lhs_id, e.lhs_group),
            ("right", &e.rhs_id, e.rhs_group),
        ];
        for (side, id, _) in ends {
            if !self.registered.contains_key(id.as_str()) {
                return Err(fail(format!(
                    "The {side}-hand id [{id}] does not yet exist. Please create the \
                     service/group before declaring an edge to it."
                )));
            }
        }
        // Mermaid lets a group id through the check above and then fails with a TypeError.
        for (side, id, _) in ends {
            if !self.nodes.contains(id) {
                return Err(fail(format!(
                    "The {side}-hand id [{id}] is a group. Add {{group}} to a service inside it \
                     instead."
                )));
            }
        }
        let parent = |id: &str| self.nodes.get(id).and_then(|n| n.parent.as_deref());
        if let Some(p) = parent(&e.lhs_id)
            && Some(p) == parent(&e.rhs_id)
        {
            for (side, id, crosses) in ends {
                if crosses {
                    return Err(fail(format!(
                        "The {side}-hand id [{id}] is modified to traverse the group boundary, \
                         but the edge does not pass through two groups."
                    )));
                }
            }
        }
        self.edges.push(ArchEdge {
            lhs_id: e.lhs_id,
            lhs_dir: e.lhs_dir,
            lhs_into: e.lhs_into,
            lhs_group: e.lhs_group,
            rhs_id: e.rhs_id,
            rhs_dir: e.rhs_dir,
            rhs_into: e.rhs_into,
            rhs_group: e.rhs_group,
            title: e.title,
        });
        Ok(())
    }

    fn add_layout_hint(&mut self, hint: ArchLayoutHint) -> Result<(), Error> {
        if hint.members.len() < 2 {
            return Err(fail(format!(
                "An align directive requires at least two members; got {}",
                hint.members.len()
            )));
        }
        let mut seen = HashSet::new();
        for id in &hint.members {
            if self.registered.get(id) != Some(&Registered::Node) {
                return Err(fail(format!(
                    "align {} references [{id}], which is not a service or junction",
                    hint.direction
                )));
            }
            if !seen.insert(id.as_str()) {
                return Err(fail(format!(
                    "align {} lists [{id}] more than once",
                    hint.direction
                )));
            }
        }
        self.layout_hints.push(hint);
        Ok(())
    }
}

/// Fills the db from the tree, as Pele's populate does after Mermaid's populateDb: every
/// group, then services, junctions, edges, and alignments.
pub(crate) fn populate(ast: ArchitectureAst, db: &mut ArchitectureDb) -> Result<(), Error> {
    if let Some(d) = ast.acc_descr.filter(|s| !s.is_empty()) {
        db.acc_descr = Some(drop_indents(&d));
    }
    if let Some(t) = ast.acc_title.filter(|s| !s.is_empty()) {
        db.acc_title = Some(trim_start(&t).to_string());
    }
    if let Some(t) = ast.title.filter(|s| !s.is_empty()) {
        db.title = Some(t);
    }
    for group in ast.groups {
        db.add_group(group)?;
    }
    for service in ast.services {
        db.add_service(service)?;
    }
    for junction in ast.junctions {
        db.add_junction(junction)?;
    }
    for edge in ast.edges {
        db.add_edge(edge)?;
    }
    for alignment in ast.alignments {
        db.add_layout_hint(ArchLayoutHint {
            direction: alignment.direction,
            members: alignment.members,
        })?;
    }
    Ok(())
}
