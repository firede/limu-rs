//! What every diagram's text goes through before its parser reads it, as Pele's
//! src/preprocess.ts does it: its front matter, its directives and its comment lines are
//! read and taken out, and its entity codes hidden.

use crate::errors::{Edit, Error, ErrorCode, Steps, push};
use crate::js::chars::{
    is_line_end, is_space, is_word, space_run, trim, trim_start, unit_at, unit_before,
};
use crate::js::json::parse_json;
use crate::value::{Map, Value};
use crate::yaml::parse_yaml;

/// A diagram's text read for its front matter and directives, as Pele's preprocess gives it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Preprocessed {
    /// The text after its front matter, with its directives and comment lines taken out and
    /// the white space it starts with trimmed.
    pub(crate) text: String,
    /// The title the front matter gives.
    pub(crate) title: Option<String>,
    pub(crate) config: Map,
    /// The text before its comment lines are taken out and it is trimmed.
    pub(crate) with_comments: String,
    /// The number of lines of front matter before the text.
    pub(crate) line_offset: usize,
}

/// The keys that would reach an object's prototype, which merge leaves out.
const UNSAFE_KEYS: [&str; 3] = ["__proto__", "constructor", "prototype"];

/// Takes out the lines that are comments, `%%` and not `%%{`, and the white space the text
/// starts with, as Pele's cleanupComments does.
pub(crate) fn cleanup_comments(text: &str, mut steps: Steps) -> String {
    let dropped;
    let text = if text.contains("%%") {
        dropped = drop_comments(text, &mut steps);
        dropped.as_str()
    } else {
        text
    };
    let trimmed = trim_start(text);
    push(&mut steps, vec![Edit::cut(0, text.len() - trimmed.len())]);
    trimmed.to_string()
}

/// Writes each CRLF and each lone CR as a line feed, as Pele's normalizeLineBreaks does.
pub(crate) fn normalize_line_breaks(text: &str) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.to_string()
    }
}

/// `text.replace(/^\s*%%(?!{)[^\n]+\n?/gm, "")`, Pele's removeAll of RE_COMMENT: at the start
/// of a line, white space over any number of lines, `%%` not followed by `{`, at least one
/// character up to the line feed, and the line feed.
fn drop_comments(text: &str, steps: &mut Steps) -> String {
    let mut out = String::new();
    let mut edits = Vec::new();
    let (mut last, mut i) = (0, 0);
    while i < text.len() {
        let line_start = i == 0 || is_line_end(unit_before(text, i));
        if line_start && let Some(end) = comment_at(text, i) {
            out.push_str(&text[last..i]);
            edits.push(Edit::cut(i, end));
            (last, i) = (end, end);
            continue;
        }
        i += unit_at(text, i).1;
    }
    push(steps, edits);
    out.push_str(&text[last..]);
    out
}

/// Where a comment that starts at byte i of text ends, if one does.
fn comment_at(text: &str, i: usize) -> Option<usize> {
    let j = space_run(text, i);
    if !text[j..].starts_with("%%") {
        return None;
    }
    let j = j + 2;
    if matches!(text.as_bytes().get(j), None | Some(b'{' | b'\n')) {
        return None;
    }
    Some(text[j..].find('\n').map_or(text.len(), |n| j + n + 1))
}

fn is_line_break(b: Option<u8>) -> bool {
    matches!(b, Some(b'\n' | b'\r'))
}

/// A `---` fenced YAML block at the start of a text.
struct FrontMatter<'a> {
    indent: &'a str,
    body: &'a str,
    length: usize,
}

