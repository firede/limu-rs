//! The parser of architecture diagrams, as Pele's parser.ts reads them.

use std::sync::OnceLock;

use crate::common::tokens::{
    ACC_DESCR, ACC_TITLE, DIRECTIVE, END, NEWLINE, Pattern, Reader, SINGLE_LINE_COMMENT, STRING,
    Table, TokenType, WHITESPACE, YAML, acc_descr_value, acc_title_value, match_title,
    string_value, title_value, token, tokenize,
};
use crate::detect::DiagramType;
use crate::errors::Error;
use crate::js::chars::{is_line_end, is_word, trim, unit_at};

// The kinds of token, by index in ARCHITECTURE_TOKENS, as Pele's T.
pub(crate) const T_HEADER: usize = 0;
pub(crate) const T_JUNCTION: usize = 1;
pub(crate) const T_SERVICE: usize = 2;
pub(crate) const T_COLUMN: usize = 3;
pub(crate) const T_GROUP: usize = 4;
pub(crate) const T_ALIGN: usize = 5;
pub(crate) const T_ROW: usize = 6;
pub(crate) const T_DASHES: usize = 7;
pub(crate) const T_IN: usize = 8;
pub(crate) const T_COLON: usize = 9;
pub(crate) const T_DASH: usize = 10;
pub(crate) const T_DIRECTION: usize = 11;
pub(crate) const T_ARROW_GROUP: usize = 12;
pub(crate) const T_INTO: usize = 13;
pub(crate) const T_ACC_DESCR: usize = 14;
pub(crate) const T_ACC_TITLE: usize = 15;
pub(crate) const T_TITLE: usize = 16;
pub(crate) const T_STRING: usize = 17;
pub(crate) const T_ID: usize = 18;
pub(crate) const T_NEWLINE: usize = 19;
pub(crate) const T_YAML: usize = 21;
pub(crate) const T_ICON: usize = 24;
pub(crate) const T_LABEL: usize = 25;

/// The ASCII characters `\w` takes, which an ID starts with.
const WORD: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ_abcdefghijklmnopqrstuvwxyz";

/// Whether `\w` takes the byte. A byte outside ASCII is not one, as `\w` takes only ASCII.
fn is_word_byte(b: u8) -> bool {
    is_word(i32::from(b))
}

/// Pele's ID, /[\w]([-\w]*\w)?/y: a run of word characters and dashes that starts with a word
/// character, given back to its last word character.
fn id(s: &str, at: usize) -> Option<usize> {
    let b = s.as_bytes();
    if !is_word_byte(b[at]) {
        return None;
    }
    let mut end = at + 1;
    let mut i = at + 1;
    while i < b.len() && (b[i] == b'-' || is_word_byte(b[i])) {
        if b[i] != b'-' {
            end = i + 1;
        }
        i += 1;
    }
    Some(end)
}

/// Whether /[-\w]*\w/ matches at byte at, which is where the run of word characters and
/// dashes there holds a word character. It is the rest of an ID that titleBefore gives way to
/// in Pele's ARCHITECTURE_TOKENS, so that `titles` is an ID.
fn id_rest(s: &str, at: usize) -> bool {
    let b = s.as_bytes();
    let mut i = at;
    while i < b.len() && (b[i] == b'-' || is_word_byte(b[i])) {
        if b[i] != b'-' {
            return true;
        }
        i += 1;
    }
    false
}

/// Pele's titleBefore(String.raw`[-\w]*\w`).
fn title(s: &str, at: usize) -> Option<usize> {
    match_title(s, at, Some(id_rest))
}

/// Pele's ARCH_ICON, /\([\w-:]+\)/y, in which the class holds word characters, dashes and
/// colons.
fn icon(s: &str, at: usize) -> Option<usize> {
    let b = s.as_bytes();
    if b[at] != b'(' {
        return None;
    }
    let mut i = at + 1;
    while i < b.len() && (is_word_byte(b[i]) || b[i] == b'-' || b[i] == b':') {
        i += 1;
    }
    if i == at + 1 || b.get(i) != Some(&b')') {
        return None;
    }
    Some(i + 1)
}

/// Pele's ARCH_TITLE, /\[(?:"([^"\\]|\\.)*"|'([^'\\]|\\.)*'|[^\[\]\r\n]+)\]/y. A quoted string
/// ends at its first quote that no backslash takes, and must be followed by the bracket.
/// Where it is not, no shorter run of its characters can be, and the text is tried as the
/// third alternative, a run of characters other than brackets and line breaks. A backslash
/// before a line terminator, which . does not take, ends no string.
fn label(s: &str, at: usize) -> Option<usize> {
    let b = s.as_bytes();
    if b[at] != b'[' {
        return None;
    }
    let i = at + 1;
    if let Some(&q) = b.get(i).filter(|&&c| c == b'"' || c == b'\'') {
        let mut j = i + 1;
        while j < b.len() {
            if b[j] == q {
                if b.get(j + 1) == Some(&b']') {
                    return Some(j + 2);
                }
                break;
            }
            if b[j] == b'\\' {
                let (c, size) = unit_at(s, j + 1);
                if c < 0 || is_line_end(c) {
                    break;
                }
                j += 1 + size;
            } else {
                j += 1;
            }
        }
    }
    let mut j = i;
    while j < b.len() && !matches!(b[j], b'[' | b']' | b'\r' | b'\n') {
        j += 1;
    }
    (j > i && b.get(j) == Some(&b']')).then_some(j + 1)
}

