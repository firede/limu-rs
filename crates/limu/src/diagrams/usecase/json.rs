//! The JSON of a `json` node, as Pele's json.ts reads it: JSON.parse, as V8 runs it, and the
//! order its keys were written in, which JSON.parse loses for keys that are array indices.
//! The text is read in UTF-16 code units, as JavaScript reads it.

use std::collections::HashMap;

use super::lexer::placed;
use super::ordered::Ordered;
use crate::errors::{Error, ErrorCode, written_codes};
use crate::js::json::parse_v8;
use crate::value::{Map, Value};

/// The value of a `json` node's text, and the keys of each object in it in the order they
/// were written, by the object's JSON Pointer, Pele's OrderedJson.
pub(crate) struct OrderedJson {
    pub(crate) value: Map,
    pub(crate) property_order: Vec<(String, Vec<String>)>,
}

/// An array or an object propertyOrder is inside, Pele's Frame.
struct Frame {
    object: bool,
    pointer: Vec<u16>,
    /// The index in the lists of propertyOrder of the keys of an object.
    keys: usize,
    /// For each key read so far, the stretch of `created` its value added.
    ranges: HashMap<Vec<u16>, (usize, usize)>,
    key: Vec<u16>,
    from: usize,
    index: usize,
}

/// What propertyOrder reads next, Pele's State.
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Value,
    Key,
    After,
}

/// Whether a code unit is an ASCII digit.
fn is_digit(c: i32) -> bool {
    crate::js::chars::is_digit(c)
}

