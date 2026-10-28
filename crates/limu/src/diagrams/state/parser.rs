//! The parser of a state diagram, as Pele's parser.ts has it: a loop over the tokens of its
//! lexer that agrees with Mermaid's stateDiagram.jison, its errors worded as Jison words
//! them. Composites are read without recursion.

use super::db::{Description, StateDb, StateNote, StateStmt, Stmt};
use super::lexer::{Tok, tokenize};
use super::tokens::{DIRECTION_TOKENS, T};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode, locate, syntax_error};
use crate::js::chars::trim;

const DIRECTIONS: [&str; 4] = ["TB", "BT", "RL", "LR"];
const STATEMENT: &str = "'NL', 'ID', 'HIDE_EMPTY', 'scale', 'COMPOSIT_STATE', 'STATE_DESCR', 'FORK', 'JOIN', 'CHOICE', 'CONCURRENT', 'note', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'CLICK', 'classDef', 'style', 'class', 'direction_tb', 'direction_bt', 'direction_rl', 'direction_lr', 'EDGE_STATE'";

struct Parser<'a> {
    src: &'a str,
    toks: Vec<Tok<'a>>,
    error: Option<String>,
    i: usize,
}

impl Parser<'_> {
    fn kind(&self) -> T {
        self.toks[self.i].kind
    }

    /// The text of the token at i, and moves past it.
    fn take(&mut self) -> String {
        let s = self.toks[self.i].text.to_string();
        self.i += 1;
        s
    }

    fn fail(&self, expected: &str) -> Error {
        let tok = &self.toks[self.i];
        if tok.kind == T::ERROR {
            if let Some(message) = &self.error {
                let loc = locate(self.src, tok.start);
                let mut e = Error::new(message.clone(), ErrorCode::Syntax);
                e.diagram = Some(DiagramType::State);
                (e.line, e.column, e.snippet) = (loc.line, loc.column, loc.snippet);
                return e;
            }
            return syntax_error(DiagramType::State, self.src, tok.start, "", true);
        }
        let detail = format!("Expecting {expected}, got '{}'", tok.kind.name());
        syntax_error(DiagramType::State, self.src, tok.start, &detail, false)
    }

    fn expect(&mut self, kind: T) -> Result<String, Error> {
        if self.kind() != kind {
            return Err(self.fail(&format!("'{}'", kind.name())));
        }
        Ok(self.take())
    }

    /// A state an id or `[*]` names, with a class after `:::`, as Pele's idStatement reads it.
    fn id_statement(&mut self) -> Result<StateStmt, Error> {
        if !matches!(self.kind(), T::ID | T::EDGE_STATE) {
            return Err(self.fail("'ID', 'EDGE_STATE'"));
        }
        let id = trim(&self.take()).to_string();
        let mut s = StateStmt::new(id, "default");
        s.description = Some(Description::One(String::new()));
        if self.kind() != T::STYLE_SEPARATOR {
            return Ok(s);
        }
        self.i += 1;
        s.classes = Some(vec![trim(&self.expect(T::ID)?).to_string()]);
        Ok(s)
    }
}

/// The documents being read: the current one, and those it is inside, as Pele's doc and outer
/// hold them.
struct Docs {
    doc: Vec<Stmt>,
    outer: Vec<Vec<Stmt>>,
}

impl Docs {
    /// Adds a statement, unless its value is the string the grammar uses for a blank line,
    /// which it drops, as Pele's add does.
    fn add(&mut self, value: Stmt) {
        if !matches!(&value, Stmt::Text(t) if t == "nl") {
            self.doc.push(value);
        }
    }

    /// Opens a composite of an id and a description, whose statements follow, as Pele's open
    /// does.
    fn open(&mut self, id: String, description: String) {
        let mut s = StateStmt::new(id, "default");
        s.description = Some(Description::One(description));
        s.doc = Some(Vec::new());
        self.outer.push(std::mem::take(&mut self.doc));
        // The composite goes into its document when the document is closed, as its own
        // statements are read into it.
        self.outer
            .last_mut()
            .expect("the outer document")
            .push(Stmt::State(s));
    }

    /// Closes the current composite's document, and goes back to the one it is in.
    fn close(&mut self) {
        let inner = std::mem::take(&mut self.doc);
        self.doc = self.outer.pop().expect("an open composite");
        if let Some(Stmt::State(s)) = self.doc.last_mut() {
            s.doc = Some(inner);
        }
    }
}