/// Pele's ARROW_DIRECTION, /L|R|T|B/y.
fn direction(s: &str, at: usize) -> Option<usize> {
    matches!(s.as_bytes()[at], b'L' | b'R' | b'T' | b'B').then_some(at + 1)
}

/// Pele's ARROW_INTO, /<|>/y.
fn into(s: &str, at: usize) -> Option<usize> {
    matches!(s.as_bytes()[at], b'<' | b'>').then_some(at + 1)
}

/// Pele's word: the text, which an identifier that goes on longer takes over.
const fn word(text: &'static str) -> TokenType {
    TokenType {
        longer: &[T_ID],
        ..token(text, Pattern::Literal(text))
    }
}

/// Pele's ARCHITECTURE_TOKENS. The direction letters come before identifiers and have no
/// longer alternative, so Mermaid reads an id that starts with L, R, T or B as a direction
/// followed by the rest.
pub(crate) const ARCHITECTURE_TOKENS: &[TokenType] = &[
    word("architecture-beta"),
    word("junction"),
    word("service"),
    word("column"),
    word("group"),
    word("align"),
    word("row"),
    TokenType {
        longer: &[T_YAML],
        ..token("--", Pattern::Literal("--"))
    },
    word("in"),
    token(":", Pattern::Literal(":")),
    TokenType {
        longer: &[T_YAML],
        ..token("-", Pattern::Literal("-"))
    },
    TokenType {
        first: Some("LRTB"),
        ..token("ARROW_DIRECTION", Pattern::Match(direction))
    },
    token("ARROW_GROUP", Pattern::Literal("{group}")),
    TokenType {
        first: Some("<>"),
        ..token("ARROW_INTO", Pattern::Match(into))
    },
    ACC_DESCR,
    ACC_TITLE,
    TokenType {
        first: Some("\t t"),
        ..token("TITLE", Pattern::Match(title))
    },
    STRING,
    TokenType {
        first: Some(WORD),
        ..token("ID", Pattern::Match(id))
    },
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
    TokenType {
        first: Some("("),
        ..token("ARCH_ICON", Pattern::Match(icon))
    },
    TokenType {
        first: Some("["),
        ..token("ARCH_TITLE", Pattern::Match(label))
    },
];

/// ARCHITECTURE_TOKENS made ready to tokenize with.
pub(crate) fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| Table::new(ARCHITECTURE_TOKENS))
}

/// A group, Pele's GroupAst. What Pele leaves undefined is none. parent is Pele's in, the
/// group it is in.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct GroupAst {
    pub(crate) id: String,
    pub(crate) icon: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) parent: Option<String>,
}

/// A service, Pele's ServiceAst.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ServiceAst {
    pub(crate) id: String,
    pub(crate) icon: Option<String>,
    pub(crate) icon_text: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) parent: Option<String>,
}

/// A junction, Pele's JunctionAst.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct JunctionAst {
    pub(crate) id: String,
    pub(crate) parent: Option<String>,
}

/// An edge, Pele's EdgeAst. A side is 'L', 'R', 'T' or 'B'.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct EdgeAst {
    pub(crate) lhs_id: String,
    pub(crate) lhs_group: bool,
    pub(crate) lhs_dir: char,
    pub(crate) lhs_into: bool,
    pub(crate) title: Option<String>,
    pub(crate) rhs_into: bool,
    pub(crate) rhs_dir: char,
    pub(crate) rhs_id: String,
    pub(crate) rhs_group: bool,
}

/// An align directive, Pele's AlignmentAst: "row" or "column", and its members.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct AlignmentAst {
    pub(crate) direction: String,
    pub(crate) members: Vec<String>,
}

/// The tree of an architecture diagram, Pele's ArchitectureAst.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ArchitectureAst {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) groups: Vec<GroupAst>,
    pub(crate) services: Vec<ServiceAst>,
    pub(crate) junctions: Vec<JunctionAst>,
    pub(crate) edges: Vec<EdgeAst>,
    pub(crate) alignments: Vec<AlignmentAst>,
}

