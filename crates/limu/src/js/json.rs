//! JSON.parse, as Pele's directives call it on the body of an init directive, and as V8 runs
//! it on the JSON a diagram's text holds, with the messages of the errors it throws, and
//! JSON.stringify, as Pele's agent flows write a value in an error.

use crate::value::{Map, Value};

/// JSON.parse as V8 runs it on text of UTF-16 code units: the value it gives, or the message
/// of the SyntaxError it throws, as V8's JsonParser words it. Arrays and objects nest to any
/// depth, as V8 keeps them on a stack of its own. A string that holds a surrogate with no
/// partner holds U+FFFD in its place, which a Rust string can hold.
pub(crate) fn parse_v8(text: &[u16]) -> Result<Value, String> {
    let mut j = V8 { text, at: 0 };
    let v = j.value()?;
    if !j.check(Token::Eos) {
        return Err(j.fail("Unexpected non-whitespace character after JSON"));
    }
    Ok(v)
}

/// The tokens V8's JSON parser reads a character as.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Token {
    Illegal,
    Eos,
    String,
    Number,
    True,
    False,
    Null,
    Whitespace,
    Colon,
    Comma,
    LBrace,
    RBrace,
    LBrack,
    RBrack,
}

/// The token V8 reads code unit c as, -1 being the end of the text.
fn token(c: i32) -> Token {
    match c {
        ..0 => Token::Eos,
        0x22 => Token::String,
        0x2d | 0x30..=0x39 => Token::Number,
        0x74 => Token::True,
        0x66 => Token::False,
        0x6e => Token::Null,
        0x20 | 0x09 | 0x0a | 0x0d => Token::Whitespace,
        0x3a => Token::Colon,
        0x2c => Token::Comma,
        0x7b => Token::LBrace,
        0x7d => Token::RBrace,
        0x5b => Token::LBrack,
        0x5d => Token::RBrack,
        _ => Token::Illegal,
    }
}

/// An array or an object the parser is inside, with what it has read of it: an object's
/// entries, and the key of the value being read.
enum Open {
    Array(Vec<Value>),
    Object(Vec<(String, Value)>, String),
}

/// V8's JsonParser, reading code units.
struct V8<'a> {
    text: &'a [u16],
    at: usize,
}

