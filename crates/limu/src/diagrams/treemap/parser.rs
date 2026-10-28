//! The parser of treemaps, as Pele's parser.ts reads them.

use std::sync::OnceLock;

use crate::common::tokens::{
    ACC_DESCR, ACC_TITLE, END, Pattern, Reader, TITLE, Table, TokenType, acc_descr_value,
    acc_title_value, match_title, title_value, token, tokenize,
};
use crate::detect::DiagramType;
use crate::errors::Error;
use crate::js::chars::{is_word, space_run, unit};
use crate::js::number::parse_float;

// The kinds of token, by index in TREEMAP_TOKENS, as Pele's T.
const T_ACC_DESCR: usize = 0;
const T_ACC_TITLE: usize = 1;
const T_TITLE: usize = 2;
const T_KEYWORD: usize = 3;
const T_CLASS_DEF: usize = 4;
const T_STYLE_SEPARATOR: usize = 5;
const T_SEPARATOR: usize = 6;
const T_COMMA: usize = 7;
const T_INDENTATION: usize = 8;
const T_ID: usize = 12;
const T_NUMBER: usize = 13;
const T_STRING: usize = 14;

/// Matches /treemap-beta|treemap/y, which needs nothing after the word.
fn keyword(s: &str, at: usize) -> Option<usize> {
    ["treemap-beta", "treemap"]
        .iter()
        .find(|w| s[at..].starts_with(*w))
        .map(|w| at + w.len())
}

/// Matches /[ \t]+/y.
fn blank_run(s: &str, at: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut end = at;
    while end < b.len() && (b[end] == b' ' || b[end] == b'\t') {
        end += 1;
    }
    (end > at).then_some(end)
}

fn id_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

/// The end of the run of [a-zA-Z0-9_] from byte i.
fn id_rest(s: &str, mut i: usize) -> usize {
    let b = s.as_bytes();
    while i < b.len() && (id_start(b[i]) || b[i].is_ascii_digit()) {
        i += 1;
    }
    i
}

/// A match of CLASS_DEF, /classDef\s+([a-zA-Z_][a-zA-Z0-9_]+)(?:\s+([^;\r\n]*))?(?:;)?/y: its
/// end, its class's name, and the text of its styles where that group took part.
struct ClassDef<'a> {
    end: usize,
    name: &'a str,
    styles: Option<&'a str>,
}

/// Matches CLASS_DEF at byte at. The white space before the styles is greedy and may cross
/// lines, and the styles then run to the end of a line or a semicolon. No path that gives
/// back characters matches where the first does not, so the first is the match.
fn match_class_def(s: &str, at: usize) -> Option<ClassDef<'_>> {
    if !s[at..].starts_with("classDef") {
        return None;
    }
    let b = s.as_bytes();
    let i = at + "classDef".len();
    let j = space_run(s, i);
    if j == i || !b.get(j).is_some_and(|&c| id_start(c)) {
        return None;
    }
    let k = id_rest(s, j + 1);
    if k == j + 1 {
        return None;
    }
    let (name, mut end, mut styles) = (&s[j..k], k, None);
    let m = space_run(s, k);
    if m > k {
        let mut e = m;
        while e < b.len() && !matches!(b[e], b';' | b'\r' | b'\n') {
            e += 1;
        }
        (styles, end) = (Some(&s[m..e]), e);
    }
    if b.get(end) == Some(&b';') {
        end += 1;
    }
    Some(ClassDef { end, name, styles })
}