/// Finds a `---` fenced YAML block at the start of the text the way Mermaid's front matter
/// regular expression does, as Pele's matchFrontMatter does.
fn match_front_matter(text: &str) -> Option<FrontMatter<'_>> {
    let b = text.as_bytes();
    // Whether byte i starts white space, and the bytes of the character there.
    let space = |i: usize| {
        let (c, size) = unit_at(text, i);
        (is_space(c), size)
    };
    let mut cursor = 0;
    while cursor < b.len() {
        let (ok, size) = space(cursor);
        if !ok || is_line_break(Some(b[cursor])) {
            break;
        }
        cursor += size;
    }
    let indent = &text[..cursor];
    if !text[cursor..].starts_with("---") {
        return None;
    }

    let mut opening_ends = Vec::new();
    let mut i = cursor + 3;
    while i < b.len() {
        let (ok, size) = space(i);
        if !ok {
            break;
        }
        if is_line_break(Some(b[i])) {
            opening_ends.push(i);
        }
        i += size;
    }
    let &first = opening_ends.first()?;

    let fence = format!("{indent}---");
    let closing_end = |i: usize| -> Option<usize> {
        if !is_line_break(b.get(i).copied()) || !text[i + 1..].starts_with(&fence) {
            return None;
        }
        let mut last = None;
        let mut j = i + 1 + fence.len();
        while j < b.len() {
            let (ok, size) = space(j);
            if !ok {
                break;
            }
            if is_line_break(Some(b[j])) {
                last = Some(j);
            }
            j += size;
        }
        last
    };

    let needle = format!("\n{fence}");
    let mut last_fence = None;
    let mut at = text.rfind(&needle);
    while let Some(i) = at.filter(|&i| i > first) {
        if closing_end(i).is_some() {
            last_fence = Some(i);
            break;
        }
        // text.lastIndexOf(needle, i - 1): the last that starts before i.
        at = text[..i - 1 + needle.len()].rfind(&needle);
    }
    let last_fence = last_fence?;

    let opening_end = opening_ends
        .iter()
        .copied()
        .rfind(|&c| c < last_fence)
        .unwrap_or(first);
    let body_start = opening_end + 1;
    let mut at = text[body_start - 1..]
        .find(&needle)
        .map(|n| body_start - 1 + n);
    while let Some(i) = at.filter(|&i| i <= last_fence) {
        if i >= body_start
            && let Some(end) = closing_end(i)
        {
            return Some(FrontMatter {
                indent,
                body: &text[body_start..i],
                length: end + 1,
            });
        }
        at = text[i + 1..].find(&needle).map(|n| i + 1 + n);
    }
    None
}

/// Lays source over target, objects into objects, as Pele's merge does, leaving out the keys
/// that would reach an object's prototype.
fn merge(target: &mut Map, source: &Map) {
    for (key, value) in source {
        if UNSAFE_KEYS.contains(&key) {
            continue;
        }
        match (value, target.get_mut(key)) {
            (Value::Object(value), Some(Value::Object(current))) => merge(current, value),
            (Value::Object(value), _) => {
                let mut fresh = Map::new();
                merge(&mut fresh, value);
                target.insert(key, Value::Object(fresh));
            }
            _ => target.insert(key, value.clone()),
        }
    }
}

/// Matches Pele's RE_DIRECTIVE at byte i of s,
/// `/%{2}{\s*(?:(\w+)\s*:|(\w+))\s*(?:(\w+)|((?:(?!}%{2}).|\r?\n)*))?\s*(?:}%{2})?/`, and
/// gives the end of the match, its type, and the body it gives an init directive.
fn directive_at(s: &str, i: usize) -> Option<(usize, &str, &str)> {
    if !s[i..].starts_with("%%{") {
        return None;
    }
    let b = s.as_bytes();
    let word_end = |from: usize| {
        from + b[from..]
            .iter()
            .take_while(|&&c| is_word(i32::from(c)))
            .count()
    };
    let j = space_run(s, i + 3);
    let k = word_end(j);
    if k == j {
        return None;
    }
    let kind = &s[j..k];
    let c = space_run(s, k);
    let mut j = if b.get(c) == Some(&b':') { c + 1 } else { k };
    j = space_run(s, j);
    let mut body = "";
    if b.get(j).is_some_and(|&c| is_word(i32::from(c))) {
        j = word_end(j);
    } else {
        let from = j;
        while j < b.len() && !s[j..].starts_with("}%%") {
            let (c, size) = unit_at(s, j);
            if !is_line_end(c) {
                j += size;
            } else if b[j] == b'\n' {
                j += 1;
            } else if s[j..].starts_with("\r\n") {
                j += 2;
            } else {
                break;
            }
        }
        body = &s[from..j];
    }
    j = space_run(s, j);
    if s[j..].starts_with("}%%") {
        j += 3;
    }
    Some((j, kind, body))
}

