//! The parser of Cynefin diagrams, as Pele's parser.ts reads them.

use std::sync::OnceLock;

use crate::common::tokens::{
    ACC_DESCR, ACC_TITLE, DIRECTIVE, END, NEWLINE, Pattern, Reader, SINGLE_LINE_COMMENT, STRING,
    TITLE, Table, TokenType, WHITESPACE, YAML, acc_descr_value, acc_title_value, keyword,
    string_value, title_value, token, tokenize,
};
use crate::detect::DiagramType;
use crate::errors::Error;

// The kinds of token, by index in CYNEFIN_TOKENS, as Pele's T.
pub(crate) const T_KEYWORD_COLON: usize = 0;
pub(crate) const T_KEYWORD: usize = 1;
pub(crate) const T_ARROW: usize = 2;
pub(crate) const T_COLON: usize = 3;
pub(crate) const T_DOMAIN: usize = 4;
pub(crate) const T_ACC_DESCR: usize = 5;
pub(crate) const T_ACC_TITLE: usize = 6;
pub(crate) const T_TITLE: usize = 7;
pub(crate) const T_STRING: usize = 8;
pub(crate) const T_NEWLINE: usize = 9;

/// The names of the five domains, in the order DOMAIN_NAME tries them.
const DOMAIN_NAMES: [&str; 5] = ["complex", "complicated", "clear", "chaotic", "confusion"];

/// What DOMAIN_NAME's pattern, /complex|complicated|clear|chaotic|confusion/y, matches at
/// byte at: the first of the names the text goes on with, whatever follows it.
fn domain_name(src: &str, at: usize) -> Option<usize> {
    let rest = &src[at..];
    DOMAIN_NAMES
        .iter()
        .find(|name| rest.starts_with(*name))
        .map(|name| at + name.len())
}

/// Pele's CYNEFIN_TOKENS.
pub(crate) const CYNEFIN_TOKENS: &[TokenType] = &[
    token("cynefin-beta:", Pattern::Literal("cynefin-beta:")),
    keyword("cynefin-beta"),
    token("-->", Pattern::Literal("-->")),
    token(":", Pattern::Literal(":")),
    TokenType {
        first: Some("c"),
        ..token("DOMAIN_NAME", Pattern::Match(domain_name))
    },
    ACC_DESCR,
    ACC_TITLE,
    TITLE,
    STRING,
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
];

/// CYNEFIN_TOKENS made ready to tokenize with.
pub(crate) fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| Table::new(CYNEFIN_TOKENS))
}

/// A block of a domain and the items it lists, Pele's DomainBlockAst.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DomainBlockAst {
    pub(crate) domain: String,
    pub(crate) items: Vec<String>,
}

/// A transition, Pele's TransitionAst. A label Pele leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TransitionAst {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) label: Option<String>,
}

/// The tree of a Cynefin diagram, Pele's CynefinAst. What Pele leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CynefinAst {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) domains: Vec<DomainBlockAst>,
    pub(crate) transitions: Vec<TransitionAst>,
}

/// Parses a Cynefin diagram's text into its tree, as Pele's parseCynefin does.
pub(crate) fn parse_cynefin(src: &str) -> Result<CynefinAst, Error> {
    let tokens = tokenize(src, table(), DiagramType::Cynefin)?;
    let mut r = Reader::new(tokens, table(), DiagramType::Cynefin);
    let mut ast = CynefinAst::default();
    let skip_lines = |r: &mut Reader| {
        while r.kind() == T_NEWLINE {
            r.i += 1;
        }
    };
    let end_of_line = |r: &mut Reader| -> Result<(), Error> {
        if r.kind() == END {
            return Ok(());
        }
        if r.kind() != T_NEWLINE {
            return Err(r.fail("a line break or the end of input"));
        }
        skip_lines(r);
        Ok(())
    };

    skip_lines(&mut r);
    if r.kind() != T_KEYWORD && r.kind() != T_KEYWORD_COLON {
        return Err(r.fail("'cynefin-beta'"));
    }
    r.i += 1;
    while r.kind() != END {
        match r.kind() {
            T_NEWLINE => r.i += 1,
            T_ACC_DESCR => {
                ast.acc_descr = Some(acc_descr_value(r.take()));
                end_of_line(&mut r)?;
            }
            T_ACC_TITLE => {
                ast.acc_title = Some(acc_title_value(r.take()));
                end_of_line(&mut r)?;
            }
            T_TITLE => {
                ast.title = Some(title_value(r.take()));
                end_of_line(&mut r)?;
            }
            T_DOMAIN => {
                let name = r.take().to_string();
                if r.accept(T_ARROW) {
                    let to = r.expect(T_DOMAIN)?.to_string();
                    let mut transition = TransitionAst {
                        from: name,
                        to,
                        label: None,
                    };
                    if r.accept(T_COLON) {
                        transition.label = Some(string_value(r.expect(T_STRING)?));
                    }
                    end_of_line(&mut r)?;
                    ast.transitions.push(transition);
                    continue;
                }
                let mut block = DomainBlockAst {
                    domain: name,
                    items: Vec::new(),
                };
                skip_lines(&mut r);
                while r.kind() == T_STRING {
                    block.items.push(string_value(r.take()));
                    skip_lines(&mut r);
                }
                ast.domains.push(block);
            }
            _ => return Err(r.fail("a domain, a transition, a title, or a line break")),
        }
    }
    Ok(ast)
}