/// Pele's TREEMAP_TOKENS, Mermaid's treemap grammar's own terminals. Every run of spaces or
/// tabs is an INDENTATION token, since that comes before WS, and line breaks are hidden, so
/// rows are told apart by their tokens alone and the indentation of a row is whatever white
/// space comes before it. Pele gives most of these types no first characters. Those given
/// here are only the characters their patterns can start with, which changes nothing but how
/// many are tried.
pub(crate) const TREEMAP_TOKENS: &[TokenType] = &[
    ACC_DESCR,
    ACC_TITLE,
    // Pele's titleBefore(String.raw`\w`): the word alone is no title where a word character
    // follows, so that a class name such as title_2 is read whole.
    TokenType {
        pattern: Pattern::Match(|s, at| match_title(s, at, Some(|s, i| is_word(unit(s, i))))),
        ..TITLE
    },
    TokenType {
        first: Some("t"),
        ..token("TREEMAP_KEYWORD", Pattern::Match(keyword))
    },
    TokenType {
        first: Some("c"),
        ..token(
            "CLASS_DEF",
            Pattern::Match(|s, at| match_class_def(s, at).map(|m| m.end)),
        )
    },
    token("STYLE_SEPARATOR", Pattern::Literal(":::")),
    token("SEPARATOR", Pattern::Literal(":")),
    token("COMMA", Pattern::Literal(",")),
    TokenType {
        first: Some(" \t"),
        ..token("INDENTATION", Pattern::Match(blank_run))
    },
    TokenType {
        first: Some(" \t"),
        hidden: true,
        ..token("WS", Pattern::Match(blank_run))
    },
    TokenType {
        first: Some("%"),
        hidden: true,
        ..token(
            "ML_COMMENT",
            Pattern::Match(|s, at| {
                if !s[at..].starts_with("%%") {
                    return None;
                }
                Some(s[at + 2..].find('\n').map_or(s.len(), |e| at + 2 + e))
            }),
        )
    },
    TokenType {
        first: Some("\r\n"),
        hidden: true,
        ..token(
            "NL",
            Pattern::Match(|s, at| {
                if s[at..].starts_with("\r\n") {
                    Some(at + 2)
                } else {
                    s[at..].starts_with('\n').then_some(at + 1)
                }
            }),
        )
    },
    TokenType {
        first: Some("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_"),
        ..token(
            "ID2",
            Pattern::Match(|s, at| id_start(s.as_bytes()[at]).then(|| id_rest(s, at + 1))),
        )
    },
    TokenType {
        first: Some("0123456789_.,"),
        ..token(
            "NUMBER2",
            Pattern::Match(|s, at| {
                let b = s.as_bytes();
                let mut end = at;
                while end < b.len()
                    && (b[end].is_ascii_digit() || matches!(b[end], b'_' | b'.' | b','))
                {
                    end += 1;
                }
                (end > at).then_some(end)
            }),
        )
    },
    TokenType {
        first: Some("\"'"),
        ..token(
            "STRING2",
            Pattern::Match(|s, at| {
                let q = s.as_bytes()[at];
                if q != b'"' && q != b'\'' {
                    return None;
                }
                s[at + 1..].find(char::from(q)).map(|end| at + 1 + end + 1)
            }),
        )
    },
];

/// TREEMAP_TOKENS made ready to tokenize with.
pub(crate) fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| Table::new(TREEMAP_TOKENS))
}

/// A section or a leaf of the tree, Pele's SectionAst and LeafAst: a leaf has a value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemAst {
    pub(crate) name: String,
    pub(crate) value: Option<f64>,
    pub(crate) class_selector: Option<String>,
}

/// A row of the tree, Pele's TreemapRowAst or ClassDefAst. An indent Pele leaves undefined is
/// none.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RowAst {
    Row {
        indent: Option<usize>,
        item: ItemAst,
    },
    ClassDef {
        indent: Option<usize>,
        class_name: String,
        style_text: Option<String>,
    },
}

/// The tree of a treemap, Pele's TreemapAst. What Pele leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TreemapAst {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) rows: Vec<RowAst>,
}

/// Parses a treemap's text into its tree, as Pele's parseTreemap does.
pub(crate) fn parse_treemap(src: &str) -> Result<TreemapAst, Error> {
    let tokens = tokenize(src, table(), DiagramType::Treemap)?;
    let mut r = Reader::new(tokens, table(), DiagramType::Treemap);
    let mut ast = TreemapAst::default();

    r.expect(T_KEYWORD)?;
    while r.kind() != END {
        let kind = r.kind();
        match kind {
            T_ACC_DESCR => {
                ast.acc_descr = Some(acc_descr_value(r.take()));
                continue;
            }
            T_ACC_TITLE => {
                ast.acc_title = Some(acc_title_value(r.take()));
                continue;
            }
            T_TITLE => {
                ast.title = Some(title_value(r.take()));
                continue;
            }
            T_INDENTATION | T_STRING | T_CLASS_DEF => {}
            _ => return Err(r.fail("a node, a class definition, or a title")),
        }

        // A run of tabs and spaces is as many code units long as it is bytes.
        let indent = (kind == T_INDENTATION).then(|| r.take().len());
        if r.kind() == T_CLASS_DEF {
            let m = match_class_def(r.take(), 0).expect("a class definition");
            ast.rows.push(RowAst::ClassDef {
                indent,
                class_name: m.name.to_string(),
                style_text: m.styles.filter(|s| !s.is_empty()).map(str::to_string),
            });
            continue;
        }

        let quoted = r.expect(T_STRING)?;
        let name = quoted[1..quoted.len() - 1].to_string();
        // A leaf is a name followed by a colon or comma, with or without white space before
        // it.
        let kinds = &r.tokens.kinds;
        let after = if kinds[r.i] == T_INDENTATION {
            r.i + 1
        } else {
            r.i
        };
        let value = if kinds[after] == T_SEPARATOR || kinds[after] == T_COMMA {
            r.i = after + 1;
            r.accept(T_INDENTATION);
            Some(parse_float(&r.expect(T_NUMBER)?.replace(',', "")))
        } else {
            None
        };
        let class_selector = if r.accept(T_STYLE_SEPARATOR) {
            Some(r.expect(T_ID)?.to_string())
        } else {
            None
        };
        ast.rows.push(RowAst::Row {
            indent,
            item: ItemAst {
                name,
                value,
                class_selector,
            },
        });
    }
    Ok(ast)
}
