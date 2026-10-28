//! The values of a diagram's configuration, as JavaScript holds what JSON and YAML read.

use crate::js::number;

/// A value of a diagram's configuration: one of JSON's kinds of value, as JavaScript holds
/// it once JSON.parse or a YAML reader has read it.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
    /// `null`.
    #[default]
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number, which may be NaN or an infinity where YAML writes one.
    Number(f64),
    /// A string.
    String(String),
    /// An array.
    Array(Vec<Value>),
    /// An object, its keys in the order JavaScript gives them.
    Object(Map),
}

impl Value {
    /// The string, if the value is one.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// The number, if the value is one.
    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            Value::Number(n) => Some(n),
            _ => None,
        }
    }

    /// The boolean, if the value is one.
    pub fn as_bool(&self) -> Option<bool> {
        match *self {
            Value::Bool(b) => Some(b),
            _ => None,
        }
    }

    /// The array, if the value is one.
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    /// The object, if the value is one.
    pub fn as_object(&self) -> Option<&Map> {
        match self {
            Value::Object(m) => Some(m),
            _ => None,
        }
    }

    /// Whether the value is null.
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// String() of the value: an array is its items' strings joined by commas, with null as
    /// nothing, and an object is "[object Object]".
    pub(crate) fn to_js_string(&self) -> String {
        match self {
            Value::Null => "null".into(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => number::to_string(*n),
            Value::String(s) => s.clone(),
            Value::Array(items) => items
                .iter()
                .map(|v| {
                    if v.is_null() {
                        String::new()
                    } else {
                        v.to_js_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(","),
            Value::Object(_) => "[object Object]".into(),
        }
    }

    /// Whether JavaScript takes the value as true: all but null, false, 0, NaN and "".
    pub(crate) fn truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0.0 && !n.is_nan(),
            Value::String(s) => !s.is_empty(),
            Value::Array(_) | Value::Object(_) => true,
        }
    }

    /// Number() of the value.
    pub(crate) fn to_number(&self) -> f64 {
        match self {
            Value::Null => 0.0,
            Value::Bool(b) => f64::from(u8::from(*b)),
            Value::Number(n) => *n,
            Value::String(s) => number::number(s),
            Value::Array(_) => number::number(&self.to_js_string()),
            Value::Object(_) => f64::NAN,
        }
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Value {
        Value::Bool(b)
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Value {
        Value::Number(n)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Value {
        Value::String(s.into())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Value {
        Value::String(s)
    }
}

impl From<Vec<Value>> for Value {
    fn from(a: Vec<Value>) -> Value {
        Value::Array(a)
    }
}

impl From<Map> for Value {
    fn from(m: Map) -> Value {
        Value::Object(m)
    }
}

/// An object's keys and values, in the order JavaScript gives an object's keys: those that
/// are array indices, "0" to "4294967294" written without leading zeros, in numeric order,
/// and then the rest in the order they were first set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Map {
    entries: Vec<(String, Value)>,
}

impl Map {
    /// An empty map.
    pub fn new() -> Map {
        Map::default()
    }

    /// The number of keys.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the map has no keys.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The value of a key.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// The value of a key, to change.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.entries
            .iter_mut()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// Whether the map has a key.
    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// Sets a key's value. A key set again keeps its place.
    pub fn insert(&mut self, key: impl Into<String>, value: Value) {
        let key = key.into();
        if let Some(v) = self.get_mut(&key) {
            *v = value;
            return;
        }
        match index_of(&key) {
            Some(n) => {
                let at = self
                    .entries
                    .iter()
                    .position(|(k, _)| index_of(k).is_none_or(|m| m > n))
                    .unwrap_or(self.entries.len());
                self.entries.insert(at, (key, value));
            }
            None => self.entries.push((key, value)),
        }
    }

    /// Takes a key out, and gives its value.
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        let at = self.entries.iter().position(|(k, _)| k == key)?;
        Some(self.entries.remove(at).1)
    }

    /// The keys and their values, in order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// The keys, in order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(k, _)| k.as_str())
    }
}

impl<K: Into<String>> FromIterator<(K, Value)> for Map {
    fn from_iter<I: IntoIterator<Item = (K, Value)>>(iter: I) -> Map {
        let mut m = Map::new();
        for (k, v) in iter {
            m.insert(k, v);
        }
        m
    }
}

impl<'a> IntoIterator for &'a Map {
    type Item = (&'a str, &'a Value);
    type IntoIter = Box<dyn Iterator<Item = (&'a str, &'a Value)> + 'a>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

/// The number a key is as an array index, if it is one as JavaScript has them.
fn index_of(k: &str) -> Option<u32> {
    if k.is_empty() || k.len() > 10 || k.len() > 1 && k.starts_with('0') {
        return None;
    }
    if !k.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    k.parse::<u64>()
        .ok()
        .filter(|&n| n < 4294967295)
        .map(|n| n as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_in_javascript_order() {
        let m: Map = [
            ("b", Value::Null),
            ("10", 1.0.into()),
            ("a", true.into()),
            ("2", "x".into()),
        ]
        .into_iter()
        .collect();
        assert_eq!(m.keys().collect::<Vec<_>>(), ["2", "10", "b", "a"]);
        let mut m = m;
        m.insert("b", 2.0.into());
        m.insert("01", Value::Null);
        m.insert("4294967295", Value::Null);
        assert_eq!(
            m.keys().collect::<Vec<_>>(),
            ["2", "10", "b", "a", "01", "4294967295"]
        );
        assert_eq!(m.get("b"), Some(&Value::Number(2.0)));
    }

    #[test]
    fn javascript_conversions() {
        let a = Value::Array(vec![1.0.into(), Value::Null, "x".into()]);
        assert_eq!(a.to_js_string(), "1,,x");
        assert!(Value::Array(vec![]).truthy());
        assert!(Value::Array(vec![]).to_number() == 0.0);
        assert!(!Value::Number(f64::NAN).truthy());
    }
}
