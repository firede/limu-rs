//! The parser of an entity relationship diagram, as Pele's parser.ts has it: recursive
//! descent over the tokens of its lexer, agreeing with Mermaid's erDiagram.jison, its errors
//! worded as Jison words them.

use super::db::{DocItem, ErDb, ParsedAttribute, RelSpec};
use super::lexer::{Tok, tokenize};
use super::tokens::{DIRECTION_TOKENS, T};
use crate::detect::DiagramType;
use crate::errors::{Error, syntax_error};
use crate::js::chars::trim;

/// The cardinality a token is, as Mermaid names it, Pele's CARDINALITY.
fn cardinality(t: T) -> Option<&'static str> {
    Some(match t {
        T::ZERO_OR_ONE => "ZERO_OR_ONE",
        T::ZERO_OR_MORE => "ZERO_OR_MORE",
        T::ONE_OR_MORE => "ONE_OR_MORE",
        T::ONLY_ONE => "ONLY_ONE",
        T::MD_PARENT => "MD_PARENT",
        _ => return None,
    })
}

const DIRECTIONS: [&str; 4] = ["TB", "BT", "RL", "LR"];

const NAME: &str = "'UNICODE_TEXT', 'NUM', 'ENTITY_NAME', 'DECIMAL_NUM', 'ENTITY_ONE'";
const CARDS: &str = "'ZERO_OR_ONE', 'ZERO_OR_MORE', 'ONE_OR_MORE', 'ONLY_ONE', 'MD_PARENT'";
const LINE: &str = "'EOF', 'NEWLINE', 'title', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'SUBGRAPH', 'direction_tb', 'direction_bt', 'direction_rl', 'direction_lr', 'CLASSDEF', 'CLASS', 'STYLE', 'UNICODE_TEXT', 'NUM', 'ENTITY_NAME', 'DECIMAL_NUM', 'ENTITY_ONE'";

/// A subgraph being read, Pele's Frame: its id and title, and what Mermaid's grammar hands
/// to addSubGraph, the value of every statement inside. Besides entities' names that holds
/// accessible text and the keywords `style`, `class` and `classDef`.
struct Frame {
    id: String,
    title: String,
    list: Vec<DocItem>,
}

/// The tokens a name is, Pele's isName.
fn is_name(t: T) -> bool {
    matches!(
        t,
        T::UNICODE_TEXT | T::ENTITY_NAME | T::NUM | T::DECIMAL_NUM | T::ENTITY_ONE
    )
}

/// The tokens a style is made of, Pele's isStyleComponent.
fn is_style_component(t: T) -> bool {
    matches!(t, T::STYLE_TEXT | T::NUM | T::COLON | T::BRKT)
}

/// text without its first and last characters, the quotes about it, as Pele's slice(1, -1)
/// takes them off. The lexer gives quoted text only with both.
fn unquote(text: &str) -> String {
    text[1..text.len() - 1].to_string()
}

struct Parser<'a, 'd> {
    source: &'a str,
    toks: Vec<Tok<'a>>,
    i: usize,
    db: &'d mut ErDb,
}

/// Parses an entity relationship diagram's text into db, as Pele's parseEr does.
pub(crate) fn parse_er(source: &str, db: &mut ErDb) -> Result<(), Error> {
    let mut p = Parser {
        source,
        toks: tokenize(source),
        i: 0,
        db,
    };
    p.parse()
}

impl Parser<'_, '_> {
    /// The type of the token at i. Past the end, where Pele reads undefined, it is END,
    /// which no rule takes either.
    fn kind(&self) -> T {
        self.toks.get(self.i).map_or(T::END, |t| t.kind)
    }

    fn text(&self) -> &str {
        self.toks.get(self.i).map_or("", |t| t.text)
    }

    /// The text of the token at i, and moves past it.
    fn take(&mut self) -> String {
        let s = self.text().to_string();
        self.i += 1;
        s
    }

    fn fail(&self, expected: &str) -> Error {
        let last = self.toks.len() - 1;
        let tok = &self.toks[self.i.min(last)];
        if tok.kind == T::ERROR {
            return syntax_error(DiagramType::Er, self.source, tok.start, "", true);
        }
        let got = if tok.kind == T::CHAR {
            tok.text
        } else {
            tok.kind.name()
        };
        let detail = format!("Expecting {expected}, got '{got}'");
        syntax_error(DiagramType::Er, self.source, tok.start, &detail, false)
    }

    fn expect(&mut self, kind: T) -> Result<String, Error> {
        if self.kind() != kind {
            return Err(self.fail(&format!("'{}'", kind.name())));
        }
        Ok(self.take())
    }

    fn separator(&mut self) -> Result<(), Error> {
        if !matches!(self.kind(), T::NEWLINE | T::SEMI | T::EOF) {
            return Err(self.fail("'EOF', 'NEWLINE', 'SEMI'"));
        }
        self.i += 1;
        Ok(())
    }

    fn entity_name(&mut self) -> Result<String, Error> {
        let t = self.kind();
        if !is_name(t) {
            return Err(self.fail(NAME));
        }
        let text = self.take();
        Ok(if t == T::ENTITY_NAME {
            unquote(&text)
        } else {
            text
        })
    }