/// Reads the directives of text into config, as Pele's directives does, and gives the text
/// without them.
fn directives(text: &str, config: &mut Map, steps: &mut Steps) -> String {
    if !text.contains("%%{") {
        return text.to_string();
    }
    let normalized = trim(text).replace('\'', "\"");
    let mut i = 0;
    while let Some(n) = normalized[i..].find("%%{") {
        let at = i + n;
        let Some((end, kind, body)) = directive_at(&normalized, at) else {
            i = at + 1;
            continue;
        };
        if kind == "wrap" {
            config.insert("wrap", Value::Bool(true));
        } else if (kind == "init" || kind == "initialize") && !body.is_empty() {
            // Mermaid ignores an init directive whose body is not valid JSON.
            if let Some(Value::Object(args)) = parse_json(trim(body)) {
                merge(config, &args);
            }
        }
        i = end;
    }
    let mut out = String::new();
    let mut edits = Vec::new();
    let (mut last, mut i) = (0, 0);
    while let Some(n) = text[i..].find("%%{") {
        let at = i + n;
        match directive_at(text, at) {
            Some((end, _, _)) => {
                out.push_str(&text[last..at]);
                edits.push(Edit::cut(at, end));
                (last, i) = (end, end);
            }
            None => i = at + 1,
        }
    }
    push(steps, edits);
    out.push_str(&text[last..]);
    out
}

/// `text.replace(/<(\w+)([^>]*)>/g, …)`, which writes the double-quoted values of a tag's
/// attributes in single quotes, for a text whose last character is the last `>`.
fn html_tags(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = String::new();
    let (mut last, mut i) = (0, 0);
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let j = i
            + 1
            + b[i + 1..]
                .iter()
                .take_while(|&&c| is_word(i32::from(c)))
                .count();
        let close = text[j..].find('>').map(|n| j + n);
        match close {
            Some(close) if j > i + 1 => {
                out.push_str(&text[last..j]);
                out.push_str(&single_quoted(&text[j..close]));
                out.push('>');
                last = close + 1;
                i = close + 1;
            }
            _ => i += 1,
        }
    }
    out.push_str(&text[last..]);
    out
}

/// `attrs.replace(/="([^"]*)"/g, "='$1'")`.
fn single_quoted(attrs: &str) -> String {
    let mut out = String::new();
    let (mut last, mut i) = (0, 0);
    while let Some(n) = attrs[i..].find("=\"") {
        let at = i + n;
        match attrs[at + 2..].find('"') {
            Some(m) => {
                let end = at + 2 + m;
                out.push_str(&attrs[last..at]);
                out.push_str("='");
                out.push_str(&attrs[at + 2..end]);
                out.push('\'');
                last = end + 1;
                i = end + 1;
            }
            None => i = at + 1,
        }
    }
    out.push_str(&attrs[last..]);
    out
}

