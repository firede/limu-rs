//! JavaScript's conversions between numbers and strings: String() of a number, toFixed,
//! toExponential and toPrecision as V8 writes them, Number() of a string and parseInt.

use super::chars::{is_digit, trim, trim_start};

/// String() of a number: the fewest digits that read back as it, written out in full from
/// 1e-7 up to 1e21 and with an exponent outside that.
pub(crate) fn to_string(f: f64) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f == 0.0 {
        return "0".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if f < 0.0 {
        return format!("-{}", to_string(-f));
    }
    if f < 9007199254740992.0 && f == f.trunc() {
        // Below 2⁵³ doubles are no more than 1 apart, so a whole number reads back only from
        // all of its digits.
        return format!("{}", f as u64);
    }
    let (digits, x) = shortest(f);
    let k = digits.len() as i32;
    let n = x + 1;
    if k <= n && n <= 21 {
        return digits + &"0".repeat((n - k) as usize);
    }
    if 0 < n && n <= 21 {
        let (a, b) = digits.split_at(n as usize);
        return format!("{a}.{b}");
    }
    if -6 < n && n <= 0 {
        return format!("0.{}{digits}", "0".repeat((-n) as usize));
    }
    let e10 = n - 1;
    let sign = if e10 < 0 { '-' } else { '+' };
    if k == 1 {
        format!("{digits}e{sign}{}", e10.abs())
    } else {
        format!("{}.{}e{sign}{}", &digits[..1], &digits[1..], e10.abs())
    }
}

/// The fewest digits that read back as f, which is finite and more than 0, and the exponent
/// of the first. Rust picks them as JavaScript does but for where f is halfway between two
/// that are as few and as near, which V8 settles to the even one, and Rust need not.
fn shortest(f: f64) -> (String, i32) {
    let e = format!("{f:e}");
    let (mant, exp) = e.split_once('e').unwrap_or((&e, "0"));
    let mut digits = mant.replace('.', "");
    let x: i32 = exp.parse().unwrap_or(0);
    let k = digits.len();
    let exact = Exact::of(f);
    let first = exact.digits.len() as i32 - 1 - exact.point as i32;
    if exact.digits.len() == k + 1 && exact.digits[k] == 5 && first == x {
        let mut even: Vec<u8> = exact.digits[..k].to_vec();
        if even[k - 1] % 2 == 1 && even[k - 1] < 9 {
            even[k - 1] += 1;
        }
        let even: String = even.iter().map(|&c| char::from(b'0' + c)).collect();
        if even != digits && (even.as_bytes()[k - 1] - b'0').is_multiple_of(2) {
            let back: f64 = format!("{even}e{}", x - (k as i32 - 1))
                .parse()
                .unwrap_or(0.0);
            if back == f {
                digits = even;
            }
        }
    }
    (digits, x)
}

// toFixed, toExponential and toPrecision write a number with a set number of digits, as the
// language defines them and V8 writes them. Each rounds the exact value of the double, and a
// value halfway between two results takes the larger one.

/// Number.prototype.toFixed: x with digits digits after the point. NaN is "NaN", and a value
/// of 1e21 or more across, or an infinity, is written as String() writes it.
pub(crate) fn to_fixed(x: f64, digits: usize) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.abs() >= 1e21 {
        return to_string(x);
    }
    let sign = if x < 0.0 { "-" } else { "" };
    let exact = Exact::of(x.abs());
    let mut s = exact.rounded(exact.digits.len() as isize - exact.point as isize + digits as isize);
    if digits > 0 {
        if s.len() <= digits {
            s = "0".repeat(digits + 1 - s.len()) + &s;
        }
        s.insert(s.len() - digits, '.');
    }
    format!("{sign}{s}")
}

/// Number.prototype.toExponential with its digits given: x as one digit, digits more after
/// the point, and an exponent, "1.50e+3".
pub(crate) fn to_exponential(x: f64, digits: usize) -> String {
    let (sign, mut m, e) = match significand(x, digits + 1) {
        Ok(s) => s,
        Err(word) => return word,
    };
    if digits > 0 {
        m.insert(1, '.');
    }
    format!("{sign}{m}{}", exponent(e))
}

/// Number.prototype.toPrecision with its precision given: x to that many significant digits,
/// written out in full when its exponent is from -6 to one less than the precision, and with
/// an exponent otherwise.
pub(crate) fn to_precision(x: f64, precision: usize) -> String {
    let (sign, mut m, e) = match significand(x, precision) {
        Ok(s) => s,
        Err(word) => return word,
    };
    let p = precision as i32;
    if e < -6 || e >= p {
        if precision > 1 {
            m.insert(1, '.');
        }
        return format!("{sign}{m}{}", exponent(e));
    }
    if e == p - 1 {
        return format!("{sign}{m}");
    }
    if e >= 0 {
        m.insert((e + 1) as usize, '.');
        return format!("{sign}{m}");
    }
    format!("{sign}0.{}{m}", "0".repeat((-(e + 1)) as usize))
}

