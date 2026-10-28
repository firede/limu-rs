//! Reads the fixtures of the pele-oracle release that `fixtures.toml` pins, for the tests of
//! limu-rs. `./scripts/fixtures.sh` downloads them into `target/pele-fixtures/<tag>`. Where
//! `LIMU_FIXTURES` is set, they are read from that folder instead, as a local pele-oracle
//! writes them.

use std::env;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use flate2::read::GzDecoder;
use serde::de::DeserializeOwned;

/// The folder the fixtures are read from.
pub fn dir() -> PathBuf {
    if let Some(d) = env::var_os("LIMU_FIXTURES") {
        return PathBuf::from(d);
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pins = std::fs::read_to_string(root.join("fixtures.toml")).expect("reading fixtures.toml");
    let tag = pins
        .lines()
        .find_map(|l| l.strip_prefix("tag = \""))
        .and_then(|l| l.strip_suffix('"'))
        .expect("no tag in fixtures.toml");
    root.join("target/pele-fixtures").join(tag)
}

/// The fixture of a name, as "svg" or "flowchart-render", read as a T.
pub fn load<T: DeserializeOwned>(name: &str) -> T {
    let path = dir().join(format!("{name}.json.gz"));
    let file = File::open(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run ./scripts/fixtures.sh", path.display()));
    let mut json = String::new();
    GzDecoder::new(file)
        .read_to_string(&mut json)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&json).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}
