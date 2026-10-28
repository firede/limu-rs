//! The parser of C4 diagrams, as Pele's parser.ts reads them: it accepts what the parser
//! Jison generates from c4Diagram.jison accepts, and calls the model builder in the order its
//! semantic actions do. Boundaries nest through a counter, not recursion.

use super::db::{C4Attr, C4Db, C4Named};
use super::lexer::*;
use crate::detect::DiagramType;
use crate::errors::{Error, syntax_error};
use crate::js::chars::{substring, trim};

/// Mermaid's typeC4Shape for each shape keyword, indexed from PERSON, Pele's KINDS.
const KINDS: [&str; 20] = [
    "person",
    "external_person",
    "system",
    "system_db",
    "system_queue",
    "external_system",
    "external_system_db",
    "external_system_queue",
    "container",
    "container_db",
    "container_queue",
    "external_container",
    "external_container_db",
    "external_container_queue",
    "component",
    "component_db",
    "component_queue",
    "external_component",
    "external_component_db",
    "external_component_queue",
];
const REL_TYPES: [&str; 8] = [
    "rel", "birel", "rel_u", "rel_d", "rel_l", "rel_r", "rel_b", "rel",
];
const NODE_TYPES: [&str; 3] = ["node", "nodeL", "nodeR"];

const HEADERS: &str = "'direction_tb', 'direction_bt', 'direction_rl', 'direction_lr', 'C4_CONTEXT', 'C4_CONTAINER', 'C4_COMPONENT', 'C4_DYNAMIC', 'C4_DEPLOYMENT'";
const ATTRIBUTES: &str = "'STR', 'STR_KEY', 'ATTRIBUTE', 'ATTRIBUTE_EMPTY'";

/// What may come next, Pele's P.
#[derive(Clone, Copy, PartialEq)]
enum P {
    /// A statement is required.
    Item,
    /// Only a diagram statement may come next: the first one in a boundary, or one written
    /// straight after a title.
    Diagram,
    ItemOrEnd,
    AfterOther,
    AfterDiagram,
}

fn is_other(t: usize) -> bool {
    matches!(
        t,
        TITLE | ACC_DESCRIPTION | ACC_TITLE | ACC_DESCR | ACC_DESCR_MULTILINE_VALUE
    )
}

fn is_diagram(t: usize) -> bool {
    (PERSON..=UPDATE_LAYOUT_CONFIG).contains(&t)
}

/// Reads tokens, as the closures of Pele's parseC4 do.
struct Parser<'a> {
    src: &'a str,
    t: Tokens,
    i: usize,
}

impl<'a> Parser<'a> {
    fn kind(&self) -> usize {
        self.t.types[self.i]
    }

