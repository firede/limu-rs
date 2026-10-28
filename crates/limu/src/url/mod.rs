//! The URLs diagrams link to and show, checked as Pele's src/util/url.ts checks them:
//! sanitize_url refuses what @braintree/sanitize-url refuses, which Mermaid applies to click
//! targets, link_url and image_url keep a URL only where the host allows its scheme, and
//! internal_link checks the name of a note that is linked to.
//!
//! Pele's linked and relAttr, which write a link's markup about what a node draws, are left
//! out. The scene carries the link, and all they decide is in link_url.

mod whatwg;

#[cfg(test)]
mod tests;

use crate::js::chars::{is_hex, is_line_end, is_word, trim, trim_start, unit_at};
use crate::js::number;
use crate::text::decode_entities;

/// What sanitize_url gives for a URL it refuses, as Pele's BLANK is.
pub(crate) const BLANK: &str = "about:blank";

/// What the host lets links and images be, as Pele's RenderOptions has it in links, images,
/// linkSchemes and imageSchemes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Schemes<'a> {
    pub(crate) links: bool,
    pub(crate) images: bool,
    /// The schemes a link may have besides a relative address.
    pub(crate) link_schemes: &'a [&'a str],
    /// The schemes an image may have besides a relative address.
    pub(crate) image_schemes: &'a [&'a str],
}

/// The schemes a link and an image may have when the host names none, as Pele's
/// LINK_SCHEMES and IMAGE_SCHEMES have them.
pub(crate) const LINK_SCHEMES: &[&str] = &["http", "https", "mailto", "tel"];
pub(crate) const IMAGE_SCHEMES: &[&str] = &["http", "https"];

impl Default for Schemes<'_> {
    fn default() -> Self {
        Schemes {
            links: true,
            images: true,
            link_schemes: LINK_SCHEMES,
            image_schemes: IMAGE_SCHEMES,
        }
    }
}

/// decodeURIComponent, or uri itself where that throws, as Pele's decode gives it.
fn decode(uri: &str) -> String {
    if !uri.contains('%') {
        return uri.to_string();
    }
    let b = uri.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut k = 0;
    while k < b.len() {
        if b[k] != b'%' {
            out.push(b[k]);
            k += 1;
            continue;
        }
        let Some(first) = hex_byte(b, k) else {
            return uri.to_string();
        };
        k += 3;
        if first < 0x80 {
            out.push(first);
            continue;
        }
        let n = first.leading_ones() as usize;
        if n == 1 || n > 4 {
            return uri.to_string();
        }
        let mut seq = vec![first];
        for _ in 1..n {
            if b.get(k) != Some(&b'%') {
                return uri.to_string();
            }
            match hex_byte(b, k) {
                Some(c) if c & 0xc0 == 0x80 => seq.push(c),
                _ => return uri.to_string(),
            }
            k += 3;
        }
        // An overlong sequence, a surrogate or a code point past U+10FFFF throws.
        if std::str::from_utf8(&seq).is_err() {
            return uri.to_string();
        }
        out.extend_from_slice(&seq);
    }
    // Each byte outside an escape is copied with the rest of its character, and each escaped
    // sequence is whole, so the bytes are UTF-8.
    String::from_utf8(out).unwrap_or_else(|_| uri.to_string())
}

/// The byte two hexadecimal digits after the `%` at b[k] write.
fn hex_byte(b: &[u8], k: usize) -> Option<u8> {
    let (h, l) = (*b.get(k + 1)?, *b.get(k + 2)?);
    if !is_hex(i32::from(h)) || !is_hex(i32::from(l)) {
        return None;
    }
    let v = |c: u8| (c as char).to_digit(16).unwrap_or(0) as u8;
    Some(v(h) << 4 | v(l))
}

/// Whether a code point is in Pele's RE_CTRL,
/// `/[\u0000-\u001F\u007F-\u009F -‍﻿\uD800-\uDFFF￾￿]/gimu`, but for
/// the lone surrogates, which a Rust string cannot hold. Only an entity code makes one, and
/// read_entities takes it out.
fn is_ctrl(c: u32) -> bool {
    matches!(c, 0..=0x1f | 0x7f..=0x9f | 0x2000..=0x200d | 0xfeff | 0xfffe | 0xffff)
}

fn has_ctrl(s: &str) -> bool {
    s.chars().any(|c| is_ctrl(c.into()))
}

