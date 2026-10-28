//! A C4 diagram's text split into tokens, as Pele's lexer.ts and tokens.ts split it, making the
//! decisions the rules of the lexer Jison compiles from c4Diagram.jison make.
//!
//! Positions are bytes. Where Pele reads a code unit with charCodeAt, [`unit_at`] reads the
//! same one, and a character outside ASCII is stepped over whole.

use crate::js::chars::{is_line_end, is_space, is_word, space_run, unit, unit_at};

// The types of token, in the order of Pele's T.
pub(crate) const END: usize = 0;
pub(crate) const ERROR: usize = 1;
pub(crate) const DIRECTION_TB: usize = 2;
pub(crate) const TITLE: usize = 6;
pub(crate) const ACC_DESCRIPTION: usize = 7;
pub(crate) const ACC_TITLE: usize = 8;
pub(crate) const ACC_TITLE_VALUE: usize = 9;
pub(crate) const ACC_DESCR: usize = 10;
pub(crate) const ACC_DESCR_VALUE: usize = 11;
pub(crate) const ACC_DESCR_MULTILINE_VALUE: usize = 12;
pub(crate) const NEWLINE: usize = 13;
pub(crate) const C4_CONTEXT: usize = 14;
pub(crate) const C4_CONTAINER: usize = 15;
pub(crate) const C4_COMPONENT: usize = 16;
pub(crate) const C4_DYNAMIC: usize = 17;
pub(crate) const C4_DEPLOYMENT: usize = 18;
pub(crate) const PERSON: usize = 19;
pub(crate) const PERSON_EXT: usize = 20;
pub(crate) const SYSTEM: usize = 21;
pub(crate) const SYSTEM_DB: usize = 22;
pub(crate) const SYSTEM_QUEUE: usize = 23;
pub(crate) const SYSTEM_EXT: usize = 24;
pub(crate) const SYSTEM_EXT_DB: usize = 25;
pub(crate) const SYSTEM_EXT_QUEUE: usize = 26;
pub(crate) const CONTAINER: usize = 27;
pub(crate) const CONTAINER_DB: usize = 28;
pub(crate) const CONTAINER_QUEUE: usize = 29;
pub(crate) const CONTAINER_EXT: usize = 30;
pub(crate) const CONTAINER_EXT_DB: usize = 31;
pub(crate) const CONTAINER_EXT_QUEUE: usize = 32;
pub(crate) const COMPONENT: usize = 33;
pub(crate) const COMPONENT_DB: usize = 34;
pub(crate) const COMPONENT_QUEUE: usize = 35;
pub(crate) const COMPONENT_EXT: usize = 36;
pub(crate) const COMPONENT_EXT_DB: usize = 37;
pub(crate) const COMPONENT_EXT_QUEUE: usize = 38;
pub(crate) const ENTERPRISE_BOUNDARY: usize = 39;
pub(crate) const SYSTEM_BOUNDARY: usize = 40;
pub(crate) const BOUNDARY: usize = 41;
pub(crate) const CONTAINER_BOUNDARY: usize = 42;
pub(crate) const NODE: usize = 43;
pub(crate) const NODE_L: usize = 44;
pub(crate) const NODE_R: usize = 45;
pub(crate) const REL: usize = 46;
pub(crate) const BIREL: usize = 47;
pub(crate) const REL_U: usize = 48;
pub(crate) const REL_D: usize = 49;
pub(crate) const REL_L: usize = 50;
pub(crate) const REL_R: usize = 51;
pub(crate) const REL_B: usize = 52;
pub(crate) const REL_INDEX: usize = 53;
pub(crate) const UPDATE_EL_STYLE: usize = 54;
pub(crate) const UPDATE_REL_STYLE: usize = 55;
pub(crate) const UPDATE_LAYOUT_CONFIG: usize = 56;
pub(crate) const EOF_IN_STRUCT: usize = 57;
pub(crate) const ATTRIBUTE_EMPTY: usize = 58;
pub(crate) const STR: usize = 59;
pub(crate) const STR_KEY: usize = 60;
pub(crate) const STR_VALUE: usize = 61;
pub(crate) const LBRACE: usize = 62;
pub(crate) const RBRACE: usize = 63;
pub(crate) const EOF: usize = 64;

