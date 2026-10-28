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
    let path = dir().join(format!("{name}.json.gz"));
    let file = File::open(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run ./scripts/fixtures.sh", path.display()));
    let mut json = String::new();
    GzDecoder::new(file)
        .read_to_string(&mut json)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&json).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}
