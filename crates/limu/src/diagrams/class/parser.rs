//! The parser of a class diagram, as Pele's parser.ts has it: a recursive descent over the
//! tokens that accepts what Mermaid's classDiagram.jison accepts, with Jison's errors.

use super::db::{
    AGGREGATION, COMPOSITION, ClassDb, ClassRelation, DEPENDENCY, DOTTED_LINE, EXTENSION, LINE,
    LOLLIPOP, RelationEnd, cleanup_label,
};
use super::lexer::{Tok, tokenize};
use super::tokens::T;
use crate::detect::DiagramType;
use crate::errors::{Error, syntax_error};
use crate::js::chars::trim;

/// Whether a token is part of a name, Pele's ALPHANUM.
fn alphanum(t: T) -> bool {
    matches!(t, T::UNICODE_TEXT | T::ALPHA | T::MINUS)
}

/// Whether a token is part of a style, Pele's STYLE_PART.
fn style_part(t: T) -> bool {
    matches!(
        t,
        T::ALPHA | T::COLON | T::BRKT | T::STYLE | T::PCT | T::LABEL
    )
}

/// The marker a token puts at an end of a relation, by Mermaid's relationType numbers, or
/// none for a token that is no marker, as Pele's END_TYPE has them.
fn end_type(t: T) -> Option<u8> {
    match t {
        T::AGGREGATION => Some(AGGREGATION),
        T::EXTENSION => Some(EXTENSION),
        T::COMPOSITION => Some(COMPOSITION),
        T::DEPENDENCY => Some(DEPENDENCY),
        T::LOLLIPOP => Some(LOLLIPOP),
        _ => None,
    }
}

const NAME_TOKENS: &str = "'UNICODE_TEXT', 'NUM', 'ALPHA', 'MINUS', 'BQUOTE_STR'";

/// A namespace being read, with the classes and notes in it, Pele's Frame.
struct Frame {
    id: String,
    classes: Vec<String>,
    notes: Vec<String>,
}

struct Parser<'a, 'd> {
    src: &'a str,
    toks: Vec<Tok<'a>>,
    i: usize,
    db: &'d mut ClassDb,
}

type Parsed<T> = Result<T, Error>;