fn drop_ctrl(s: &str) -> String {
    s.chars().filter(|&c| !is_ctrl(c.into())).collect()
}

/// Matches Pele's RE_HTML_ENTITIES, `/&#(\w+)(^\w|;)?/`, at byte i of s, where `^\w` can
/// never match: the end of the match and its `\w+`.
fn entity_at(s: &str, i: usize) -> Option<(usize, &str)> {
    if !s[i..].starts_with("&#") {
        return None;
    }
    let b = s.as_bytes();
    let j = i
        + 2
        + b[i + 2..]
            .iter()
            .take_while(|&&c| is_word(i32::from(c)))
            .count();
    if j == i + 2 {
        return None;
    }
    let digits = &s[i + 2..j];
    Some((if b.get(j) == Some(&b';') { j + 1 } else { j }, digits))
}

fn has_entity(s: &str) -> bool {
    s.match_indices("&#")
        .any(|(i, _)| entity_at(s, i).is_some())
}

/// JavaScript's ToUint16.
fn to_uint16(f: f64) -> u16 {
    if !f.is_finite() {
        return 0;
    }
    f.trunc().rem_euclid(65536.0) as u16
}

/// Replaces each match of RE_HTML_ENTITIES with String.fromCharCode of its digits, as Pele's
/// unwrap does. It gives code units, as two of them may make a pair of surrogates and one may
/// be half of a pair.
fn decode_entity_codes(s: &str) -> Vec<u16> {
    let mut out = Vec::new();
    let (mut last, mut i) = (0, 0);
    while let Some(n) = s[i..].find("&#") {
        let at = i + n;
        match entity_at(s, at) {
            Some((end, digits)) => {
                out.extend(s[last..at].encode_utf16());
                out.push(to_uint16(number::number(digits)));
                (last, i) = (end, end);
            }
            None => i = at + 1,
        }
    }
    out.extend(s[last..].encode_utf16());
    out
}

/// Matches Pele's RE_HTML_CTRL_ENTITY, `/&(newline|tab);/i`, at unit i of u, and gives where
/// the match ends. The i flag without u folds ASCII only.
fn ctrl_entity_at(u: &[u16], i: usize) -> Option<usize> {
    if u[i] != u16::from(b'&') {
        return None;
    }
    ["newline", "tab"].iter().find_map(|name| {
        let end = i + 1 + name.len();
        let word = u.get(i + 1..end)?;
        let same = word
            .iter()
            .zip(name.bytes())
            .all(|(&c, n)| u8::try_from(c).is_ok_and(|c| c.to_ascii_lowercase() == n));
        (same && u.get(end) == Some(&u16::from(b';'))).then_some(end + 1)
    })
}

fn has_ctrl_entity(s: &str) -> bool {
    let u: Vec<u16> = s.encode_utf16().collect();
    (0..u.len()).any(|i| ctrl_entity_at(&u, i).is_some())
}

/// Makes three of a round of Pele's unwrap's replacements: RE_HTML_ENTITIES,
/// RE_HTML_CTRL_ENTITY and RE_CTRL. It makes them in code units, since an entity code may
/// give half of a pair of surrogates. Its other half may follow at once, or once a
/// `&newline;` or `&tab;` between them is gone, and otherwise RE_CTRL, with its u flag, takes
/// the lone half out.
fn read_entities(s: &str) -> String {
    let u = decode_entity_codes(s);
    let mut kept = Vec::with_capacity(u.len());
    let mut i = 0;
    while i < u.len() {
        match ctrl_entity_at(&u, i) {
            Some(end) => i = end,
            None => {
                kept.push(u[i]);
                i += 1;
            }
        }
    }
    let mut out = Vec::with_capacity(kept.len());
    let mut i = 0;
    while i < kept.len() {
        let c = kept[i];
        if (0xd800..0xdc00).contains(&c)
            && kept
                .get(i + 1)
                .is_some_and(|d| (0xdc00..0xe000).contains(d))
        {
            out.extend_from_slice(&kept[i..i + 2]);
            i += 2;
            continue;
        }
        if !(0xd800..0xe000).contains(&c) && !is_ctrl(u32::from(c)) {
            out.push(c);
        }
        i += 1;
    }
    String::from_utf16(&out).unwrap_or_default()
}

