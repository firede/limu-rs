//! Text read as JavaScript reads it: by UTF-16 code units, with the white space of its
//! regular expressions and its trim, and its toLowerCase.
//!
//! The parsers keep their text as Rust strings and step through it by bytes, which is what
//! JavaScript does by code units wherever the characters looked for are ASCII. Where a
//! character outside ASCII is read, [`unit_at`] gives the code unit JavaScript's charCodeAt
//! would give at the same place. That is the character itself in the Basic Multilingual
//! Plane, and the first of its two surrogates above it. Positions shown to people, such as
//! the column of a syntax error, are counted in code units with [`len16`].

use std::sync::OnceLock;

// The tests on code units, as Pele's src/util/chars.ts has them. Past the end of a string a
// lexer reads -1, which is none of these, as charCodeAt's NaN is none of them.

/// Whether code unit c is white space to a regular expression's `\s`.
pub(crate) fn is_space(c: i32) -> bool {
    if c < 128 {
        return c == 32 || (9..=13).contains(&c);
    }
    matches!(
        c,
        0xa0 | 0x1680 | 0x2000..=0x200a | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff
    )
}

/// Whether c is a space or a tab.
pub(crate) fn is_blank(c: i32) -> bool {
    c == 32 || c == 9
}

/// Whether c ends a line, as what a regular expression's `.` does not match.
pub(crate) fn is_line_end(c: i32) -> bool {
    matches!(c, 10 | 13 | 0x2028 | 0x2029)
}

/// Whether c is an ASCII digit.
pub(crate) fn is_digit(c: i32) -> bool {
    (48..=57).contains(&c)
}

/// Whether c is an ASCII letter.
pub(crate) fn is_letter(c: i32) -> bool {
    (65..=90).contains(&c) || (97..=122).contains(&c)
}

/// Whether c is an ASCII hexadecimal digit.
pub(crate) fn is_hex(c: i32) -> bool {
    is_digit(c) || (65..=70).contains(&c) || (97..=102).contains(&c)
}

/// Whether c is in a regular expression's `\w`.
pub(crate) fn is_word(c: i32) -> bool {
    is_digit(c) || is_letter(c) || c == 95
}

/// Whether a character is white space to `\s` and to trim.
pub(crate) fn is_space_char(c: char) -> bool {
    is_space(c as i32)
}

/// The code unit JavaScript reads at byte i of s, with the number of bytes of the character
/// it starts, and -1 and 0 past the end. A character above the Basic Multilingual Plane reads
/// as its high surrogate, and its bytes are all stepped over at once. No lexer stops on the
/// low surrogate that JavaScript would read next, as it is no character a lexer looks for.
pub(crate) fn unit_at(s: &str, i: usize) -> (i32, usize) {
    let Some(&b) = s.as_bytes().get(i) else {
        return (-1, 0);
    };
    if b < 0x80 {
        return (i32::from(b), 1);
    }
    let Some(c) = s.get(i..).and_then(|t| t.chars().next()) else {
        return (-1, 0);
    };
    let mut units = [0u16; 2];
    (i32::from(c.encode_utf16(&mut units)[0]), c.len_utf8())
}

/// [`unit_at`] without the size.
pub(crate) fn unit(s: &str, i: usize) -> i32 {
    unit_at(s, i).0
}

/// The code unit JavaScript reads just before byte i of s: the low surrogate of a character
/// above the Basic Multilingual Plane, and -1 at the start.
pub(crate) fn unit_before(s: &str, i: usize) -> i32 {
    let Some(c) = s.get(..i).and_then(|t| t.chars().next_back()) else {
        return -1;
    };
    let mut units = [0u16; 2];
    let u = c.encode_utf16(&mut units);
    i32::from(u[u.len() - 1])
}

