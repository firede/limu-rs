//! The functions checked against what V8 makes of the same cases, in the oracle's jsmath,
//! jsmath-log10, jsmath-format, chars and draw fixtures.

use limu_fixtures::{Num, load};
use serde::Deserialize;

use super::{chars, math, number};
use crate::testing::Failures;

fn same(got: f64, want: f64) -> bool {
    Num::same(got, want)
}

/// An angle as the oracle writes it: a number, or [k, n] for k·π/n.
#[derive(Deserialize)]
#[serde(untagged)]
enum Angle {
    Turn([f64; 2]),
    Plain(Num),
}

impl Angle {
    fn value(&self) -> f64 {
        match *self {
            Angle::Turn([k, n]) => k * std::f64::consts::PI / n,
            Angle::Plain(Num(x)) => x,
        }
    }
}

#[derive(Deserialize)]
struct JsMath {
    trig: Vec<(Angle, Num, Num)>,
    atan2: Vec<[Num; 3]>,
    acos: Vec<[Num; 2]>,
    asin: Vec<[Num; 2]>,
    round: Vec<[Num; 2]>,
}

#[test]
fn math_matches_v8() {
    let p: JsMath = load("jsmath");
    let mut f = Failures::new();
    for (x, Num(s), Num(c)) in p.trig {
        let x = x.value();
        f.check(same(math::sin(x), s), || {
            format!("sin({x:e}) = {:e}, V8 {s:e}", math::sin(x))
        });
        f.check(same(math::cos(x), c), || {
            format!("cos({x:e}) = {:e}, V8 {c:e}", math::cos(x))
        });
    }
    for [Num(y), Num(x), Num(r)] in p.atan2 {
        let got = math::atan2(y, x);
        f.check(same(got, r), || {
            format!("atan2({y:e}, {x:e}) = {got:e}, V8 {r:e}")
        });
    }
    for [Num(x), Num(r)] in p.acos {
        f.check(same(math::acos(x), r), || {
            format!("acos({x:e}) = {:e}, V8 {r:e}", math::acos(x))
        });
    }
    for [Num(x), Num(r)] in p.asin {
        f.check(same(math::asin(x), r), || {
            format!("asin({x:e}) = {:e}, V8 {r:e}", math::asin(x))
        });
    }
    for [Num(x), Num(r)] in p.round {
        f.check(same(math::round(x), r), || {
            format!("round({x:e}) = {:e}, V8 {r:e}", math::round(x))
        });
    }
    f.done("jsmath", 100_000);
}

#[derive(Deserialize)]
struct Cases<T> {
    cases: Vec<T>,
}

#[test]
fn log10_matches_v8() {
    let p: Cases<[Num; 2]> = load("jsmath-log10");
    let mut f = Failures::new();
    for [Num(x), Num(r)] in p.cases {
        let got = math::log10(x);
        f.check(same(got, r), || format!("log10({x:e}) = {got:e}, V8 {r:e}"));
    }
    f.done("jsmath-log10", 20_000);
}

#[test]
fn hypot_matches_v8() {
    #[derive(Deserialize)]
    struct Draw {
        hypot: Vec<[Num; 3]>,
    }
    let p: Draw = load("draw");
    let mut f = Failures::new();
    for [Num(a), Num(b), Num(r)] in p.hypot {
        let got = math::hypot(a, b);
        f.check(same(got, r), || {
            format!("hypot({a:e}, {b:e}) = {got:e}, V8 {r:e}")
        });
    }
    f.done("draw.hypot", 1000);
}

