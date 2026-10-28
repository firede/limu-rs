//! The parser of pie charts, as Pele's parser.ts reads them.

use std::sync::OnceLock;

use crate::common::tokens::{
    ACC_DESCR, ACC_TITLE, DIRECTIVE, END, NEWLINE, Pattern, Reader, SINGLE_LINE_COMMENT, STRING,
    Scan, TITLE, Table, TokenType, WHITESPACE, YAML, acc_descr_value, acc_title_value, keyword,
    string_value, title_value, token, tokenize,
};
use crate::detect::DiagramType;
use crate::errors::Error;
use crate::js::number::number;

// The kinds of token, by index in PIE_TOKENS, as Pele's T.
const T_SHOW_DATA: usize = 0;
const T_PIE: usize = 1;
const T_COLON: usize = 2;
const T_NUMBER: usize = 3;
const T_ACC_DESCR: usize = 4;
const T_ACC_TITLE: usize = 5;
const T_TITLE: usize = 6;
const T_STRING: usize = 7;
const T_NEWLINE: usize = 8;

/// The end of the run of ASCII digits from byte from, as Pele's digits gives it.
fn digits(src: &str, from: usize) -> usize {
    let b = src.as_bytes();
    let mut i = from;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    i
}

/// Makes a matcher that answers as NUMBER_PIE's pattern,
/// /(?:-?[0-9]+\.[0-9]+(?!\.))|(?:-?(0|[1-9][0-9]*)(?!\.))/y, does, as Pele's numberScanner
/// does. The pattern reads a whole run of digits before it finds there is no fraction, and
/// would do so again from every digit of a long run of zeros, so the matcher remembers the
/// last run it read.
fn number_scanner() -> Scan {
    let (mut from, mut end, mut fraction) = (0, 0, None);
    Box::new(move |src: &str, p: usize, _| {
        let b = src.as_bytes();
        let at = if b[p] == b'-' { p + 1 } else { p };
        if at < from || at >= end {
            from = at;
            end = digits(src, at);
            fraction = None;
            if end > at && b.get(end) == Some(&b'.') {
                let stop = digits(src, end + 1);
                let length = stop - end - 1;
                if length > 0 && b.get(stop) != Some(&b'.') {
                    fraction = Some(stop);
                } else if length > 1 {
                    fraction = Some(stop - 1);
                }
            }
        }
        if end == at {
            return None;
        }
        if fraction.is_some() {
            return fraction;
        }
        if b[at] == b'0' {
            return (b.get(at + 1) != Some(&b'.')).then_some(at + 1);
        }
        if b.get(end) != Some(&b'.') {
            return Some(end);
        }
        (end - 1 > at).then_some(end - 1)
    })
}

/// Pele's PIE_TOKENS.
pub(crate) const PIE_TOKENS: &[TokenType] = &[
    keyword("showData"),
    keyword("pie"),
    token(":", Pattern::Literal(":")),
    TokenType {
        first: Some("-0123456789"),
        ..token("NUMBER_PIE", Pattern::Scanner(number_scanner))
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

/// PIE_TOKENS made ready to tokenize with.
pub(crate) fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| Table::new(PIE_TOKENS))
}

/// The tree of a pie chart, Pele's PieAst, its sections a label with its value. What Pele
/// leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PieAst {
    pub(crate) show_data: bool,
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) sections: Vec<(String, f64)>,
}

/// Parses a pie chart's text into its tree, as Pele's parsePie does.
pub(crate) fn parse_pie(src: &str) -> Result<PieAst, Error> {
    let tokens = tokenize(src, table(), DiagramType::Pie)?;
    let mut r = Reader::new(tokens, table(), DiagramType::Pie);
    let mut ast = PieAst::default();
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
    r.expect(T_PIE)?;
    ast.show_data = r.accept(T_SHOW_DATA);
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
            T_STRING => {
                let label = string_value(r.take());
                r.expect(T_COLON)?;
                let value = number(r.expect(T_NUMBER)?);
                ast.sections.push((label, value));
                end_of_line(&mut r)?;
            }
            _ => return Err(r.fail("a section, a title, or a line break")),
        }
    }
    Ok(ast)
}
