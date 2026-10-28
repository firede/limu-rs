//! A venn diagram's tokens read into its db, as Pele's parser.ts reads them, failing where
//! Mermaid's Jison parser fails, with its words.

use super::db::{IndentHost, VennDb, text_without_set, union_of_one};
use super::lexer::{
    BRACKET_LABEL, COLON, COMMA, EOF, ERROR, IDENTIFIER, INDENT_TEXT, NEWLINE, NUMERIC, RGBACOLOR,
    SET, STRING, STYLE, TEXT, TITLE, TOKEN_NAMES, UNION, VENN, VennLexer,
};
use crate::detect::DiagramType;
use crate::errors::{Error, syntax_error};
use crate::js::chars::substring;
use crate::js::number::number;

const STATEMENT: &str = "'EOF', 'NEWLINE', 'TITLE', 'SET', 'UNION', 'TEXT', 'INDENT_TEXT', 'STYLE'";
const VALUE: &str = "'NUMERIC', 'IDENTIFIER', 'STRING', 'HEXCOLOR', 'RGBCOLOR', 'RGBACOLOR'";

/// The parser over the lexer, as the closures of Pele's parseVenn share it. A token is read
/// only when the grammar has to look at one to decide, because the lexer asks the db whether
/// an indented `text` continues a set, and the statements are what tell the db so.
struct Parser<'a, 'd> {
    src: &'a str,
    lexer: VennLexer<'a>,
    db: &'d mut VennDb,
    /// The type of the token read and not yet taken.
    kind: Option<usize>,
}

impl Parser<'_, '_> {
    fn peek(&mut self) -> usize {
        match self.kind {
            Some(k) => k,
            None => {
                let k = self.lexer.next(self.db as &mut dyn IndentHost);
                self.kind = Some(k);
                k
            }
        }
    }

    fn fail<T>(&mut self, expected: &str) -> Result<T, Error> {
        let kind = self.peek();
        let at = self.lexer.start;
        if kind == ERROR {
            return Err(syntax_error(DiagramType::Venn, self.src, at, "", true));
        }
        let detail = format!("Expecting {expected}, got '{}'", TOKEN_NAMES[kind]);
        Err(syntax_error(
            DiagramType::Venn,
            self.src,
            at,
            &detail,
            false,
        ))
    }

    fn take(&mut self) -> String {
        self.kind = None;
        self.lexer.text.clone()
    }

    fn expect(&mut self, wanted: usize) -> Result<String, Error> {
        if self.peek() != wanted {
            return self.fail(&format!("'{}'", TOKEN_NAMES[wanted]));
        }
        Ok(self.take())
    }

    fn identifier(&mut self) -> Result<String, Error> {
        let kind = self.peek();
        if kind != IDENTIFIER && kind != STRING {
            return self.fail("'IDENTIFIER', 'STRING'");
        }
        Ok(self.take())
    }

    fn identifiers(&mut self) -> Result<Vec<String>, Error> {
        let mut list = vec![self.identifier()?];
        while self.peek() == COMMA {
            self.take();
            list.push(self.identifier()?);
        }
        Ok(list)
    }

    fn subset(&mut self, ids: &[String], union: bool) -> Result<(), Error> {
        let label = if self.peek() == BRACKET_LABEL {
            Some(self.take())
        } else {
            None
        };
        let mut size = None;
        if self.peek() == COLON {
            self.take();
            size = Some(number(&self.expect(NUMERIC)?));
        }
        if union {
            if ids.len() < 2 {
                return Err(union_of_one());
            }
            self.db.validate_union_identifiers(ids)?;
        }
        self.db.add_subset_data(ids, label.as_deref(), size);
        self.db.set_indent_mode(true);
        Ok(())
    }

    /// The id of a text, and the label in brackets that an id which is not a number may
    /// carry. sets are those of the statement, or none for an indented text, which takes the
    /// db's current sets.
    fn text(&mut self, sets: Option<Vec<String>>) -> Result<(), Error> {
        let kind = self.peek();
        if kind != IDENTIFIER && kind != STRING && kind != NUMERIC {
            return self.fail("'NUMERIC', 'IDENTIFIER', 'STRING'");
        }
        let id = self.take();
        let label = if kind != NUMERIC && self.peek() == BRACKET_LABEL {
            Some(self.take())
        } else {
            None
        };
        let sets = match sets {
            Some(sets) => sets,
            None => self
                .db
                .current_sets()
                .ok_or_else(text_without_set)?
                .to_vec(),
        };
        self.db.add_text_data(&sets, &id, label.as_deref());
        Ok(())
    }

    fn style(&mut self) -> Result<(), Error> {
        let ids = self.identifiers()?;
        let mut styles: Vec<(String, String)> = Vec::new();
        loop {
            let key = self.expect(IDENTIFIER)?;
            self.expect(COLON)?;
            let value = if self.peek() == STRING {
                self.take()
            } else {
                let mut parts = Vec::new();
                loop {
                    let k = self.peek();
                    if k != IDENTIFIER && !(NUMERIC..=RGBACOLOR).contains(&k) {
                        break;
                    }
                    parts.push(self.take());
                }
                if parts.is_empty() {
                    return self.fail(VALUE);
                }
                parts.join(" ")
            };
            styles.push((key, value));
            if self.peek() != COMMA {
                break;
            }
            self.take();
        }
        self.db.add_style_data(&ids, &styles);
        Ok(())
    }
}

/// Parses a venn diagram's text into db, as Pele's parseVenn does, porting Mermaid's venn
/// grammar.
pub(crate) fn parse_venn(src: &str, db: &mut VennDb) -> Result<(), Error> {
    let mut p = Parser {
        src,
        lexer: VennLexer::new(src),
        db,
        kind: None,
    };
    while p.peek() == NEWLINE {
        p.take();
    }
    p.expect(VENN)?;
    loop {
        let statement = p.peek();
        if statement == EOF {
            return Ok(());
        }
        if !(NEWLINE..=STYLE).contains(&statement) || statement == VENN {
            return p.fail(STATEMENT);
        }
        p.take();
        match statement {
            TITLE => {
                let title = substring(&p.lexer.text, 6).to_string();
                p.db.set_diagram_title(&title);
            }
            SET => {
                let id = p.identifier()?;
                p.subset(&[id], false)?;
            }
            UNION => {
                let ids = p.identifiers()?;
                p.subset(&ids, true)?;
            }
            TEXT => {
                let ids = p.identifiers()?;
                p.text(Some(ids))?;
            }
            INDENT_TEXT => p.text(None)?,
            STYLE => p.style()?,
            _ => {}
        }
    }
}
