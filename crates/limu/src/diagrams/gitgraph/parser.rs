//! The parser of git graphs, as Pele's parser.ts reads them.

use std::sync::OnceLock;

use crate::common::tokens::{
    ACC_DESCR, ACC_TITLE, DIRECTIVE, END, NEWLINE, Pattern, Reader, SINGLE_LINE_COMMENT, STRING,
    Table, TokenType, WHITESPACE, YAML, acc_descr_value, acc_title_value, keyword, match_title,
    string_value, title_value, token, tokenize,
};
use crate::detect::DiagramType;
use crate::errors::Error;
use crate::js::chars::{is_word, space_run};
use crate::js::number::parse_int;

// The kinds of token, by index in GIT_TOKENS, as Pele's T.
pub(crate) const T_CHERRY_PICK: usize = 0;
pub(crate) const T_GIT_GRAPH_COLON: usize = 1;
pub(crate) const T_HIGHLIGHT: usize = 2;
pub(crate) const T_GIT_GRAPH: usize = 3;
pub(crate) const T_CHECKOUT: usize = 4;
pub(crate) const T_REVERSE: usize = 5;
pub(crate) const T_PARENT: usize = 6;
pub(crate) const T_COMMIT: usize = 7;
pub(crate) const T_NORMAL: usize = 8;
pub(crate) const T_BRANCH: usize = 9;
pub(crate) const T_ORDER: usize = 10;
pub(crate) const T_SWITCH: usize = 11;
pub(crate) const T_TYPE: usize = 12;
pub(crate) const T_MERGE: usize = 13;
pub(crate) const T_MSG: usize = 14;
pub(crate) const T_TAG: usize = 15;
pub(crate) const T_ID: usize = 16;
pub(crate) const T_LR: usize = 17;
pub(crate) const T_TB: usize = 18;
pub(crate) const T_BT: usize = 19;
pub(crate) const T_COLON: usize = 20;
pub(crate) const T_ACC_DESCR: usize = 21;
pub(crate) const T_ACC_TITLE: usize = 22;
pub(crate) const T_TITLE: usize = 23;
pub(crate) const T_INT: usize = 24;
pub(crate) const T_STRING: usize = 25;
pub(crate) const T_NEWLINE: usize = 26;
pub(crate) const T_REFERENCE: usize = 31;

/// The ASCII characters `\w` takes, which a reference starts with.
const WORD: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ_abcdefghijklmnopqrstuvwxyz";

/// Whether `\w` takes the byte. A byte outside ASCII is not one, as `\w` takes only ASCII.
fn is_word_byte(b: u8) -> bool {
    is_word(i32::from(b))
}

/// Pele's REFERENCE, /\w([-\./\w]*[-\w])?/y: a word character, then a run of word characters,
/// dashes, dots and slashes given back to its last word character or dash.
fn reference(s: &str, at: usize) -> Option<usize> {
    let b = s.as_bytes();
    if !is_word_byte(b[at]) {
        return None;
    }
    let mut end = at + 1;
    let mut i = at + 1;
    while i < b.len() && (matches!(b[i], b'-' | b'.' | b'/') || is_word_byte(b[i])) {
        if b[i] != b'.' && b[i] != b'/' {
            end = i + 1;
        }
        i += 1;
    }
    Some(end)
}

/// Whether /[-./\w]*[-\w]/ matches at byte at, which is where the run of word characters,
/// dashes, dots and slashes there holds a word character or a dash. It is the rest of a
/// reference that titleBefore gives way to in Pele's GIT_TOKENS, so that `titles` is a
/// reference.
fn reference_rest(s: &str, at: usize) -> bool {
    let b = s.as_bytes();
    let mut i = at;
    while i < b.len() && (matches!(b[i], b'-' | b'.' | b'/') || is_word_byte(b[i])) {
        if b[i] != b'.' && b[i] != b'/' {
            return true;
        }
        i += 1;
    }
    false
}

/// Pele's titleBefore(String.raw`[-./\w]*[-\w]`).
fn title(s: &str, at: usize) -> Option<usize> {
    match_title(s, at, Some(reference_rest))
}

