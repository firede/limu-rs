//! The parser of info diagrams, as Pele's parser.ts reads them.

use std::sync::OnceLock;

use crate::common::tokens::{
    ACC_DESCR, ACC_TITLE, DIRECTIVE, END, NEWLINE, Reader, SINGLE_LINE_COMMENT, TITLE, Table,
    TokenType, WHITESPACE, YAML, acc_descr_value, acc_title_value, keyword, title_value, tokenize,
};
use crate::detect::DiagramType;
use crate::errors::Error;

// The kinds of token, by index in INFO_TOKENS, as Pele's T.
const T_SHOW_INFO: usize = 0;
const T_INFO: usize = 1;
const T_ACC_DESCR: usize = 2;
const T_ACC_TITLE: usize = 3;
const T_TITLE: usize = 4;
const T_NEWLINE: usize = 5;

/// Pele's INFO_TOKENS.
pub(crate) const INFO_TOKENS: &[TokenType] = &[
    keyword("showInfo"),
    keyword("info"),
    ACC_DESCR,
    ACC_TITLE,
    TITLE,
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
];

/// INFO_TOKENS made ready to tokenize with.
pub(crate) fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| Table::new(INFO_TOKENS))
}

/// The tree of an info diagram, Pele's InfoAst. What Pele leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct InfoAst {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
}

/// Parses an info diagram's text into its tree, as Pele's parseInfo does.
pub(crate) fn parse_info(src: &str) -> Result<InfoAst, Error> {
    let tokens = tokenize(src, table(), DiagramType::Info)?;
    let mut r = Reader::new(tokens, table(), DiagramType::Info);
    let mut ast = InfoAst::default();
    let newlines = |r: &mut Reader| {
        while r.kind() == T_NEWLINE {
            r.i += 1;
        }
    };

    newlines(&mut r);
    r.expect(T_INFO)?;
    newlines(&mut r);
    if r.accept(T_SHOW_INFO) {
        newlines(&mut r);
    }
    while r.kind() != END {
        match r.kind() {
            T_ACC_DESCR => ast.acc_descr = Some(acc_descr_value(r.take())),
            T_ACC_TITLE => ast.acc_title = Some(acc_title_value(r.take())),
            T_TITLE => ast.title = Some(title_value(r.take())),
            _ => return Err(r.fail("a title or the end of input")),
        }
        if r.kind() == END {
            break;
        }
        if r.kind() != T_NEWLINE {
            return Err(r.fail("a line break or the end of input"));
        }
        newlines(&mut r);
    }
    Ok(ast)
}
