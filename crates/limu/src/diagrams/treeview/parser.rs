//! The parser of tree views, as Pele's parser.ts reads them.

use std::sync::OnceLock;

use crate::common::tokens::{
    ACC_TITLE, END, Pattern, Reader, Table, TokenType, acc_descr_named, acc_descr_value,
    acc_title_value, keyword, line_break, match_title, title_value, token, tokenize,
};
use crate::detect::DiagramType;
use crate::errors::Error;
use crate::js::chars::{is_letter, is_word, trim};

// The kinds of token, by index in TREEVIEW_TOKENS, as Pele's T.
pub(crate) const T_KEYWORD: usize = 0;
pub(crate) const T_ACC_DESCR: usize = 1;
pub(crate) const T_ACC_TITLE: usize = 2;
pub(crate) const T_TITLE: usize = 3;
pub(crate) const T_CLASS_ANNOTATION: usize = 4;
pub(crate) const T_ICON_ANNOTATION: usize = 5;
pub(crate) const T_DESC_ANNOTATION: usize = 6;
pub(crate) const T_INDENTATION: usize = 7;
pub(crate) const T_QUOTED_NAME: usize = 8;
pub(crate) const T_BARE_NAME: usize = 12;

/// Whether byte i of s is a space or a tab, and not past its end, as Pele's isBlank of
/// charCodeAt.
fn is_blank(s: &str, i: usize) -> bool {
    matches!(s.as_bytes().get(i), Some(b' ' | b'\t'))
}

/// The end of the run of spaces and tabs from byte i.
fn blanks(s: &str, mut i: usize) -> usize {
    while is_blank(s, i) {
        i += 1;
    }
    i
}

/// Whether an annotation starts at byte at, as Pele's annotationAt finds: a description, an
/// icon, or a class whose name starts as one may.
fn annotation_at(src: &str, at: usize) -> bool {
    let rest = &src[at.min(src.len())..];
    if rest.starts_with("##") || rest.starts_with("icon(") {
        return true;
    }
    if !rest.starts_with(":::") {
        return false;
    }
    let i = blanks(src, at + 3);
    src.as_bytes()
        .get(i)
        .is_some_and(|&c| is_letter(i32::from(c)) || c == b'_')
}