impl<'a> Parser<'a, '_> {
    fn kind(&self) -> T {
        self.toks[self.i].kind
    }

    fn kind_at(&self, i: usize) -> T {
        self.toks[i].kind
    }

    fn take(&mut self) -> &'a str {
        let text = self.toks[self.i].text;
        self.i += 1;
        text
    }

    /// The error of a token where expected was, as Pele's fail makes it.
    fn fail(&self, expected: &str) -> Error {
        let tok = &self.toks[self.i];
        if tok.kind == T::ERROR {
            return syntax_error(DiagramType::Class, self.src, tok.start, "", true);
        }
        let detail = format!("Expecting {expected}, got '{}'", tok.kind.name());
        syntax_error(DiagramType::Class, self.src, tok.start, &detail, false)
    }

    fn expect(&mut self, kind: T) -> Parsed<&'a str> {
        if self.kind() != kind {
            return Err(self.fail(&format!("'{}'", kind.name())));
        }
        Ok(self.take())
    }

    fn alpha_num(&mut self) -> Parsed<&'a str> {
        if !alphanum(self.kind()) {
            return Err(self.fail("'UNICODE_TEXT', 'NUM', 'ALPHA', 'MINUS'"));
        }
        Ok(self.take())
    }

    /// A name of parts joined by dots, or one in backticks, and with generic, the generic
    /// type after it, as Pele's name reads it.
    fn name(&mut self, generic: bool) -> Parsed<String> {
        let mut out = String::new();
        loop {
            if self.kind() == T::BQUOTE_STR {
                out.push_str(self.take());
                break;
            }
            if !alphanum(self.kind()) {
                return Err(self.fail(NAME_TOKENS));
            }
            out.push_str(self.take());
            let t = self.kind();
            if t == T::DOT {
                out.push('.');
                self.i += 1;
            } else if !alphanum(t) && t != T::BQUOTE_STR {
                break;
            }
        }
        if generic && self.kind() == T::GENERICTYPE {
            out.push('~');
            out.push_str(self.take());
            out.push('~');
        }
        Ok(out)
    }

    fn class_label(&mut self) -> Parsed<&'a str> {
        self.i += 1;
        let label = self.expect(T::STR)?;
        self.expect(T::SQE)?;
        Ok(label)
    }

    /// The members of a class's body, in the order written. Pele's grammar collects them last
    /// to first, and addMembers turns them back.
    fn members(&mut self) -> Parsed<Vec<&'a str>> {
        if self.kind() != T::MEMBER {
            return Err(self.fail("'STRUCT_STOP', 'MEMBER'"));
        }
        let mut list = Vec::new();
        while self.kind() == T::MEMBER {
            list.push(self.take());
        }
        self.expect(T::STRUCT_STOP)?;
        Ok(list)
    }

    fn body(&mut self, id: &str) -> Parsed<()> {
        if self.kind() == T::STRUCT_STOP {
            self.i += 1;
        } else {
            let members = self.members()?;
            self.db.add_members(id, &members);
        }
        Ok(())
    }

    fn class_statement(&mut self) -> Parsed<String> {
        self.i += 1;
        let id = self.name(true)?;
        self.db.add_class(&id);
        if self.kind() == T::SQS {
            let label = self.class_label()?;
            self.db.set_class_label(&id, label);
        }
        match self.kind() {
            T::STYLE_SEPARATOR => {
                self.i += 1;
                let css = self.alpha_num()?;
                self.db.set_css_class(&id, css);
                if self.kind() == T::STRUCT_START {
                    self.i += 1;
                    let members = self.members()?;
                    self.db.add_members(&id, &members);
                }
            }
            T::STRUCT_START => {
                self.i += 1;
                self.body(&id)?;
            }
            T::ANNOTATION_START => {
                self.i += 1;
                let annotation = self.alpha_num()?;
                self.expect(T::ANNOTATION_END)?;
                self.db.add_annotation(&id, annotation);
                if self.kind() == T::STRUCT_START {
                    self.i += 1;
                    self.body(&id)?;
                }
            }
            _ => {}
        }
        Ok(id)
    }

    fn note_statement(&mut self) -> Parsed<String> {
        let t = self.kind();
        self.i += 1;
        if t == T::NOTE {
            let text = self.expect(T::STR)?;
            return Ok(self.db.add_note(text, None));
        }
        let id = self.name(true)?;
        let text = self.expect(T::STR)?;
        Ok(self.db.add_note(text, Some(&id)))
    }

    /// Namespaces nest, so they are read with a stack of the open ones, as Pele's
    /// namespaceStatement reads them.
    fn namespace_statement(&mut self) -> Parsed<()> {
        let mut open: Vec<Frame> = Vec::new();
        loop {
            self.i += 1;
            let ns = self.name(false)?;
            let id = if self.kind() == T::SQS {
                let label = self.class_label()?;
                self.db.add_namespace(&ns, Some(label))
            } else {
                self.db.add_namespace(&ns, None)
            };
            self.expect(T::STRUCT_START)?;
            if self.kind() == T::NEWLINE {
                self.i += 1;
            }
            open.push(Frame {
                id,
                classes: Vec::new(),
                notes: Vec::new(),
            });
            let mut item = true;
            while item {
                let t = self.kind();
                if t == T::NAMESPACE {
                    break;
                }
                if t == T::CLASS {
                    let id = self.class_statement()?;
                    open.last_mut().expect("a namespace open").classes.push(id);
                } else if t == T::NOTE || t == T::NOTE_FOR {
                    let id = self.note_statement()?;
                    open.last_mut().expect("a namespace open").notes.push(id);
                } else {
                    return Err(self.fail("'NAMESPACE', 'CLASS', 'NOTE_FOR', 'NOTE'"));
                }
                loop {
                    if self.kind() == T::NEWLINE {
                        let next = self.kind_at(self.i + 1);
                        if matches!(next, T::NAMESPACE | T::CLASS | T::NOTE | T::NOTE_FOR) {
                            self.i += 1;
                            break;
                        }
                        self.i += 1;
                    }
                    self.expect(T::STRUCT_STOP)?;
                    let done = open.pop().expect("a namespace open");
                    self.db
                        .add_classes_to_namespace(&done.id, &done.classes, &done.notes);
                    self.db.pop_namespace();
                    if open.is_empty() {
                        item = false;
                        break;
                    }
                }
            }
            if open.is_empty() {
                return Ok(());
            }
        }
    }

    fn relation_end(&mut self) -> RelationEnd {
        let end = end_type(self.kind());
        if end.is_some() {
            self.i += 1;
        }
        end
    }

    fn class_or_relation(&mut self) -> Parsed<()> {
        let id1 = self.name(true)?;
        let mut t = self.kind();
        if t == T::LABEL {
            let label = cleanup_label(self.take());
            self.db.add_member(&id1, &label);
            return Ok(());
        }
        let mut title1 = "none";
        if t == T::STR {
            title1 = self.take();
        } else if end_type(t).is_none() && t != T::LINE && t != T::DOTTED_LINE {
            return Ok(());
        }
        let type1 = self.relation_end();
        t = self.kind();
        if t != T::LINE && t != T::DOTTED_LINE {
            return Err(self.fail(
                "'AGGREGATION', 'EXTENSION', 'COMPOSITION', 'DEPENDENCY', 'LOLLIPOP', 'LINE', 'DOTTED_LINE'",
            ));
        }
        self.i += 1;
        let type2 = self.relation_end();
        let title2 = if self.kind() == T::STR {
            self.take()
        } else {
            "none"
        };
        let id2 = self.name(true)?;
        let title = if self.kind() == T::LABEL {
            Some(cleanup_label(self.take()))
        } else {
            None
        };
        self.db.add_relation(ClassRelation {
            id1,
            id2,
            relation_title1: title1.to_string(),
            relation_title2: title2.to_string(),
            title,
            type1,
            type2,
            line_type: if t == T::LINE { LINE } else { DOTTED_LINE },
        });
        Ok(())
    }

    fn styles(&mut self) -> Parsed<Vec<String>> {
        let mut out = Vec::new();
        loop {
            if !style_part(self.kind()) {
                return Err(self.fail(
                    "'ALPHA', 'NUM', 'COLON', 'UNIT', 'SPACE', 'BRKT', 'STYLE', 'PCT', 'LABEL'",
                ));
            }
            let mut s = self.take().to_string();
            while style_part(self.kind()) {
                s.push_str(self.take());
            }
            out.push(s);
            if self.kind() != T::COMMA {
                return Ok(out);
            }
            self.i += 1;
        }
    }

    fn tooltip(&mut self, id: &str) {
        if self.kind() == T::STR {
            let tip = self.take();
            self.db.set_tooltip(id, tip);
        }
    }

    fn link_statement(&mut self, id: &str) -> Parsed<()> {
        let href = self.expect(T::STR)?;
        let tip = if self.kind() == T::STR {
            Some(self.take())
        } else {
            None
        };
        if self.kind() == T::LINK_TARGET {
            let target = self.take();
            self.db.set_link(id, href, Some(target));
        } else {
            self.db.set_link(id, href, None);
        }
        if let Some(tip) = tip {
            self.db.set_tooltip(id, tip);
        }
        Ok(())
    }

    fn statement(&mut self) -> Parsed<()> {
        let t = self.kind();
        match t {
            T::UNICODE_TEXT | T::ALPHA | T::MINUS | T::BQUOTE_STR => self.class_or_relation(),
            T::NAMESPACE => self.namespace_statement(),
            T::CLASS => self.class_statement().map(|_| ()),
            T::MEMBER => {
                self.i += 1;
                Ok(())
            }
            T::ANNOTATION_START => {
                self.i += 1;
                let annotation = self.alpha_num()?;
                self.expect(T::ANNOTATION_END)?;
                let name = self.name(true)?;
                self.db.add_annotation(&name, annotation);
                Ok(())
            }
            T::CALLBACK => {
                self.i += 1;
                let id = self.name(true)?;
                self.expect(T::STR)?;
                self.db.set_click_event(&id);
                self.tooltip(&id);
                Ok(())
            }
            T::LINK => {
                self.i += 1;
                let id = self.name(true)?;
                self.link_statement(&id)
            }
            T::CLICK => {
                self.i += 1;
                let id = self.name(true)?;
                if self.kind() == T::HREF {
                    self.i += 1;
                    return self.link_statement(&id);
                }
                self.expect(T::CALLBACK_NAME)?;
                if self.kind() == T::CALLBACK_ARGS {
                    self.i += 1;
                }
                self.db.set_click_event(&id);
                self.tooltip(&id);
                Ok(())
            }
            T::STYLE => {
                self.i += 1;
                let id = self.expect(T::ALPHA)?;
                let styles = self.styles()?;
                self.db.set_css_style(id, &styles);
                Ok(())
            }
            T::CSSCLASS => {
                self.i += 1;
                let ids = self.expect(T::STR)?;
                let class = self.expect(T::ALPHA)?;
                self.db.set_css_class(ids, class);
                Ok(())
            }
            T::NOTE | T::NOTE_FOR => self.note_statement().map(|_| ()),
            T::CLASSDEF => {
                self.i += 1;
                let mut ids = vec![self.expect(T::ALPHA)?];
                while self.kind() == T::COMMA {
                    self.i += 1;
                    ids.push(self.expect(T::ALPHA)?);
                }
                let styles = self.styles()?;
                self.db.define_class(&ids, &styles);
                Ok(())
            }
            T::direction_tb | T::direction_bt | T::direction_rl | T::direction_lr => {
                self.i += 1;
                self.db.set_direction(match t {
                    T::direction_tb => "TB",
                    T::direction_bt => "BT",
                    T::direction_rl => "RL",
                    _ => "LR",
                });
                Ok(())
            }
            T::acc_title => {
                self.i += 1;
                let text = self.expect(T::acc_title_value)?;
                self.db.set_acc_title(trim(text));
                Ok(())
            }
            T::acc_descr => {
                self.i += 1;
                let text = self.expect(T::acc_descr_value)?;
                self.db.set_acc_description(trim(text));
                Ok(())
            }
            T::acc_descr_multiline_value => {
                let text = self.take();
                self.db.set_acc_description(trim(text));
                Ok(())
            }
            _ => Err(self.fail(
                "'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'NAMESPACE', 'ANNOTATION_START', 'CLASS', 'MEMBER', 'NOTE_FOR', 'NOTE', 'CLASSDEF', 'direction_tb', 'CALLBACK', 'LINK', 'CLICK', 'STYLE', 'CSSCLASS', 'ALPHA', 'MINUS', 'BQUOTE_STR'",
            )),
        }
    }

    fn diagram(&mut self) -> Parsed<()> {
        // Mermaid's grammar also takes statements with no `classDiagram` line before them.
        let header = self.kind() == T::CLASS_DIAGRAM;
        if header {
            self.i += 1;
            self.expect(T::NEWLINE)?;
        }
        loop {
            self.statement()?;
            if self.kind() != T::NEWLINE {
                break;
            }
            self.i += 1;
            if matches!(self.kind(), T::EOF | T::END) {
                break;
            }
        }
        if header {
            self.expect(T::EOF)?;
        }
        self.expect(T::END)?;
        Ok(())
    }
}

/// Parses a class diagram's text into db, as Pele's parseClassDiagram does.
pub(crate) fn parse_class_diagram(src: &str, db: &mut ClassDb) -> Result<(), Error> {
    let mut p = Parser {
        src,
        toks: tokenize(src),
        i: 0,
        db,
    };
    p.diagram()
}
