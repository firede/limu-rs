//! What the tests of limu share: reading values as the oracle writes them, and counting the
//! cases that differ from it.

use serde::Deserialize;

use crate::value::{Map, Value};

/// A value as the oracle writes it: JSON, with an object as `{"$o": [[key, value], …]}` in
/// JavaScript's order, and the numbers JSON has no words for as `{"$n": "NaN"}` and the like.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Enc(pub(crate) Value);

impl<'de> Deserialize<'de> for Enc {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Enc, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Null(()),
            Bool(bool),
            Number(f64),
            String(String),
            Array(Vec<Enc>),
            Object {
                #[serde(rename = "$o")]
                o: Vec<(String, Enc)>,
            },
            Word(limu_fixtures::Num),
        }
        Ok(Enc(match Raw::deserialize(d)? {
            Raw::Null(()) => Value::Null,
            Raw::Bool(b) => Value::Bool(b),
            Raw::Number(n) => Value::Number(n),
            Raw::String(s) => Value::String(s),
            Raw::Array(a) => Value::Array(a.into_iter().map(|e| e.0).collect()),
            Raw::Object { o } => Value::Object(o.into_iter().map(|(k, v)| (k, v.0)).collect()),
            Raw::Word(n) => Value::Number(n.0),
        }))
    }
}

/// Whether two values are the same, numbers to the bit and keys in order.
pub(crate) fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => limu_fixtures::Num::same(*x, *y),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|((k, a), (l, b))| k == l && same(a, b))
        }
        _ => a == b,
    }
}

/// The value with every object's keys sorted, as the oracle writes some.
pub(crate) fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(m) => {
            let mut entries: Vec<(&str, Value)> = m.iter().map(|(k, v)| (k, sorted(v))).collect();
            // JavaScript sorts strings by their code units.
            entries.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
            // Map keeps array indices first whatever the order they are set in, so sorted keys
            // go into a map as entries the map does not reorder.
            Value::Object(Map::sorted(entries))
        }
        _ => v.clone(),
    }
}

/// Counts the cases where got is not want, and shows the first of them.
pub(crate) struct Failures(usize, usize);

impl Failures {
    pub(crate) fn new() -> Failures {
        Failures(0, 0)
    }

    pub(crate) fn check(&mut self, ok: bool, show: impl FnOnce() -> String) {
        self.1 += 1;
        if !ok {
            self.0 += 1;
            if self.0 <= 20 {
                eprintln!("{}", show());
            }
        }
    }

    pub(crate) fn done(self, name: &str, at_least: usize) {
        assert!(self.0 == 0, "{name}: {} of {} differ", self.0, self.1);
        assert!(self.1 >= at_least, "{name}: only {} cases", self.1);
    }
}

/// A number as Pele's num writes it into its SVG: to two decimals, or as String writes it
/// where the hundredths are 1e11 or more either way, or are not a finite number.
pub(crate) fn pele_num(v: f64) -> String {
    let n = crate::js::math::round(v * 100.0);
    if n == 0.0 {
        return "0".to_string();
    }
    if !(n.abs() < 1e11) {
        return crate::js::number::to_string(n / 100.0);
    }
    let (sign, m) = if n < 0.0 { ("-", -n) } else { ("", n) };
    let q = (m / 100.0).floor();
    let i = (m - q * 100.0) as u32;
    let fraction = match i {
        0 => String::new(),
        _ if i.is_multiple_of(10) => format!(".{}", i / 10),
        _ => format!(".{i:02}"),
    };
    format!("{sign}{q}{fraction}")
}

/// A path as Pele writes the d of one.
pub(crate) fn pele_d(p: &crate::scene::Path) -> String {
    use crate::scene::Op;
    let mut out = String::new();
    let mut at = 0;
    for &op in &p.ops {
        out.push(match op {
            Op::MoveTo => 'M',
            Op::LineTo => 'L',
            Op::QuadTo => 'Q',
            Op::CubeTo => 'C',
            Op::Close => 'Z',
        });
        let a = &p.args[at..at + op.args()];
        at += op.args();
        for (k, xy) in a.chunks(2).enumerate() {
            if k > 0 {
                out.push(' ');
            }
            out.push_str(&format!("{},{}", pele_num(xy[0]), pele_num(xy[1])));
        }
    }
    out
}
