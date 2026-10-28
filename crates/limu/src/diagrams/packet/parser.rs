//! The parser of packet diagrams, as Pele's parser.ts reads them.

use std::sync::OnceLock;

use crate::common::tokens::{
    ACC_DESCR, ACC_TITLE, DIRECTIVE, END, NEWLINE, Pattern, Reader, SINGLE_LINE_COMMENT, STRING,
    TITLE, Table, TokenType, WHITESPACE, YAML, acc_descr_value, acc_title_value, keyword,
    string_value, title_value, token, tokenize,
};
use crate::detect::DiagramType;
use crate::errors::Error;
use crate::js::number::number;

// The kinds of token, by index in PACKET_TOKENS, as Pele's T.
const T_PACKET_BETA: usize = 0;
const T_PACKET: usize = 1;
const T_DASH: usize = 2;
const T_PLUS: usize = 3;
const T_COLON: usize = 4;
const T_ACC_DESCR: usize = 5;
const T_ACC_TITLE: usize = 6;
const T_TITLE: usize = 7;
const T_INT: usize = 8;
const T_STRING: usize = 9;
const T_NEWLINE: usize = 10;
const T_YAML: usize = 12;

/// Pele's INT, /0|[1-9][0-9]*(?!\.)/y: a lone 0, or digits not followed by a dot, which give
/// back the last digit rather than fail where they are.
fn int(s: &str, at: usize) -> Option<usize> {
    let b = s.as_bytes();
    match b[at] {
        b'0' => return Some(at + 1),
        b'1'..=b'9' => {}
        _ => return None,
    }
    let mut end = at + 1;
    while end < b.len() && b[end].is_ascii_digit() {
        end += 1;
    }
    if b.get(end) == Some(&b'.') {
        return (end - 1 > at).then_some(end - 1);
    }
    Some(end)
}

/// Pele's PACKET_TOKENS.
pub(crate) const PACKET_TOKENS: &[TokenType] = &[
    token("packet-beta", Pattern::Literal("packet-beta")),
    keyword("packet"),
    // A front matter block also starts with dashes, so Langium lets it win over a dash.
    TokenType {
        longer: &[T_YAML],
        ..token("-", Pattern::Literal("-"))
    },
    token("+", Pattern::Literal("+")),
    token(":", Pattern::Literal(":")),
    ACC_DESCR,
    ACC_TITLE,
    TITLE,
    token("INT", Pattern::Match(int)),
    STRING,
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
];

/// PACKET_TOKENS made ready to tokenize with.
pub(crate) fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| Table::new(PACKET_TOKENS))
}

/// A field of the tree, Pele's PacketBlockAst. What Pele leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PacketBlockAst {
    pub(crate) start: Option<f64>,
    pub(crate) end: Option<f64>,
    pub(crate) bits: Option<f64>,
    pub(crate) label: String,
}

/// The tree of a packet diagram, Pele's PacketAst. What Pele leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PacketAst {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) blocks: Vec<PacketBlockAst>,
}

/// Parses a packet diagram's text into its tree, as Pele's parsePacket does.
pub(crate) fn parse_packet(src: &str) -> Result<PacketAst, Error> {
    let tokens = tokenize(src, table(), DiagramType::Packet)?;
    let mut r = Reader::new(tokens, table(), DiagramType::Packet);
    let mut ast = PacketAst::default();
    let end_of_line = |r: &mut Reader| -> Result<(), Error> {
        if r.kind() == END {
            return Ok(());
        }
        if r.kind() != T_NEWLINE {
            return Err(r.fail("a line break or the end of input"));
        }
        while r.kind() == T_NEWLINE {
            r.i += 1;
        }
        Ok(())
    };

    while r.kind() == T_NEWLINE {
        r.i += 1;
    }
    if !r.accept(T_PACKET_BETA) {
        r.expect(T_PACKET)?;
    }
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
            T_INT | T_PLUS => {
                let mut block = PacketBlockAst::default();
                if r.accept(T_PLUS) {
                    block.bits = Some(number(r.expect(T_INT)?));
                } else {
                    block.start = Some(number(r.take()));
                    if r.accept(T_DASH) {
                        block.end = Some(number(r.expect(T_INT)?));
                    }
                }
                r.expect(T_COLON)?;
                block.label = string_value(r.expect(T_STRING)?);
                ast.blocks.push(block);
                end_of_line(&mut r)?;
            }
            _ => return Err(r.fail("a field, a title, or a line break")),
        }
    }
    Ok(ast)
}
