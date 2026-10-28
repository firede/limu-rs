//! The indented outline grammar that Mermaid's mindmap and kanban diagrams share, as Pele's
//! src/diagrams/common/outline.ts reads it. Kanban adds `@{ ... }` metadata after a node and
//! keeps `@` out of node ids.

use crate::detect::DiagramType;
use crate::errors::{Error, syntax_error};
use crate::js::chars::{is_line_end, is_word, len16, space_run, unit, unit_at};

/// The outline's tokens, Pele's T.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum T {
    End,
    Error,
    SpaceLine,
    Nl,
    Eof,
    Keyword,
    SpaceList,
    Icon,
    Class,
    NodeDStart,
    NodeId,
    NodeDescr,
    NodeDEnd,
    ShapeData,
}

/// The tokens as the generated parser prints them, Pele's NAMES. It has no name for the end
/// of input and prints its number.
const NAMES: [&str; 14] = [
    "1",
    "INVALID",
    "SPACELINE",
    "NL",
    "EOF",
    "",
    "SPACELIST",
    "ICON",
    "CLASS",
    "NODE_DSTART",
    "NODE_ID",
    "NODE_DESCR",
    "NODE_DEND",
    "SHAPE_DATA",
];

/// A token as the generated parser prints it, as Pele's tokenName does.
pub(crate) fn token_name(t: T, kanban: bool) -> &'static str {
    match t {
        T::Keyword if kanban => "KANBAN",
        T::Keyword => "MINDMAP",
        _ => NAMES[t as usize],
    }
}

/// An outline's tokens, Pele's Tokens: their types, their texts, and the byte offsets they
/// start at.
#[derive(Clone, Debug, Default)]
pub(crate) struct Tokens {
    pub(crate) types: Vec<T>,
    pub(crate) texts: Vec<String>,
    pub(crate) starts: Vec<usize>,
}

/// The shapes of a node, Pele's NODE_TYPE, by the delimiters about its text.
pub(crate) mod node_type {
    pub(crate) const DEFAULT: usize = 0;
    pub(crate) const ROUNDED_RECT: usize = 1;
    pub(crate) const RECT: usize = 2;
    pub(crate) const CIRCLE: usize = 3;
    pub(crate) const CLOUD: usize = 4;
    pub(crate) const BANG: usize = 5;
    pub(crate) const HEXAGON: usize = 6;
}

/// The shape a node's delimiters give it, as Pele's getType does. Any opening delimiter pairs
/// with any closing one, as in Mermaid.
pub(crate) fn get_type(start: &str, end: &str) -> usize {
    match start {
        "[" => node_type::RECT,
        "(" if end == ")" => node_type::ROUNDED_RECT,
        "(" => node_type::CLOUD,
        "((" => node_type::CIRCLE,
        ")" => node_type::CLOUD,
        "))" => node_type::BANG,
        "{{" => node_type::HEXAGON,
        _ => node_type::DEFAULT,
    }
}