/// Pele's matchBareName, which matches as its pattern
/// /(?!:::|icon\(|##)[^ \t\n\r"'](?:(?![ \t]+:::[ \t]*[A-Za-z_]|[ \t]+icon\(|[ \t]+##)[^\n\r])*/y
/// does. A name runs to the end of its line or to the blanks before an annotation. The
/// pattern looks ahead from every blank for an annotation, which rescans long runs of
/// blanks, and this looks once for each run.
fn match_bare_name(src: &str, at: usize) -> Option<usize> {
    let b = src.as_bytes();
    if is_blank(src, at) || matches!(b[at], b'\n' | b'\r' | b'"' | b'\'') {
        return None;
    }
    let rest = &src[at..];
    if rest.starts_with(":::") || rest.starts_with("icon(") || rest.starts_with("##") {
        return None;
    }
    let mut i = at + 1;
    while i < b.len() {
        if b[i] == b'\n' || b[i] == b'\r' {
            break;
        }
        if is_blank(src, i) {
            let j = blanks(src, i + 1);
            if annotation_at(src, j) {
                break;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    Some(i)
}

/// [ \t]+ at byte at, and then what more matches from the end of the blanks.
fn after_blanks(s: &str, at: usize, more: impl FnOnce(usize) -> Option<usize>) -> Option<usize> {
    let i = blanks(s, at);
    if i == at { None } else { more(i) }
}

/// The end of the run of [\w-] from byte i.
fn word_run(s: &str, mut i: usize) -> usize {
    let b = s.as_bytes();
    while i < b.len() && (is_word(i32::from(b[i])) || b[i] == b'-') {
        i += 1;
    }
    i
}

/// Pele's CLASS_ANNOTATION, /[ \t]+:::[ \t]*[A-Za-z_][\w-]*/y.
fn class_annotation(s: &str, at: usize) -> Option<usize> {
    after_blanks(s, at, |i| {
        if !s[i..].starts_with(":::") {
            return None;
        }
        let i = blanks(s, i + 3);
        let c = *s.as_bytes().get(i)?;
        (is_letter(i32::from(c)) || c == b'_').then(|| word_run(s, i + 1))
    })
}

/// Pele's ICON_ANNOTATION, /[ \t]+icon\([\w-]*(?::[\w-]+)?\)/y.
fn icon_annotation(s: &str, at: usize) -> Option<usize> {
    after_blanks(s, at, |i| {
        if !s[i..].starts_with("icon(") {
            return None;
        }
        let mut i = word_run(s, i + "icon(".len());
        if s.as_bytes().get(i) == Some(&b':') {
            let j = word_run(s, i + 1);
            if j > i + 1 {
                i = j;
            }
        }
        (s.as_bytes().get(i) == Some(&b')')).then_some(i + 1)
    })
}

/// Pele's DESC_ANNOTATION, /[ \t]+##[^\n\r]*/y.
fn desc_annotation(s: &str, at: usize) -> Option<usize> {
    after_blanks(s, at, |i| {
        if !s[i..].starts_with("##") {
            return None;
        }
        let b = s.as_bytes();
        let end = (i + 2..b.len())
            .find(|&k| b[k] == b'\n' || b[k] == b'\r')
            .unwrap_or(b.len());
        Some(end)
    })
}

/// Pele's INDENTATION, /[ \t]{1,}/y, and its WS, /[ \t]+/y, which INDENTATION always takes
/// first.
fn indentation(s: &str, at: usize) -> Option<usize> {
    after_blanks(s, at, Some)
}

/// Pele's QUOTED_NAME, /"[^"]*"|'[^']*'/y, which may run over lines.
fn quoted_name(s: &str, at: usize) -> Option<usize> {
    let q = s.as_bytes()[at];
    if q != b'"' && q != b'\'' {
        return None;
    }
    let end = s[at + 1..].find(char::from(q))?;
    Some(at + 1 + end + 1)
}

/// Pele's ML_COMMENT, /\%\%[^\n]*/y.
fn comment(s: &str, at: usize) -> Option<usize> {
    if !s[at..].starts_with("%%") {
        return None;
    }
    Some(s[at..].find('\n').map_or(s.len(), |end| at + end))
}

/// Whether /(?!%%)[^\n\r]/ matches at byte i, which is where a character other than a line
/// break follows that does not start a comment. The title gives way to it in Pele's
/// TREEVIEW_TOKENS, through titleBefore, so that `titles` is a name.
fn name_rest(s: &str, i: usize) -> bool {
    s.as_bytes()
        .get(i)
        .is_some_and(|&c| c != b'\n' && c != b'\r' && !s[i..].starts_with("%%"))
}

/// Pele's titleBefore(String.raw`(?!%%)[^\n\r]`).
fn title(s: &str, at: usize) -> Option<usize> {
    match_title(s, at, Some(name_rest))
}

/// Pele's TREEVIEW_TOKENS.
pub(crate) const TREEVIEW_TOKENS: &[TokenType] = &[
    TokenType {
        longer: &[T_BARE_NAME],
        ..keyword("treeView-beta")
    },
    acc_descr_named("ACC_DESCR"),
    ACC_TITLE,
    TokenType {
        first: Some("\t t"),
        ..token("TITLE", Pattern::Match(title))
    },
    TokenType {
        first: Some("\t "),
        ..token("CLASS_ANNOTATION", Pattern::Match(class_annotation))
    },
    TokenType {
        first: Some("\t "),
        ..token("ICON_ANNOTATION", Pattern::Match(icon_annotation))
    },
    TokenType {
        first: Some("\t "),
        ..token("DESC_ANNOTATION", Pattern::Match(desc_annotation))
    },
    TokenType {
        first: Some("\t "),
        ..token("INDENTATION", Pattern::Match(indentation))
    },
    TokenType {
        first: Some("\"'"),
        ..token("QUOTED_NAME", Pattern::Match(quoted_name))
    },
    TokenType {
        first: Some("\t "),
        hidden: true,
        ..token("WS", Pattern::Match(indentation))
    },
    TokenType {
        first: Some("%"),
        hidden: true,
        ..token("ML_COMMENT", Pattern::Match(comment))
    },
    TokenType {
        first: Some("\r\n"),
        hidden: true,
        ..token("NL", Pattern::Match(line_break))
    },
    token("BARE_NAME", Pattern::Match(match_bare_name)),
];

/// TREEVIEW_TOKENS made ready to tokenize with.
pub(crate) fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| Table::new(TREEVIEW_TOKENS))
}

/// An entry as written, Pele's TreeNodeAst. What Pele leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TreeNodeAst {
    pub(crate) indent: Option<usize>,
    pub(crate) name: String,
    pub(crate) class_annotation: Option<String>,
    pub(crate) icon_annotation: Option<String>,
    pub(crate) desc_annotation: Option<String>,
}

