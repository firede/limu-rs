//! The parser of a requirement diagram, as Pele's parser.ts has it: recursive descent over
//! the tokens of its lexer, agreeing with Mermaid's requirementDiagram.jison, its errors
//! worded as Jison words them.

use super::db::{RELATIONSHIPS, REQUIREMENT_TYPES, RISK_LEVELS, RequirementDb, VERIFY_TYPES};
use super::lexer::{Tok, tokenize};
use super::tokens::{T, TOKEN_NAMES};
use crate::detect::DiagramType;
use crate::errors::{Error, syntax_error};
use crate::js::chars::trim;

/// What the keyword tokens from REQUIREMENT to TRACES stand for, in token order, Pele's
/// VALUES. ELEMENT, between them, stands for nothing.
fn value(t: T) -> &'static str {
    let k = t as usize - T::REQUIREMENT as usize;
    let mut values = REQUIREMENT_TYPES
        .iter()
        .chain(&RISK_LEVELS)
        .chain(&VERIFY_TYPES)
        .chain(&[""])
        .chain(&RELATIONSHIPS);
    values.nth(k).copied().unwrap_or("")
}

/// Whether a token is part of a style, Pele's isStylePart.
fn is_style_part(t: T) -> bool {
    matches!(t, T::ALPHA | T::COLON | T::BRKT | T::MINUS | T::SEMICOLON)
}

struct Parser<'a, 'd> {
    src: &'a str,
    toks: Vec<Tok<'a>>,
    i: usize,
    db: &'d mut RequirementDb,
}

/// Parses a requirement diagram's text into db, as Pele's parseRequirement does.
pub(crate) fn parse_requirement(src: &str, db: &mut RequirementDb) -> Result<(), Error> {
    let mut p = Parser {
        src,
        toks: tokenize(src),
        i: 0,
        db,
    };
    p.parse()
}