/// What a `::icon()` or a `:::` line gives the node before it, Pele's decoration.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Decoration<'a> {
    Icon(&'a str),
    Class(&'a str),
}

/// What the outline's parser builds, Pele's OutlineDb. The shape data is the text of a
/// metadata block, where the node has one.
pub(crate) trait OutlineDb {
    fn get_type(&self, start: &str, end: &str) -> usize {
        get_type(start, end)
    }

    fn add_node(
        &mut self,
        level: usize,
        id: &str,
        descr: &str,
        kind: usize,
        shape_data: Option<&str>,
    ) -> Result<(), Error>;

    fn decorate_node(&mut self, decoration: Decoration);
}

/// The lexer's states, Pele's S.
#[derive(Clone, Copy, PartialEq, Eq)]
enum S {
    Initial,
    Class,
    Icon,
    Node,
    NStr,
    NStr2,
    Data,
    DataStr,
}

/// Whether word, in lowercase ASCII, is at byte p of src, its letters in either case, as
/// Pele's letters says: Jison's case-insensitive flag folds ASCII letters only.
fn letters(src: &str, p: usize, word: &str) -> bool {
    let b = src.as_bytes();
    word.bytes()
        .enumerate()
        .all(|(k, w)| b.get(p + k).is_some_and(|&c| c | 32 == w))
}

/// The end of /.+/y at byte i: the run of characters that do not end a line, or none where
/// there is none.
fn line_run(src: &str, i: usize) -> Option<usize> {
    let mut j = i;
    while j < src.len() {
        let (c, size) = unit_at(src, j);
        if is_line_end(c) {
            break;
        }
        j += size;
    }
    (j > i).then_some(j)
}

/// The end of a regular expression's run of characters none of which is in set, starting at
/// byte i, or none where there is none. Every character in set is ASCII, so the run steps
/// over whole characters.
fn run_not(src: &str, i: usize, set: &[u8]) -> Option<usize> {
    let b = src.as_bytes();
    let mut j = i;
    while j < b.len() && !set.contains(&b[j]) {
        j += 1;
    }
    (j > i).then_some(j)
}

/// Writes each line feed of s, with the white space after it, as `<br/>`, as Pele's replace
/// of /\n\s*/g does.
fn breaks(s: &str) -> String {
    if !s.contains('\n') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while let Some(k) = s[i..].find('\n') {
        out.push_str(&s[i..i + k]);
        out.push_str("<br/>");
        i = space_run(s, i + k + 1);
    }
    out.push_str(&s[i..]);
    out
}

/// Reads an outline's tokens, as Pele's tokenize does.
pub(crate) fn tokenize(src: &str, kanban: bool) -> Tokens {
    let n = src.len();
    let mut t = Tokens::default();
    let keyword = if kanban { "kanban" } else { "mindmap" };
    let id_set: &[u8] = if kanban { b"([\n){}@" } else { b"([\n){}" };
    let mut state = S::Initial;
    let mut p = 0;
    let mut next_break: Option<usize> = None;

    // A token that starts at p and ends at end, which is where the lexer goes on from.
    let put = |t: &mut Tokens, p: usize, kind: T, text: String, end: usize| {
        t.types.push(kind);
        t.texts.push(text);
        t.starts.push(p);
        end
    };
    let emit = |t: &mut Tokens, p: usize, kind: T, end: usize| {
        put(t, p, kind, src[p..end].to_string(), end)
    };
    let up_to = |p: usize, ch: char| src[p..].find(ch).map_or(n, |e| p + e);

    while p < n {
        let c = unit(src, p);
        let d = unit(src, p + 1);
        match state {
            S::Initial => {
                let w = if c <= 32 || c >= 160 {
                    space_run(src, p)
                } else {
                    p
                };
                if kanban && c == '@' as i32 && d == '{' as i32 {
                    p = put(&mut t, p, T::ShapeData, String::new(), p + 2);
                    state = S::Data;
                } else if unit(src, w) == '%' as i32 && unit(src, w + 1) == '%' as i32 {
                    // A comment's line has at least its two percent signs.
                    let end = line_run(src, w).unwrap_or(w);
                    p = emit(&mut t, p, T::SpaceLine, end);
                } else if w > p {
                    if next_break.is_none_or(|b| b < p) {
                        next_break = Some(up_to(p, '\n'));
                    }
                    if next_break.is_some_and(|b| b >= w) {
                        p = emit(&mut t, p, T::SpaceList, w);
                    } else if let Some(e) = src[..w].rfind('\n').filter(|&e| e > p) {
                        p = emit(&mut t, p, T::SpaceLine, e + 1);
                    } else {
                        p = emit(&mut t, p, T::Nl, p + 1);
                    }
                } else if letters(src, p, keyword) && !is_word(unit(src, p + keyword.len())) {
                    p = emit(&mut t, p, T::Keyword, p + keyword.len());
                } else if c == ':' as i32 && d == ':' as i32 && unit(src, p + 2) == ':' as i32 {
                    p += 3;
                    state = S::Class;
                } else if c == ':' as i32
                    && d == ':' as i32
                    && letters(src, p + 2, "icon")
                    && unit(src, p + 6) == '(' as i32
                {
                    p += 7;
                    state = S::Icon;
                } else if (c == '-' as i32 && d == ')' as i32)
                    || (c == '(' as i32 && (d == '-' as i32 || d == '(' as i32))
                    || ((c == ')' as i32 || c == '{' as i32) && d == c)
                {
                    p = emit(&mut t, p, T::NodeDStart, p + 2);
                    state = S::Node;
                } else if c == ')' as i32 || c == '(' as i32 || c == '[' as i32 {
                    p = emit(&mut t, p, T::NodeDStart, p + 1);
                    state = S::Node;
                } else if let Some(e) = run_not(src, p, id_set) {
                    p = emit(&mut t, p, T::NodeId, e);
                } else {
                    break;
                }
            }
            S::Node => {
                if c == '"' as i32 {
                    let md = d == '`' as i32;
                    state = if md { S::NStr2 } else { S::NStr };
                    p += if md { 2 } else { 1 };
                } else if c == ')' as i32 || c == '(' as i32 {
                    let two = d == c || (c == '(' as i32 && d == '-' as i32);
                    p = emit(&mut t, p, T::NodeDEnd, if two { p + 2 } else { p + 1 });
                    state = S::Initial;
                } else if c == ']' as i32 {
                    p = emit(&mut t, p, T::NodeDEnd, p + 1);
                    state = S::Initial;
                } else if (c == '}' as i32 && d == '}' as i32)
                    || (c == '-' as i32 && d == ')' as i32)
                {
                    p = emit(&mut t, p, T::NodeDEnd, p + 2);
                    state = S::Initial;
                } else {
                    // A lone closing brace takes the rest of its line as text. Either run
                    // takes at least the character at p, which is none of the delimiters
                    // above.
                    let end = if c == '}' as i32 {
                        line_run(src, p)
                    } else {
                        run_not(src, p, b")](}")
                    };
                    p = emit(&mut t, p, T::NodeDescr, end.unwrap_or(p + 1));
                }
            }
            S::NStr => {
                if c == '"' as i32 {
                    p += 1;
                    state = S::Node;
                } else {
                    p = emit(&mut t, p, T::NodeDescr, up_to(p, '"'));
                }
            }
            S::NStr2 => {
                if c == '`' as i32 && d == '"' as i32 {
                    p += 2;
                    state = S::Node;
                } else if let Some(e) = run_not(src, p, b"`\"") {
                    p = emit(&mut t, p, T::NodeDescr, e);
                } else {
                    break;
                }
            }
            S::Class => {
                if let Some(e) = line_run(src, p) {
                    p = emit(&mut t, p, T::Class, e);
                    state = S::Initial;
                } else if c == '\n' as i32 {
                    p += 1;
                    state = S::Initial;
                } else {
                    break;
                }
            }
            S::Icon => {
                if c == ')' as i32 {
                    p += 1;
                    state = S::Initial;
                } else {
                    p = emit(&mut t, p, T::Icon, up_to(p, ')'));
                }
            }
            S::Data => {
                if c == '"' as i32 {
                    p = emit(&mut t, p, T::ShapeData, p + 1);
                    state = S::DataStr;
                } else if c == '}' as i32 {
                    p += 1;
                    state = S::Initial;
                } else if let Some(e) = run_not(src, p, b"}^\"") {
                    p = emit(&mut t, p, T::ShapeData, e);
                } else {
                    break;
                }
            }
            S::DataStr => {
                if c == '"' as i32 {
                    p = emit(&mut t, p, T::ShapeData, p + 1);
                    state = S::Data;
                } else {
                    let e = up_to(p, '"');
                    p = put(&mut t, p, T::ShapeData, breaks(&src[p..e]), e);
                }
            }
        }
    }

    if p < n {
        put(&mut t, p, T::Error, String::new(), p);
    } else {
        if state == S::Initial {
            put(&mut t, p, T::Eof, String::new(), n);
        }
        put(&mut t, n, T::End, String::new(), n);
    }
    t
}

const STOP: &str = "'SPACELINE', 'NL', 'EOF'";

fn is_stop(t: T) -> bool {
    matches!(t, T::SpaceLine | T::Nl | T::Eof)
}

/// Whether a statement may start with a token, Pele's START.
fn starts_statement(t: T) -> bool {
    matches!(
        t,
        T::SpaceLine | T::SpaceList | T::Icon | T::Class | T::NodeDStart | T::NodeId
    )
}

/// Makes the calls on db that the grammar's actions make, in the same order, and reports
/// errors in the state where the generated parser would, as Pele's parseOutline does.
pub(crate) fn parse_outline(src: &str, db: &mut dyn OutlineDb, kanban: bool) -> Result<(), Error> {
    let diagram = if kanban {
        DiagramType::Kanban
    } else {
        DiagramType::Mindmap
    };
    let Tokens {
        types,
        texts,
        starts,
    } = tokenize(src, kanban);
    let data = if kanban { ", 'SHAPE_DATA'" } else { "" };
    let mut i = 0;

    let fail = |i: usize, expected: &str| -> Error {
        let t = types[i];
        if t == T::Error {
            return syntax_error(diagram, src, starts[i], "", true);
        }
        let detail = format!("Expecting {expected}, got '{}'", token_name(t, kanban));
        syntax_error(diagram, src, starts[i], &detail, false)
    };

    let keyword = format!("'{}'", token_name(T::Keyword, kanban));
    if types[i] == T::SpaceLine {
        i += 1;
        while matches!(types[i], T::SpaceLine | T::Nl) {
            i += 1;
        }
        if types[i] != T::Keyword {
            return Err(fail(i, &format!("'SPACELINE', 'NL', {keyword}")));
        }
    } else if types[i] != T::Keyword {
        return Err(fail(i, &format!("'SPACELINE', {keyword}")));
    }
    i += 1;
    let mut first = "'SPACELINE', 'NL', 'SPACELIST', 'ICON', 'CLASS', 'NODE_DSTART', 'NODE_ID'";
    if types[i] == T::Nl {
        i += 1;
        first = "'SPACELINE', 'SPACELIST', 'ICON', 'CLASS', 'NODE_DSTART', 'NODE_ID'";
    }

    loop {
        let mut t = types[i];
        let mut level = 0;
        let indented = t == T::SpaceList;
        if indented {
            level = len16(&texts[i]);
            i += 1;
            t = types[i];
        }
        if t == T::NodeId || t == T::NodeDStart {
            let mut id = None;
            let mut kind = 0;
            if t == T::NodeId {
                id = Some(texts[i].as_str());
                i += 1;
            }
            let descr = if types[i] == T::NodeDStart {
                let start = &texts[i];
                i += 1;
                if types[i] != T::NodeDescr {
                    return Err(fail(i, "'NODE_DESCR'"));
                }
                let descr = texts[i].as_str();
                i += 1;
                if types[i] != T::NodeDEnd {
                    return Err(fail(i, "'NODE_DEND'"));
                }
                let end = &texts[i];
                i += 1;
                if !is_stop(types[i]) && types[i] != T::ShapeData {
                    return Err(fail(i, &format!("{STOP}{data}")));
                }
                kind = db.get_type(start, end);
                descr
            } else {
                if !is_stop(types[i]) && types[i] != T::ShapeData {
                    return Err(fail(i, &format!("{STOP}, 'NODE_DSTART'{data}")));
                }
                id.unwrap_or_default()
            };
            let id = id.unwrap_or(descr);
            if types[i] == T::ShapeData {
                let mut shape_data = texts[i].clone();
                i += 1;
                while types[i] == T::ShapeData {
                    shape_data.push_str(&texts[i]);
                    i += 1;
                }
                if !is_stop(types[i]) {
                    return Err(fail(i, &format!("{STOP}{data}")));
                }
                db.add_node(level, id, descr, kind, Some(&shape_data))?;
            } else {
                db.add_node(level, id, descr, kind, None)?;
            }
        } else if t == T::Icon || t == T::Class {
            let text = texts[i].as_str();
            i += 1;
            if !is_stop(types[i]) {
                return Err(fail(i, STOP));
            }
            db.decorate_node(if t == T::Icon {
                Decoration::Icon(text)
            } else {
                Decoration::Class(text)
            });
        } else if indented {
            if !is_stop(t) {
                return Err(fail(
                    i,
                    &format!("{STOP}, 'ICON', 'CLASS', 'NODE_DSTART', 'NODE_ID'"),
                ));
            }
        } else if t == T::SpaceLine {
            i += 1;
            if !is_stop(types[i]) {
                return Err(fail(i, STOP));
            }
        } else {
            return Err(fail(i, first));
        }

        i += 1;
        while matches!(types[i], T::Nl | T::Eof) {
            i += 1;
        }
        if types[i] == T::End {
            return Ok(());
        }
        if !starts_statement(types[i]) {
            return Err(fail(
                i,
                &format!("{STOP}, 'SPACELIST', 'ICON', 'CLASS', 'NODE_DSTART', 'NODE_ID'"),
            ));
        }
    }
}
