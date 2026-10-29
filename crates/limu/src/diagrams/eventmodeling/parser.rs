//! The parser of event modeling diagrams, as Pele's parser.ts reads them.

use std::sync::OnceLock;

use crate::common::tokens::{
    ACC_TITLE, DIRECTIVE, END, Pattern, Reader, SINGLE_LINE_COMMENT, Table, TokenType, YAML,
    acc_descr_named, closed, keyword, line_break, match_title, token, tokenize,
};
use crate::detect::DiagramType;
use crate::errors::Error;
use crate::js::chars::{is_letter, is_word, space_run};

// The kinds of token, by index in EVENTMODELING_TOKENS, as Pele's T.
pub(crate) const T_KEYWORD: usize = 0;
pub(crate) const T_ENTITY: usize = 6;
pub(crate) const T_GIVEN: usize = 10;
pub(crate) const T_DATA: usize = 15;
pub(crate) const T_NOTE: usize = 16;
pub(crate) const T_WHEN: usize = 17;
pub(crate) const T_THEN: usize = 18;
pub(crate) const T_ARROW: usize = 24;
pub(crate) const T_GWT: usize = 25;
pub(crate) const T_OPEN: usize = 29;
pub(crate) const T_CLOSE: usize = 30;
pub(crate) const T_TICK: usize = 32;
pub(crate) const T_DOT: usize = 33;
pub(crate) const T_ID: usize = 34;
pub(crate) const T_FRAME_ID: usize = 35;
pub(crate) const T_INLINE: usize = 36;
pub(crate) const T_BLOCK: usize = 37;
pub(crate) const T_ACC_DESCR: usize = 38;
pub(crate) const T_ACC_TITLE: usize = 39;
pub(crate) const T_TITLE: usize = 40;

/// The kinds of the words of Pele's TIME_FRAME, RESET_FRAME, ENTITY_TYPE and DATA_TYPE.
pub(crate) const TIME_FRAME: [usize; 2] = [28, 4];
pub(crate) const RESET_FRAME: [usize; 2] = [31, 1];
pub(crate) const ENTITY_TYPE: [usize; 9] = [19, 2, 26, 20, 5, 21, 7, 22, 3];
pub(crate) const DATA_TYPE: [usize; 8] = [11, 8, 9, 12, 23, 27, 13, 14];

/// Pele's word: a keyword that an identifier may also start with, which Langium gives the
/// identifier as a longer alternative.
const fn word(text: &'static str) -> TokenType {
    TokenType {
        longer: &[T_ID],
        ..token(text, Pattern::Literal(text))
    }
}

/// Pele's sign: a literal with no alternative.
const fn sign(text: &'static str) -> TokenType {
    token(text, Pattern::Literal(text))
}

/// Pele's EM_ID, /[_a-zA-Z][\w_]*/y.
fn id(s: &str, at: usize) -> Option<usize> {
    let b = s.as_bytes();
    if b[at] != b'_' && !is_letter(i32::from(b[at])) {
        return None;
    }
    let mut end = at + 1;
    while end < b.len() && is_word(i32::from(b[end])) {
        end += 1;
    }
    Some(end)
}

/// Pele's EM_FID, /\d{1,3}/y.
fn frame_id(s: &str, at: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut end = at;
    while end < b.len() && end - at < 3 && b[end].is_ascii_digit() {
        end += 1;
    }
    (end > at).then_some(end)
}

/// Whether a line terminator, which JavaScript's . does not take, starts at byte i of s.
fn is_line_end(s: &str, i: usize) -> bool {
    let b = &s.as_bytes()[i..];
    // U+2028 and U+2029 are E2 80 A8 and E2 80 A9 in UTF-8.
    matches!(b, [b'\n' | b'\r', ..] | [0xe2, 0x80, 0xa8 | 0xa9, ..])
}