    fn id_list(&mut self) -> Result<Vec<String>, Error> {
        let mut ids = Vec::new();
        loop {
            if !matches!(self.kind(), T::UNICODE_TEXT | T::STYLE_TEXT) {
                return Err(self.fail("'UNICODE_TEXT', 'STYLE_TEXT'"));
            }
            ids.push(self.take());
            if self.kind() != T::COMMA {
                return Ok(ids);
            }
            self.i += 1;
        }
    }

    fn styles_opt(&mut self) -> Result<Vec<String>, Error> {
        let mut styles = Vec::new();
        loop {
            if !is_style_component(self.kind()) {
                return Err(self.fail("'STYLE_TEXT', 'COLON', 'NUM', 'BRKT'"));
            }
            let mut style = self.take();
            while is_style_component(self.kind()) {
                style.push_str(self.text());
                self.i += 1;
            }
            styles.push(style);
            if self.kind() != T::COMMA {
                return Ok(styles);
            }
            self.i += 1;
        }
    }

    fn rel_spec(&mut self) -> Result<RelSpec, Error> {
        let card_b = cardinality(self.kind()).expect("a cardinality, as its caller checks");
        self.i += 1;
        let rel_type = match self.kind() {
            T::IDENTIFYING => "IDENTIFYING",
            T::NON_IDENTIFYING => "NON_IDENTIFYING",
            _ => return Err(self.fail("'NON_IDENTIFYING', 'IDENTIFYING'")),
        };
        self.i += 1;
        let Some(card_a) = cardinality(self.kind()) else {
            return Err(self.fail(CARDS));
        };
        self.i += 1;
        Ok(RelSpec {
            card_a,
            rel_type,
            card_b,
        })
    }

    fn role(&mut self) -> Result<String, Error> {
        let t = self.kind();
        if !matches!(t, T::WORD | T::ENTITY_NAME | T::UNICODE_TEXT) {
            return Err(self.fail("'UNICODE_TEXT', 'ENTITY_NAME', 'WORD'"));
        }
        let text = self.take();
        Ok(if t == T::UNICODE_TEXT {
            text
        } else {
            unquote(&text)
        })
    }

    fn is_char(&self, ch: &str) -> bool {
        self.kind() == T::CHAR && self.text() == ch
    }

    /// The attributes of a block, last to first, as Mermaid's right-recursive rule builds
    /// them, as Pele's attributes reads them.
    fn attributes(&mut self) -> Result<Vec<ParsedAttribute>, Error> {
        let mut list = Vec::new();
        loop {
            let mut kind = self.expect(T::ATTRIBUTE_WORD)?;
            if self.is_char("?") {
                kind.push_str(self.text());
                self.i += 1;
            }
            if self.kind() != T::ATTRIBUTE_WORD {
                return Err(self.fail(if kind.ends_with('?') {
                    "'ATTRIBUTE_WORD'"
                } else {
                    "'ATTRIBUTE_WORD', '?'"
                }));
            }
            let mut attribute = ParsedAttribute {
                kind,
                name: self.take(),
                ..ParsedAttribute::default()
            };
            if self.kind() == T::ATTRIBUTE_KEY {
                let mut keys = vec![self.take()];
                while self.is_char(",") {
                    self.i += 1;
                    keys.push(self.expect(T::ATTRIBUTE_KEY)?);
                }
                attribute.keys = Some(keys);
            }
            if self.kind() == T::COMMENT {
                attribute.comment = Some(unquote(&self.take()));
            }
            list.push(attribute);
            if self.kind() == T::BLOCK_STOP {
                break;
            }
        }
        list.reverse();
        Ok(list)
    }

    /// An optional `{ ... }` block, none where there is none or it is empty, as Pele's block
    /// reads it.
    fn block(&mut self) -> Result<Option<Vec<ParsedAttribute>>, Error> {
        if self.kind() != T::BLOCK_START {
            return Ok(None);
        }
        self.i += 1;
        if self.kind() == T::BLOCK_STOP {
            self.i += 1;
            return Ok(None);
        }
        let list = self.attributes()?;
        self.i += 1;
        Ok(Some(list))
    }

    /// Classes after `:::`, where they are written.
    fn classes(&mut self) -> Result<Option<Vec<String>>, Error> {
        if self.kind() != T::STYLE_SEPARATOR {
            return Ok(None);
        }
        self.i += 1;
        self.id_list().map(Some)
    }