/// The opener of the braced form of Pele's ACC_DESCR here, /[\t ]*accDescr\s*{/y. Without a
/// closing brace the block form scans to the end of the text, and a brace is not a token on
/// its own, so the first failure is final.
fn acc_descr_opener(s: &str, at: usize) -> bool {
    let b = s.as_bytes();
    let mut i = at;
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    if !s[i..].starts_with("accDescr") {
        return false;
    }
    let i = space_run(s, i + "accDescr".len());
    b.get(i) == Some(&b'{')
}

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

/// Pele's word: the text, which a reference that goes on longer takes over, so that `commits`
/// is a branch.
const fn word(text: &'static str) -> TokenType {
    TokenType {
        longer: &[T_REFERENCE],
        ..token(text, Pattern::Literal(text))
    }
}

/// Pele's GIT_TOKENS.
pub(crate) const GIT_TOKENS: &[TokenType] = &[
    word("cherry-pick"),
    token("gitGraph:", Pattern::Literal("gitGraph:")),
    word("HIGHLIGHT"),
    TokenType {
        longer: &[T_REFERENCE],
        ..keyword("gitGraph")
    },
    word("checkout"),
    word("REVERSE"),
    token("parent:", Pattern::Literal("parent:")),
    word("commit"),
    word("NORMAL"),
    word("branch"),
    token("order:", Pattern::Literal("order:")),
    word("switch"),
    token("type:", Pattern::Literal("type:")),
    word("merge"),
    token("msg:", Pattern::Literal("msg:")),
    token("tag:", Pattern::Literal("tag:")),
    token("id:", Pattern::Literal("id:")),
    word("LR"),
    word("TB"),
    word("BT"),
    token(":", Pattern::Literal(":")),
    TokenType {
        opener: Some(acc_descr_opener),
        ..ACC_DESCR
    },
    ACC_TITLE,
    TokenType {
        first: Some("\t t"),
        ..token("TITLE", Pattern::Match(title))
    },
    TokenType {
        first: Some("0123456789"),
        ..token("INT", Pattern::Match(int))
    },
    STRING,
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
    TokenType {
        first: Some(WORD),
        ..token("REFERENCE", Pattern::Match(reference))
    },
];

/// GIT_TOKENS made ready to tokenize with.
pub(crate) fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| Table::new(GIT_TOKENS))
}

/// A statement of a git graph, Pele's StatementAst. What Pele leaves undefined is none. A
/// commit's type is its CommitTypeName: "NORMAL", "REVERSE" or "HIGHLIGHT".
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum StatementAst {
    /// Pele's CommitAst.
    Commit {
        id: Option<String>,
        message: Option<String>,
        tags: Vec<String>,
        kind: Option<&'static str>,
    },
    /// Pele's BranchAst.
    Branch { name: String, order: Option<f64> },
    /// Pele's MergeAst.
    Merge {
        branch: String,
        id: Option<String>,
        tags: Vec<String>,
        kind: Option<&'static str>,
    },
    /// Pele's CheckoutAst.
    Checkout { branch: String },
    /// Pele's CherryPickingAst.
    CherryPicking {
        id: Option<String>,
        tags: Vec<String>,
        parent: Option<String>,
    },
}

/// The tree of a git graph, Pele's GitGraphAst. Langium names the root after the Direction
/// rule when the header gives a direction, which is_direction says.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct GitGraphAst {
    pub(crate) is_direction: bool,
    pub(crate) dir: Option<&'static str>,
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) statements: Vec<StatementAst>,
}

/// The name of a commit type the token at the reader is, as Pele's commitType reads it.
fn commit_type(r: &mut Reader) -> Result<&'static str, Error> {
    let name = match r.kind() {
        T_NORMAL => "NORMAL",
        T_REVERSE => "REVERSE",
        T_HIGHLIGHT => "HIGHLIGHT",
        _ => return Err(r.fail("NORMAL, REVERSE, or HIGHLIGHT")),
    };
    r.i += 1;
    Ok(name)
}