impl V8<'_> {
    fn cur(&self) -> i32 {
        self.text.get(self.at).map_or(-1, |&c| i32::from(c))
    }

    fn next(&mut self) -> i32 {
        self.at += 1;
        self.cur()
    }

    fn skip(&mut self) {
        while token(self.cur()) == Token::Whitespace {
            self.at += 1;
        }
    }

    /// The text from..to, a surrogate with no partner in it read as U+FFFD.
    fn decode(&self, from: usize, to: usize) -> String {
        String::from_utf16_lossy(&self.text[from..to.min(self.text.len())])
    }

    /// The line and column of the cursor, as V8's CalculateFileLocation counts them: a
    /// carriage return and a line feed after it end one line.
    fn file_location(&self) -> (usize, usize) {
        let (mut line, mut last_break, mut k) = (1, 0, 0);
        while k < self.at {
            if self.text[k] == 13 && k + 1 < self.at && self.text[k + 1] == 10 {
                k += 1;
            }
            if self.text[k] == 13 || self.text[k] == 10 {
                line += 1;
                last_break = k + 1;
            }
            k += 1;
        }
        (line, 1 + k - last_break)
    }

    /// The message of an error that gives the cursor's place.
    fn fail(&self, message: &str) -> String {
        let (line, column) = self.file_location();
        format!(
            "{message} in JSON at position {} (line {line} column {column})",
            self.at
        )
    }

    /// V8's ReportUnexpectedToken with no message of its own: what it says depends on the
    /// token, and for most, on the character at the cursor and the text about it.
    fn unexpected(&self, t: Token) -> String {
        match t {
            Token::Eos => return "Unexpected end of JSON input".into(),
            Token::Number => return self.fail("Unexpected number"),
            Token::String => return self.fail("Unexpected string"),
            _ => {}
        }
        const CONTEXT: usize = 10;
        let c = self.decode(self.at, self.at + 1);
        let (n, pos) = (self.text.len(), self.at);
        if n < 2 * CONTEXT + 1 {
            format!(
                "Unexpected token '{c}', \"{}\" is not valid JSON",
                self.decode(0, n)
            )
        } else if pos < CONTEXT {
            format!(
                "Unexpected token '{c}', \"{}\"... is not valid JSON",
                self.decode(0, pos + CONTEXT)
            )
        } else if pos < n - CONTEXT {
            format!(
                "Unexpected token '{c}', ...\"{}\"... is not valid JSON",
                self.decode(pos - CONTEXT, pos + CONTEXT)
            )
        } else {
            format!(
                "Unexpected token '{c}', ...\"{}\" is not valid JSON",
                self.decode(pos - CONTEXT, n)
            )
        }
    }

    /// V8's ReportUnexpectedCharacter.
    fn unexpected_character(&self, c: i32) -> String {
        self.unexpected(if c < 0 {
            Token::Eos
        } else if c <= 0xff {
            token(c)
        } else {
            Token::Illegal
        })
    }

    /// Takes the token at the cursor if it is t, and fails with message if not.
    fn expect(&mut self, t: Token, message: &str) -> Result<(), String> {
        if token(self.cur()) != t {
            return Err(self.fail(message));
        }
        self.at += 1;
        Ok(())
    }

    fn check(&mut self, t: Token) -> bool {
        self.skip();
        if token(self.cur()) != t {
            return false;
        }
        self.at += 1;
        true
    }

    /// Reads true, false or null, whose first letter is at the cursor, as V8's ScanLiteral
    /// does.
    fn literal(&mut self, word: &str) -> Result<(), String> {
        let word = word.as_bytes();
        let remaining = self.text.len() - self.at;
        if remaining >= word.len()
            && (1..word.len()).all(|k| self.text[self.at + k] == u16::from(word[k]))
        {
            self.at += word.len();
            return Ok(());
        }
        self.at += 1;
        for &w in &word[1..word.len().min(remaining)] {
            if i32::from(w) != self.cur() {
                return Err(self.unexpected_character(self.cur()));
            }
            self.at += 1;
        }
        Err(self.unexpected(Token::Eos))
    }

    /// Reads a string whose opening quote the cursor is past, as V8's ScanJsonString does.
    fn string(&mut self) -> Result<String, String> {
        let mut out: Vec<u16> = Vec::new();
        loop {
            let c = self.cur();
            match c {
                ..0 => return Err(self.fail("Unterminated string")),
                0x22 => {
                    self.at += 1;
                    return Ok(String::from_utf16_lossy(&out));
                }
                0x5c => {
                    let e = self.next();
                    if !(0..=0xff).contains(&e) {
                        return Err(self.unexpected_character(e));
                    }
                    out.push(match e {
                        0x22 | 0x5c | 0x2f => e as u16,
                        0x62 => 8,
                        0x66 => 12,
                        0x6e => 10,
                        0x72 => 13,
                        0x74 => 9,
                        0x75 => {
                            let mut value = 0;
                            for _ in 0..4 {
                                let c = self.next();
                                let Some(d) =
                                    u8::try_from(c).ok().and_then(|c| (c as char).to_digit(16))
                                else {
                                    return Err(self.fail("Bad Unicode escape"));
                                };
                                value = value * 16 + d;
                            }
                            value as u16
                        }
                        _ => return Err(self.fail("Bad escaped character")),
                    });
                    self.at += 1;
                }
                0..0x20 => return Err(self.fail("Bad control character in string literal")),
                _ => {
                    out.push(c as u16);
                    self.at += 1;
                }
            }
        }
    }

    /// Reads a number at the cursor, as V8's ParseJsonNumber does.
    fn number(&mut self) -> Result<f64, String> {
        let digit = |c: i32| (0x30..=0x39).contains(&c);
        let start = self.at;
        let mut c = self.cur();
        if c == 0x2d {
            c = self.next();
        }
        if c == 0x30 {
            // A leading zero is only allowed as the only digit before a decimal point or an
            // exponent.
            if digit(self.next()) {
                return Err(self.unexpected(Token::Number));
            }
        } else {
            let from = self.at;
            while digit(self.cur()) {
                self.at += 1;
            }
            if self.at == from {
                return Err(self.fail("No number after minus sign"));
            }
        }
        if self.cur() == 0x2e {
            if !digit(self.next()) {
                return Err(self.fail("Unterminated fractional number"));
            }
            while digit(self.cur()) {
                self.at += 1;
            }
        }
        let c = self.cur();
        if c >= 0 && c | 0x20 == 0x65 {
            let mut c = self.next();
            if c == 0x2d || c == 0x2b {
                c = self.next();
            }
            if !digit(c) {
                return Err(self.fail("Exponent part is missing a number"));
            }
            while digit(self.cur()) {
                self.at += 1;
            }
        }
        // Rust reads a decimal correctly rounded, as V8 does.
        Ok(self.decode(start, self.at).parse().unwrap_or(f64::NAN))
    }

    /// Reads a key of an object and the colon after it: the first, where the cursor is past
    /// the object's brace, or the next, where it is past a comma.
    fn key(&mut self, message: &str) -> Result<String, String> {
        self.skip();
        self.expect(Token::String, message)?;
        let key = self.string()?;
        self.skip();
        self.expect(Token::Colon, "Expected ':' after property name")?;
        Ok(key)
    }

    /// Reads a value, as V8's ParseJsonValue does, keeping the arrays and objects it is
    /// inside on a stack.
    fn value(&mut self) -> Result<Value, String> {
        let mut open: Vec<Open> = Vec::new();
        loop {
            self.skip();
            let mut v = match token(self.cur()) {
                Token::String => {
                    self.at += 1;
                    Value::String(self.string()?)
                }
                Token::Number => Value::Number(self.number()?),
                Token::LBrace => {
                    self.at += 1;
                    if self.check(Token::RBrace) {
                        Value::Object(Map::new())
                    } else {
                        let key = self.key("Expected property name or '}'")?;
                        open.push(Open::Object(Vec::new(), key));
                        continue;
                    }
                }
                Token::LBrack => {
                    self.at += 1;
                    if self.check(Token::RBrack) {
                        Value::Array(Vec::new())
                    } else {
                        open.push(Open::Array(Vec::new()));
                        continue;
                    }
                }
                Token::True => {
                    self.literal("true")?;
                    Value::Bool(true)
                }
                Token::False => {
                    self.literal("false")?;
                    Value::Bool(false)
                }
                Token::Null => {
                    self.literal("null")?;
                    Value::Null
                }
                _ => return Err(self.unexpected_character(self.cur())),
            };
            // The value may end what it is the last of, and that what that is the last of.
            loop {
                match open.last_mut() {
                    None => return Ok(v),
                    Some(Open::Array(items)) => {
                        items.push(v);
                        if self.check(Token::Comma) {
                            break;
                        }
                        self.expect(Token::RBrack, "Expected ',' or ']' after array element")?;
                        let Some(Open::Array(items)) = open.pop() else {
                            unreachable!("an array is open")
                        };
                        v = Value::Array(items);
                    }
                    Some(Open::Object(entries, key)) => {
                        entries.push((std::mem::take(key), v));
                        if self.check(Token::Comma) {
                            *key = self.key("Expected double-quoted property name")?;
                            break;
                        }
                        self.expect(Token::RBrace, "Expected ',' or '}' after property value")?;
                        let Some(Open::Object(entries, _)) = open.pop() else {
                            unreachable!("an object is open")
                        };
                        // A key written twice keeps its first place and its last value.
                        v = Value::Object(entries.into_iter().collect());
                    }
                }
            }
        }
    }
}

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

/// JSON.stringify of a value: strings with JSON's escapes, numbers that are not finite as
/// null, and an object's keys in its order.
pub(crate) fn stringify(v: &Value) -> String {
    let mut out = String::new();
    write(&mut out, v);
    out
}

fn write(out: &mut String, v: &Value) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) if n.is_finite() => out.push_str(&crate::js::number::to_string(*n)),
        Value::Number(_) => out.push_str("null"),
        Value::String(s) => quote(out, s),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write(out, item);
            }
            out.push(']');
        }
        Value::Object(m) => {
            out.push('{');
            for (i, (k, item)) in m.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                quote(out, k);
                out.push(':');
                write(out, item);
            }
            out.push('}');
        }
    }
}

/// A string as JSON.stringify writes it.
fn quote(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_json() {
        let v = parse_json(r#"{"a": [1, -0.5, true, null], "b": "q\"\\\n\u0001é"}"#).unwrap();
        assert_eq!(
            stringify(&v),
            r#"{"a":[1,-0.5,true,null],"b":"q\"\\\n\u0001é"}"#
        );
        assert_eq!(stringify(&Value::Number(f64::NAN)), "null");
    }

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