/// Pele's EM_DATA_INLINE, /\{(.*)\}|"(.*)"|'(.*)'/y. From an opening brace or quote, the
/// greedy .* gives back up to the last closing one on the line.
fn inline(s: &str, at: usize) -> Option<usize> {
    let closer = match s.as_bytes()[at] {
        b'{' => b'}',
        q @ (b'"' | b'\'') => q,
        _ => return None,
    };
    let mut end = at + 1;
    while end < s.len() && !is_line_end(s, end) {
        end += 1;
    }
    let last = s.as_bytes()[at + 1..end]
        .iter()
        .rposition(|&c| c == closer)?;
    Some(at + 1 + last + 1)
}

/// Matches /\{[\t ]*\r?\n/y at byte at.
fn block_opener(s: &str, at: usize) -> Option<usize> {
    if s.as_bytes()[at] != b'{' {
        return None;
    }
    let b = s.as_bytes();
    let mut i = at + 1;
    while i < b.len() && matches!(b[i], b' ' | b'\t') {
        i += 1;
    }
    line_break(s, i)
}

/// Pele's EM_DATA_BLOCK, /\{[\t ]*\r?\n(?:[\S\s]*?\r?\n)?\}(?:\r?\n|(?!\S))/y. The group is
/// tried first, so the closing brace is the first after a line break, past one that follows
/// the opener at once.
fn block(s: &str, at: usize) -> Option<usize> {
    let q = block_opener(s, at)?;
    for k in q..s.len() {
        if let Some(j) = line_break(s, k)
            && s.as_bytes().get(j) == Some(&b'}')
            && let Some(end) = closed(s, j + 1)
        {
            return Some(end);
        }
    }
    if s.as_bytes().get(q) == Some(&b'}') {
        return closed(s, q + 1);
    }
    None
}

/// Whether \w matches at byte i. The title gives way to it in Pele's EVENTMODELING_TOKENS,
/// through titleBefore, so that `titles` is a name.
fn word_at(s: &str, i: usize) -> bool {
    s.as_bytes().get(i).is_some_and(|&c| is_word(i32::from(c)))
}

/// Pele's titleBefore(String.raw`\w`).
fn title(s: &str, at: usize) -> Option<usize> {
    match_title(s, at, Some(word_at))
}

/// Pele's EM_WS, /\s+/y.
fn space(s: &str, at: usize) -> Option<usize> {
    Some(space_run(s, at)).filter(|&end| end > at)
}

/// Pele's EM_ML_COMMENT, /\/\*[\s\S]*?\*\//y.
fn block_comment(s: &str, at: usize) -> Option<usize> {
    if !s[at..].starts_with("/*") {
        return None;
    }
    s[at + 2..].find("*/").map(|end| at + 2 + end + 2)
}

/// Pele's EM_SL_COMMENT, /\/\/[^\n\r]*/y.
fn line_comment(s: &str, at: usize) -> Option<usize> {
    if !s[at..].starts_with("//") {
        return None;
    }
    let b = s.as_bytes();
    let mut end = at + 2;
    while end < b.len() && b[end] != b'\n' && b[end] != b'\r' {
        end += 1;
    }
    Some(end)
}