/// The order the keys of JSON text were written in, by the JSON Pointer of each object, as
/// Pele's propertyOrder walks text JSON.parse has accepted. On text JSON.parse rejected, it
/// gives the offset of the first code unit that is not valid JSON, as Pele's WalkError
/// holds it. Nesting is kept on a stack of its own, so depth is limited by memory and not by
/// the call stack.
fn property_order(text: &[u16]) -> Result<Vec<(String, Vec<String>)>, usize> {
    let mut order: Ordered<Vec<u16>, usize> = Ordered::default();
    let mut lists: Vec<Vec<Vec<u16>>> = Vec::new();
    let mut created: Vec<Vec<u16>> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let n = text.len();
    let mut at = 0;
    let mut pointer: Vec<u16> = Vec::new();
    let mut state = State::Value;
    let unit = |i: usize| text.get(i).map_or(-1, |&u| i32::from(u));

    let skip = |at: &mut usize| {
        while *at < n {
            let c = text[*at];
            if c != 32 && c != 9 && c != 10 && c != 13 {
                return;
            }
            *at += 1;
        }
    };

    let read_string = |at: &mut usize, decode: bool| -> Result<Vec<u16>, usize> {
        *at += 1;
        let mut value = Vec::new();
        while *at < n {
            let start = *at;
            let ch = text[*at];
            *at += 1;
            if ch == u16::from(b'"') {
                return Ok(value);
            }
            if ch < 0x20 {
                return Err(start);
            }
            if ch != u16::from(b'\\') {
                if decode {
                    value.push(ch);
                }
                continue;
            }
            let escape_at = *at;
            let escape = text.get(*at).copied();
            *at += 1;
            let decoded = match escape.and_then(|e| u8::try_from(e).ok()) {
                Some(e @ (b'"' | b'\\' | b'/')) => u16::from(e),
                Some(b'b') => 8,
                Some(b'f') => 12,
                Some(b'n') => 10,
                Some(b'r') => 13,
                Some(b't') => 9,
                Some(b'u') => {
                    let hex = text
                        .get(*at..*at + 4)
                        .filter(|h| h.iter().all(|&u| crate::js::chars::is_hex(i32::from(u))));
                    let Some(hex) = hex else {
                        return Err(*at);
                    };
                    *at += 4;
                    hex.iter().fold(0u16, |v, &u| {
                        v * 16 + (char::from(u as u8).to_digit(16).unwrap_or(0) as u16)
                    })
                }
                _ => return Err(escape_at),
            };
            if decode {
                value.push(decoded);
            }
        }
        Err(*at)
    };

    let literal = |at: &mut usize, word: &str| -> Result<(), usize> {
        for (k, w) in word.bytes().enumerate() {
            if text.get(*at + k) != Some(&u16::from(w)) {
                return Err(*at + k);
            }
        }
        *at += word.len();
        Ok(())
    };

    let number = |at: &mut usize| -> Result<(), usize> {
        if unit(*at) == 45 {
            *at += 1;
        }
        if unit(*at) == 48 {
            *at += 1;
        } else if is_digit(unit(*at)) {
            while is_digit(unit(*at)) {
                *at += 1;
            }
        } else {
            return Err(*at);
        }
        if unit(*at) == 46 {
            *at += 1;
            if !is_digit(unit(*at)) {
                return Err(*at);
            }
            while is_digit(unit(*at)) {
                *at += 1;
            }
        }
        if unit(*at) >= 0 && (unit(*at) | 32) == 101 {
            *at += 1;
            if unit(*at) == 43 || unit(*at) == 45 {
                *at += 1;
            }
            if !is_digit(unit(*at)) {
                return Err(*at);
            }
            while is_digit(unit(*at)) {
                *at += 1;
            }
        }
        Ok(())
    };

    skip(&mut at);
    loop {
        match state {
            State::Value => {
                skip(&mut at);
                let c = unit(at);
                state = State::After;
                if c == 123 {
                    at += 1;
                    let keys = lists.len();
                    lists.push(Vec::new());
                    order.set(pointer.clone(), keys);
                    created.push(pointer.clone());
                    skip(&mut at);
                    if unit(at) == 125 {
                        at += 1;
                    } else {
                        stack.push(Frame {
                            object: true,
                            pointer: pointer.clone(),
                            keys,
                            ranges: HashMap::new(),
                            key: Vec::new(),
                            from: 0,
                            index: 0,
                        });
                        state = State::Key;
                    }
                } else if c == 91 {
                    at += 1;
                    skip(&mut at);
                    if unit(at) == 93 {
                        at += 1;
                    } else {
                        stack.push(Frame {
                            object: false,
                            pointer: pointer.clone(),
                            keys: 0,
                            ranges: HashMap::new(),
                            key: Vec::new(),
                            from: 0,
                            index: 0,
                        });
                        pointer.extend("/0".encode_utf16());
                        state = State::Value;
                    }
                } else if c == 34 {
                    read_string(&mut at, false)?;
                } else if c == 116 {
                    literal(&mut at, "true")?;
                } else if c == 102 {
                    literal(&mut at, "false")?;
                } else if c == 110 {
                    literal(&mut at, "null")?;
                } else if c == 45 || is_digit(c) {
                    number(&mut at)?;
                } else {
                    return Err(at);
                }
            }
            State::Key => {
                if unit(at) != 34 {
                    return Err(at);
                }
                let key = read_string(&mut at, true)?;
                let Some(frame) = stack.last_mut() else {
                    return Err(at);
                };
                // A repeated key keeps its first place in the order and takes the nested order
                // of its last value.
                match frame.ranges.get(&key) {
                    None => lists[frame.keys].push(key.clone()),
                    Some(&(from, to)) => {
                        for p in &created[from..to] {
                            order.delete(p);
                        }
                    }
                }
                frame.from = created.len();
                skip(&mut at);
                if unit(at) != 58 {
                    return Err(at);
                }
                at += 1;
                pointer = frame.pointer.clone();
                pointer.push(u16::from(b'/'));
                for &u in &key {
                    match u {
                        0x7e => pointer.extend("~0".encode_utf16()),
                        0x2f => pointer.extend("~1".encode_utf16()),
                        _ => pointer.push(u),
                    }
                }
                frame.key = key;
                state = State::Value;
            }
            State::After => {
                let Some(frame) = stack.last_mut() else {
                    break;
                };
                if frame.object {
                    frame
                        .ranges
                        .insert(frame.key.clone(), (frame.from, created.len()));
                } else {
                    frame.index += 1;
                }
                skip(&mut at);
                let c = unit(at);
                if c == if frame.object { 125 } else { 93 } {
                    at += 1;
                    stack.pop();
                } else if c != 44 {
                    return Err(at);
                } else {
                    at += 1;
                    skip(&mut at);
                    pointer = frame.pointer.clone();
                    pointer.extend(format!("/{}", frame.index).encode_utf16());
                    state = if frame.object {
                        State::Key
                    } else {
                        State::Value
                    };
                }
            }
        }
    }
    skip(&mut at);
    if at != n {
        return Err(at);
    }
    let text = |u: &[u16]| String::from_utf16_lossy(u);
    Ok(order
        .iter()
        .map(|(p, &keys)| (text(p), lists[keys].iter().map(|k| text(k)).collect()))
        .collect())
}