/// The sign of x, its first n significant digits rounded, and the exponent of the first, as
/// toExponential and toPrecision find them: the n digits and exponent whose value is nearest
/// x, the larger of two as near. Of NaN and the infinities it gives what they write as.
fn significand(x: f64, n: usize) -> Result<(&'static str, String, i32), String> {
    if x.is_nan() {
        return Err("NaN".into());
    }
    let sign = if x < 0.0 { "-" } else { "" };
    let x = x.abs();
    if x.is_infinite() {
        return Err(format!("{sign}Infinity"));
    }
    if x == 0.0 {
        return Ok((sign, "0".repeat(n), 0));
    }
    let exact = Exact::of(x);
    // The exponent of x's first digit: 10^e <= x < 10^(e+1).
    let mut e = exact.digits.len() as i32 - 1 - exact.point as i32;
    let mut digits = exact.rounded(n as isize);
    if digits.len() > n {
        // Rounded up to the next power of ten.
        e += 1;
        digits.truncate(n);
    }
    Ok((sign, digits, e))
}

fn exponent(e: i32) -> String {
    if e < 0 {
        format!("e-{}", -e)
    } else {
        format!("e+{e}")
    }
}

/// The exact value of a double that is finite and not negative, in decimal: its digits, with
/// no zeros before them, and how many of them are after the point.
struct Exact {
    digits: Vec<u8>,
    point: usize,
}

impl Exact {
    fn of(x: f64) -> Exact {
        let bits = x.to_bits();
        let exp = ((bits >> 52) & 0x7ff) as i32;
        let frac = bits & ((1 << 52) - 1);
        let (m, e) = if exp == 0 {
            (frac, -1074)
        } else {
            (frac | (1 << 52), exp - 1075)
        };
        let mut n = Big::from(m);
        let point = if e >= 0 {
            n.shl(e as u32);
            0
        } else {
            // m·2^e is m·5^-e / 10^-e. 5^13 is the largest power of 5 a u32 holds.
            let (thirteens, rest) = (-e / 13, -e % 13);
            for _ in 0..thirteens {
                n.mul_small(5u32.pow(13));
            }
            n.mul_small(5u32.pow(rest as u32));
            (-e) as usize
        };
        let mut digits = n.decimal();
        // Zeros at the end after the point say nothing.
        let mut point = point;
        while point > 0 && digits.last() == Some(&0) {
            digits.pop();
            point -= 1;
        }
        Exact { digits, point }
    }

    /// The first k digits, rounded half up by those after them, as a string with no zeros
    /// before it but "0" for nothing. Where k is more than there are digits, zeros follow.
    fn rounded(&self, k: isize) -> String {
        let d = &self.digits;
        if k <= 0 {
            let up = k == 0 && d.first().is_some_and(|&c| c >= 5);
            return if up { "1" } else { "0" }.into();
        }
        let k = k as usize;
        let mut out: Vec<u8> = d.iter().copied().take(k).collect();
        out.resize(k, 0);
        if d.get(k).is_some_and(|&c| c >= 5) {
            let mut i = out.len();
            loop {
                if i == 0 {
                    out.insert(0, 1);
                    break;
                }
                i -= 1;
                if out[i] == 9 {
                    out[i] = 0;
                } else {
                    out[i] += 1;
                    break;
                }
            }
        }
        let first = out.iter().position(|&c| c != 0).unwrap_or(out.len() - 1);
        out[first..].iter().map(|&c| char::from(b'0' + c)).collect()
    }
}

/// A whole number of any size, its 32-bit pieces least first.
struct Big(Vec<u32>);

impl Big {
    fn from(n: u64) -> Big {
        Big(vec![n as u32, (n >> 32) as u32])
    }

    fn mul_small(&mut self, k: u32) {
        let mut carry = 0u64;
        for d in &mut self.0 {
            let v = u64::from(*d) * u64::from(k) + carry;
            *d = v as u32;
            carry = v >> 32;
        }
        if carry > 0 {
            self.0.push(carry as u32);
        }
    }

    fn shl(&mut self, bits: u32) {
        let (words, bits) = ((bits / 32) as usize, bits % 32);
        if bits > 0 {
            let mut carry = 0u32;
            for d in &mut self.0 {
                let v = (*d << bits) | carry;
                carry = *d >> (32 - bits);
                *d = v;
            }
            if carry > 0 {
                self.0.push(carry);
            }
        }
        self.0.splice(0..0, std::iter::repeat_n(0, words));
    }

    fn is_zero(&self) -> bool {
        self.0.iter().all(|&d| d == 0)
    }

    /// Divides by a small number, and gives the remainder.
    fn div_small(&mut self, k: u32) -> u32 {
        let mut rem = 0u64;
        for d in self.0.iter_mut().rev() {
            let v = (rem << 32) | u64::from(*d);
            *d = (v / u64::from(k)) as u32;
            rem = v % u64::from(k);
        }
        rem as u32
    }