/// Matches Pele's RE_WS_ESCAPE, `/(\\|%5[cC])((%(6[eE]|72|74))|[nrt])/`, at byte i of s, and
/// gives where the match ends.
fn ws_escape_at(s: &str, i: usize) -> Option<usize> {
    let rest = &s[i..];
    let j = if rest.starts_with('\\') {
        i + 1
    } else if rest.starts_with("%5c") || rest.starts_with("%5C") {
        i + 3
    } else {
        return None;
    };
    let after = &s[j..];
    if after.starts_with(['n', 'r', 't']) {
        return Some(j + 1);
    }
    ["%6e", "%6E", "%72", "%74"]
        .iter()
        .any(|e| after.starts_with(e))
        .then_some(j + 3)
}

/// Each place a match of RE_WS_ESCAPE could start, `\` or `%`.
fn ws_escape_starts(s: &str) -> impl Iterator<Item = usize> + '_ {
    s.match_indices(['\\', '%']).map(|(i, _)| i)
}

fn has_ws_escape(s: &str) -> bool {
    ws_escape_starts(s).any(|i| ws_escape_at(s, i).is_some())
}

/// `s.replace(RE_WS_ESCAPE, '')`.
fn drop_ws_escapes(s: &str) -> String {
    let mut out = String::new();
    let mut last = 0;
    for i in ws_escape_starts(s) {
        if i < last {
            continue;
        }
        if let Some(end) = ws_escape_at(s, i) {
            out.push_str(&s[last..i]);
            last = end;
        }
    }
    out.push_str(&s[last..]);
    out
}

/// Takes out the control characters, entity codes and escapes that could hide a scheme, and
/// with percent, percent escapes too, as Pele's unwrap does. None for a URL wrapped too many
/// times to be a real one.
fn unwrap(url: &str, percent: bool) -> Option<String> {
    let mut out = if percent {
        decode(trim(url))
    } else {
        trim(url).to_string()
    };
    for _ in 0..32 {
        out = drop_ctrl(&out);
        out = read_entities(&out);
        out = drop_ws_escapes(&out);
        out = trim(&out).to_string();
        if percent {
            out = decode(&out);
        }
        if !has_ctrl(&out) && !has_entity(&out) && !has_ctrl_entity(&out) && !has_ws_escape(&out) {
            return Some(out);
        }
    }
    None
}

/// The first match of Pele's RE_SCHEME, `/^.+(:|&colon;)/gim`: from the start of a line, as
/// much of it as runs up to its last colon.
fn scheme_of(s: &str) -> Option<&str> {
    let mut start = 0;
    loop {
        let mut end = start;
        while end < s.len() {
            let (c, size) = unit_at(s, end);
            if is_line_end(c) {
                break;
            }
            end += size;
        }
        // `.+` takes all it can and gives it back until a colon or `&colon;` follows, and
        // takes at least one character.
        let line = &s[start..end];
        for x in (1..line.len()).rev() {
            if line.as_bytes()[x] == b':' {
                return Some(&line[..=x]);
            }
            if line.as_bytes()[x..].starts_with(b"&colon;") {
                return Some(&line[..x + 7]);
            }
        }
        if end >= s.len() {
            return None;
        }
        start = end + unit_at(s, end).1;
    }
}

/// Whether a protocol, which has no line breaks, matches Pele's RE_INVALID_PROTOCOL,
/// `/^([^\w]*)(javascript|data|vbscript)/im`. The i flag without u folds ASCII only.
fn invalid_protocol(p: &str) -> bool {
    let rest = p.trim_start_matches(|c: char| !c.is_ascii() || !is_word(c as i32));
    ["javascript", "data", "vbscript"].iter().any(|name| {
        rest.len() >= name.len()
            && rest.as_bytes()[..name.len()].eq_ignore_ascii_case(name.as_bytes())
    })
}