/// The text of an ARCH_TITLE token, as Mermaid's ArchitectureValueConverter and Pele's
/// labelValue read it: the brackets taken off and the rest trimmed, and a string in quotes
/// unquoted, with its escaped quotes made quotes.
fn label_value(text: &str) -> String {
    let mut result = trim(&text[1..text.len() - 1]).to_string();
    if (result.starts_with('"') && result.ends_with('"'))
        || (result.starts_with('\'') && result.ends_with('\''))
    {
        result = if result.len() < 2 {
            String::new()
        } else {
            result[1..result.len() - 1].to_string()
        };
        result = result.replace("\\\"", "\"").replace("\\'", "'");
    }
    trim(&result).to_string()
}

/// Parses an architecture diagram's text into its tree, as Pele's parseArchitecture does.
pub(crate) fn parse_architecture(src: &str) -> Result<ArchitectureAst, Error> {
    let tokens = tokenize(src, table(), DiagramType::Architecture)?;
    let mut r = Reader::new(tokens, table(), DiagramType::Architecture);
    let mut ast = ArchitectureAst::default();
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
    let icon = |r: &mut Reader| {
        let s = r.take();
        s[1..s.len() - 1].to_string()
    };

    while r.kind() == T_NEWLINE {
        r.i += 1;
    }
    r.expect(T_HEADER)?;
    while r.kind() != END {
        match r.kind() {
            T_NEWLINE => {
                r.i += 1;
                continue;
            }
            T_ACC_DESCR => ast.acc_descr = Some(acc_descr_value(r.take())),
            T_ACC_TITLE => ast.acc_title = Some(acc_title_value(r.take())),
            T_TITLE => ast.title = Some(title_value(r.take())),
            T_GROUP => {
                r.i += 1;
                let mut group = GroupAst {
                    id: r.expect(T_ID)?.to_string(),
                    ..GroupAst::default()
                };
                if r.kind() == T_ICON {
                    group.icon = Some(icon(&mut r));
                }
                if r.kind() == T_LABEL {
                    group.title = Some(label_value(r.take()));
                }
                if r.accept(T_IN) {
                    group.parent = Some(r.expect(T_ID)?.to_string());
                }
                ast.groups.push(group);
            }
            T_SERVICE => {
                r.i += 1;
                let mut service = ServiceAst {
                    id: r.expect(T_ID)?.to_string(),
                    ..ServiceAst::default()
                };
                if r.kind() == T_STRING {
                    service.icon_text = Some(string_value(r.take()));
                } else if r.kind() == T_ICON {
                    service.icon = Some(icon(&mut r));
                }
                if r.kind() == T_LABEL {
                    service.title = Some(label_value(r.take()));
                }
                if r.accept(T_IN) {
                    service.parent = Some(r.expect(T_ID)?.to_string());
                }
                ast.services.push(service);
            }
            T_JUNCTION => {
                r.i += 1;
                let mut junction = JunctionAst {
                    id: r.expect(T_ID)?.to_string(),
                    parent: None,
                };
                if r.accept(T_IN) {
                    junction.parent = Some(r.expect(T_ID)?.to_string());
                }
                ast.junctions.push(junction);
            }
            T_ID => {
                let lhs_id = r.take().to_string();
                let lhs_group = r.accept(T_ARROW_GROUP);
                r.expect(T_COLON)?;
                let lhs_dir = side(r.expect(T_DIRECTION)?);
                let lhs_into = r.accept(T_INTO);
                let mut title = None;
                if !r.accept(T_DASHES) {
                    if r.kind() != T_DASH {
                        return Err(r.fail("'--' or '-'"));
                    }
                    r.i += 1;
                    title = Some(label_value(r.expect(T_LABEL)?));
                    r.expect(T_DASH)?;
                }
                let rhs_into = r.accept(T_INTO);
                let rhs_dir = side(r.expect(T_DIRECTION)?);
                r.expect(T_COLON)?;
                let rhs_id = r.expect(T_ID)?.to_string();
                let rhs_group = r.accept(T_ARROW_GROUP);
                ast.edges.push(EdgeAst {
                    lhs_id,
                    lhs_group,
                    lhs_dir,
                    lhs_into,
                    title,
                    rhs_into,
                    rhs_dir,
                    rhs_id,
                    rhs_group,
                });
            }
            T_ALIGN => {
                r.i += 1;
                if r.kind() != T_ROW && r.kind() != T_COLUMN {
                    return Err(r.fail("'row' or 'column'"));
                }
                let direction = r.take().to_string();
                let mut members = vec![r.expect(T_ID)?.to_string(), r.expect(T_ID)?.to_string()];
                while r.kind() == T_ID {
                    members.push(r.take().to_string());
                }
                ast.alignments.push(AlignmentAst { direction, members });
            }
            _ => {
                return Err(r.fail(
                    "a group, a service, a junction, an edge, an alignment, a title, or a line \
                     break",
                ));
            }
        }
        end_of_line(&mut r)?;
    }
    Ok(ast)
}

/// The side an ARROW_DIRECTION token names.
fn side(text: &str) -> char {
    char::from(text.as_bytes()[0])
}
