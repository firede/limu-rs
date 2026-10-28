//! What the tests of limu share: reading values as the oracle writes them.

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
