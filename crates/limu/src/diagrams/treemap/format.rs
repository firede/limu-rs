//! Values written as a specifier of d3-format says, as Pele's format.ts writes them.

use crate::js::chars::is_line_end;
use crate::js::math::round;
use crate::js::number::{number, to_exponential, to_fixed, to_precision, to_string};

/// What Pele's formatter reads of a specifier of d3-format, the parts of RE_SPEC's match it
/// uses: a sign of +, a $, a comma for thousands separators, a precision, ~ to trim trailing
/// zeros, and the type, where there is one.
#[derive(Clone, Copy, Debug, Default)]
struct Spec {
    plus: bool,
    dollar: bool,
    comma: bool,
    trim: bool,
    precision: Option<usize>,
    kind: Option<u8>,
}

/// Whether c is one of d3-format's alignments, [<>=^].
fn is_align(c: u8) -> bool {
    matches!(c, b'<' | b'>' | b'=' | b'^')
}

/// Matches Pele's RE_SPEC,
/// /^(?:.?[<>=^])?([-+( ])?(\$)?0?\d*(,)?(?:\.(\d+))?(~)?([a-zA-Z%])?$/, against s. The fill
/// and the alignment are tried as a regular expression tries them: a character and an
/// alignment, an alignment alone, and neither. A character is a UTF-16 code unit other than a
/// line terminator, so one outside the Basic Multilingual Plane, two units, is no fill. What
/// follows has one way to match at most, as each part takes characters none after it can.
fn read_spec(s: &str) -> Option<Spec> {
    let b = s.as_bytes();
    let mut starts = Vec::new();
    if let Some(c) = s.chars().next() {
        let size = c.len_utf8();
        if c.len_utf16() == 1 && !is_line_end(c as i32) && b.get(size).is_some_and(|&a| is_align(a))
        {
            starts.push(size + 1);
        }
    }
    if b.first().is_some_and(|&a| is_align(a)) {
        starts.push(1);
    }
    starts.push(0);
    starts.into_iter().find_map(|i| read_spec_rest(s, i))
}

/// Matches what follows the fill and the alignment of RE_SPEC, from byte i to the end of s.
fn read_spec_rest(s: &str, mut i: usize) -> Option<Spec> {
    let b = s.as_bytes();
    let mut spec = Spec::default();
    let at = |i: usize, c: u8| b.get(i) == Some(&c);
    let digit = |k: usize| b.get(k).is_some_and(u8::is_ascii_digit);
    if b.get(i).is_some_and(|c| b"-+( ".contains(c)) {
        spec.plus = b[i] == b'+';
        i += 1;
    }
    if at(i, b'$') {
        spec.dollar = true;
        i += 1;
    }
    while digit(i) {
        i += 1;
    }
    if at(i, b',') {
        spec.comma = true;
        i += 1;
    }
    if at(i, b'.') && digit(i + 1) {
        i += 1;
        // Number(m[4]), at most 20, which a precision of more digits than two always is.
        let mut p = 0;
        while digit(i) {
            p = (p * 10 + usize::from(b[i] - b'0')).min(21);
            i += 1;
        }
        spec.precision = Some(p.min(20));
    }
    if at(i, b'~') {
        spec.trim = true;
        i += 1;
    }
    if b.get(i)
        .is_some_and(|&c| c.is_ascii_alphabetic() || c == b'%')
    {
        spec.kind = Some(b[i]);
        i += 1;
    }
    (i == b.len()).then_some(spec)
}

/// Puts a comma between each three digits of digits from its end, as Pele's group does.
fn group(digits: &str) -> String {
    let mut out = String::new();
    let mut i = digits.len();
    while i > 0 {
        let piece = &digits[i.saturating_sub(3)..i];
        out = if out.is_empty() {
            piece.to_string()
        } else {
            format!("{piece},{out}")
        };
        i = i.saturating_sub(3);
    }
    out
}

/// A function that writes values as a specifier of d3-format says, as Pele's formatter makes
/// it: the parts of d3-format that treemap values use, an optional $, a comma for thousands
/// separators, a precision, and a type of f, %, d, e or none. Fill, alignment and width are
/// read and ignored. Anything else falls back to the default, ",".
pub(crate) fn formatter(spec: &str) -> impl Fn(f64) -> String {
    // Mermaid documents $0,0 for a dollar amount with thousands separators.
    let spec = if spec == "$0,0" { "$," } else { spec };
    let spec = read_spec(spec)
        .or_else(|| read_spec(","))
        .unwrap_or_default();
    move |value: f64| {
        let abs = value.abs();
        let precision = |fallback: usize| spec.precision.unwrap_or(fallback);
        let mut suffix = "";
        let mut body = match spec.kind {
            Some(b'f') => to_fixed(abs, precision(6)),
            Some(b'%') => {
                suffix = "%";
                to_fixed(abs * 100.0, precision(6))
            }
            Some(b'd') => to_fixed(round(abs), 0),
            Some(b'e') => to_exponential(abs, precision(6)),
            _ => to_string(number(&to_precision(abs, precision(12).max(1)))),
        };
        if spec.trim && body.contains('.') && !body.contains('e') {
            // body.replace(/\.?0+$/, ""): the zeros at the end, and the point before them.
            let kept = body.trim_end_matches('0');
            body = kept.strip_suffix('.').unwrap_or(kept).to_string();
        }
        if spec.comma && !body.contains('e') {
            body = match body.find('.') {
                Some(dot) => format!("{}{}", group(&body[..dot]), &body[dot..]),
                None => group(&body),
            };
        }
        let sign = if value < 0.0 {
            "-"
        } else if spec.plus {
            "+"
        } else {
            ""
        };
        let dollar = if spec.dollar { "$" } else { "" };
        format!("{sign}{dollar}{body}{suffix}")
    }
}