/// Parses a state diagram's text into db, as Pele's parseState does.
pub(crate) fn parse_state(src: &str, db: &mut StateDb) -> Result<(), Error> {
    let tokens = tokenize(src);
    let mut p = Parser {
        src,
        toks: tokens.toks,
        error: tokens.error,
        i: 0,
    };

    while p.kind() == T::NL {
        p.i += 1;
    }
    if p.kind() != T::SD {
        return Err(p.fail("'NL', 'SD'"));
    }
    p.i += 1;

    let mut d = Docs {
        doc: Vec::new(),
        outer: Vec::new(),
    };
    loop {
        let t = p.kind();
        match t {
            T::NL => p.i += 1,
            T::ID | T::EDGE_STATE => {
                let mut first = p.id_statement()?;
                if p.kind() == T::DESCR {
                    first.description = Some(Description::One(StateDb::trim_colon(&p.take())));
                    d.add(Stmt::State(first));
                } else if p.kind() == T::ARROW {
                    p.i += 1;
                    let second = p.id_statement()?;
                    let description =
                        (p.kind() == T::DESCR).then(|| StateDb::trim_colon(&p.take()));
                    d.add(Stmt::Relation {
                        state1: first,
                        state2: second,
                        description,
                    });
                } else {
                    d.add(Stmt::State(first));
                }
            }
            T::HIDE_EMPTY => {
                let text = p.take();
                d.add(Stmt::Text(text));
            }
            T::scale => {
                let text = p.take();
                p.expect(T::WIDTH)?;
                d.add(Stmt::Text(text));
            }
            T::COMPOSIT_STATE => {
                let text = p.take();
                if p.kind() == T::STRUCT_START {
                    p.i += 1;
                    d.open(text, String::new());
                } else {
                    d.add(Stmt::Text(text));
                }
            }
            T::STATE_DESCR => {
                let description = p.take();
                p.expect(T::AS)?;
                let id = p.expect(T::ID)?;
                if p.kind() == T::STRUCT_START {
                    p.i += 1;
                    d.open(id, description);
                } else if id.contains(':') {
                    let mut parts = id.split(':');
                    let first = parts.next().unwrap_or("").to_string();
                    let second = parts.next().unwrap_or("").to_string();
                    let mut s = StateStmt::new(first, "default");
                    s.description = Some(Description::Many(vec![
                        trim(&description).to_string(),
                        second,
                    ]));
                    d.add(Stmt::State(s));
                } else {
                    let mut s = StateStmt::new(id, "default");
                    s.description = Some(Description::One(trim(&description).to_string()));
                    d.add(Stmt::State(s));
                }
            }
            T::FORK | T::JOIN | T::CHOICE => {
                let kind = match t {
                    T::FORK => "fork",
                    T::JOIN => "join",
                    _ => "choice",
                };
                let id = p.take();
                d.add(Stmt::State(StateStmt::new(id, kind)));
            }
            T::CONCURRENT => {
                p.i += 1;
                d.add(Stmt::State(StateStmt::new(db.get_divider_id(), "divider")));
            }
            T::note => {
                let keyword = p.take();
                if matches!(p.kind(), T::left_of | T::right_of) {
                    let position = trim(&p.take()).to_string();
                    let id = trim(&p.expect(T::ID)?).to_string();
                    let text = trim(&p.expect(T::NOTE_TEXT)?).to_string();
                    d.add(Stmt::State(StateStmt {
                        id,
                        note: Some(StateNote { position, text }),
                        ..StateStmt::default()
                    }));
                } else {
                    if p.kind() != T::NOTE_TEXT {
                        return Err(p.fail("'NOTE_TEXT', 'left_of', 'right_of'"));
                    }
                    p.i += 1;
                    p.expect(T::AS)?;
                    p.expect(T::ID)?;
                    d.add(Stmt::Text(keyword));
                }
            }
            T::direction_tb | T::direction_bt | T::direction_rl | T::direction_lr => {
                let k = DIRECTION_TOKENS.iter().position(|&d| d == t).unwrap_or(0);
                let value = DIRECTIONS[k];
                p.i += 1;
                db.set_direction(value);
                d.add(Stmt::Dir(value.to_string()));
            }
            T::acc_title => {
                p.i += 1;
                let value = trim(&p.expect(T::acc_title_value)?).to_string();
                db.set_acc_title(&value);
                d.add(Stmt::Text(value));
            }
            T::acc_descr => {
                p.i += 1;
                let value = trim(&p.expect(T::acc_descr_value)?).to_string();
                db.set_acc_description(&value);
                d.add(Stmt::Text(value));
            }
            T::acc_descr_multiline_value => {
                let value = trim(&p.take()).to_string();
                db.set_acc_description(&value);
                d.add(Stmt::Text(value));
            }
            T::CLICK => {
                p.i += 1;
                let id = p.id_statement()?;
                let (url, tooltip);
                if p.kind() == T::HREF {
                    p.i += 1;
                    url = p.expect(T::STRING)?;
                    tooltip = String::new();
                } else {
                    if p.kind() != T::STRING {
                        return Err(p.fail("'STRING', 'HREF'"));
                    }
                    url = p.take();
                    tooltip = p.expect(T::STRING)?;
                }
                p.expect(T::NL)?;
                d.add(Stmt::Click { id, url, tooltip });
            }
            T::classDef => {
                p.i += 1;
                let id = trim(&p.expect(T::CLASSDEF_ID)?).to_string();
                let classes = trim(&p.expect(T::CLASSDEF_STYLEOPTS)?).to_string();
                d.add(Stmt::ClassDef { id, classes });
            }
            T::style => {
                p.i += 1;
                let id = trim(&p.expect(T::STYLE_IDS)?).to_string();
                let style_class = trim(&p.expect(T::STYLEDEF_STYLEOPTS)?).to_string();
                d.add(Stmt::Style { id, style_class });
            }
            T::class => {
                p.i += 1;
                let id = trim(&p.expect(T::CLASSENTITY_IDS)?).to_string();
                let style_class = trim(&p.expect(T::STYLECLASS)?).to_string();
                d.add(Stmt::ApplyClass { id, style_class });
            }
            T::STRUCT_STOP => {
                if d.outer.is_empty() {
                    return Err(p.fail(&format!("'$end', {STATEMENT}")));
                }
                p.i += 1;
                d.close();
            }
            T::END => {
                if !d.outer.is_empty() {
                    return Err(p.fail(&format!("'STRUCT_STOP', {STATEMENT}")));
                }
                return db.set_root_doc(d.doc);
            }
            _ => {
                let expected = if d.outer.is_empty() {
                    format!("'$end', {STATEMENT}")
                } else {
                    format!("'STRUCT_STOP', {STATEMENT}")
                };
                return Err(p.fail(&expected));
            }
        }
    }
}