/// Pele's EVENTMODELING_TOKENS.
pub(crate) const EVENTMODELING_TOKENS: &[TokenType] = &[
    TokenType {
        longer: &[T_ID],
        ..keyword("eventmodeling")
    },
    word("resetframe"),
    word("readmodel"),
    word("processor"),
    word("timeframe"),
    word("command"),
    word("entity"),
    word("event"),
    word("jsobj"),
    word("figma"),
    word("given"),
    word("json"),
    word("salt"),
    word("html"),
    word("text"),
    word("data"),
    word("note"),
    word("when"),
    word("then"),
    word("rmo"),
    word("cmd"),
    word("evt"),
    word("pcr"),
    word("uri"),
    sign("->>"),
    word("gwt"),
    word("ui"),
    word("md"),
    word("tf"),
    sign("[["),
    sign("]]"),
    word("rf"),
    sign("`"),
    sign("."),
    TokenType {
        first: Some("_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz"),
        ..token("EM_ID", Pattern::Match(id))
    },
    TokenType {
        first: Some("0123456789"),
        ..token("EM_FID", Pattern::Match(frame_id))
    },
    TokenType {
        first: Some("{\"'"),
        ..token("EM_DATA_INLINE", Pattern::Match(inline))
    },
    TokenType {
        first: Some("{"),
        ..token("EM_DATA_BLOCK", Pattern::Match(block))
    },
    acc_descr_named("EM_ACC_DESCR"),
    TokenType {
        name: "EM_ACC_TITLE",
        ..ACC_TITLE
    },
    TokenType {
        first: Some("\t t"),
        ..token("EM_TITLE", Pattern::Match(title))
    },
    TokenType {
        hidden: true,
        ..token("EM_WS", Pattern::Match(space))
    },
    TokenType {
        name: "EM_YAML",
        ..YAML
    },
    TokenType {
        name: "EM_DIRECTIVE",
        ..DIRECTIVE
    },
    TokenType {
        name: "EM_SINGLE_LINE_COMMENT",
        ..SINGLE_LINE_COMMENT
    },
    TokenType {
        first: Some("/"),
        hidden: true,
        ..token("EM_ML_COMMENT", Pattern::Match(block_comment))
    },
    TokenType {
        first: Some("/"),
        hidden: true,
        ..token("EM_SL_COMMENT", Pattern::Match(line_comment))
    },
];

/// EVENTMODELING_TOKENS made ready to tokenize with.
pub(crate) fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| Table::new(EVENTMODELING_TOKENS))
}

/// A frame, Pele's EmFrame. Its references are the text of each, as Mermaid reads them,
/// never linked, and what Pele leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct EmFrame {
    pub(crate) reset: bool,
    pub(crate) name: String,
    pub(crate) model_entity_type: String,
    pub(crate) entity_identifier: String,
    pub(crate) source_frames: Vec<String>,
    pub(crate) data_reference: Option<String>,
    pub(crate) data_type: Option<String>,
    pub(crate) data_inline_value: Option<String>,
    /// The byte of the source the frame starts at, for error messages.
    pub(crate) offset: usize,
}

/// A data block, Pele's EmDataEntity.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct EmDataEntity {
    pub(crate) name: String,
    pub(crate) data_type: Option<String>,
    pub(crate) data_block_value: String,
}

/// A note, Pele's EmNoteEntity.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct EmNoteEntity {
    pub(crate) source_frame: String,
    pub(crate) data_type: Option<String>,
    pub(crate) data_block_value: String,
}

/// A given/when/then specification, Pele's EmGwt, its statements the entities they name.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct EmGwt {
    pub(crate) source_frame: String,
    pub(crate) given_statements: Vec<String>,
    pub(crate) when_statements: Vec<String>,
    pub(crate) then_statements: Vec<String>,
}

/// The tree of an event modeling diagram, Pele's EventModelAst. The title and the accessible
/// title and description are whole tokens, keyword and all, as the grammar's own terminals get
/// no value conversion.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct EventModelAst {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) model_entities: Vec<String>,
    pub(crate) frames: Vec<EmFrame>,
    pub(crate) data_entities: Vec<EmDataEntity>,
    pub(crate) note_entities: Vec<EmNoteEntity>,
    pub(crate) gwt_entities: Vec<EmGwt>,
}

/// Reads a token of one of the kinds of a set, as Pele's one does.
fn one<'a>(r: &mut Reader<'a>, set: &[usize], expected: &str) -> Result<&'a str, Error> {
    if set.contains(&r.kind()) {
        Ok(r.take())
    } else {
        Err(r.fail(expected))
    }
}

/// A name of identifiers joined by dots, as Pele's qualifiedName reads it.
fn qualified_name(r: &mut Reader) -> Result<String, Error> {
    let mut name = r.expect(T_ID)?.to_string();
    while r.accept(T_DOT) {
        name.push('.');
        name.push_str(r.expect(T_ID)?);
    }
    Ok(name)
}