/// The names of the types of token, as Mermaid's errors give them, Pele's TOKEN_NAMES.
pub(crate) const TOKEN_NAMES: [&str; 65] = [
    "$end",
    "INVALID",
    "direction_tb",
    "direction_bt",
    "direction_rl",
    "direction_lr",
    "title",
    "accDescription",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "NEWLINE",
    "C4_CONTEXT",
    "C4_CONTAINER",
    "C4_COMPONENT",
    "C4_DYNAMIC",
    "C4_DEPLOYMENT",
    "PERSON",
    "PERSON_EXT",
    "SYSTEM",
    "SYSTEM_DB",
    "SYSTEM_QUEUE",
    "SYSTEM_EXT",
    "SYSTEM_EXT_DB",
    "SYSTEM_EXT_QUEUE",
    "CONTAINER",
    "CONTAINER_DB",
    "CONTAINER_QUEUE",
    "CONTAINER_EXT",
    "CONTAINER_EXT_DB",
    "CONTAINER_EXT_QUEUE",
    "COMPONENT",
    "COMPONENT_DB",
    "COMPONENT_QUEUE",
    "COMPONENT_EXT",
    "COMPONENT_EXT_DB",
    "COMPONENT_EXT_QUEUE",
    "ENTERPRISE_BOUNDARY",
    "SYSTEM_BOUNDARY",
    "BOUNDARY",
    "CONTAINER_BOUNDARY",
    "NODE",
    "NODE_L",
    "NODE_R",
    "REL",
    "BIREL",
    "REL_U",
    "REL_D",
    "REL_L",
    "REL_R",
    "REL_B",
    "REL_INDEX",
    "UPDATE_EL_STYLE",
    "UPDATE_REL_STYLE",
    "UPDATE_LAYOUT_CONFIG",
    "EOF_IN_STRUCT",
    "ATTRIBUTE_EMPTY",
    "STR",
    "STR_KEY",
    "STR_VALUE",
    "LBRACE",
    "RBRACE",
    "EOF",
];

/// The tokens of a text, Pele's Tokens: the text of token i runs from starts[i] to ends[i].
#[derive(Clone, Debug, Default)]
pub(crate) struct Tokens {
    pub(crate) types: Vec<usize>,
    pub(crate) starts: Vec<usize>,
    pub(crate) ends: Vec<usize>,
}

/// The start conditions of the lexer, Pele's S.
#[derive(Clone, Copy, PartialEq)]
enum S {
    Initial,
    Struct,
    Attribute,
    String,
    StringKv,
    StringKvKey,
    StringKvValue,
    Acc,
    AccDescrMultiline,
}

/// The type of token of a keyword, as Pele's KEYWORDS gives it.
fn keyword(word: &str) -> Option<usize> {
    Some(match word {
        "C4Context" => C4_CONTEXT,
        "C4Container" => C4_CONTAINER,
        "C4Component" => C4_COMPONENT,
        "C4Dynamic" => C4_DYNAMIC,
        "C4Deployment" => C4_DEPLOYMENT,
        "Person" => PERSON,
        "Person_Ext" => PERSON_EXT,
        "System" => SYSTEM,
        "SystemDb" => SYSTEM_DB,
        "SystemQueue" => SYSTEM_QUEUE,
        "System_Ext" => SYSTEM_EXT,
        "SystemDb_Ext" => SYSTEM_EXT_DB,
        "SystemQueue_Ext" => SYSTEM_EXT_QUEUE,
        "Container" => CONTAINER,
        "ContainerDb" => CONTAINER_DB,
        "ContainerQueue" => CONTAINER_QUEUE,
        "Container_Ext" => CONTAINER_EXT,
        "ContainerDb_Ext" => CONTAINER_EXT_DB,
        "ContainerQueue_Ext" => CONTAINER_EXT_QUEUE,
        "Component" => COMPONENT,
        "ComponentDb" => COMPONENT_DB,
        "ComponentQueue" => COMPONENT_QUEUE,
        "Component_Ext" => COMPONENT_EXT,
        "ComponentDb_Ext" => COMPONENT_EXT_DB,
        "ComponentQueue_Ext" => COMPONENT_EXT_QUEUE,
        "Enterprise_Boundary" => ENTERPRISE_BOUNDARY,
        "System_Boundary" => SYSTEM_BOUNDARY,
        "Boundary" => BOUNDARY,
        "Container_Boundary" => CONTAINER_BOUNDARY,
        "Deployment_Node" | "Node" => NODE,
        "Node_L" => NODE_L,
        "Node_R" => NODE_R,
        "Rel" => REL,
        "BiRel" => BIREL,
        "Rel_Up" | "Rel_U" => REL_U,
        "Rel_Down" | "Rel_D" => REL_D,
        "Rel_Left" | "Rel_L" => REL_L,
        "Rel_Right" | "Rel_R" => REL_R,
        "Rel_Back" => REL_B,
        "RelIndex" => REL_INDEX,
        "UpdateElementStyle" => UPDATE_EL_STYLE,
        "UpdateRelStyle" => UPDATE_REL_STYLE,
        "UpdateLayoutConfig" => UPDATE_LAYOUT_CONFIG,
        _ => return None,
    })
}