/// The line and column of offset in text, from a line and column the text starts at, as
/// Pele's locationAt counts them.
fn location_at(text: &[u16], offset: usize, mut line: usize, mut column: usize) -> (usize, usize) {
    let end = offset.min(text.len());
    for i in 0..end {
        let c = text[i];
        if c == 13 {
            line += 1;
            column = 1;
        } else if c == 10 {
            if i == 0 || text[i - 1] != 13 {
                line += 1;
                column = 1;
            }
        } else {
            column += 1;
        }
    }
    (line, column)
}

/// Whether a byte is in a regular expression's `\w`.
fn word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Where a regular expression of the form /\bword (\d+)\b/ first matches in a message, and the
/// number its digits make, which is past what a usize holds for too many of them. Each word
/// of words is followed by a number, the words are matched one after another with a space
/// between, as /\bline (\d+) column (\d+)\b/ is.
fn numbers_after(message: &str, words: &[&str]) -> Option<Vec<usize>> {
    let b = message.as_bytes();
    let mut start = 0;
    'next: while let Some(k) = message[start..].find(words[0]) {
        let at = start + k;
        start = at + 1;
        if at > 0 && word(b[at - 1]) {
            continue;
        }
        let mut i = at;
        let mut out = Vec::new();
        for (w, &name) in words.iter().enumerate() {
            if w > 0 {
                if b.get(i) != Some(&b' ') {
                    continue 'next;
                }
                i += 1;
            }
            if !message[i..].starts_with(name) || b.get(i + name.len()) != Some(&b' ') {
                continue 'next;
            }
            i += name.len() + 1;
            let digits = i;
            while b.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            if i == digits {
                continue 'next;
            }
            out.push(message[digits..i].parse().unwrap_or(usize::MAX));
        }
        // \d+ backtracks to no shorter match that ends at a boundary, as a digit is \w.
        if b.get(i).is_some_and(|&c| word(c)) {
            continue;
        }
        return Some(out);
    }
    None
}

/// The place an error of JSON.parse is at, from its message, as Pele's locationFromMessage
/// finds it: engines word the errors differently, some giving an offset, and some a line and
/// column.
fn location_from_message(
    message: &str,
    text: &[u16],
    line: usize,
    column: usize,
) -> Option<(usize, usize)> {
    if let Some(p) = numbers_after(message, &["position"]) {
        return Some(location_at(text, p[0], line, column));
    }
    if let Some(l) = numbers_after(message, &["line", "column"]) {
        let (local_line, local_column) = (l[0], l[1]);
        return Some((
            (line + local_line).saturating_sub(1),
            if local_line == 1 {
                (column + local_column).saturating_sub(1)
            } else {
                local_column
            },
        ));
    }
    let lower = message.to_ascii_lowercase();
    if lower.contains("unexpected end") || lower.contains("end of json input") {
        return Some(location_at(text, text.len(), line, column));
    }
    None
}

/// An error in a `json` node's text, as Pele's jsonError words it.
fn json_error(message: &str, line: usize, column: usize) -> Error {
    placed(
        format!("{message} (line {line}, column {column})"),
        ErrorCode::Syntax,
        line,
        column,
    )
}

/// Parses the body of a `json` node, as Pele's parseOrderedJson does. `origin` gives the line
/// and column the text starts at in the diagram.
pub(crate) fn parse_ordered_json(
    text: &str,
    origin: impl Fn() -> (usize, usize),
) -> Result<OrderedJson, Error> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let parsed = match parse_v8(&units) {
        Ok(v) => v,
        Err(mut message) => {
            // Parse again with the entity codes restored, so that the message and the place
            // match the source.
            let shown = written_codes(text);
            let shown: Vec<u16> = shown.encode_utf16().collect();
            if shown != units
                && let Err(again) = parse_v8(&shown)
            {
                message = again;
            }
            let (line, column) = origin();
            let location =
                location_from_message(&message, &shown, line, column).unwrap_or_else(|| {
                    let offset = property_order(&shown).err().unwrap_or(0);
                    location_at(&shown, offset, line, column)
                });
            return Err(json_error(
                &format!("Invalid JSON: {message}"),
                location.0,
                location.1,
            ));
        }
    };
    let Value::Object(value) = parsed else {
        let (line, column) = origin();
        return Err(json_error(
            "JSON value must have an object root",
            line,
            column,
        ));
    };
    let property_order = property_order(&units).unwrap_or_default();
    Ok(OrderedJson {
        value,
        property_order,
    })
}