/// The length of s in UTF-16 code units, JavaScript's length.
pub(crate) fn len16(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// String.prototype.trim.
pub(crate) fn trim(s: &str) -> &str {
    s.trim_matches(is_space_char)
}

/// String.prototype.trimStart.
pub(crate) fn trim_start(s: &str) -> &str {
    s.trim_start_matches(is_space_char)
}

/// String.prototype.trimEnd.
pub(crate) fn trim_end(s: &str) -> &str {
    s.trim_end_matches(is_space_char)
}

/// The end of the run of white space that starts at byte i of s.
pub(crate) fn space_run(s: &str, mut i: usize) -> usize {
    loop {
        let (c, size) = unit_at(s, i);
        if !is_space(c) {
            return i;
        }
        i += size;
    }
}

/// `s.replace(/\n\s+/g, "\n")`, as the diagrams set an accessible description: the white
/// space after each line feed taken off, with the line breaks in it.
pub(crate) fn drop_indents(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut from = 0;
    while let Some(n) = s[from..].find('\n') {
        let at = from + n + 1;
        out.push_str(&s[from..at]);
        from = space_run(s, at);
    }
    out.push_str(&s[from..]);
    out
}

/// String.prototype.toLowerCase. Rust lowers strings by Unicode's full case mapping, as V8
/// does through ICU. U+0130 lowers to an i and a combining dot above, and a capital sigma
/// that ends a word to a final sigma, by Unicode's Final_Sigma condition.
pub(crate) fn to_lower(s: &str) -> String {
    s.to_lowercase()
}

/// Whether s starts with prefix but for the case of ASCII letters, as a regular
/// expression's i flag without u compares them.
pub(crate) fn has_prefix_fold(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len() && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

/// The letters outside ASCII, from Pele's src/util/unicode.ts: ranges of the Basic
/// Multilingual Plane, each as its start's distance from the end of the one before and its
/// length, in base 36.
const PACKED: &str = "4q 0 b 0 5 0 6 m 2 u 2 cp 5 b f 4 8 0 2 0 3m 4 2 1 3 3 9 0 2 2 2 0 2 j 2 2a 2 3u 9 4d a \
    11 3 0 8 12 21 q 6 2 1a 16 10 1 2 2q 2 0 g 1 8 1 b 2 3 0 h 0 2 t u 2g c 0 p w a 1 5 0 6 \
    l 5 0 a 0 4 0 o o 20 0 2 a 2g 1h 4 0 j 0 8 9 g 6 2 6 6 7 3 1 3 l 2 6 2 0 4 3 4 0 h 0 e 1 \
    2 2 f 1 k 5 5 1 3 l 2 6 2 1 2 1 2 1 w 3 2 0 k 2 h 8 2 2 2 l 2 6 2 1 2 4 4 0 j 0 g 1 10 7 \
    3 1 3 l 2 6 2 1 2 4 4 0 v 1 2 2 g 0 i 0 2 5 4 2 2 3 4 1 2 0 2 1 4 1 4 2 4 b n 0 1h 7 2 2 \
    2 m 2 9 2 4 4 0 r 1 7 1 10 7 2 2 2 m 2 9 2 4 4 0 x 0 2 1 g 1 j 7 2 2 2 14 3 0 h 0 i 1 p \
    5 6 h 4 n 2 8 2 0 3 6 1n 1b 2 1 d 6 1n 1 2 0 3 1 2 0 3 0 7 3 2 6 2 2 2 0 2 0 3 1 2 3 2 1 \
    a 0 3 4 2 0 m 3 x 0 1s 7 2 z s 4 38 16 l 0 h 5 5 3 4 0 4 1 8 2 5 c d 0 i 11 2 0 6 0 3 16 \
    2 98 2 3 3 6 2 0 2 3 3 14 2 3 3 w 2 3 3 6 2 0 2 3 3 e 2 1k 2 3 3 1u 12 f h 2c d h7 3 g 2 \
    p 6 22 m c 2 3 f h f h f c 2 2 g 1f 10 0 5 0 1w 2f 9 14 2 0 6 1x b s 1g t 3 4 c 17 m 6 \
    1l m a 1g 2b 0 2m 1a i 6 1k t e 1 b 17 r z 16 2 b z 30 3 2 3 4 1 a 5b 1t 7p 3 5 3 11 3 5 \
    3 7 2 0 2 0 2 0 2 u 3 1g 2 6 2 0 4 2 2 6 4 3 3 5 5 c 6 2 2 6 39 0 e 0 h c 2u 0 5 0 3 9 2 \
    0 4 4 7 0 2 0 2 0 2 3 2 a 3 3 6 4 5 0 1h 1 22k 1a 2 1a 2 3o 7 3 4 1 d 11 2 0 6 0 3 1j 8 \
    0 h m a 6 2 6 2 6 2 6 2 6 2 6 2 6 2 6 29 0 d2 1 17 4 6 1 5 2d 7 2 2 2h 2 3 6 14 4 2l i q \
    1i f e9 52t 23 g5o 1g wc 1w 19 3 7g 4 f b 1 l 1a h o 9 1x 1e 8 3 2u 3 3 2 3 d a 26 9 2 2 \
    2 3 2 m u 1f f 1d 1r 5 4 0 f r b m q s 8 1a t 0 1d 14 o 2 2 7 l m 4 0 6 1b 2 0 4 1 3 4 3 \
    0 2 0 p 2 3 a 8 2 d 5 3 5 3 5 a 6 2 6 42 y u 8mb d m 5 1c 6it a5 3 2x 13 6 d 4 6 0 2 9 2 \
    c 2 4 2 0 2 1 2 1 2 2z y a2 j 1r 3 1h 15 b 39 4 2 3q 11 p 7 p c 2g 4 5 3 5 3 5 3 2";

/// The ranges of [`PACKED`], each its first and last code unit.
fn letters() -> &'static [(u16, u16)] {
    static LETTERS: OnceLock<Vec<(u16, u16)>> = OnceLock::new();
    LETTERS.get_or_init(|| {
        let mut parts = PACKED
            .split(' ')
            .map(|p| u32::from_str_radix(p, 36).unwrap_or(0));
        let mut out = Vec::new();
        let mut prev = 0;
        while let (Some(a), Some(b)) = (parts.next(), parts.next()) {
            let start = prev + a;
            prev = start + b;
            out.push((start as u16, prev as u16));
        }
        out
    })
}

/// Whether code unit c is a letter outside ASCII, as Pele's isUnicodeLetter does.
pub(crate) fn is_unicode_letter(c: i32) -> bool {
    if !(0xaa..=0xffdc).contains(&c) {
        return false;
    }
    letters()
        .binary_search_by(|&(lo, hi)| {
            if c < i32::from(lo) {
                std::cmp::Ordering::Greater
            } else if c > i32::from(hi) {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}