// Each case is a number, the digits given to toFixed and toExponential and what they write,
// and the precision given to toPrecision and what it writes.
#[test]
fn format_matches_v8() {
    let p: Cases<(Num, usize, String, String, usize, String)> = load("jsmath-format");
    let mut f = Failures::new();
    for (Num(x), digits, fixed, exp, precision, prec) in p.cases {
        let got = number::to_fixed(x, digits);
        f.check(got == fixed, || {
            format!("to_fixed({x:e}, {digits}) = {got}, V8 {fixed}")
        });
        let got = number::to_exponential(x, digits);
        f.check(got == exp, || {
            format!("to_exponential({x:e}, {digits}) = {got}, V8 {exp}")
        });
        let got = number::to_precision(x, precision);
        f.check(got == prec, || {
            format!("to_precision({x:e}, {precision}) = {got}, V8 {prec}")
        });
    }
    f.done("jsmath-format", 150_000);
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Chars {
    letters: Vec<[i32; 2]>,
    to_string: Vec<ToString>,
    number: Vec<NumberCase>,
    to_lower_case: Vec<Lower>,
}

#[derive(Deserialize)]
struct ToString {
    n: Num,
    s: String,
}

#[derive(Deserialize)]
struct NumberCase {
    s: String,
    n: Num,
    hex: Num,
    dec: Num,
}

#[derive(Deserialize)]
struct Lower {
    s: String,
    lower: String,
}

#[test]
fn chars_match_javascript() {
    let p: Chars = load("chars");
    let mut f = Failures::new();
    let mut want = vec![false; 0x10000];
    for [lo, hi] in p.letters {
        for c in lo..=hi {
            want[c as usize] = true;
        }
    }
    for (c, &w) in want.iter().enumerate() {
        let got = chars::is_unicode_letter(c as i32);
        f.check(got == w, || format!("is_unicode_letter({c:#x}) = {got}"));
    }
    for c in p.to_lower_case {
        let got = chars::to_lower(&c.s);
        f.check(got == c.lower, || {
            format!("to_lower({:?}) = {got:?}, JavaScript {:?}", c.s, c.lower)
        });
    }
    for c in p.to_string {
        let got = number::to_string(c.n.0);
        f.check(got == c.s, || {
            format!("to_string({:e}) = {got}, JavaScript {}", c.n.0, c.s)
        });
    }
    for c in p.number {
        for (name, got, want) in [
            ("number", number::number(&c.s), c.n.0),
            ("parse_int 16", number::parse_int(&c.s, 16), c.hex.0),
            ("parse_int 10", number::parse_int(&c.s, 10), c.dec.0),
        ] {
            f.check(same(got, want), || {
                format!("{name}({:?}) = {got:e}, JavaScript {want:e}", c.s)
            });
        }
    }
    f.done("chars", 70_000);
}

#[test]
fn units() {
    let s = "a😀é";
    assert_eq!(chars::len16(s), 4);
    assert_eq!(chars::unit_at(s, 1), (0xd83d, 4));
    assert_eq!(chars::unit_before(s, 5), 0xde00);
    assert_eq!(chars::unit_at(s, 9), (-1, 0));
    assert_eq!(chars::to_lower("İA"), "i̇a");
    assert_eq!(chars::drop_indents("a\n  b\n\n c"), "a\nb\nc");
    assert_eq!(chars::trim("\u{feff} a\u{3000}"), "a");
}

#[test]
fn max_and_min() {
    assert!(math::max(1.0, f64::NAN).is_nan() && math::min(f64::NAN, 1.0).is_nan());
    assert!(math::max(-0.0, 0.0).is_sign_positive() && math::max(0.0, -0.0).is_sign_positive());
    assert!(math::min(-0.0, 0.0).is_sign_negative() && math::min(0.0, -0.0).is_sign_negative());
    assert_eq!((math::max(2.0, 3.0), math::min(2.0, 3.0)), (3.0, 2.0));
}

#[test]
fn parse_float() {
    let inf = f64::INFINITY;
    for (s, want) in [
        ("1", 1.0),
        (" 2.5x", 2.5),
        (".5", 0.5),
        ("5.", 5.0),
        ("-.5", -0.5),
        ("+3", 3.0),
        ("1e3", 1000.0),
        ("1e", 1.0),
        ("1e+", 1.0),
        ("1E-2", 0.01),
        ("0x10", 0.0),
        ("1_000", 1.0),
        ("Infinityx", inf),
        ("-Infinity", -inf),
        ("1e400", inf),
        (" \n7", 7.0),
    ] {
        assert_eq!(number::parse_float(s), want, "parse_float({s:?})");
    }
    for s in ["", "x", ".", "-", "+.e1", "infinity", "e5"] {
        assert!(number::parse_float(s).is_nan(), "parse_float({s:?})");
    }
}
