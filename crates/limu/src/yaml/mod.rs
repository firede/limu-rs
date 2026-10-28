//! The YAML Mermaid diagrams carry, in their front matter and in the metadata of flowchart
//! nodes, as Pele's src/util/yaml.ts reads it: a small reader that agrees with js-yaml and
//! its JSON schema on what Mermaid feeds it, with block and flow collections, quoted and
//! plain scalars, and block scalars.
//!
//! Text is stepped through by bytes, which is what Pele does by code units wherever the
//! characters looked for are ASCII. Indents of white space are counted in characters, as
//! JavaScript counts them in code units, since white space is all in the Basic Multilingual
//! Plane.

use crate::js::chars::{is_space_char, trim, trim_start};
use crate::js::number;
use crate::{Map, Value};

/// An error reading YAML, with the message Pele's error has.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct YamlError(pub(crate) String);

/// Why reading stopped: a quoted scalar or a flow collection that runs past the text read so
/// far, as Pele's Incomplete is, or any other error.
enum Fail {
    Incomplete(&'static str),
    Error(String),
}

impl Fail {
    fn message(self) -> String {
        match self {
            Fail::Incomplete(m) => m.to_string(),
            Fail::Error(m) => m,
        }
    }
}

fn error(message: impl Into<String>) -> Fail {
    Fail::Error(message.into())
}

type Read<T> = Result<T, Fail>;

const MAX_DEPTH: usize = 64;

/// The lines of the text, and the one being read, as Pele's Reader holds them.
struct Reader {
    lines: Vec<String>,
    i: usize,
}

impl Reader {
    fn new(src: &str) -> Reader {
        let src = if src.contains('\r') {
            src.replace("\r\n", "\n").replace('\r', "\n")
        } else {
            src.to_string()
        };
        Reader {
            lines: src.split('\n').map(String::from).collect(),
            i: 0,
        }
    }

    /// Steps over blank lines, comments and document markers, as Pele's skipBlank does.
    fn skip_blank(&mut self) {
        while self.i < self.lines.len() {
            let t = trim(&self.lines[self.i]);
            if !t.is_empty() && !t.starts_with('#') && t != "---" {
                break;
            }
            self.i += 1;
        }
    }

