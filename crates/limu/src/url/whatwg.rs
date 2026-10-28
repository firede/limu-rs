//! `new URL(input).toString()` for an http or https URL, as the WHATWG URL Standard parses
//! and serializes it and Node's URL, which is Ada, does. Pele's sanitizeUrl calls it, and
//! lowers the protocol and host name, which the parser has lowered already.
//!
//! sanitize_url comes here only with a URL whose first line has no colon but its scheme's, so
//! neither a port nor an IPv6 address can be read, and an address in brackets is refused. A
//! host of ASCII is lowered, as Ada does, without a check of the Punycode in it. A host name
//! outside ASCII is lowered and written in Punycode without the whole of UTS #46. A host with
//! white space, controls, format characters or private use characters is refused, as UTS #46
//! refuses them, and full-width letters and stops are made ASCII. Its other mappings, as of ß,
//! ligatures and combining marks, are not made, and unassigned code points are not refused,
//! as the standard library has no table of them.

use super::hex_byte;

/// The URL serialized, or none where the constructor throws.
pub(crate) fn parse_http(input: &str) -> Option<String> {
    let input: String = input
        .trim_matches(|c: char| c <= '\u{20}')
        .chars()
        .filter(|&c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let b = input.as_bytes();

    if !b.first().is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }
    let i = b
        .iter()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.'))
        .count();
    if b.get(i) != Some(&b':') {
        return None;
    }
    let scheme = input[..i].to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let rest = &input[i + 1..];

    let p = rest
        .bytes()
        .take_while(|c| matches!(c, b'/' | b'\\'))
        .count();
    let end = p + rest[p..]
        .bytes()
        .take_while(|c| !matches!(c, b'/' | b'?' | b'#' | b'\\'))
        .count();
    let auth = &rest[p..end];

    let (mut user, mut pass) = (String::new(), String::new());
    let mut host_port = auth;
    if let Some(at) = auth.rfind('@') {
        let mut seen = false;
        for c in auth[..at].chars() {
            if c == ':' && !seen {
                seen = true;
                continue;
            }
            encode_char(if seen { &mut pass } else { &mut user }, c, USERINFO_SET);
        }
        host_port = &auth[at + 1..];
        if host_port.is_empty() {
            return None;
        }
    }

    let (mut host_text, mut port_text, mut has_port) = (host_port, "", false);
    let mut inside = false;
    for (k, c) in host_port.bytes().enumerate() {
        match c {
            b'[' => inside = true,
            b']' => inside = false,
            b':' if !inside => {
                (host_text, port_text, has_port) = (&host_port[..k], &host_port[k + 1..], true);
                break;
            }
            _ => {}
        }
    }
    if host_text.is_empty() {
        return None;
    }
    let host = parse_host(host_text)?;
    if !port_text.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut port = String::new();
    if has_port && !port_text.is_empty() {
        let mut n: u32 = 0;
        for c in port_text.bytes() {
            n = n * 10 + u32::from(c - b'0');
            if n > 65535 {
                return None;
            }
        }
        if !(scheme == "http" && n == 80 || scheme == "https" && n == 443) {
            port = format!(":{n}");
        }
    }

    let tail = &rest[end..];
    let tb = tail.as_bytes();
    let mut q = usize::from(matches!(tb.first(), Some(b'/' | b'\\')));
    let mut segs: Vec<String> = Vec::new();
    let mut buf = String::new();
    let (mut query, mut fragment) = (None, None);
    loop {
        let c = tb.get(q).copied();
        if matches!(c, None | Some(b'/' | b'\\' | b'?' | b'#')) {
            let seg = std::mem::take(&mut buf);
            let slash = matches!(c, Some(b'/' | b'\\'));
            if is_double_dot(&seg) {
                segs.pop();
                if !slash {
                    segs.push(String::new());
                }
            } else if is_single_dot(&seg) {
                if !slash {
                    segs.push(String::new());
                }
            } else {
                segs.push(seg);
            }
            match c {
                None => break,
                Some(b'?') => {
                    let after = &tail[q + 1..];
                    let (text, frag) = match after.split_once('#') {
                        Some((text, frag)) => (text, Some(frag)),
                        None => (after, None),
                    };
                    query = Some(encode_str(text, QUERY_SET));
                    fragment = frag.map(|f| encode_str(f, FRAGMENT_SET));
                    break;
                }
                Some(b'#') => {
                    fragment = Some(encode_str(&tail[q + 1..], FRAGMENT_SET));
                    break;
                }
                _ => {
                    q += 1;
                    continue;
                }
            }
        }
        let ch = tail[q..].chars().next()?;
        encode_char(&mut buf, ch, PATH_SET);
        q += ch.len_utf8();
    }

    let mut out = scheme + "://";
    if !user.is_empty() || !pass.is_empty() {
        out.push_str(&user);
        if !pass.is_empty() {
            out.push(':');
            out.push_str(&pass);
        }
        out.push('@');
    }
    out.push_str(&host);
    out.push_str(&port);
    for s in &segs {
        out.push('/');
        out.push_str(s);
    }
    if let Some(query) = query {
        out.push('?');
        out.push_str(&query);
    }
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(&fragment);
    }
    Some(out)
}