/// The words after `direction` in Pele's RE_DIRECTION, in its order.
const DIRECTIONS: [&str; 4] = ["TB", "BT", "RL", "LR"];

/// Where the line that byte p is on ends: the first line terminator at or after p, as
/// RE_LINE_END, /[\n\r  ]/g, finds it, or the end of src.
fn line_end(src: &str, p: usize) -> usize {
    let mut q = p;
    while q < src.len() {
        let (c, size) = unit_at(src, q);
        if is_line_end(c) {
            return q;
        }
        q += size;
    }
    src.len()
}

/// The end of what /.*direction\s+WORD[^\n]*/y matches at p, where the line ends at end, or
/// none. The greedy .* backtracks from the end of the line, so the last `direction` on the
/// line that white space and the word follow is the one matched. \s+ takes all the white space
/// after it, line breaks too, as nothing else lets the word follow.
fn match_direction(src: &str, p: usize, end: usize, word: &str) -> Option<usize> {
    const KEYWORD: &[u8] = b"direction";
    let b = src.as_bytes();
    let mut d = end.checked_sub(KEYWORD.len())?;
    loop {
        if b[d..].starts_with(KEYWORD) {
            let q = d + KEYWORD.len();
            let ws = space_run(src, q);
            if ws > q && b[ws..].starts_with(word.as_bytes()) {
                let q = ws + word.len();
                return Some(
                    b[q..]
                        .iter()
                        .position(|&c| c == b'\n')
                        .map_or(b.len(), |k| q + k),
                );
            }
        }
        if d == p {
            return None;
        }
        d -= 1;
    }
}

/// The end of what RE_BARE_TITLE, /title[^\S\r\n]*(?=[\r\n]|$)\s*/y, matches at p, or none: a
/// title with nothing after it on its line, with the line breaks and white space after it,
/// which the grammar would reject as blank statements. Giving back white space before the line
/// break never lets the lookahead see a break, so only the longest run is tried.
fn bare_title(src: &str, p: usize) -> Option<usize> {
    if !src[p..].starts_with("title") {
        return None;
    }
    let mut q = p + "title".len();
    while q < src.len() {
        let (c, size) = unit_at(src, q);
        if !is_space(c) || c == 13 || c == 10 {
            break;
        }
        q += size;
    }
    if q < src.len() && !matches!(src.as_bytes()[q], b'\r' | b'\n') {
        return None;
    }
    Some(space_run(src, q))
}