    /// The number's decimal digits, as numbers from 0 to 9, most significant first.
    fn decimal(mut self) -> Vec<u8> {
        let mut chunks = Vec::new();
        while !self.is_zero() {
            chunks.push(self.div_small(1_000_000_000));
        }
        let mut out = Vec::with_capacity(chunks.len() * 9);
        for (i, c) in chunks.iter().rev().enumerate() {
            let s = if i == 0 {
                c.to_string()
            } else {
                format!("{c:09}")
            };
            out.extend(s.bytes().map(|b| b - b'0'));
        }
        if out.is_empty() {
            out.push(0);
        }
        out
    }

    /// The nearest double, the even of two as near.
    fn to_f64(&self) -> f64 {
        let mut words = self.0.clone();
        while words.last() == Some(&0) {
            words.pop();
        }
        let Some(&top) = words.last() else { return 0.0 };
        let bits = (words.len() as u32 - 1) * 32 + (32 - top.leading_zeros());
        if bits <= 64 {
            let v = words
                .iter()
                .rev()
                .fold(0u64, |a, &w| (a << 32) | u64::from(w));
            return v as f64;
        }
        // The top 64 bits, with whether any bit below them is set.
        let shift = bits - 64;
        let bit = |i: u32| (words[(i / 32) as usize] >> (i % 32)) & 1;
        let mut v = 0u64;
        for i in (shift..bits).rev() {
            v = (v << 1) | u64::from(bit(i));
        }
        let sticky = (0..shift).any(|i| bit(i) == 1);
        // Round the 64 bits to 53, by the 11 below and the sticky bits, to even.
        let (keep, rest) = (v >> 11, v & 0x7ff);
        let up = rest > 0x400 || (rest == 0x400 && (sticky || keep & 1 == 1));
        let m = keep + u64::from(up);
        (m as f64) * 2f64.powi((shift + 11) as i32)
    }
}

/// Number() of a string: white space around it is ignored, an empty string is 0, and
/// anything that is not a number as JavaScript writes one is NaN.
pub(crate) fn number(s: &str) -> f64 {
    let s = trim(s);
    if s.is_empty() {
        return 0.0;
    }
    let b = s.as_bytes();
    if b.len() > 2 && b[0] == b'0' {
        let base = match b[1] {
            b'x' | b'X' => 16,
            b'o' | b'O' => 8,
            b'b' | b'B' => 2,
            _ => 0,
        };
        if base != 0 {
            return digits_value(&s[2..], base, false);
        }
    }
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    if body == "Infinity" {
        return if b[0] == b'-' {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    if !is_decimal(body) {
        return f64::NAN;
    }
    s.parse().unwrap_or(f64::NAN)
}

/// Whether s is digits with at most one point and some digit, and then an optional exponent:
/// JavaScript's StrUnsignedDecimalLiteral.
fn is_decimal(s: &str) -> bool {
    let b = s.as_bytes();
    let (mut i, mut digits) = (0, 0);
    let run = |i: &mut usize| {
        let from = *i;
        while *i < b.len() && is_digit(i32::from(b[*i])) {
            *i += 1;
        }
        *i - from
    };
    digits += run(&mut i);
    if i < b.len() && b[i] == b'.' {
        i += 1;
        digits += run(&mut i);
    }
    if digits == 0 {
        return false;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        if run(&mut i) == 0 {
            return false;
        }
    }
    i == b.len()
}

/// The value of s in base, rounded to the nearest number. With prefix set, it reads the
/// digits s starts with, as parseInt does, and otherwise all of s must be digits. Either way
/// no digits is NaN.
fn digits_value(s: &str, base: u32, prefix: bool) -> f64 {
    let end = s
        .bytes()
        .position(|c| char::from(c).to_digit(36).is_none_or(|d| d >= base))
        .unwrap_or(s.len());
    if end == 0 || !prefix && end != s.len() {
        return f64::NAN;
    }
    let mut n = Big::from(0);
    for c in s[..end].chars() {
        n.mul_small(base);
        let d = c.to_digit(base).unwrap_or(0);
        let mut carry = u64::from(d);
        for w in &mut n.0 {
            let v = u64::from(*w) + carry;
            *w = v as u32;
            carry = v >> 32;
        }
        if carry > 0 {
            n.0.push(carry as u32);
        }
    }
    n.to_f64()
}

/// parseInt(s, radix) for a radix from 2 to 36: white space before the number is skipped, a
/// sign is read, and so is 0x before a number in base 16, then as many digits as there are.
/// With none, it is NaN.
pub(crate) fn parse_int(s: &str, radix: u32) -> f64 {
    let mut s = trim_start(s);
    let mut neg = false;
    if let Some(rest) = s.strip_prefix(['+', '-']) {
        neg = s.starts_with('-');
        s = rest;
    }
    if radix == 16 && (s.starts_with("0x") || s.starts_with("0X")) {
        s = &s[2..];
    }
    let f = if radix == 10 {
        let end = s
            .bytes()
            .position(|c| !c.is_ascii_digit())
            .unwrap_or(s.len());
        if end == 0 {
            return f64::NAN;
        }
        s[..end].parse().unwrap_or(f64::NAN)
    } else {
        digits_value(s, radix, true)
    };
    if neg { -f } else { f }
}
