//! A block diagram's tokens read into its statements, as Pele's parser.ts reads them,
//! making the decisions Mermaid's Jison parser makes.

use super::db::{
    Block, BlockDb, Statement, edge_str_to_edge_data, edge_str_to_edge_start_data,
    edge_str_to_pattern, edge_str_to_thickness, type_str2_type,
};
use super::lexer::{
    BLOCK_ARROW_END, BLOCK_ARROW_START, CLASS, CLASSDEF, CLASSDEF_ID, CLASSDEF_STYLEOPTS,
    CLASSENTITY_IDS, COLUMNS, DIR, END_KEYWORD, EOF, ERROR, ID_BLOCK, KEY, LINK, LINK_LABEL,
    NODE_DEND, NODE_DSTART, NODE_ID, SIZE, SPACE_BLOCK, START_LINK, STR, STYLE,
    STYLE_DEFINITION_DATA, STYLE_ENTITY_IDS, STYLECLASS, TOKEN_NAMES, Tokens, tokenize,
};
use crate::detect::DiagramType;
use crate::errors::{Error, syntax_error};
use crate::js::chars::trim;
use crate::js::number::parse_int;

/// What the start of a node statement says of its block, Pele's NodeInfo.
struct NodeInfo {
    id: String,
    label: Option<String>,
    type_str: Option<String>,
    directions: Option<Vec<String>>,
}

/// A statement list being read, and the statement that opened it, Pele's Frame.
struct Frame {
    list: Vec<Statement>,
    header: Option<Statement>,
}

const STATEMENT_START: &str = "'NODE_ID', 'COLUMNS', 'SPACE_BLOCK', 'BLOCK_DIAGRAM_KEY', 'id-block', 'classDef', 'class', 'style'";

/// The statements of a document, with its last statement inlined where it is a list, as
/// the grammar does and Pele's closeDocument does.
fn close_document(mut list: Vec<Statement>) -> Vec<Statement> {
    if let Some(Statement::Chain(last)) = list.last() {
        let last = last.clone();
        list.pop();
        list.extend(last.into_iter().map(Statement::One));
    }
    list
}

/// What the parser reads: the tokens, the next of them, and the model it fills.
struct Parser<'a> {
    src: &'a str,
    t: Tokens,
    i: usize,
    db: &'a mut BlockDb,
}