impl<'a> Parser<'a, '_> {
    fn kind(&self) -> T {
        self.toks[self.i].kind
    }

    /// The text of the token at i, and moves past it.
    fn take(&mut self) -> &'a str {
        let s = self.toks[self.i].text;
        self.i += 1;
        s
    }

    /// Fails with Jison's wording, naming the token types from to to and any others as
    /// expected, as Pele's fail does.
    fn fail(&self, from: T, to: T, others: &[T]) -> Error {
        let tok = &self.toks[self.i];
        if tok.kind == T::ERROR {
            return syntax_error(DiagramType::Requirement, self.src, tok.start, "", true);
        }
        let expected = TOKEN_NAMES[from as usize..=to as usize]
            .iter()
            .copied()
            .chain(others.iter().map(|t| t.name()))
            .map(|name| format!("'{name}'"))
            .collect::<Vec<_>>()
            .join(", ");
        let detail = format!("Expecting {expected}, got '{}'", tok.kind.name());
        syntax_error(
            DiagramType::Requirement,
            self.src,
            tok.start,
            &detail,
            false,
        )
    }

    fn expect(&mut self, kind: T) -> Result<&'a str, Error> {
        if self.kind() != kind {
            return Err(self.fail(kind, kind, &[]));
        }
        Ok(self.take())
    }

    /// The value of the next token if it is a keyword in the given range of token types.
    fn keyword(&mut self, from: T, to: T) -> Result<&'static str, Error> {
        let t = self.kind();
        if t < from || t > to {
            return Err(self.fail(from, to, &[]));
        }
        self.i += 1;
        Ok(value(t))
    }

    fn string(&mut self) -> Result<&'a str, Error> {
        if !matches!(self.kind(), T::unqString | T::qString) {
            return Err(self.fail(T::qString, T::unqString, &[]));
        }
        Ok(self.take())
    }

    fn directive(&mut self) -> Result<bool, Error> {
        match self.kind() {
            T::acc_title => {
                self.i += 1;
                let v = self.expect(T::acc_title_value)?;
                self.db.set_acc_title(trim(v));
            }
            T::acc_descr => {
                self.i += 1;
                let v = self.expect(T::acc_descr_value)?;
                self.db.set_acc_description(trim(v));
            }
            T::acc_descr_multiline_value => {
                let v = self.take();
                self.db.set_acc_description(trim(v));
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn id_list(&mut self) -> Result<Vec<String>, Error> {
        let mut list = Vec::new();
        loop {
            if !matches!(self.kind(), T::ALPHA | T::unqString | T::qString) {
                return Err(self.fail(T::qString, T::unqString, &[T::ALPHA]));
            }
            list.push(self.take().to_string());
            if self.kind() != T::COMMA {
                return Ok(list);
            }
            self.i += 1;
        }
    }

    fn styles_opt(&mut self) -> Result<Vec<String>, Error> {
        let mut styles = Vec::new();
        loop {
            if !is_style_part(self.kind()) {
                return Err(self.fail(T::ALPHA, T::SEMICOLON, &[T::MINUS, T::BRKT]));
            }
            let mut style = self.take().to_string();
            while is_style_part(self.kind()) {
                style.push_str(self.take());
            }
            styles.push(style);
            if self.kind() != T::COMMA {
                return Ok(styles);
            }
            self.i += 1;
        }
    }

    fn class_list(&mut self) -> Result<Option<Vec<String>>, Error> {
        let mut classes = None;
        match self.kind() {
            T::STYLE_SEPARATOR => {
                self.i += 1;
                classes = Some(self.id_list()?);
                self.expect(T::STRUCT_START)?;
            }
            T::STRUCT_START => self.i += 1,
            _ => {
                return Err(self.fail(T::STRUCT_START, T::STRUCT_START, &[T::STYLE_SEPARATOR]));
            }
        }
        self.expect(T::NEWLINE)?;
        Ok(classes)
    }

    /// Reads a body up to its closing brace. The grammar is right recursive, so Mermaid
    /// applies the fields last to first and the first of two repeated fields wins.
    fn body(&mut self, requirement: bool) -> Result<(), Error> {
        let mut fields: Vec<(T, &str)> = Vec::new();
        loop {
            let t = self.kind();
            if t == T::STRUCT_STOP {
                self.i += 1;
                break;
            }
            if t == T::NEWLINE {
                self.i += 1;
                continue;
            }
            let field = if requirement {
                (T::ID..=T::VERIFYMTHD).contains(&t)
            } else {
                matches!(t, T::TYPE | T::DOCREF)
            };
            if !field {
                if requirement {
                    return Err(self.fail(T::ID, T::VERIFYMTHD, &[T::NEWLINE, T::STRUCT_STOP]));
                }
                return Err(self.fail(T::TYPE, T::DOCREF, &[T::NEWLINE, T::STRUCT_STOP]));
            }
            self.i += 1;
            self.expect(T::COLONSEP)?;
            let value = match t {
                T::RISK => self.keyword(T::LOW_RISK, T::HIGH_RISK)?,
                T::VERIFYMTHD => self.keyword(T::VERIFY_ANALYSIS, T::VERIFY_TEST)?,
                _ => self.string()?,
            };
            fields.push((t, value));
            self.expect(T::NEWLINE)?;
        }
        for &(field, value) in fields.iter().rev() {
            match field {
                T::ID => self.db.set_new_req_id(value),
                T::TEXT => self.db.set_new_req_text(value),
                T::RISK => self.db.set_new_req_risk(value),
                T::VERIFYMTHD => self.db.set_new_req_verify_method(value),
                T::TYPE => self.db.set_new_element_type(value),
                _ => self.db.set_new_element_doc_ref(value),
            }
        }
        Ok(())
    }

    fn parse(&mut self) -> Result<(), Error> {
        while self.directive()? {
            if self.kind() == T::NEWLINE {
                self.i += 1;
            }
        }
        if self.kind() != T::RD {
            return Err(self.fail(
                T::RD,
                T::RD,
                &[T::acc_title, T::acc_descr, T::acc_descr_multiline_value],
            ));
        }
        self.i += 1;
        self.expect(T::NEWLINE)?;

        loop {
            let t = self.kind();
            match t {
                T::EOF => return Ok(()),
                T::NEWLINE => self.i += 1,
                T::acc_title | T::acc_descr | T::acc_descr_multiline_value => {
                    self.directive()?;
                }
                T::direction_tb | T::direction_bt | T::direction_rl | T::direction_lr => {
                    self.i += 1;
                    self.db.set_direction(match t {
                        T::direction_tb => "TB",
                        T::direction_bt => "BT",
                        T::direction_rl => "RL",
                        _ => "LR",
                    });
                }
                _ if (T::REQUIREMENT..=T::DESIGN_CONSTRAINT).contains(&t) => {
                    let kind = self.keyword(T::REQUIREMENT, T::DESIGN_CONSTRAINT)?;
                    let name = self.string()?;
                    let classes = self.class_list()?;
                    self.body(true)?;
                    self.db.add_requirement(name, kind);
                    if let Some(classes) = classes {
                        self.db.set_class(&[name.to_string()], &classes)?;
                    }
                }
                T::ELEMENT => {
                    self.i += 1;
                    let name = self.string()?;
                    let classes = self.class_list()?;
                    self.body(false)?;
                    self.db.add_element(name);
                    if let Some(classes) = classes {
                        self.db.set_class(&[name.to_string()], &classes)?;
                    }
                }
                T::STYLE => {
                    self.i += 1;
                    let ids = self.id_list()?;
                    let styles = self.styles_opt()?;
                    self.db.set_css_style(&ids, &styles)?;
                }
                T::CLASSDEF => {
                    self.i += 1;
                    let ids = self.id_list()?;
                    let styles = self.styles_opt()?;
                    self.db.define_class(&ids, &styles)?;
                }
                T::CLASS => {
                    self.i += 1;
                    let ids = self.id_list()?;
                    let classes = self.id_list()?;
                    self.db.set_class(&ids, &classes)?;
                }
                T::unqString | T::qString => {
                    let id = self.take();
                    let next = self.kind();
                    match next {
                        T::STYLE_SEPARATOR => {
                            self.i += 1;
                            let classes = self.id_list()?;
                            self.db.set_class(&[id.to_string()], &classes)?;
                        }
                        T::END_ARROW_L | T::LINE => {
                            self.i += 1;
                            let relationship = self.keyword(T::CONTAINS, T::TRACES)?;
                            self.expect(if next == T::LINE {
                                T::END_ARROW_R
                            } else {
                                T::LINE
                            })?;
                            let other = self.string()?;
                            if next == T::LINE {
                                self.db.add_relationship(relationship, id, other);
                            } else {
                                self.db.add_relationship(relationship, other, id);
                            }
                        }
                        _ => {
                            return Err(self.fail(
                                T::STYLE_SEPARATOR,
                                T::STYLE_SEPARATOR,
                                &[T::END_ARROW_L, T::LINE],
                            ));
                        }
                    }
                }
                _ => {
                    return Err(self.fail(
                        T::NEWLINE,
                        T::EOF,
                        &[
                            T::acc_title,
                            T::acc_descr,
                            T::acc_descr_multiline_value,
                            T::direction_tb,
                            T::direction_bt,
                            T::direction_rl,
                            T::direction_lr,
                            T::REQUIREMENT,
                            T::FUNCTIONAL_REQUIREMENT,
                            T::INTERFACE_REQUIREMENT,
                            T::PERFORMANCE_REQUIREMENT,
                            T::PHYSICAL_REQUIREMENT,
                            T::DESIGN_CONSTRAINT,
                            T::ELEMENT,
                            T::CLASSDEF,
                            T::CLASS,
                            T::STYLE,
                            T::unqString,
                            T::qString,
                        ],
                    ));
                }
            }
        }
    }
}
