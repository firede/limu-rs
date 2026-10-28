//! The parser of a quadrant chart, as Pele's parser.ts has it: a statement at a time over the
//! tokens of its lexer, agreeing with Mermaid's quadrant.jison, its errors worded as Jison
//! words them.

use super::db::{QuadrantDb, QuadrantText};
use super::lexer::{Tok, tokenize};
use super::tokens::T;
use crate::detect::DiagramType;
use crate::errors::{Error, syntax_error};
use crate::js::chars::trim;

/// Whether a token is a word of a text, Pele's WORD.
fn is_word(t: T) -> bool {
    matches!(
        t,
        T::PUNCTUATION
            | T::AMP
            | T::NUM
            | T::ALPHA
            | T::COMMA
            | T::PLUS
            | T::EQUALS
            | T::MULT
            | T::DOT
            | T::BRKT
            | T::UNDERSCORE
            | T::UNICODE_TEXT
    )
}

/// Whether a token is part of a class's name, Pele's ID.
fn is_id(t: T) -> bool {
    matches!(
        t,
        T::ALPHA
            | T::NUM
            | T::MINUS
            | T::COMMA
            | T::COLON
            | T::AMP
            | T::BRKT
            | T::MULT
            | T::UNICODE_TEXT
    )
}

/// Whether a token is part of a style, Pele's STYLE.
fn is_style(t: T) -> bool {
    matches!(
        t,
        T::ALPHA | T::NUM | T::COLON | T::SPACE | T::BRKT | T::MINUS
    )
}

/// Whether a token ends a statement, Pele's EOL.
fn is_eol(t: T) -> bool {
    matches!(t, T::NEWLINE | T::SEMI | T::EOF)
}

/// What a text may start with, as an error names them, Pele's TEXT_TOKENS.
const TEXT_TOKENS: &str = "'STR', 'MD_STR', 'ALPHA', 'NUM', 'UNICODE_TEXT', 'PUNCTUATION'";

struct Parser<'a, 'd> {
    src: &'a str,
    toks: Vec<Tok<'a>>,
    i: usize,
    db: &'d mut QuadrantDb,
}

/// Parses a quadrant chart's text into db, as Pele's parseQuadrant does.
pub(crate) fn parse_quadrant(src: &str, db: &mut QuadrantDb) -> Result<(), Error> {
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

    /// Fails with Jison's wording, as Pele's fail does.
    fn fail(&self, expected: &str) -> Error {
        let tok = &self.toks[self.i];
        if tok.kind == T::ERROR {
            return syntax_error(DiagramType::QuadrantChart, self.src, tok.start, "", true);
        }
        let detail = format!("Expecting {expected}, got '{}'", tok.kind.name());
        syntax_error(
            DiagramType::QuadrantChart,
            self.src,
            tok.start,
            &detail,
            false,
        )
    }

    fn expect(&mut self, kind: T) -> Result<&'a str, Error> {
        if self.kind() != kind {
            return Err(self.fail(&format!("'{}'", kind.name())));
        }
        Ok(self.take())
    }

    fn is_text(t: T) -> bool {
        t == T::STR || t == T::MD_STR || is_word(t)
    }

    fn text(&mut self) -> Result<QuadrantText, Error> {
        let first = self.kind();
        if !Self::is_text(first) {
            return Err(self.fail(TEXT_TOKENS));
        }
        let mut s = self.take().to_string();
        while is_word(self.kind()) || matches!(self.kind(), T::SPACE | T::MINUS) {
            s.push_str(self.take());
        }
        Ok(QuadrantText {
            text: s,
            markdown: first == T::MD_STR,
        })
    }

    fn styles(&mut self) -> Result<Vec<String>, Error> {
        let mut out = Vec::new();
        loop {
            if !is_style(self.kind()) {
                return Err(self.fail("'ALPHA', 'NUM', 'COLON', 'SPACE', 'BRKT', 'MINUS'"));
            }
            let mut s = self.take().to_string();
            while is_style(self.kind()) {
                s.push_str(self.take());
            }
            out.push(trim(&s).to_string());
            if self.kind() != T::COMMA {
                return Ok(out);
            }
            self.i += 1;
        }
    }

    fn parse(&mut self) -> Result<(), Error> {
        while is_eol(self.kind()) || self.kind() == T::SPACE {
            self.i += 1;
        }
        self.expect(T::QUADRANT)?;

        while self.kind() != T::END {
            while self.kind() == T::SPACE {
                self.i += 1;
            }
            let t = self.kind();
            match t {
                T::CLASSDEF => {
                    self.i += 1;
                    self.expect(T::SPACE)?;
                    if !is_id(self.kind()) {
                        return Err(self.fail("'ALPHA', 'NUM', 'UNICODE_TEXT'"));
                    }
                    let mut id = self.take().to_string();
                    while is_id(self.kind()) {
                        id.push_str(self.take());
                    }
                    self.expect(T::SPACE)?;
                    let styles = self.styles()?;
                    self.db.add_class(&id, &styles)?;
                }
                T::X_AXIS | T::Y_AXIS => {
                    self.i += 1;
                    let mut from = self.text()?;
                    let mut to = None;
                    if self.kind() == T::DELIMITER {
                        self.i += 1;
                        if Self::is_text(self.kind()) {
                            to = Some(self.text()?);
                        } else {
                            from.text.push_str(" ⟶ ");
                        }
                    }
                    if t == T::X_AXIS {
                        self.db.set_x_axis_left_text(from);
                        if let Some(to) = to {
                            self.db.set_x_axis_right_text(to);
                        }
                    } else {
                        self.db.set_y_axis_bottom_text(from);
                        if let Some(to) = to {
                            self.db.set_y_axis_top_text(to);
                        }
                    }
                }
                T::QUADRANT_1 | T::QUADRANT_2 | T::QUADRANT_3 | T::QUADRANT_4 => {
                    self.i += 1;
                    let label = self.text()?;
                    self.db
                        .set_quadrant_text(t as usize - T::QUADRANT_1 as usize, label);
                }
                T::title => {
                    self.i += 1;
                    let v = self.expect(T::title_value)?;
                    self.db.set_diagram_title(trim(v));
                }
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
                _ if Self::is_text(t) => {
                    let label = self.text()?;
                    let class_name = if self.kind() == T::class_name {
                        self.take()
                    } else {
                        ""
                    };
                    self.expect(T::point_start)?;
                    let x = self.expect(T::point_x)?;
                    let y = self.expect(T::point_y)?;
                    let styles = if is_style(self.kind()) {
                        self.styles()?
                    } else {
                        Vec::new()
                    };
                    self.db.add_point(label, class_name, x, y, &styles)?;
                }
                _ => {}
            }
            if !is_eol(self.kind()) {
                return Err(self.fail("'NEWLINE', 'SEMI', 'EOF'"));
            }
            self.i += 1;
        }
        Ok(())
    }
}
