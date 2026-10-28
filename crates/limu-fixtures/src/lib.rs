//! Reads the fixtures of the pele-oracle release that `fixtures.toml` pins, and the fonts it
//! names, for the tests of limu-rs. `./scripts/fixtures.sh` downloads the fixtures into
//! `target/pele-fixtures/<tag>` and the fonts into `target/fonts`. Where `LIMU_FIXTURES` is
//! set, the fixtures are read from that folder instead, as a local pele-oracle writes them.

use std::env;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use flate2::read::GzDecoder;
use serde::de::DeserializeOwned;

mod drawing;

pub use drawing::{Drawing, Drawings, FULL_CASES, at, digest, first_diff, load_drawings};

/// The root of the workspace.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The value of a key of fixtures.toml.
fn pinned(key: &str) -> String {
    let pins =
        std::fs::read_to_string(root().join("fixtures.toml")).expect("reading fixtures.toml");
    pins.lines()
        .find_map(|l| l.strip_prefix(&format!("{key} = \"")))
        .and_then(|l| l.strip_suffix('"'))
        .unwrap_or_else(|| panic!("no {key} in fixtures.toml"))
        .to_string()
}

/// The folder the fixtures are read from.
pub fn dir() -> PathBuf {
    if let Some(d) = env::var_os("LIMU_FIXTURES") {
        return PathBuf::from(d);
    }
    root().join("target/pele-fixtures").join(pinned("tag"))
}

/// The data of a font of the archive fixtures.toml names, by its file's name, as
/// "LiberationSans-Regular.ttf".
pub fn font(name: &str) -> Vec<u8> {
    let fonts = pinned("fonts");
    let folder = fonts
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .trim_end_matches(".tar.gz");
    let path = root().join("target/fonts").join(folder).join(name);
    std::fs::read(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run ./scripts/fixtures.sh", path.display()))
}

/// The fixture of a name, as "svg" or "flowchart-render", read as a T.
pub fn load<T: DeserializeOwned>(name: &str) -> T {
    try_load(name).unwrap_or_else(|e| panic!("{e}"))
}

/// The fixture of a name read as a T, or what keeps it from being read.
fn try_load<T: DeserializeOwned>(name: &str) -> Result<T, String> {
    let path = dir().join(format!("{name}.json.gz"));
    let file = File::open(&path)
        .map_err(|e| format!("{}: {e}; run ./scripts/fixtures.sh", path.display()))?;
    let mut json = String::new();
    GzDecoder::new(file)
        .read_to_string(&mut json)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&lone_surrogates_replaced(&json))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// JSON with each `\u` escape of half a surrogate pair that has no other half beside it
/// written as one of U+FFFD, which serde_json reads where it reads no lone surrogate. A Rust
/// string cannot hold half a pair, so limu-rs takes U+FFFD for one, as JSON readers commonly
/// do. JavaScript's strings can hold one, and JSON.stringify writes it as an escape.
fn lone_surrogates_replaced(json: &str) -> std::borrow::Cow<'_, str> {
    if !json.contains("\\ud") && !json.contains("\\uD") {
        return json.into();
    }
    // The code unit of the \u escape at i, if there is one.
    let unit = |i: usize| {
        let b = json.as_bytes();
        (b.get(i) == Some(&b'\\') && b.get(i + 1) == Some(&b'u'))
            .then(|| json.get(i + 2..i + 6))
            .flatten()
            .and_then(|hex| u16::from_str_radix(hex, 16).ok())
    };
    let is_high = |u: u16| (0xd800..0xdc00).contains(&u);
    let is_low = |u: u16| (0xdc00..0xe000).contains(&u);
    let b = json.as_bytes();
    let mut out = String::with_capacity(json.len());
    let (mut last, mut i) = (0, 0);
    while i < b.len() {
        if b[i] != b'\\' {
            i += 1;
            continue;
        }
        match unit(i) {
            Some(u) if is_high(u) && unit(i + 6).is_some_and(is_low) => i += 12,
            Some(u) if is_high(u) || is_low(u) => {
                out.push_str(&json[last..i]);
                out.push_str("\\ufffd");
                i += 6;
                last = i;
            }
            Some(_) => i += 6,
            None => i += 2,
        }
    }
    out.push_str(&json[last..]);
    out.into()
}

/// A number as the oracle writes it: a JSON number, or `{"$n": …}` for -0, NaN and the
/// infinities, which JSON has no words for.
#[derive(Clone, Copy, Debug)]
pub struct Num(pub f64);

impl<'de> serde::Deserialize<'de> for Num {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Num, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Plain(f64),
            Word {
                #[serde(rename = "$n")]
                n: String,
            },
        }
        Ok(Num(match Raw::deserialize(d)? {
            Raw::Plain(f) => f,
            Raw::Word { n } => match n.as_str() {
                "-0" => -0.0,
                "NaN" => f64::NAN,
                "Infinity" => f64::INFINITY,
                "-Infinity" => f64::NEG_INFINITY,
                _ => return Err(serde::de::Error::custom(format!("no number {n:?}"))),
            },
        }))
    }
}

impl Num {
    /// Whether two numbers are the same to the bit, as every NaN is the same.
    pub fn same(a: f64, b: f64) -> bool {
        a.to_bits() == b.to_bits() || a.is_nan() && b.is_nan()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lone_surrogates_read_as_replacement() {
        let json = r#"["😀", "a\ud83dx", "\ude00", "\\ud83d", "é"]"#;
        let v: Vec<String> = serde_json::from_str(&lone_surrogates_replaced(json)).unwrap();
        assert_eq!(v, ["😀", "a\u{fffd}x", "\u{fffd}", "\\ud83d", "é"]);
    }
}