    /// An entity, with its alias, classes or attributes, or a relationship, as Pele's
    /// entityStatement reads it. It gives the names of the entities it names.
    fn entity_statement(&mut self) -> Result<Vec<String>, Error> {
        let a = self.entity_name()?;
        if self.kind() == T::SQS {
            self.i += 1;
            let alias = self.entity_name()?;
            self.expect(T::SQE)?;
            let classes = self.classes()?;
            let attrs = self.block()?;
            self.db.add_entity(&a, &alias);
            if let Some(attrs) = attrs {
                self.db.add_attributes(&a, attrs);
            }
            if let Some(classes) = classes {
                self.db.set_class(std::slice::from_ref(&a), &classes);
            }
            return Ok(vec![a]);
        }
        let classes = self.classes()?;
        if cardinality(self.kind()).is_some() {
            let spec = self.rel_spec()?;
            let b = self.entity_name()?;
            let classes_b = self.classes()?;
            self.expect(T::COLON)?;
            let label = self.role()?;
            self.db.add_entity(&a, "");
            self.db.add_entity(&b, "");
            self.db.add_relationship(&a, &label, &b, spec);
            if let Some(classes) = classes {
                self.db.set_class(std::slice::from_ref(&a), &classes);
            }
            if let Some(classes) = classes_b {
                self.db.set_class(std::slice::from_ref(&b), &classes);
            }
            return Ok(vec![a, b]);
        }
        let attrs = self.block()?;
        self.db.add_entity(&a, "");
        if let Some(attrs) = attrs {
            self.db.add_attributes(&a, attrs);
        }
        if let Some(classes) = classes {
            self.db.set_class(std::slice::from_ref(&a), &classes);
        }
        Ok(vec![a])
    }

    fn parse(&mut self) -> Result<(), Error> {
        self.expect(T::ER_DIAGRAM)?;
        // Subgraphs nest on this stack rather than on the call stack. The first frame is the
        // diagram itself.
        let mut frames = vec![Frame {
            id: String::new(),
            title: String::new(),
            list: Vec::new(),
        }];
        loop {
            let nested = frames.len() > 1;
            let frame = frames.last_mut().expect("the diagram's frame");
            let t = self.kind();
            match t {
                T::NEWLINE => self.i += 1,
                T::EOF => {
                    self.i += 1;
                    if nested {
                        return Err(self.fail(&format!("'END', {LINE}")));
                    }
                    return Ok(());
                }
                T::END_KW => {
                    if !nested {
                        return Err(self.fail(LINE));
                    }
                    self.i += 1;
                    let done = frames.pop().expect("a subgraph's frame");
                    let id = self.db.add_sub_graph(&done.id, &done.list, &done.title);
                    if frames.len() > 1 {
                        let frame = frames.last_mut().expect("a subgraph's frame");
                        frame.list.push(DocItem::Text(id));
                    }
                }
                T::SUBGRAPH => {
                    self.i += 1;
                    let id = self.entity_name()?;
                    let mut title = id.clone();
                    if self.kind() == T::SQS {
                        self.i += 1;
                        title = self.entity_name()?;
                        while is_name(self.kind()) {
                            title.push(' ');
                            title.push_str(&self.entity_name()?);
                        }
                        self.expect(T::SQE)?;
                    }
                    self.separator()?;
                    frames.push(Frame {
                        id,
                        title,
                        list: Vec::new(),
                    });
                }
                T::acc_title => {
                    self.i += 1;
                    let value = trim(&self.expect(T::acc_title_value)?).to_string();
                    self.db.set_acc_title(&value);
                    if nested {
                        frame.list.push(DocItem::Text(value));
                    }
                }
                T::acc_descr => {
                    self.i += 1;
                    let value = trim(&self.expect(T::acc_descr_value)?).to_string();
                    self.db.set_acc_description(&value);
                    if nested {
                        frame.list.push(DocItem::Text(value));
                    }
                }
                T::acc_descr_multiline_value => {
                    let value = trim(&self.take()).to_string();
                    self.db.set_acc_description(&value);
                    if nested {
                        frame.list.push(DocItem::Text(value));
                    }
                }
                T::direction_tb | T::direction_bt | T::direction_rl | T::direction_lr => {
                    self.i += 1;
                    let at = DIRECTION_TOKENS
                        .iter()
                        .position(|&d| d == t)
                        .expect("a direction");
                    let value = DIRECTIONS[at];
                    if nested {
                        frame.list.push(DocItem::Dir(value));
                    } else {
                        self.db.set_direction(value);
                    }
                }
                T::CLASSDEF | T::STYLE => {
                    let keyword = self.take();
                    let ids = self.id_list()?;
                    let styles = self.styles_opt()?;
                    self.separator()?;
                    if t == T::CLASSDEF {
                        self.db.add_class(&ids, &styles);
                    } else {
                        self.db.add_css_styles(&ids, &styles);
                    }
                    if nested {
                        frame.list.push(DocItem::Text(keyword));
                    }
                }
                T::CLASS => {
                    let keyword = self.take();
                    let ids = self.id_list()?;
                    let classes = self.id_list()?;
                    self.db.set_class(&ids, &classes);
                    if nested {
                        frame.list.push(DocItem::Text(keyword));
                    }
                }
                _ => {
                    if !is_name(t) {
                        return Err(self.fail(&if nested {
                            format!("'END', {LINE}")
                        } else {
                            LINE.to_string()
                        }));
                    }
                    let names = self.entity_statement()?;
                    if nested {
                        let frame = frames.last_mut().expect("a subgraph's frame");
                        frame.list.extend(names.into_iter().map(DocItem::Text));
                    }
                }
            }
        }
    }
}