/// The type of data between backticks, or none where no backtick follows, as Pele's
/// dataType reads it.
fn data_type(r: &mut Reader) -> Result<Option<String>, Error> {
    if !r.accept(T_TICK) {
        return Ok(None);
    }
    let kind = one(r, &DATA_TYPE, "a data type")?.to_string();
    r.expect(T_TICK)?;
    Ok(Some(kind))
}

/// The entities a run of statements names, as Pele's statements reads them.
fn statements(r: &mut Reader) -> Result<Vec<String>, Error> {
    let mut out = Vec::new();
    loop {
        one(r, &ENTITY_TYPE, "an entity type")?;
        out.push(r.expect(T_ID)?.to_string());
        if !ENTITY_TYPE.contains(&r.kind()) {
            return Ok(out);
        }
    }
}

/// Parses an event modeling diagram's text into its tree, as Pele's parseEventModel does.
pub(crate) fn parse_event_model(src: &str) -> Result<EventModelAst, Error> {
    let tokens = tokenize(src, table(), DiagramType::EventModeling)?;
    let mut r = Reader::new(tokens, table(), DiagramType::EventModeling);
    let mut ast = EventModelAst::default();
    r.expect(T_KEYWORD)?;
    while r.kind() != END {
        let kind = r.kind();
        match kind {
            T_ACC_DESCR => ast.acc_descr = Some(r.take().to_string()),
            T_ACC_TITLE => ast.acc_title = Some(r.take().to_string()),
            T_TITLE => ast.title = Some(r.take().to_string()),
            T_ENTITY => {
                r.i += 1;
                ast.model_entities.push(qualified_name(&mut r)?);
            }
            _ if TIME_FRAME.contains(&kind) || RESET_FRAME.contains(&kind) => {
                let mut frame = EmFrame {
                    reset: !TIME_FRAME.contains(&kind),
                    offset: r.tokens.starts[r.i],
                    ..EmFrame::default()
                };
                r.i += 1;
                frame.name = r.expect(T_FRAME_ID)?.to_string();
                frame.model_entity_type = one(&mut r, &ENTITY_TYPE, "an entity type")?.to_string();
                frame.entity_identifier = qualified_name(&mut r)?;
                while r.accept(T_ARROW) {
                    frame.source_frames.push(r.expect(T_FRAME_ID)?.to_string());
                }
                if r.accept(T_OPEN) {
                    frame.data_reference = Some(r.expect(T_ID)?.to_string());
                    r.expect(T_CLOSE)?;
                }
                if r.kind() == T_TICK || r.kind() == T_INLINE {
                    frame.data_type = data_type(&mut r)?;
                    frame.data_inline_value = Some(r.expect(T_INLINE)?.to_string());
                }
                ast.frames.push(frame);
            }
            T_DATA => {
                r.i += 1;
                let name = r.expect(T_ID)?.to_string();
                let data_type = data_type(&mut r)?;
                let data_block_value = r.expect(T_BLOCK)?.to_string();
                ast.data_entities.push(EmDataEntity {
                    name,
                    data_type,
                    data_block_value,
                });
            }
            T_NOTE => {
                r.i += 1;
                let source_frame = r.expect(T_FRAME_ID)?.to_string();
                let data_type = data_type(&mut r)?;
                let data_block_value = r.expect(T_BLOCK)?.to_string();
                ast.note_entities.push(EmNoteEntity {
                    source_frame,
                    data_type,
                    data_block_value,
                });
            }
            T_GWT => {
                r.i += 1;
                let mut gwt = EmGwt {
                    source_frame: r.expect(T_FRAME_ID)?.to_string(),
                    ..EmGwt::default()
                };
                r.expect(T_GIVEN)?;
                gwt.given_statements = statements(&mut r)?;
                if r.accept(T_WHEN) {
                    gwt.when_statements = statements(&mut r)?;
                }
                r.expect(T_THEN)?;
                gwt.then_statements = statements(&mut r)?;
                ast.gwt_entities.push(gwt);
            }
            _ => {
                return Err(r.fail("a frame, an entity, a data block, a note, or a specification"));
            }
        }
    }
    Ok(ast)
}