fn is_single_dot(s: &str) -> bool {
    s == "." || s.eq_ignore_ascii_case("%2e")
}

fn is_double_dot(s: &str) -> bool {
    ["..", ".%2e", "%2e.", "%2e%2e"]
        .iter()
        .any(|d| s.eq_ignore_ascii_case(d))
}

// The ASCII characters each part of a URL percent-encodes, besides the C0 controls and all
// above U+007E.
const PATH_SET: &[u8] = b" \"#<>?^`{}";
const QUERY_SET: &[u8] = b" \"#'<>";
const FRAGMENT_SET: &[u8] = b" \"<>`";
const USERINFO_SET: &[u8] = b" \"#<>?^`{}/:;=@[\\]|";

fn encode_byte(out: &mut String, c: u8, set: &[u8]) {
    if !(0x20..=0x7e).contains(&c) || set.contains(&c) {
        out.push_str(&format!("%{c:02X}"));
    } else {
        out.push(char::from(c));
    }
}

fn encode_char(out: &mut String, c: char, set: &[u8]) {
    let mut tmp = [0u8; 4];
    for &b in c.encode_utf8(&mut tmp).as_bytes() {
        encode_byte(out, b, set);
    }
}

fn encode_str(s: &str, set: &[u8]) -> String {
    let mut out = String::new();
    for c in s.chars() {
        encode_char(&mut out, c, set);
    }
    out
}