    /// The spaces the line begins with, as Pele's indent counts them.
    fn indent(&self) -> Read<usize> {
        let line = self.lines[self.i].as_bytes();
        let k = line.iter().take_while(|&&c| c == b' ').count();
        if line.get(k) == Some(&b'\t') {
            return Err(error("YAML: tabs are not allowed for indentation"));
        }
        Ok(k)
    }
}

/// The byte of s at i, or 0 past its end, where JavaScript reads undefined: a byte no test
/// here looks for.
fn at(s: &str, i: usize) -> u8 {
    s.as_bytes().get(i).copied().unwrap_or(0)
}

/// Whether the character before byte i of s is white space to `\s`.
fn space_before(s: &str, i: usize) -> bool {
    s[..i].chars().next_back().is_some_and(is_space_char)
}

/// The white space a line begins with, in code units: `line.length - line.trimStart().length`.
fn leading(line: &str) -> usize {
    line.chars().take_while(|&c| is_space_char(c)).count()
}

/// `line.slice(n)` where the first n code units of line are white space.
fn drop_units(line: &str, n: usize) -> &str {
    let at = line.char_indices().nth(n).map_or(line.len(), |(i, _)| i);
    &line[at..]
}

fn is_blank(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}

/// Whether c can begin no plain key, as Pele's RE_KEY_START has it.
fn is_key_start(c: char) -> bool {
    is_space_char(c) || "#:,[]{}&*!|>'\"%@`".contains(c)
}

/// Splits `key: rest` at the start of a line, as Pele's splitKey does, and gives the raw key
/// and where the value starts.
fn split_key(line: &str) -> Option<(&str, usize)> {
    let b = line.as_bytes();
    let first = *b.first()?;
    let (end, colon);
    if first == b'"' || first == b'\'' {
        let mut i = 1;
        loop {
            if i >= b.len() {
                return None;
            }
            let c = b[i];
            if first == b'"' && c == b'\\' {
                i += 2;
            } else if c == first {
                if first == b'"' || at(line, i + 1) != b'\'' {
                    break;
                }
                i += 2;
            } else {
                i += 1;
            }
        }
        end = i + 1;
        let mut k = end;
        while is_blank(at(line, k)) {
            k += 1;
        }
        if at(line, k) != b':' {
            return None;
        }
        colon = k;
    } else {
        if line.chars().next().is_some_and(is_key_start) {
            return None;
        }
        if first == b'-' && (b.len() < 2 || is_blank(b[1]) || b[1] == b':' || b[1] == b'#') {
            return None;
        }
        let mut k = 1;
        while k < b.len() && b[k] != b':' && b[k] != b'#' {
            k += 1;
        }
        if at(line, k) != b':' {
            return None;
        }
        colon = k;
        let mut e = colon;
        while e > 0 && is_blank(b[e - 1]) {
            e -= 1;
        }
        end = e;
    }
    let mut length = colon + 1;
    if length < b.len() && !is_blank(b[length]) {
        return None;
    }
    while is_blank(at(line, length)) {
        length += 1;
    }
    Some((&line[..end], length))
}

fn strip_sign(s: &str) -> &str {
    s.strip_prefix(['-', '+']).unwrap_or(s)
}

fn all_digits(s: &str) -> bool {
    s.bytes().all(|c| c.is_ascii_digit())
}

/// `/^[-+]?(?:0|[1-9][0-9]*)$/`, Pele's RE_INT.
fn is_int(s: &str) -> bool {
    let d = strip_sign(s);
    d == "0" || d.starts_with(|c: char| ('1'..='9').contains(&c)) && all_digits(d)
}

/// `/^[-+]?(?:[0-9][0-9]*(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][-+]?[0-9]+)?$/`, Pele's RE_FLOAT.
/// No backtracking finds a match the greedy runs miss, as each run is followed by what it
/// cannot hold.
fn is_float(s: &str) -> bool {
    let b = strip_sign(s).as_bytes();
    let digits = |i: usize| i + b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
    let mut i;
    if b.first().is_some_and(u8::is_ascii_digit) {
        i = digits(0);
        if b.get(i) == Some(&b'.') {
            i = digits(i + 1);
        }
    } else if b.first() == Some(&b'.') {
        i = digits(1);
        if i == 1 {
            return false;
        }
    } else {
        return false;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(b.get(j), Some(b'-' | b'+')) {
            j += 1;
        }
        i = digits(j);
        if i == j {
            return false;
        }
    }
    i == b.len()
}

/// `/^[-+]?0x[0-9a-fA-F]+$/` and its kin for octal and binary, Pele's RE_HEX, RE_OCT and
/// RE_BIN.
fn is_prefixed(s: &str) -> bool {
    let Some(rest) = strip_sign(s).strip_prefix('0') else {
        return false;
    };
    let (digits, keep): (&str, fn(&u8) -> bool) = match rest.as_bytes().first() {
        Some(b'x') => (&rest[1..], u8::is_ascii_hexdigit),
        Some(b'o') => (&rest[1..], |c| (b'0'..=b'7').contains(c)),
        Some(b'b') => (&rest[1..], |c| *c == b'0' || *c == b'1'),
        _ => return false,
    };
    !digits.is_empty() && digits.as_bytes().iter().all(keep)
}

/// A plain scalar's value, as Pele's scalar reads it by the JSON schema.
fn scalar(raw: &str) -> Value {
    let s = trim(raw);
    match s {
        "" | "~" | "null" | "Null" | "NULL" => return Value::Null,
        "true" | "True" | "TRUE" => return Value::Bool(true),
        "false" | "False" | "FALSE" => return Value::Bool(false),
        _ => {}
    }
    let c = s.as_bytes()[0];
    if c != b'-' && c != b'+' && c != b'.' && !c.is_ascii_digit() {
        return s.into();
    }
    if is_int(s) || is_float(s) {
        return Value::Number(number::number(s));
    }
    if is_prefixed(s) {
        let neg = c == b'-';
        let v = number::number(if neg || c == b'+' { &s[1..] } else { s });
        return Value::Number(if neg { -v } else { v });
    }
    if matches!(strip_sign(s), ".inf" | ".Inf" | ".INF") {
        return Value::Number(if c == b'-' {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        });
    }
    if matches!(s, ".nan" | ".NaN" | ".NAN") {
        return Value::Number(f64::NAN);
    }
    s.into()
}

/// What an escape of a double-quoted scalar stands for, as Pele's ESCAPES has them.
fn escape(e: u16) -> Option<char> {
    Some(match char::from_u32(u32::from(e))? {
        '0' => '\0',
        'a' => '\x07',
        'b' => '\x08',
        't' => '\t',
        'n' => '\n',
        'v' => '\x0b',
        'f' => '\x0c',
        'r' => '\r',
        'e' => '\x1b',
        ' ' => ' ',
        '"' => '"',
        '/' => '/',
        '\\' => '\\',
        'N' => '\u{85}',
        '_' => '\u{a0}',
        'L' => '\u{2028}',
        'P' => '\u{2029}',
        _ => return None,
    })
}

/// Folds the line breaks of a scalar, as Pele's fold does: blanks around a break go, one
/// break becomes a space, and each further break stays.
fn fold(text: &str) -> String {
    if !text.contains('\n') {
        return text.to_string();
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let last = lines.len() - 1;
    let blank = |c: char| c == ' ' || c == '\t';
    let joined = lines
        .iter()
        .enumerate()
        .map(|(i, &line)| {
            let line = if i > 0 {
                line.trim_start_matches(blank)
            } else {
                line
            };
            if i < last {
                line.trim_end_matches(blank)
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut out = String::with_capacity(joined.len());
    let mut breaks = 0;
    for c in joined.chars() {
        if c == '\n' {
            breaks += 1;
            continue;
        }
        push_breaks(&mut out, breaks);
        breaks = 0;
        out.push(c);
    }
    push_breaks(&mut out, breaks);
    out
}

/// A run of n line breaks, folded: one is a space, and more are one fewer breaks.
fn push_breaks(out: &mut String, n: usize) {
    match n {
        0 => {}
        1 => out.push(' '),
        _ => out.extend(std::iter::repeat_n('\n', n - 1)),
    }
}

/// `body.replace(/\\\n[ \t]*/g, '')`: an escaped line break, and the blanks after it.
fn drop_escaped_breaks(body: &str) -> String {
    let b = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let (mut last, mut i) = (0, 0);
    while i < b.len() {
        if b[i] == b'\\' && b.get(i + 1) == Some(&b'\n') {
            out.push_str(&body[last..i]);
            i += 2;
            while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
                i += 1;
            }
            last = i;
        } else {
            i += 1;
        }
    }
    out.push_str(&body[last..]);
    out
}

/// The text of a double-quoted scalar, as Pele's unquoteDouble reads it. It reads the text
/// by UTF-16 code units, as an escape of four hexadecimal digits names one. A surrogate an
/// escape names that has no other half is U+FFFD here, where JavaScript keeps it.
fn unquote_double(body: &str) -> Read<String> {
    if !body.contains('\\') {
        return Ok(fold(body));
    }
    let s: Vec<u16> = fold(&drop_escaped_breaks(body)).encode_utf16().collect();
    let mut out: Vec<u16> = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let ch = s[i];
        if ch != u16::from(b'\\') {
            out.push(ch);
            i += 1;
            continue;
        }
        i += 1;
        let e = s.get(i).copied();
        if let Some(n) = e.and_then(|e| match e {
            0x78 => Some(2),
            0x75 => Some(4),
            0x55 => Some(8),
            _ => None,
        }) {
            let digits = &s[(i + 1).min(s.len())..(i + 1 + n).min(s.len())];
            let v = number::parse_int(&String::from_utf16_lossy(digits), 16);
            // String.fromCodePoint's RangeError, whose message Pele passes on.
            if v.is_nan() || v < 0.0 || v > 1114111.0 {
                return Err(error(format!(
                    "Invalid code point {}",
                    number::to_string(v)
                )));
            }
            let c = v as u32;
            match char::from_u32(c) {
                Some(c) => out.extend_from_slice(c.encode_utf16(&mut [0; 2])),
                None => out.push(c as u16),
            }
            i += n + 1;
        } else if let Some(c) = e.and_then(escape) {
            out.extend_from_slice(c.encode_utf16(&mut [0; 2]));
            i += 1;
        } else {
            return Err(error("YAML: unknown escape sequence"));
        }
    }
    Ok(String::from_utf16_lossy(&out))
}

/// Steps over blanks, line breaks and comments, and commas where commas is set, as Pele's
/// skip does.
fn skip(s: &str, mut pos: usize, commas: bool) -> usize {
    let b = s.as_bytes();
    while pos < b.len() {
        let c = b[pos];
        if c == b' ' || c == b'\n' || c == b'\t' || c == b'\r' || commas && c == b',' {
            pos += 1;
        } else if c == b'#' && (pos == 0 || space_before(s, pos)) {
            pos = s[pos..].find('\n').map_or(s.len(), |n| pos + n);
        } else {
            break;
        }
    }
    pos
}

/// Checks that an entry of a flow collection is set off from the one before by exactly one
/// comma, as js-yaml requires and Pele's entryStart checks.
fn entry_start(ch: u8, separated: bool) -> Read<()> {
    if !separated {
        return Err(error("YAML: missed comma between flow collection entries"));
    }
    if ch == b',' {
        return Err(error("YAML: expected the node content, but found ','"));
    }
    Ok(())
}

/// Reads a flow value from byte pos of s, as Pele's flow does, and gives it and where it
/// ends. A plain scalar ends at a character of stops.
fn flow(s: &str, pos: usize, stops: &str, depth: usize) -> Read<(Value, usize)> {
    if depth > MAX_DEPTH {
        return Err(error("YAML: nesting is too deep"));
    }
    let b = s.as_bytes();
    let mut pos = skip(s, pos, false);
    let ch = at(s, pos);
    if ch == b'{' || ch == b'[' {
        let object = ch == b'{';
        let close = if object { b'}' } else { b']' };
        let mut map = Map::new();
        let mut items = Vec::new();
        pos += 1;
        let mut separated = true;
        loop {
            pos = skip(s, pos, false);
            if pos >= b.len() {
                return Err(Fail::Incomplete(
                    "YAML: unexpected end of the stream within a flow collection",
                ));
            }
            if b[pos] == close {
                let value = if object {
                    Value::Object(map)
                } else {
                    Value::Array(items)
                };
                return Ok((value, pos + 1));
            }
            entry_start(b[pos], separated)?;
            if object {
                let (key, end) = flow(s, pos, ":,}", depth + 1)?;
                pos = skip(s, end, false);
                let mut value = Value::Null;
                if at(s, pos) == b':' {
                    (value, pos) = flow(s, pos + 1, ",}", depth + 1)?;
                }
                map.insert(key.to_js_string(), value);
            } else {
                let (value, end) = flow(s, pos, ",]", depth + 1)?;
                items.push(value);
                pos = end;
            }
            pos = skip(s, pos, false);
            separated = at(s, pos) == b',';
            if separated {
                pos += 1;
            }
        }
    }
    if ch == b'"' {
        let mut end = pos + 1;
        while end < b.len() && b[end] != b'"' {
            end += if b[end] == b'\\' { 2 } else { 1 };
        }
        if end >= b.len() {
            return Err(Fail::Incomplete(
                "YAML: unexpected end of the stream within a double quoted scalar",
            ));
        }
        return Ok((unquote_double(&s[pos + 1..end])?.into(), end + 1));
    }
    if ch == b'\'' {
        let mut end = pos + 1;
        while end < b.len() {
            if b[end] == b'\'' {
                if at(s, end + 1) != b'\'' {
                    break;
                }
                end += 1;
            }
            end += 1;
        }
        if end >= b.len() {
            return Err(Fail::Incomplete(
                "YAML: unexpected end of the stream within a single quoted scalar",
            ));
        }
        return Ok((fold(&s[pos + 1..end]).replace("''", "'").into(), end + 1));
    }
    let mut end = pos;
    while end < b.len() {
        let c = b[end];
        if stops.as_bytes().contains(&c)
            && (c != b':'
                || end + 1 >= b.len()
                || s[end + 1..]
                    .chars()
                    .next()
                    .is_some_and(|n| is_space_char(n) || ",[]{}".contains(n)))
        {
            break;
        }
        if c == b'#' && end > pos && space_before(s, end) {
            let nl = s[end..].find('\n').map_or(s.len(), |n| end + n);
            return Ok((scalar(&fold(trim(&s[pos..end]))), nl));
        }
        end += 1;
    }
    Ok((scalar(&fold(trim(&s[pos..end]))), end))
}

/// The line without its comment, if it has one outside quotes, as Pele's stripComment
/// takes it off.
fn strip_comment(s: &str) -> &str {
    let b = s.as_bytes();
    let mut quote = 0u8;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if quote != 0 {
            if c == b'\\' && quote == b'"' {
                i += 1;
            } else if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b'#' && (i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\t') {
            return &s[..i];
        }
        i += 1;
    }
    s
}

/// Reads a literal or folded block scalar under its header, as Pele's blockScalar does.
fn block_scalar(r: &mut Reader, header: &str, parent_indent: isize) -> String {
    let folded = header.starts_with('>');
    let chomp = if header.contains('-') {
        b'-'
    } else if header.contains('+') {
        b'+'
    } else {
        0
    };
    let explicit = header.bytes().find(|c| (b'1'..=b'9').contains(c));
    let mut indent = explicit.map_or(-1, |d| parent_indent + isize::from(d - b'0'));
    let mut out: Vec<String> = Vec::new();
    while r.i < r.lines.len() {
        let line = &r.lines[r.i];
        if trim(line).is_empty() {
            out.push(String::new());
            r.i += 1;
            continue;
        }
        let k = leading(line) as isize;
        if indent == -1 {
            if k <= parent_indent {
                break;
            }
            indent = k;
        }
        if k < indent {
            break;
        }
        out.push(drop_units(line, indent as usize).to_string());
        r.i += 1;
    }
    let mut trailing = 0;
    while out.last().is_some_and(String::is_empty) {
        out.pop();
        trailing += 1;
    }
    let text = if folded {
        let starts_blank = |s: &str| s.starts_with([' ', '\t']);
        let mut text = String::new();
        for (i, cur) in out.iter().enumerate() {
            if i > 0 {
                let prev = &out[i - 1];
                if cur.is_empty() {
                    text.push('\n');
                } else if !prev.is_empty() {
                    text.push(if starts_blank(prev) || starts_blank(cur) {
                        '\n'
                    } else {
                        ' '
                    });
                }
            }
            text.push_str(cur);
        }
        text
    } else {
        out.join("\n")
    };
    if out.is_empty() {
        return if chomp == b'+' {
            "\n".repeat(trailing)
        } else {
            String::new()
        };
    }
    match chomp {
        b'-' => text,
        b'+' => text + &"\n".repeat(trailing + 1),
        _ => text + "\n",
    }
}

/// `/^[|>][-+0-9]*\s*(?:#.*)?$/`: the header of a block scalar, with a comment after it. No
/// backtracking finds a match the greedy runs miss.
fn is_block_header(rest: &str) -> bool {
    let b = rest.as_bytes();
    if !matches!(b.first(), Some(b'|' | b'>')) {
        return false;
    }
    let i = 1 + b[1..]
        .iter()
        .take_while(|&&c| c == b'-' || c == b'+' || c.is_ascii_digit())
        .count();
    let rest = trim_start(&rest[i..]);
    rest.is_empty() || rest.starts_with('#') && !rest.contains(['\n', '\r', '\u{2028}', '\u{2029}'])
}

/// Reads the value that follows a key or a dash on its line, and on the lines after it that
/// it runs on to, as Pele's inlineValue does.
fn inline_value(r: &mut Reader, rest: &str, indent: isize) -> Read<Value> {
    let first = at(rest, 0);
    if (first == b'|' || first == b'>') && is_block_header(rest) {
        return Ok(block_scalar(r, rest, indent).into());
    }
    if matches!(first, b'{' | b'[' | b'"' | b'\'') {
        // The value may run on to later lines. They are read in growing batches until it
        // closes.
        let start = r.i;
        let mut text = rest.to_string();
        let mut taken = 0;
        let mut batch = 1;
        loop {
            match flow(&text, 0, "", 0) {
                Ok((value, end)) => {
                    let used = text[..end].matches('\n').count();
                    let line_end = text[end..].find('\n').map_or(text.len(), |n| end + n);
                    if !trim(strip_comment(&text[end..line_end])).is_empty() {
                        return Err(error("YAML: unexpected content after a value"));
                    }
                    r.i = start + used;
                    return Ok(value);
                }
                Err(Fail::Incomplete(message)) => {
                    if start + taken >= r.lines.len() {
                        return Err(error(message));
                    }
                    let more = &r.lines[start + taken..(start + taken + batch).min(r.lines.len())];
                    text.push('\n');
                    text.push_str(&more.join("\n"));
                    taken += more.len();
                    batch *= 2;
                }
                Err(e) => return Err(e),
            }
        }
    }
    let mut text = trim(strip_comment(rest)).to_string();
    while r.i < r.lines.len() {
        let line = &r.lines[r.i];
        if trim(line).is_empty() {
            let mut j = r.i;
            while j < r.lines.len() && trim(&r.lines[j]).is_empty() {
                j += 1;
            }
            if j >= r.lines.len() || leading(&r.lines[j]) as isize <= indent {
                break;
            }
            text.push_str(&"\n".repeat(j - r.i));
            r.i = j;
            continue;
        }
        if leading(line) as isize <= indent {
            break;
        }
        let more = trim(strip_comment(line));
        if !text.ends_with('\n') {
            text.push(' ');
        }
        text.push_str(more);
        r.i += 1;
    }
    Ok(scalar(&text))
}

/// Whether a line, past its indent, is an item of a block sequence.
fn is_item(line: &str) -> bool {
    line == "-" || line.starts_with("- ")
}

/// Reads a block collection, or a scalar, from the line being read, indented by at least
/// min_indent, as Pele's block does.
fn block(r: &mut Reader, min_indent: usize, depth: usize) -> Read<Value> {
    if depth > MAX_DEPTH {
        return Err(error("YAML: nesting is too deep"));
    }
    r.skip_blank();
    if r.i >= r.lines.len() {
        return Ok(Value::Null);
    }
    let indent = r.indent()?;
    if indent < min_indent {
        return Ok(Value::Null);
    }
    let line = r.lines[r.i][indent..].to_string();

    if is_item(&line) {
        let mut out = Vec::new();
        while r.i < r.lines.len() {
            r.skip_blank();
            if r.i >= r.lines.len() || r.indent()? != indent {
                break;
            }
            let cur = r.lines[r.i][indent..].to_string();
            if !is_item(&cur) {
                break;
            }
            let rest = &cur[1..];
            let pad = leading(rest) + 1;
            let body = trim(strip_comment(rest));
            if body.is_empty() {
                r.i += 1;
                out.push(block(r, indent + 1, depth + 1)?);
            } else if split_key(body).is_some() {
                r.lines[r.i] = " ".repeat(indent + pad) + trim_start(rest);
                out.push(block(r, indent + pad, depth + 1)?);
            } else {
                r.i += 1;
                out.push(inline_value(r, trim(rest), indent as isize)?);
            }
        }
        return Ok(Value::Array(out));
    }

    if split_key(&line).is_none() {
        r.i += 1;
        return inline_value(r, trim(&line), indent as isize - 1);
    }

    let mut out = Map::new();
    while r.i < r.lines.len() {
        r.skip_blank();
        if r.i >= r.lines.len() {
            break;
        }
        let k = r.indent()?;
        if k < indent {
            break;
        }
        if k > indent {
            return Err(error("YAML: bad indentation of a mapping entry"));
        }
        let cur = r.lines[r.i][indent..].to_string();
        let Some((raw, length)) = split_key(&cur) else {
            return Err(error("YAML: can not read a block mapping entry"));
        };
        let key = flow(raw, 0, "", 0)?.0.to_js_string();
        let rest = &cur[length..];
        r.i += 1;
        let value = if trim(strip_comment(rest)).is_empty() {
            r.skip_blank();
            if r.i < r.lines.len() {
                let next = r.indent()?;
                let seq = is_item(&r.lines[r.i][next..]);
                if next > indent || seq && next == indent {
                    block(r, if seq { indent } else { indent + 1 }, depth + 1)?
                } else {
                    Value::Null
                }
            } else {
                Value::Null
            }
        } else {
            inline_value(r, trim(rest), indent as isize)?
        };
        out.insert(key, value);
    }
    Ok(Value::Object(out))
}

/// Reads YAML, as Pele's parseYaml does. An error's message is the one Pele's error has.
pub(crate) fn parse_yaml(src: &str) -> Result<Value, YamlError> {
    read(src).map_err(|f| YamlError(f.message()))
}

fn read(src: &str) -> Read<Value> {
    let trimmed = trim(src);
    if trimmed.starts_with(['{', '[']) {
        let (value, end) = flow(trimmed, 0, "", 0)?;
        if skip(trimmed, end, false) == trimmed.len() {
            return Ok(value);
        }
    }
    let mut r = Reader::new(src);
    let value = block(&mut r, 0, 0)?;
    r.skip_blank();
    if r.i < r.lines.len() && trim(&r.lines[r.i]) != "..." {
        return Err(error(
            "YAML: end of the stream or a document separator is expected",
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests;