    fn text(&self, at: usize) -> &'a str {
        &self.src[self.t.starts[at]..self.t.ends[at]]
    }

    fn fail(&self, expected: &str) -> Error {
        let t = self.kind();
        let at = self.t.starts[self.i];
        if t == ERROR {
            return syntax_error(DiagramType::C4, self.src, at, "", true);
        }
        let detail = format!("Expecting {expected}, got '{}'", TOKEN_NAMES[t]);
        syntax_error(DiagramType::C4, self.src, at, &detail, false)
    }

    fn expect(&mut self, kind: usize) -> Result<(), Error> {
        if self.kind() != kind {
            return Err(self.fail(&format!("'{}'", TOKEN_NAMES[kind])));
        }
        self.i += 1;
        Ok(())
    }

    fn attributes(&mut self) -> Result<Vec<C4Attr>, Error> {
        let mut attrs = Vec::new();
        loop {
            match self.kind() {
                STR => {
                    attrs.push(C4Attr::Text(trim(self.text(self.i)).to_string()));
                    self.i += 1;
                }
                ATTRIBUTE_EMPTY => {
                    attrs.push(C4Attr::Text(String::new()));
                    self.i += 1;
                }
                STR_KEY => {
                    self.i += 1;
                    if self.kind() != STR_VALUE {
                        return Err(self.fail("'STR_VALUE'"));
                    }
                    attrs.push(C4Attr::Named(C4Named {
                        key: trim(self.text(self.i - 1)).to_string(),
                        value: trim(self.text(self.i)).to_string(),
                    }));
                    self.i += 1;
                }
                _ => break,
            }
        }
        if attrs.is_empty() {
            return Err(self.fail(ATTRIBUTES));
        }
        Ok(attrs)
    }

    fn other(&mut self, t: usize, db: &mut C4Db) -> Result<(), Error> {
        match t {
            TITLE => {
                db.set_title(substring(self.text(self.i), 6));
                self.i += 1;
            }
            ACC_DESCRIPTION => {
                db.set_acc_description(substring(self.text(self.i), 15));
                self.i += 1;
            }
            ACC_DESCR_MULTILINE_VALUE => {
                db.set_acc_description(trim(self.text(self.i)));
                self.i += 1;
            }
            _ => {
                self.i += 1;
                if t == ACC_TITLE {
                    if self.kind() != ACC_TITLE_VALUE {
                        return Err(self.fail("'acc_title_value'"));
                    }
                    // The grammar hands `accTitle:` to setTitle, so it becomes the visible
                    // title.
                    db.set_title(trim(self.text(self.i)));
                } else {
                    if self.kind() != ACC_DESCR_VALUE {
                        return Err(self.fail("'acc_descr_value'"));
                    }
                    db.set_acc_description(trim(self.text(self.i)));
                }
                self.i += 1;
            }
        }
        Ok(())
    }

    /// Reads a diagram statement, and gives whether it opens a boundary.
    fn diagram(&mut self, t: usize, db: &mut C4Db) -> Result<bool, Error> {
        self.i += 1;
        let mut attrs = self.attributes()?;
        if t <= SYSTEM_EXT_QUEUE {
            db.add_person_or_system(KINDS[t - PERSON], &attrs);
        } else if t <= CONTAINER_EXT_QUEUE {
            db.add_container(KINDS[t - PERSON], &attrs);
        } else if t <= COMPONENT_EXT_QUEUE {
            db.add_component(KINDS[t - PERSON], &attrs);
        } else if t <= NODE_R {
            let kind = match t {
                ENTERPRISE_BOUNDARY => Some("ENTERPRISE"),
                SYSTEM_BOUNDARY => Some("SYSTEM"),
                CONTAINER_BOUNDARY => Some("CONTAINER"),
                _ => None,
            };
            if let Some(kind) = kind {
                // splice(2, 0, kind) puts it at the end where there are fewer.
                let at = attrs.len().min(2);
                attrs.insert(at, C4Attr::Text(kind.to_string()));
            }
            if t >= NODE {
                db.add_deployment_node(NODE_TYPES[t - NODE], &attrs);
            } else if t == CONTAINER_BOUNDARY {
                db.add_container_boundary(&attrs);
            } else {
                db.add_person_or_system_boundary(&attrs, "system");
            }
            if self.kind() == LBRACE {
                self.i += 1;
                self.expect(NEWLINE)?;
            } else if self.kind() == NEWLINE {
                self.i += 1;
                self.expect(LBRACE)?;
                if self.kind() == NEWLINE {
                    self.i += 1;
                }
            } else {
                return Err(self.fail("'NEWLINE', 'LBRACE'"));
            }
            return Ok(true);
        } else if t <= REL_INDEX {
            // RelIndex takes a sequence number first, which Mermaid ignores.
            if t == REL_INDEX {
                attrs.remove(0);
            }
            db.add_rel(REL_TYPES[t - REL], &attrs);
        } else if t == UPDATE_EL_STYLE {
            db.update_el_style(&attrs);
        } else if t == UPDATE_REL_STYLE {
            db.update_rel_style(&attrs);
        } else {
            db.update_layout_config(&attrs);
        }
        Ok(false)
    }
}

/// Parses a C4 diagram's text into db, as Pele's parseC4 does.
pub(crate) fn parse_c4(src: &str, db: &mut C4Db) -> Result<(), Error> {
    let mut r = Parser {
        src,
        t: tokenize(src),
        i: 0,
    };

    let header = r.kind();
    if !(C4_CONTEXT..=C4_DEPLOYMENT).contains(&header) {
        return Err(r.fail(HEADERS));
    }
    r.i += 1;
    r.expect(NEWLINE)?;

    let mut depth = 0;
    let mut state = P::Item;
    loop {
        let t = r.kind();
        if state == P::AfterOther || state == P::AfterDiagram {
            if t == NEWLINE {
                r.i += 1;
                state = P::ItemOrEnd;
                continue;
            }
            if state == P::AfterOther && is_diagram(t) {
                state = P::Diagram;
                continue;
            }
        } else if is_diagram(t) {
            if r.diagram(t, db)? {
                depth += 1;
                state = P::Diagram;
            } else {
                state = P::AfterDiagram;
            }
            continue;
        } else if is_other(t) && state != P::Diagram {
            r.other(t, db)?;
            state = P::AfterOther;
            continue;
        } else if state != P::ItemOrEnd {
            return Err(r.fail(
                "'title', 'accDescription', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'PERSON', 'REL'",
            ));
        }

        if depth > 0 {
            if t != RBRACE {
                return Err(r.fail("'NEWLINE', 'RBRACE'"));
            }
            r.i += 1;
            db.pop_boundary_parse_stack();
            depth -= 1;
            state = P::AfterDiagram;
        } else {
            if t != EOF {
                return Err(r.fail("'NEWLINE', 'EOF'"));
            }
            r.i += 1;
            r.expect(END)?;
            db.set_c4_type(r.text(0));
            return Ok(());
        }
    }
}
