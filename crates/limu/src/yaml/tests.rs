//! The reader checked against what Pele makes of the same cases, in the oracle's yaml
//! fixture.

use limu_fixtures::load;
use serde::Deserialize;

use super::parse_yaml;
use crate::Value;
use crate::testing::{Enc, same};

#[derive(Deserialize)]
struct Case {
    source: String,
    value: Option<Enc>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct Fixture {
    yaml: Vec<Case>,
}

#[test]
fn parse_matches_pele() {
    let p: Fixture = load("yaml");
    assert!(p.yaml.len() >= 1000, "only {} cases", p.yaml.len());
    let mut failed = 0;
    for c in &p.yaml {
        let got = parse_yaml(&c.source);
        let ok = match (&got, &c.error) {
            (Err(e), Some(want)) => e.0 == *want,
            (Ok(v), None) => same(v, c.value.as_ref().map_or(&Value::Null, |e| &e.0)),
            _ => false,
        };
        if !ok {
            failed += 1;
            if failed <= 20 {
                eprintln!(
                    "parse_yaml({:?})\n got {got:?}\nPele {:?} {:?}",
                    c.source, c.value, c.error
                );
            }
        }
    }
    assert_eq!(failed, 0, "{failed} of {} differ", p.yaml.len());
}

#[test]
fn reads_block_and_flow() {
    let v = parse_yaml("title: A\nconfig:\n  theme: dark\n  list: [1, two, {a: b}]\n").unwrap();
    let m = v.as_object().unwrap();
    assert_eq!(m.get("title"), Some(&Value::from("A")));
    let config = m.get("config").and_then(Value::as_object).unwrap();
    assert_eq!(config.keys().collect::<Vec<_>>(), ["theme", "list"]);
    assert_eq!(
        config
            .get("list")
            .and_then(Value::as_array)
            .map(<[Value]>::len),
        Some(3)
    );
    let pair = parse_yaml("a: \"\\ud83d\\ude00\"").unwrap();
    assert_eq!(pair.as_object().unwrap().get("a"), Some(&Value::from("😀")));
}