/// Splits a C4 diagram's text into tokens, as Pele's tokenize does. It follows the lexer Jison
/// compiles from c4Diagram.jison: its rule order, its start conditions, and what it returns
/// once the input runs out in each of them.
pub(crate) fn tokenize(src: &str) -> Tokens {
    let n = src.len();
    let b = src.as_bytes();
    let mut t = Tokens::default();
    let mut state = S::Initial;
    let mut acc_value = ACC_TITLE_VALUE;
    let mut p = 0;
    let mut done = false;
    // Where the next `direction` is, and the end of the line it was last looked for on, as Pele
    // keeps them so as not to search the whole text again for each statement. None is Pele's
    // -1, and the search is first made at the start.
    let mut dir_next: Option<usize> = Some(0);
    let mut searched = false;
    let mut line_at: Option<usize> = None;
    let mut dir_failed = 0;

    let emit = |t: &mut Tokens, kind: usize, s: usize, e: usize| -> usize {
        t.types.push(kind);
        t.starts.push(s);
        t.ends.push(e);
        e
    };
    // Where the next ch is from from, or the end of the text, as Pele's upTo gives it.
    let up_to = |ch: u8, from: usize| -> usize {
        b.get(from..)
            .and_then(|rest| rest.iter().position(|&c| c == ch))
            .map_or(n, |q| from + q)
    };
    // The byte at q, or none past the end, as charCodeAt's NaN.
    let at = |q: usize| b.get(q).copied();

    loop {
        // Jison notes the end of input one call before it reports it, so a rule that matches
        // nothing gets one last turn.
        if done {
            emit(&mut t, END, n, n);
            break;
        }
        if p >= n {
            done = true;
        }
        let c = at(p);

        match state {
            S::Initial => {
                if p >= n {
                    p = emit(&mut t, EOF, n, n);
                    continue;
                }
                if let Some(next) = dir_next
                    && p >= dir_failed
                {
                    if !searched || next < p {
                        searched = true;
                        dir_next = src[p..].find("direction").map(|k| p + k);
                    }
                    if let Some(next) = dir_next {
                        let line = match line_at {
                            Some(l) if l >= p => l,
                            _ => line_end(src, p),
                        };
                        line_at = Some(line);
                        if next < line {
                            let matched = DIRECTIONS.iter().enumerate().find_map(|(i, word)| {
                                match_direction(src, p, line, word).map(|e| (i, e))
                            });
                            if let Some((i, e)) = matched {
                                p = emit(&mut t, DIRECTION_TB + i, p, e);
                                continue;
                            }
                            // No direction statement starts anywhere in the rest of this line
                            // either.
                            dir_failed = line;
                        }
                    }
                }
                let (c, size) = unit_at(src, p);
                if c == 116 {
                    if let Some(e) = bare_title(src, p) {
                        p = e;
                        continue;
                    }
                    if let Some(e) = statement(src, p, "title") {
                        p = emit(&mut t, TITLE, p, e);
                        continue;
                    }
                }
                if c == 97 {
                    if let Some(e) = statement(src, p, "accDescription") {
                        p = emit(&mut t, ACC_DESCRIPTION, p, e);
                        continue;
                    }
                    let title = src[p..].starts_with("accTitle");
                    if title || src[p..].starts_with("accDescr") {
                        let q = space_run(src, p + 8);
                        let d = at(q);
                        if d == Some(b':') {
                            let kind = if title { ACC_TITLE } else { ACC_DESCR };
                            acc_value = if title {
                                ACC_TITLE_VALUE
                            } else {
                                ACC_DESCR_VALUE
                            };
                            p = emit(&mut t, kind, p, space_run(src, q + 1));
                            state = S::Acc;
                            continue;
                        }
                        if d == Some(b'{') && !title {
                            p = space_run(src, q + 1);
                            state = S::AccDescrMultiline;
                            continue;
                        }
                    }
                }
                if c == 37 && unit(src, p + 1) == 37 {
                    // A comment takes the line breaks after it, so no NEWLINE follows.
                    let mut q = up_to(b'\n', p + 2);
                    while q < n && matches!(b[q], b'\n' | b'\r') {
                        q += 1;
                    }
                    p = q;
                    continue;
                }
                if is_space(c) {
                    let mut q = p;
                    let mut last = None;
                    while q < n {
                        let (d, size) = unit_at(src, q);
                        if !is_space(d) {
                            break;
                        }
                        if d == 10 {
                            last = Some(q);
                        }
                        q += size;
                    }
                    match last {
                        Some(last) => p = emit(&mut t, NEWLINE, p, last + 1),
                        None => p = q,
                    }
                    continue;
                }
                if c == 123 {
                    p = emit(&mut t, LBRACE, p, p + 1);
                    continue;
                }
                if c == 125 {
                    p = emit(&mut t, RBRACE, p, p + 1);
                    continue;
                }
                if is_word(c) {
                    let mut q = p + size;
                    while q < n && is_word(i32::from(b[q])) {
                        q += 1;
                    }
                    let kind = if q - p <= 19 {
                        keyword(&src[p..q])
                    } else {
                        None
                    };
                    if let Some(kind) = kind {
                        if kind > C4_DEPLOYMENT {
                            state = S::Struct;
                        }
                        p = emit(&mut t, kind, p, q);
                        continue;
                    }
                }
                emit(&mut t, ERROR, p, p);
                break;
            }
            S::Struct => {
                if p >= n {
                    p = emit(&mut t, EOF_IN_STRUCT, n, n);
                } else if c == Some(b'(') {
                    let mut q = p + 1;
                    while at(q) == Some(b' ') {
                        q += 1;
                    }
                    if at(q) == Some(b',') {
                        p = emit(&mut t, ATTRIBUTE_EMPTY, p, q + 1);
                    } else {
                        p += 1;
                    }
                    state = S::Attribute;
                } else if c == Some(b')') {
                    p += 1;
                    state = S::Initial;
                } else {
                    emit(&mut t, ERROR, p, p);
                    break;
                }
            }
            S::Attribute => {
                if p >= n {
                    continue;
                }
                if c == Some(b')') {
                    p += 1;
                    state = S::Initial;
                } else if c == Some(b',') {
                    if at(p + 1) == Some(b',') {
                        p = emit(&mut t, ATTRIBUTE_EMPTY, p, p + 2);
                    } else {
                        p += 1;
                    }
                } else {
                    let mut q = p;
                    while at(q) == Some(b' ') {
                        q += 1;
                    }
                    match at(q) {
                        Some(b'"') => {
                            if at(q + 1) == Some(b'"') {
                                p = emit(&mut t, ATTRIBUTE_EMPTY, p, q + 2);
                            } else {
                                p = q + 1;
                                state = S::String;
                            }
                        }
                        Some(b'$') => {
                            p = q + 1;
                            state = S::StringKv;
                        }
                        // An unquoted value runs to the next comma, through closing
                        // parentheses and line breaks.
                        _ => p = emit(&mut t, STR, p, up_to(b',', p)),
                    }
                }
            }
            S::String => {
                if c == Some(b'"') {
                    p += 1;
                    state = S::Attribute;
                } else {
                    p = emit(&mut t, STR, p, up_to(b'"', p));
                }
            }
            S::StringKv => {
                p = emit(&mut t, STR_KEY, p, up_to(b'=', p));
                state = S::StringKvKey;
            }
            S::StringKvKey => {
                if p >= n {
                    continue;
                }
                let mut q = p + 1;
                if c == Some(b'=') {
                    while at(q) == Some(b' ') {
                        q += 1;
                    }
                }
                if c != Some(b'=') || at(q) != Some(b'"') {
                    emit(&mut t, ERROR, p, p);
                    break;
                }
                p = q + 1;
                state = S::StringKvValue;
            }
            S::StringKvValue => {
                if p >= n {
                    continue;
                }
                if c == Some(b'"') {
                    p += 1;
                    state = S::Attribute;
                } else {
                    p = emit(&mut t, STR_VALUE, p, up_to(b'"', p));
                }
            }
            S::Acc => {
                p = emit(&mut t, acc_value, p, up_to(b'\n', p));
                state = S::Initial;
            }
            S::AccDescrMultiline => {
                if c == Some(b'}') {
                    p += 1;
                    state = S::Initial;
                } else {
                    p = emit(&mut t, ACC_DESCR_MULTILINE_VALUE, p, up_to(b'}', p));
                }
            }
        }
    }
    t
}

/// The end of word\s[^#\n;]+ at byte p, as Pele's statement matches it, or none.
fn statement(src: &str, p: usize, word: &str) -> Option<usize> {
    if !src[p..].starts_with(word) {
        return None;
    }
    let (c, size) = unit_at(src, p + word.len());
    if !is_space(c) {
        return None;
    }
    let from = p + word.len() + size;
    let b = src.as_bytes();
    let mut q = from;
    while q < b.len() && !matches!(b[q], b'#' | b'\n' | b';') {
        q += 1;
    }
    (q > from).then_some(q)
}