/// Refuses what @braintree/sanitize-url refuses, the check Mermaid applies to click targets,
/// as Pele's sanitizeUrl does. Unlike it, it keeps percent escapes, so `file=A%26B` still
/// names the note `A&B`.
pub(crate) fn sanitize_url(url: &str) -> String {
    if url.is_empty() {
        return BLANK.into();
    }
    let decoded = unwrap(url, true);
    let kept = unwrap(url, false);
    let (Some(decoded), Some(kept)) = (decoded, kept) else {
        return BLANK.into();
    };
    if decoded.is_empty() || kept.is_empty() {
        return BLANK.into();
    }
    if decoded.starts_with(['.', '/']) {
        return kept;
    }

    let Some(scheme) = scheme_of(trim_start(&decoded)) else {
        return kept;
    };
    let protocol = crate::js::chars::to_lower(scheme);
    let protocol = trim(&protocol);
    if invalid_protocol(protocol) {
        return BLANK.into();
    }

    let normalized = trim_start(&kept).replace('\\', "/");
    if protocol == "mailto:" || protocol.contains("://") {
        return normalized;
    }
    if protocol == "http:" || protocol == "https:" {
        // new URL(normalized), with its protocol and hostname lowered, which the parser has
        // lowered already.
        return whatwg::parse_http(&normalized).unwrap_or_else(|| BLANK.into());
    }
    normalized
}

/// A URL as written in a diagram, checked, as Pele's safeUrl does: entity codes are read
/// first, so that they cannot hide a scheme.
pub(crate) fn safe_url(url: &str) -> String {
    sanitize_url(&decode_entities(url))
}

/// Whether a URL is relative or its scheme is one of schemes, as Pele's allowed tests it.
/// It is stricter than Mermaid, which lets any scheme but three through: a diagram in a
/// shared note could otherwise open a local file, a network share, or another application.
fn allowed(url: &str, schemes: &[&str]) -> bool {
    // RE_NETWORK_PATH, /^[\\/]{2}/.
    let b = url.as_bytes();
    if b.len() >= 2 && matches!(b[0], b'/' | b'\\') && matches!(b[1], b'/' | b'\\') {
        return false;
    }
    // RE_SCHEME_NAME, /^([a-z][a-z0-9+.-]*):/i.
    if !b.first().is_some_and(u8::is_ascii_alphabetic) {
        return true;
    }
    let n = 1 + b[1..]
        .iter()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'.' | b'-'))
        .count();
    if b.get(n) != Some(&b':') {
        return true;
    }
    let scheme = url[..n].to_ascii_lowercase();
    schemes.contains(&scheme.as_str())
}

/// The URL, if allowed passes it both as it is and decoded, for an app that decodes the href
/// before opening it, as Pele's allow does.
fn allow(url: &str, schemes: &[&str]) -> Option<String> {
    let decoded = unwrap(url, true)?;
    (allowed(url, schemes) && allowed(&decoded, schemes)).then(|| url.to_string())
}

/// A URL that sanitize_url has passed, kept when it is relative or its scheme is one the host
/// allows for a link, as Pele's linkUrl keeps it. One that is refused is left out of the
/// drawing: written as about:blank, a link would still take the page it is in somewhere when
/// clicked.
pub(crate) fn link_url(url: &str, o: &Schemes) -> Option<String> {
    if !o.links {
        return None;
    }
    allow(url, o.link_schemes)
}

/// A URL that sanitize_url has passed, kept when it is relative or its scheme is one the host
/// allows for an image, as Pele's imageUrl keeps it.
pub(crate) fn image_url(url: &str, o: &Schemes) -> Option<String> {
    if !o.images {
        return None;
    }
    allow(url, o.image_schemes)
}

/// Whether a code point is in Pele's RE_INVALID_XML,
/// `/[\x00-\x08\x0b\x0c\x0e-\x1f\x7f\ud800-\udfff￾￿]/u`, which a Rust string holds
/// no surrogates of.
fn is_invalid_xml(c: char) -> bool {
    matches!(u32::from(c), 0..=0x08 | 0x0b | 0x0c | 0x0e..=0x1f | 0x7f | 0xfffe | 0xffff)
}

/// Whether the name of an internal link may be its address, as Pele's internalLink decides.
/// The address keeps the name as written, so passing sanitize_url and link_url is not
/// enough. The name is checked again as the browser reads it, without the characters Pele's
/// esc drops and the tabs and line breaks a browser takes out, and with the control
/// characters and spaces at its start taken off.
pub(crate) fn internal_link(name: &str, o: &Schemes) -> bool {
    let url = sanitize_url(name);
    if url == BLANK || link_url(&url, o).is_none() {
        return false;
    }
    let seen: String = name
        .chars()
        .filter(|&c| !is_invalid_xml(c) && !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let seen = seen.trim_start_matches(|c: char| c <= '\u{20}');
    link_url(seen, o).is_some()
}
