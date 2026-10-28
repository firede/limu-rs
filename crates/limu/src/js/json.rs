//! JSON.parse, as Pele's directives call it on the body of an init directive.

use crate::value::{Map, Value};

/// JSON.parse: strict JSON, or none. A number too large for a double is an infinity, as
/// JavaScript reads it. A `\u` escape of a lone surrogate, which a Rust string cannot hold,
/// reads as U+FFFD.
pub(crate) fn parse_json(s: &str) -> Option<Value> {
    let mut p = Parser {
        s: s.as_bytes(),
        src: s,
        i: 0,
    };
    p.space();
    let v = p.value(0)?;
    p.space();
    (p.i == s.len()).then_some(v)
}

/// How deep arrays and objects may nest. V8 reads them to any depth, but reading them here
/// takes stack, so deeper text is taken as not JSON, and the directive is ignored. No
/// configuration nests so deep.
const MAX_DEPTH: usize = 2000;

struct Parser<'a> {
    s: &'a [u8],
    src: &'a str,
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn eat(&mut self, c: u8) -> bool {
        let ok = self.peek() == Some(c);
        if ok {
            self.i += 1;
        }
        ok
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn value(&mut self, depth: usize) -> Option<Value> {
        if depth > MAX_DEPTH {
            return None;
        }
        match self.peek()? {
            b'{' => {
                self.i += 1;
                let mut out = Map::new();
                self.space();
                if self.eat(b'}') {
                    return Some(Value::Object(out));
                }
                loop {
                    self.space();
                    if self.peek() != Some(b'"') {
                        return None;
                    }
                    let key = self.string()?;
                    self.space();
                    if !self.eat(b':') {
                        return None;
                    }
                    self.space();
                    let v = self.value(depth + 1)?;
                    out.insert(key, v);
                    self.space();
                    if self.eat(b',') {
                        continue;
                    }
                    return self.eat(b'}').then_some(Value::Object(out));
                }
            }
            b'[' => {
                self.i += 1;
                let mut out = Vec::new();
                self.space();
                if self.eat(b']') {
                    return Some(Value::Array(out));
                }
                loop {
                    self.space();
                    out.push(self.value(depth + 1)?);
                    self.space();
                    if self.eat(b',') {
                        continue;
                    }
                    return self.eat(b']').then_some(Value::Array(out));
                }
            }
            b'"' => self.string().map(Value::String),
            b'-' | b'0'..=b'9' => self.number(),
            _ => {
                for (word, v) in [
                    ("true", Value::Bool(true)),
                    ("false", Value::Bool(false)),
                    ("null", Value::Null),
                ] {
                    if self.src[self.i..].starts_with(word) {
                        self.i += word.len();
                        return Some(v);
                    }
                }
                None
            }
        }
    }

    fn digits(&mut self) -> usize {
        let from = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        self.i - from
    }

    fn number(&mut self) -> Option<Value> {
        let start = self.i;
        self.eat(b'-');
        if !self.eat(b'0') && self.digits() == 0 {
            return None;
        }
        if self.eat(b'.') && self.digits() == 0 {
            return None;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if self.digits() == 0 {
                return None;
            }
        }
        // Rust reads a decimal correctly rounded, as JavaScript does.
        self.src[start..self.i].parse().ok().map(Value::Number)
    }

    fn string(&mut self) -> Option<String> {
        self.i += 1;
        let mut out = String::new();
        let mut units: Vec<u16> = Vec::new();
        let flush = |out: &mut String, units: &mut Vec<u16>| {
            if !units.is_empty() {
                out.push_str(&String::from_utf16_lossy(units));
                units.clear();
            }
        };
        loop {
            let c = self.peek()?;
            match c {
                b'"' => {
                    self.i += 1;
                    flush(&mut out, &mut units);
                    return Some(out);
                }
                0..0x20 => return None,
                b'\\' => {
                    let e = *self.s.get(self.i + 1)?;
                    self.i += 2;
                    if e == b'u' {
                        let hex = self.s.get(self.i..self.i + 4)?;
                        if !hex.iter().all(u8::is_ascii_hexdigit) {
                            return None;
                        }
                        units.push(u16::from_str_radix(&self.src[self.i..self.i + 4], 16).ok()?);
                        self.i += 4;
                        continue;
                    }
                    flush(&mut out, &mut units);
                    out.push(match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        _ => return None,
                    });
                }
                _ => {
                    flush(&mut out, &mut units);
                    let ch = self.src[self.i..].chars().next()?;
                    out.push(ch);
                    self.i += ch.len_utf8();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_json() {
        let v =
            parse_json(r#" {"a": [1, -0.5e1, true, null], "b": "x\u00e9\ud83d\ude00\n", "a": 2} "#);
        let m = v.as_ref().and_then(Value::as_object).unwrap();
        assert_eq!(m.keys().collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(m.get("a"), Some(&Value::Number(2.0)));
        assert_eq!(m.get("b").and_then(Value::as_str), Some("xé😀\n"));
        assert_eq!(parse_json("1e400"), Some(Value::Number(f64::INFINITY)));
        for bad in [
            "",
            "01",
            "1.",
            "{'a': 1}",
            "[1,]",
            "\"\\u+123\"",
            "\"a\tb\"",
            "nul",
            "1 2",
        ] {
            assert_eq!(parse_json(bad), None, "{bad}");
        }
    }
}
