//! The parser of an XY chart, as Pele's parser.ts has it: a statement at a time over the
//! tokens of its lexer, agreeing with Mermaid's xychart.jison, its errors worded as Jison
//! words them.

use super::db::{DataPoint, XyChartDb};
use super::lexer::{Tok, tokenize};
use super::tokens::T;
use crate::detect::DiagramType;
use crate::errors::{Error, syntax_error};
use crate::js::chars::trim;
use crate::js::number::number;

/// Whether a token is a word of a text, Pele's WORD.
fn is_word(t: T) -> bool {
    matches!(
        t,
        T::AMP
            | T::NUM
            | T::ALPHA
            | T::PLUS
            | T::EQUALS
            | T::MULT
            | T::DOT
            | T::BRKT
            | T::MINUS
            | T::UNDERSCORE
    )
}

/// Whether a token ends a statement, Pele's EOL.
fn is_eol(t: T) -> bool {
    matches!(t, T::NEWLINE | T::SEMI | T::EOF)
}

fn is_text(t: T) -> bool {
    t == T::STR || is_word(t)
}

struct Parser<'a, 'd> {
    src: &'a str,
    toks: Vec<Tok<'a>>,
    i: usize,
    db: &'d mut XyChartDb,
}

/// Parses an XY chart's text into db, as Pele's parseXyChart does.
pub(crate) fn parse_xy_chart(src: &str, db: &mut XyChartDb) -> Result<(), Error> {
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
            return syntax_error(DiagramType::XyChart, self.src, tok.start, "", true);
        }
        let detail = format!("Expecting {expected}, got '{}'", tok.kind.name());
        syntax_error(DiagramType::XyChart, self.src, tok.start, &detail, false)
    }

    fn expect(&mut self, kind: T) -> Result<&'a str, Error> {
        if self.kind() != kind {
            return Err(self.fail(&format!("'{}'", kind.name())));
        }
        Ok(self.take())
    }

    /// A text: a string, or a run of words.
    fn text(&mut self) -> Result<String, Error> {
        if !is_text(self.kind()) {
            return Err(self.fail("'STR', 'ALPHA', 'NUM'"));
        }
        let quoted = self.kind() == T::STR;
        let mut s = self.take().to_string();
        if !quoted {
            while is_word(self.kind()) {
                s.push_str(self.take());
            }
        }
        Ok(s)
    }

    fn range(&mut self) -> Result<(f64, f64), Error> {
        let min = number(self.expect(T::NUMBER_WITH_DECIMAL)?);
        self.expect(T::ARROW_DELIMITER)?;
        Ok((min, number(self.expect(T::NUMBER_WITH_DECIMAL)?)))
    }

    fn points(&mut self) -> Result<Vec<DataPoint>, Error> {
        let mut out = Vec::new();
        self.expect(T::SQUARE_BRACES_START)?;
        loop {
            let value = number(self.expect(T::NUMBER_WITH_DECIMAL)?);
            let label = if self.kind() == T::STR {
                self.take()
            } else {
                ""
            };
            out.push(DataPoint {
                value,
                label: label.to_string(),
            });
            if self.kind() != T::COMMA {
                break;
            }
            self.i += 1;
        }
        self.expect(T::SQUARE_BRACES_END)?;
        Ok(out)
    }

    fn parse(&mut self) -> Result<(), Error> {
        loop {
            if is_eol(self.kind()) {
                self.i += 1;
            } else if self.kind() != T::XYCHART {
                break;
            } else {
                self.i += 1;
                if self.kind() == T::CHART_ORIENTATION {
                    let o = self.take();
                    self.db.set_orientation(o);
                }
            }
        }

        while self.kind() != T::END {
            let t = self.kind();
            self.i += 1;
            match t {
                T::title => {
                    let title = self.text()?;
                    self.db.set_diagram_title(trim(&title));
                }
                T::X_AXIS | T::Y_AXIS => {
                    let title = if is_text(self.kind()) {
                        Some(self.text()?)
                    } else {
                        None
                    };
                    if t == T::X_AXIS && self.kind() == T::SQUARE_BRACES_START {
                        let mut categories = Vec::new();
                        loop {
                            self.i += 1;
                            categories.push(self.text()?);
                            if self.kind() != T::COMMA {
                                break;
                            }
                        }
                        self.expect(T::SQUARE_BRACES_END)?;
                        self.db.set_x_axis_band(&categories);
                    } else if self.kind() == T::NUMBER_WITH_DECIMAL || title.is_none() {
                        let (min, max) = self.range()?;
                        if t == T::X_AXIS {
                            self.db.set_x_axis_range_data(min, max);
                        } else {
                            self.db.set_y_axis_range_data(min, max);
                        }
                    }
                    let title = title.as_deref().unwrap_or("");
                    if t == T::X_AXIS {
                        self.db.set_x_axis_title(title);
                    } else {
                        self.db.set_y_axis_title(title);
                    }
                }
                T::LINE | T::BAR => {
                    let title = if self.kind() == T::SQUARE_BRACES_START {
                        String::new()
                    } else {
                        self.text()?
                    };
                    let data = self.points()?;
                    if t == T::LINE {
                        self.db.set_line_data(&title, &data);
                    } else {
                        self.db.set_bar_data(&title, &data);
                    }
                }
                T::acc_title => {
                    let v = self.expect(T::acc_title_value)?;
                    self.db.set_acc_title(trim(v));
                }
                T::acc_descr => {
                    let v = self.expect(T::acc_descr_value)?;
                    self.db.set_acc_description(trim(v));
                }
                T::acc_descr_multiline_value => {
                    let v = self.toks[self.i - 1].text;
                    self.db.set_acc_description(trim(v));
                }
                _ => {
                    self.i -= 1;
                    return Err(self.fail(
                        "'title', 'X_AXIS', 'Y_AXIS', 'LINE', 'BAR', 'acc_title', 'acc_descr', \
                         'acc_descr_multiline_value'",
                    ));
                }
            }
            while is_eol(self.kind()) {
                self.i += 1;
            }
        }
        Ok(())
    }
}