/// The bytes the percent escapes in s write, with a `%` that starts none kept as it is.
fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(c) = hex_byte(b, i)
        {
            out.push(c);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

fn parse_host(input: &str) -> Option<String> {
    if input.starts_with('[') {
        return None;
    }
    let domain = String::from_utf8(percent_decode(input)).ok()?;
    let ascii = domain_to_ascii(&domain)?;
    let forbidden = |c: u8| c <= 0x20 || c == 0x7f || b"#%/:<>?@[\\]^|".contains(&c);
    if ascii.bytes().any(forbidden) {
        return None;
    }
    if ends_in_number(&ascii) {
        return parse_ipv4(&ascii);
    }
    Some(ascii)
}

/// Whether a character is of Unicode's category Cf, a format character, as of Unicode 16.
fn is_format(c: char) -> bool {
    matches!(
        u32::from(c),
        0xad | 0x600..=0x605
            | 0x61c
            | 0x6dd
            | 0x70f
            | 0x890..=0x891
            | 0x8e2
            | 0x180e
            | 0x200b..=0x200f
            | 0x202a..=0x202e
            | 0x2060..=0x2064
            | 0x2066..=0x206f
            | 0xfeff
            | 0xfff9..=0xfffb
            | 0x110bd
            | 0x110cd
            | 0x13430..=0x1343f
            | 0x1bca0..=0x1bca3
            | 0x1d173..=0x1d17a
            | 0xe0001
            | 0xe0020..=0xe007f
    )
}

/// Whether a character is of Unicode's category Co, for private use.
fn is_private_use(c: char) -> bool {
    matches!(u32::from(c), 0xe000..=0xf8ff | 0xf0000..=0xffffd | 0x100000..=0x10fffd)
}

fn domain_to_ascii(domain: &str) -> Option<String> {
    // Node's URL, as Ada has it, lowers a host of ASCII and checks no Punycode in it.
    if domain.is_ascii() {
        return Some(domain.to_ascii_lowercase());
    }
    let mut mapped = String::with_capacity(domain.len());
    for c in domain.chars() {
        match u32::from(c) {
            0x3002 | 0xff0e | 0xff61 => mapped.push('.'),
            u @ 0xff01..=0xff5e => {
                mapped.push(char::from((u - 0xff01 + 0x21) as u8).to_ascii_lowercase())
            }
            _ if c.is_whitespace()
                || c.is_control()
                || is_format(c)
                || is_private_use(c)
                || c == char::REPLACEMENT_CHARACTER =>
            {
                return None;
            }
            _ => mapped.extend(c.to_lowercase()),
        }
    }
    let mut labels = Vec::new();
    for l in mapped.split('.') {
        if l.len() >= 4 && l.is_char_boundary(4) && &l[..4] == "xn--" {
            puny_decode(&l[4..])?;
            labels.push(l.to_string());
        } else if !l.is_ascii() {
            let points: Vec<u32> = l.chars().map(u32::from).collect();
            labels.push(format!("xn--{}", puny_encode(&points)));
        } else {
            labels.push(l.to_string());
        }
    }
    let out = labels.join(".");
    (!out.is_empty()).then_some(out)
}

fn ends_in_number(s: &str) -> bool {
    let mut parts: Vec<&str> = s.split('.').collect();
    if parts.last() == Some(&"") {
        if parts.len() == 1 {
            return false;
        }
        parts.pop();
    }
    let last = parts[parts.len() - 1];
    if !last.is_empty() && last.bytes().all(|c| c.is_ascii_digit()) {
        return true;
    }
    ipv4_number(last).is_some()
}

/// A part of an IPv4 address: decimal, octal after a 0, or hexadecimal after 0x. A value past
/// 2^40 is kept at 2^40, which is past every limit it is compared with.
fn ipv4_number(s: &str) -> Option<u64> {
    if s.is_empty() {
        return None;
    }
    let (digits, base) = if s.len() >= 2 && (s.starts_with("0x") || s.starts_with("0X")) {
        (&s[2..], 16)
    } else if s.len() >= 2 && s.starts_with('0') {
        (&s[1..], 8)
    } else {
        (s, 10)
    };
    let mut n: u64 = 0;
    for c in digits.chars() {
        let d = c.to_digit(base)?;
        n = (n * u64::from(base) + u64::from(d)).min(1 << 40);
    }
    Some(n)
}

fn parse_ipv4(s: &str) -> Option<String> {
    let mut parts: Vec<&str> = s.split('.').collect();
    if parts.len() > 1 && parts.last() == Some(&"") {
        parts.pop();
    }
    if parts.len() > 4 {
        return None;
    }
    let nums = parts
        .iter()
        .map(|p| ipv4_number(p))
        .collect::<Option<Vec<_>>>()?;
    let (&last, init) = nums.split_last()?;
    if init.iter().any(|&n| n > 255) || last >= 1 << (8 * (5 - nums.len())) {
        return None;
    }
    let addr = init
        .iter()
        .enumerate()
        .fold(last, |a, (i, &n)| a + (n << (8 * (3 - i))));
    Some(format!(
        "{}.{}.{}.{}",
        addr >> 24,
        addr >> 16 & 255,
        addr >> 8 & 255,
        addr & 255
    ))
}

// Punycode, as RFC 3492 has it.
const BASE: u64 = 36;
const T_MIN: u64 = 1;
const T_MAX: u64 = 26;
const SKEW: u64 = 38;
const DAMP: u64 = 700;
const INITIAL_BIAS: u64 = 72;
const INITIAL_N: u64 = 128;

fn adapt(delta: u64, points: u64, first: bool) -> u64 {
    let mut delta = if first { delta / DAMP } else { delta / 2 };
    delta += delta / points;
    let mut k = 0;
    while delta > (BASE - T_MIN) * T_MAX / 2 {
        delta /= BASE - T_MIN;
        k += BASE;
    }
    k + (BASE - T_MIN + 1) * delta / (delta + SKEW)
}

fn threshold(k: u64, bias: u64) -> u64 {
    k.saturating_sub(bias).clamp(T_MIN, T_MAX)
}

fn digit(d: u64) -> char {
    char::from(if d < 26 {
        b'a' + d as u8
    } else {
        b'0' + (d - 26) as u8
    })
}

fn puny_encode(input: &[u32]) -> String {
    let mut out: String = input
        .iter()
        .filter(|&&c| c < 0x80)
        .map(|&c| char::from(c as u8))
        .collect();
    let b = out.len() as u64;
    let mut h = b;
    if b > 0 {
        out.push('-');
    }
    let (mut n, mut delta, mut bias) = (INITIAL_N, 0u64, INITIAL_BIAS);
    while (h as usize) < input.len() {
        let m = input
            .iter()
            .map(|&c| u64::from(c))
            .filter(|&c| c >= n)
            .min()
            .unwrap_or(n);
        delta += (m - n) * (h + 1);
        n = m;
        for c in input.iter().map(|&c| u64::from(c)) {
            if c < n {
                delta += 1;
            }
            if c == n {
                let mut q = delta;
                let mut k = BASE;
                loop {
                    let t = threshold(k, bias);
                    if q < t {
                        break;
                    }
                    out.push(digit(t + (q - t) % (BASE - t)));
                    q = (q - t) / (BASE - t);
                    k += BASE;
                }
                out.push(digit(q));
                bias = adapt(delta, h + 1, h == b);
                delta = 0;
                h += 1;
            }
        }
        delta += 1;
        n += 1;
    }
    out
}

fn puny_decode(s: &str) -> Option<Vec<u32>> {
    let mut out: Vec<u32> = Vec::new();
    let mut s = s.as_bytes();
    if let Some(b) = s.iter().rposition(|&c| c == b'-') {
        if s[..b].iter().any(|&c| c >= 0x80) {
            return None;
        }
        out.extend(s[..b].iter().map(|&c| u32::from(c)));
        s = &s[b + 1..];
    }
    let (mut n, mut i, mut bias) = (INITIAL_N, 0u64, INITIAL_BIAS);
    let mut p = 0;
    while p < s.len() {
        let (old, mut w) = (i, 1u64);
        let mut k = BASE;
        loop {
            let c = *s.get(p)?;
            p += 1;
            let d = match c {
                b'0'..=b'9' => c - b'0' + 26,
                b'a'..=b'z' => c - b'a',
                b'A'..=b'Z' => c - b'A',
                _ => return None,
            };
            i += u64::from(d) * w;
            if i > 1 << 30 {
                return None;
            }
            let t = threshold(k, bias);
            if u64::from(d) < t {
                break;
            }
            w *= BASE - t;
            k += BASE;
        }
        let len = out.len() as u64 + 1;
        bias = adapt(i - old, len, old == 0);
        n += i / len;
        i %= len;
        if n > 0x10ffff {
            return None;
        }
        out.insert(i as usize, n as u32);
        i += 1;
    }
    Some(out)
}