/// Reads a diagram's front matter and directives and takes out its comment lines, as Pele's
/// preprocess does. Steps, where they are recorded, map the text before its comments are
/// taken out back to the source, once its line breaks are normalized.
pub(crate) fn preprocess(source: &str, mut steps: Steps) -> Result<Preprocessed, Error> {
    let mut text = normalize_line_breaks(source);
    // A tag needs a closing bracket, so nothing after the last one can match.
    if text.contains('<')
        && let Some(last_close) = text.rfind('>').filter(|&n| n > 0)
    {
        text = html_tags(&text[..=last_close]) + &text[last_close + 1..];
    }

    let mut config = Map::new();
    let mut title = None;
    let mut line_offset = 0;
    let front = if text.contains("---") {
        match_front_matter(&text)
    } else {
        None
    };
    if let Some(front) = front {
        let body = if front.indent.is_empty() {
            front.body.to_string()
        } else {
            front
                .body
                .split('\n')
                .map(|line| line.strip_prefix(front.indent).unwrap_or(line))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let parsed = parse_yaml(&body).map_err(|e| {
            Error::new(
                format!("Front matter is not valid YAML. {}", e.0),
                ErrorCode::Syntax,
            )
        })?;
        if let Value::Object(parsed) = parsed {
            if let Some(t) = parsed.get("title").filter(|t| t.truthy()) {
                title = Some(t.to_js_string());
            }
            if let Some(Value::Object(c)) = parsed.get("config") {
                merge(&mut config, c);
            }
            if let Some(mode) = parsed.get("displayMode").filter(|m| m.truthy()) {
                let mode = Value::String(mode.to_js_string());
                match config.get_mut("gantt") {
                    Some(Value::Object(gantt)) => gantt.insert("displayMode", mode),
                    _ => config.insert(
                        "gantt",
                        Value::Object([("displayMode", mode)].into_iter().collect()),
                    ),
                }
            }
        }
        line_offset = text[..front.length].matches('\n').count();
        let length = front.length;
        text = text[length..].to_string();
        push(&mut steps, vec![Edit::cut(0, length)]);
    }

    let text = directives(&text, &mut config, &mut steps);
    Ok(Preprocessed {
        text: cleanup_comments(&text, None),
        title,
        config,
        with_comments: text,
        line_offset,
    })
}

/// Does what Mermaid's `text.replace(/keyword.*:\S*#.*;/g, drop the last character)` does,
/// as Pele's dropColorSemicolons does: on a line where the keyword is followed by a colon, a
/// color-like `#`, and later a semicolon, the last semicolon goes. Mermaid's expression
/// backtracks without bound, and this is one pass.
fn drop_color_semicolons(text: String, keyword: &str, steps: &mut Steps) -> String {
    let mut edits = Vec::new();
    let mut from = text.find(keyword);
    let mut out = String::new();
    let mut last = 0;
    while let Some(start) = from {
        let mut line_end = (text.len(), 0);
        let mut j = start;
        while j < text.len() {
            let (c, n) = unit_at(&text, j);
            if is_line_end(c) {
                line_end = (j, n);
                break;
            }
            j += n;
        }
        let (end, size) = line_end;
        let mut after_colon = false;
        let mut hash = None;
        let mut j = start + keyword.len();
        while j < end {
            let (c, n) = unit_at(&text, j);
            if c == i32::from(b':') {
                after_colon = true;
            } else if c == i32::from(b'#') {
                if after_colon {
                    hash = Some(j);
                    break;
                }
            } else if is_space(c) {
                after_colon = false;
            }
            j += n;
        }
        if let Some(hash) = hash
            && let Some(semi) = text[..end].rfind(';').filter(|&s| s > hash)
        {
            edits.push(Edit::cut(semi, semi + 1));
            out.push_str(&text[last..semi]);
            last = semi + 1;
        }
        from = if end >= text.len() {
            None
        } else {
            text[end + size..].find(keyword).map(|n| end + size + n)
        };
    }
    push(steps, edits);
    if last == 0 {
        return text;
    }
    out.push_str(&text[last..]);
    out
}

/// Hides `#name;` entity codes behind placeholder characters so that `#` and `;` survive
/// parsing, as Mermaid does and Pele's encodeEntities does.
pub(crate) fn encode_entities(text: &str, mut steps: Steps) -> String {
    if !text.contains('#') {
        return text.to_string();
    }
    let text = drop_color_semicolons(text.to_string(), "style", &mut steps);
    let text = drop_color_semicolons(text, "classDef", &mut steps);
    let b = text.as_bytes();
    let mut edits = Vec::new();
    let mut out = String::new();
    let (mut last, mut i) = (0, 0);
    while let Some(n) = text[i..].find('#') {
        let at = i + n;
        let j = at
            + 1
            + b[at + 1..]
                .iter()
                .take_while(|&&c| is_word(i32::from(c)))
                .count();
        if j == at + 1 || b.get(j) != Some(&b';') {
            i = at + 1;
            continue;
        }
        let inner = &text[at + 1..j];
        let code = if inner.bytes().all(|c| c.is_ascii_digit()) {
            format!("ﬂ°°{inner}¶ß")
        } else {
            format!("ﬂ°{inner}¶ß")
        };
        out.push_str(&text[last..at]);
        out.push_str(&code);
        edits.push(Edit {
            from: at,
            to: j + 1,
            text: code,
        });
        last = j + 1;
        i = j + 1;
    }
    push(&mut steps, edits);
    out.push_str(&text[last..]);
    out
}

#[cfg(test)]
mod tests;