impl Parser<'_> {
    fn kind(&self) -> usize {
        self.t.types[self.i]
    }

    fn fail(&self, expected: &str) -> Error {
        let kind = self.kind();
        let at = self.t.starts[self.i];
        if kind == ERROR {
            return syntax_error(DiagramType::Block, self.src, at, "", true);
        }
        let detail = format!("Expecting {expected}, got '{}'", TOKEN_NAMES[kind]);
        syntax_error(DiagramType::Block, self.src, at, &detail, false)
    }

    fn expect(&mut self, kind: usize) -> Result<String, Error> {
        if self.kind() != kind {
            return Err(self.fail(&format!("'{}'", TOKEN_NAMES[kind])));
        }
        self.i += 1;
        Ok(self.t.texts[self.i - 1].clone())
    }

    fn take(&mut self) -> String {
        self.i += 1;
        self.t.texts[self.i - 1].clone()
    }

    fn node(&mut self) -> Result<NodeInfo, Error> {
        let id = self.expect(NODE_ID)?;
        if self.kind() == NODE_DSTART {
            let open = self.take();
            let label = self.expect(STR)?;
            let close = self.expect(NODE_DEND)?;
            return Ok(NodeInfo {
                id,
                label: Some(label),
                type_str: Some(open + &close),
                directions: None,
            });
        }
        if self.kind() == BLOCK_ARROW_START {
            let open = self.take();
            let label = self.expect(STR)?;
            let mut directions = vec![self.expect(DIR)?];
            while self.kind() == DIR {
                directions.push(self.take());
            }
            let close = self.expect(BLOCK_ARROW_END)?;
            return Ok(NodeInfo {
                id,
                label: Some(label),
                type_str: Some(open + &close),
                directions: Some(directions),
            });
        }
        Ok(NodeInfo {
            id,
            label: None,
            type_str: None,
            directions: None,
        })
    }

    fn node_statement(&mut self) -> Result<Statement, Error> {
        let first = self.node()?;
        let size = if self.kind() == SIZE {
            parse_int(&self.take(), 10)
        } else {
            1.0
        };
        let mut from = Block {
            id: Some(first.id.clone()),
            label: first.label.clone(),
            r#type: type_str2_type(first.type_str.as_deref()).into(),
            directions: first.directions.clone(),
            width_in_columns: Some(size),
            ..Block::default()
        };
        let mut chain: Option<Vec<usize>> = None;
        while self.kind() == LINK || self.kind() == START_LINK {
            let mut label = String::new();
            self.i += 1;
            if self.t.types[self.i - 1] == START_LINK {
                self.expect(LINK_LABEL)?;
                label = self.expect(STR)?;
                self.expect(LINK)?;
            }
            let link = self.t.texts[self.i - 1].clone();
            let to = self.node()?;
            // Mermaid keeps only the last link of `A --> B --> C` and loses its start. Here
            // every link counts.
            let list = chain.get_or_insert_with(|| {
                vec![self.db.add(Block {
                    id: from.id.clone(),
                    label: from.label.clone(),
                    r#type: from.r#type.clone(),
                    directions: from.directions.clone(),
                    ..Block::default()
                })]
            });
            from = Block {
                id: Some(to.id.clone()),
                label: to.label,
                r#type: type_str2_type(to.type_str.as_deref()).into(),
                directions: to.directions.clone(),
                ..Block::default()
            };
            let last = self.db.arena[*list.last().expect("a block")]
                .id
                .clone()
                .expect("an id");
            let edge = self.db.add(Block {
                id: Some(format!("{last}-{}", to.id)),
                start: Some(last),
                end: Some(to.id),
                label: Some(label),
                r#type: "edge".into(),
                thickness: Some(edge_str_to_thickness(&link).into()),
                pattern: Some(edge_str_to_pattern(&link).into()),
                directions: to.directions,
                arrow_type_end: Some(edge_str_to_edge_data(&link).into()),
                arrow_type_start: Some(edge_str_to_edge_start_data(&link).into()),
                ..Block::default()
            });
            let to = self.db.add(from.clone());
            list.extend([edge, to]);
        }
        Ok(match chain {
            Some(list) => Statement::Chain(list),
            None => Statement::One(self.db.add(from)),
        })
    }

    /// Reads the statements to the end of the text into the model, as Pele's parseBlock
    /// does. Composites nest without recursion: each open one keeps the statement list of
    /// its parent.
    fn parse(&mut self) -> Result<(), Error> {
        self.expect(KEY)?;
        let mut open: Vec<Frame> = Vec::new();
        let mut list: Vec<Statement> = Vec::new();
        loop {
            match self.kind() {
                NODE_ID => {
                    let s = self.node_statement()?;
                    list.push(s);
                }
                COLUMNS => {
                    let columns = parse_int(&self.take(), 10);
                    list.push(Statement::One(self.db.add(Block {
                        r#type: "column-setting".into(),
                        columns: Some(columns),
                        ..Block::default()
                    })));
                }
                SPACE_BLOCK => {
                    let width = parse_int(&self.take(), 10);
                    let id = self.db.generate_id();
                    list.push(Statement::One(self.db.add(Block {
                        id: Some(id),
                        r#type: "space".into(),
                        label: Some(String::new()),
                        width: Some(width),
                        children: Some(Vec::new()),
                        ..Block::default()
                    })));
                }
                KEY => {
                    self.i += 1;
                    open.push(Frame {
                        list: std::mem::take(&mut list),
                        header: None,
                    });
                }
                ID_BLOCK => {
                    self.i += 1;
                    if self.kind() != NODE_ID {
                        return Err(self.fail("'NODE_ID'"));
                    }
                    let header = self.node_statement()?;
                    open.push(Frame {
                        list: std::mem::take(&mut list),
                        header: Some(header),
                    });
                }
                END_KEYWORD => {
                    let Some(frame) = open.pop().filter(|_| !list.is_empty()) else {
                        return Err(self.fail(STATEMENT_START));
                    };
                    self.i += 1;
                    let id = self.db.generate_id();
                    let children = close_document(std::mem::replace(&mut list, frame.list));
                    let composite = match frame.header {
                        None => Block {
                            id: Some(id),
                            r#type: "composite".into(),
                            label: Some(String::new()),
                            statements: children,
                            ..Block::default()
                        },
                        Some(Statement::One(header)) => Block {
                            r#type: "composite".into(),
                            statements: children,
                            ..self.db.arena[header].clone()
                        },
                        // A list spread into an object leaves it none of a block's fields.
                        Some(Statement::Chain(_)) => Block {
                            r#type: "composite".into(),
                            statements: children,
                            ..Block::default()
                        },
                    };
                    list.push(Statement::One(self.db.add(composite)));
                }
                CLASSDEF => {
                    self.i += 1;
                    let id = trim(&self.expect(CLASSDEF_ID)?).to_string();
                    let css = trim(&self.expect(CLASSDEF_STYLEOPTS)?).to_string();
                    list.push(Statement::One(self.db.add(Block {
                        r#type: "classDef".into(),
                        id: Some(id),
                        css: Some(css),
                        ..Block::default()
                    })));
                }
                CLASS => {
                    self.i += 1;
                    let id = trim(&self.expect(CLASSENTITY_IDS)?).to_string();
                    let class = trim(&self.expect(STYLECLASS)?).to_string();
                    list.push(Statement::One(self.db.add(Block {
                        r#type: "applyClass".into(),
                        id: Some(id),
                        style_class: Some(class),
                        ..Block::default()
                    })));
                }
                STYLE => {
                    self.i += 1;
                    let id = trim(&self.expect(STYLE_ENTITY_IDS)?).to_string();
                    let styles = trim(&self.expect(STYLE_DEFINITION_DATA)?).to_string();
                    list.push(Statement::One(self.db.add(Block {
                        r#type: "applyStyles".into(),
                        id: Some(id),
                        styles_str: Some(styles),
                        ..Block::default()
                    })));
                }
                EOF => {
                    if !open.is_empty() {
                        return Err(self.fail(&format!("{STATEMENT_START}, 'end'")));
                    }
                    if list.is_empty() {
                        return Err(self.fail(STATEMENT_START));
                    }
                    return self.db.set_hierarchy(&close_document(list));
                }
                _ => {
                    let expected = if list.is_empty() {
                        STATEMENT_START.to_string()
                    } else if !open.is_empty() {
                        format!("{STATEMENT_START}, 'end'")
                    } else {
                        format!("{STATEMENT_START}, 'EOF'")
                    };
                    return Err(self.fail(&expected));
                }
            }
        }
    }
}

/// Parses a block diagram's text into a model, as Pele's parseBlock does.
pub(crate) fn parse_block(src: &str, db: &mut BlockDb) -> Result<(), Error> {
    let t = tokenize(src);
    Parser { src, t, i: 0, db }.parse()
}
