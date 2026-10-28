//! The model of a Cynefin diagram, as Pele's model.ts makes it.

use super::parser::{CynefinAst, DomainBlockAst, TransitionAst};
use crate::js::chars::{drop_indents, trim_start};
use crate::js::map::Ordered;

/// A domain the text declares and the items it lists in it, Pele's CynefinDomain.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CynefinDomain {
    pub(crate) name: String,
    pub(crate) items: Vec<String>,
}

/// A move from one domain to another, Pele's CynefinTransition. A label Pele leaves undefined
/// is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CynefinTransition {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) label: Option<String>,
}

/// A parsed Cynefin diagram, Pele's CynefinModel. Its domains are those the text declares, in
/// the order Pele's Map of them holds: where each was first declared, with the items of the
/// last block for it. All five are drawn either way.
#[derive(Clone, Debug, Default)]
pub(crate) struct CynefinModel {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) domains: Ordered<CynefinDomain>,
    pub(crate) transitions: Vec<CynefinTransition>,
}

/// What the parser's tree fills, Pele's CynefinDb.
#[derive(Default)]
struct CynefinDb {
    domains: Ordered<CynefinDomain>,
    transitions: Vec<CynefinTransition>,
}

impl CynefinDb {
    /// Adds the domains of blocks, as Pele's setDomains does. A later block for a domain
    /// replaces an earlier one, which keeps its place.
    fn set_domains(&mut self, blocks: Vec<DomainBlockAst>) {
        for block in blocks {
            let domain = CynefinDomain {
                name: block.domain.clone(),
                items: block.items,
            };
            self.domains.set(&block.domain, domain);
        }
    }

    /// Sets the transitions, as Pele's setTransitions does. A transition from a domain to
    /// itself is dropped, and an empty label is none.
    fn set_transitions(&mut self, transitions: Vec<TransitionAst>) {
        self.transitions = transitions
            .into_iter()
            .filter(|t| t.from != t.to)
            .map(|t| CynefinTransition {
                from: t.from,
                to: t.to,
                label: t.label.filter(|l| !l.is_empty()),
            })
            .collect();
    }
}

/// Makes the model of a Cynefin diagram's tree, as Pele's buildCynefin does. A title the text
/// gives takes the place of the one its front matter gives, unless it is empty, as `||` has
/// it.
pub(crate) fn build_cynefin(ast: CynefinAst, title: Option<&str>) -> CynefinModel {
    let mut db = CynefinDb::default();
    db.set_domains(ast.domains);
    db.set_transitions(ast.transitions);
    let given = |s: Option<String>| s.filter(|s| !s.is_empty());
    CynefinModel {
        title: given(ast.title).or_else(|| title.map(str::to_string)),
        acc_title: given(ast.acc_title).map(|s| trim_start(&s).to_string()),
        acc_descr: given(ast.acc_descr).map(|s| drop_indents(&s)),
        domains: db.domains,
        transitions: db.transitions,
    }
}