/// The tree of a tree view, Pele's TreeViewAst.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TreeViewAst {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) nodes: Vec<TreeNodeAst>,
}

/// The text with the spaces and tabs at its end taken off, as Pele's trimBlanksEnd.
fn trim_blanks_end(text: &str) -> &str {
    text.trim_end_matches([' ', '\t'])
}

/// Mermaid's TreeViewValueConverter for the annotations, by terminal name, as Pele's
/// convertValue converts them. The parser converts the names and the indentation itself.
fn convert_value(rule: usize, input: &str) -> String {
    match rule {
        T_CLASS_ANNOTATION => trim(&trim(input)[3..]).to_string(),
        T_ICON_ANNOTATION => {
            let trimmed = trim(input);
            trimmed[5..trimmed.len() - 1].to_string()
        }
        T_DESC_ANNOTATION => trim(&trim(input)[2..]).to_string(),
        _ => unreachable!("no annotation"),
    }
}

/// Parses a tree view's text into its tree, as Pele's parseTreeView does.
pub(crate) fn parse_tree_view(src: &str) -> Result<TreeViewAst, Error> {
    let tokens = tokenize(src, table(), DiagramType::TreeView)?;
    let mut r = Reader::new(tokens, table(), DiagramType::TreeView);
    let mut ast = TreeViewAst::default();
    r.expect(T_KEYWORD)?;
    loop {
        match r.kind() {
            T_ACC_DESCR => ast.acc_descr = Some(acc_descr_value(r.take())),
            T_ACC_TITLE => ast.acc_title = Some(acc_title_value(r.take())),
            T_TITLE => ast.title = Some(title_value(r.take())),
            _ => break,
        }
    }
    while r.kind() != END {
        let mut node = TreeNodeAst::default();
        if r.kind() == T_INDENTATION {
            node.indent = Some(r.take().len());
        }
        match r.kind() {
            T_QUOTED_NAME => {
                let text = r.take();
                node.name = text[1..text.len() - 1].to_string();
            }
            T_BARE_NAME => node.name = trim_blanks_end(r.take()).to_string(),
            _ => return Err(r.fail("a name")),
        }
        loop {
            let kind = r.kind();
            let annotation = match kind {
                T_CLASS_ANNOTATION => &mut node.class_annotation,
                T_ICON_ANNOTATION => &mut node.icon_annotation,
                T_DESC_ANNOTATION => &mut node.desc_annotation,
                _ => break,
            };
            *annotation = Some(convert_value(kind, r.take()));
        }
        ast.nodes.push(node);
    }
    Ok(ast)
}