/// A branch's name, as Pele's name reads it: a reference, or a string.
fn name(r: &mut Reader) -> Result<String, Error> {
    match r.kind() {
        T_REFERENCE => Ok(r.take().to_string()),
        T_STRING => Ok(string_value(r.take())),
        _ => Err(r.fail("a branch name")),
    }
}

/// A string's value, as Pele's string reads it.
fn string(r: &mut Reader) -> Result<String, Error> {
    Ok(string_value(r.expect(T_STRING)?))
}

/// The end of a line, as Pele's endOfLine reads it: the end of the text, or line breaks.
fn end_of_line(r: &mut Reader) -> Result<(), Error> {
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
}

/// Parses a git graph's text into its tree, as Pele's parseGit does.
pub(crate) fn parse_git(src: &str) -> Result<GitGraphAst, Error> {
    let tokens = tokenize(src, table(), DiagramType::GitGraph)?;
    let mut r = Reader::new(tokens, table(), DiagramType::GitGraph);
    let mut ast = GitGraphAst::default();

    while r.kind() == T_NEWLINE {
        r.i += 1;
    }
    if !r.accept(T_GIT_GRAPH_COLON) {
        r.expect(T_GIT_GRAPH)?;
        let dir = match r.kind() {
            T_LR => Some("LR"),
            T_TB => Some("TB"),
            T_BT => Some("BT"),
            _ => None,
        };
        if dir.is_some() && r.tokens.kinds[r.i + 1] == T_COLON {
            ast.is_direction = true;
            ast.dir = dir;
            r.i += 2;
        } else {
            r.accept(T_COLON);
        }
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
            T_COMMIT => {
                r.i += 1;
                let (mut id, mut message, mut tags, mut kind) = (None, None, Vec::new(), None);
                loop {
                    if r.accept(T_ID) {
                        id = Some(string(&mut r)?);
                    } else if r.accept(T_TAG) {
                        tags.push(string(&mut r)?);
                    } else if r.accept(T_TYPE) {
                        kind = Some(commit_type(&mut r)?);
                    } else if r.accept(T_MSG) || r.kind() == T_STRING {
                        message = Some(string(&mut r)?);
                    } else {
                        break;
                    }
                }
                end_of_line(&mut r)?;
                ast.statements.push(StatementAst::Commit {
                    id,
                    message,
                    tags,
                    kind,
                });
            }
            T_BRANCH => {
                r.i += 1;
                let name = name(&mut r)?;
                let mut order = None;
                if r.accept(T_ORDER) {
                    order = Some(parse_int(r.expect(T_INT)?, 10));
                }
                end_of_line(&mut r)?;
                ast.statements.push(StatementAst::Branch { name, order });
            }
            T_MERGE => {
                r.i += 1;
                let branch = name(&mut r)?;
                let (mut id, mut tags, mut kind) = (None, Vec::new(), None);
                loop {
                    if r.accept(T_ID) {
                        id = Some(string(&mut r)?);
                    } else if r.accept(T_TAG) {
                        tags.push(string(&mut r)?);
                    } else if r.accept(T_TYPE) {
                        kind = Some(commit_type(&mut r)?);
                    } else {
                        break;
                    }
                }
                end_of_line(&mut r)?;
                ast.statements.push(StatementAst::Merge {
                    branch,
                    id,
                    tags,
                    kind,
                });
            }
            T_CHECKOUT | T_SWITCH => {
                r.i += 1;
                let branch = name(&mut r)?;
                end_of_line(&mut r)?;
                ast.statements.push(StatementAst::Checkout { branch });
            }
            T_CHERRY_PICK => {
                r.i += 1;
                let (mut id, mut tags, mut parent) = (None, Vec::new(), None);
                loop {
                    if r.accept(T_ID) {
                        id = Some(string(&mut r)?);
                    } else if r.accept(T_TAG) {
                        tags.push(string(&mut r)?);
                    } else if r.accept(T_PARENT) {
                        parent = Some(string(&mut r)?);
                    } else {
                        break;
                    }
                }
                end_of_line(&mut r)?;
                ast.statements
                    .push(StatementAst::CherryPicking { id, tags, parent });
            }
            _ => return Err(r.fail("a statement, a title, or a line break")),
        }
    }
    Ok(ast)
}
