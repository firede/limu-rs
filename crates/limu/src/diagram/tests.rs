//! Diagrams read as a whole against what Pele's parse and render make of the same texts, in
//! the oracle's index fixture.

use limu_fixtures::load;
use serde::Deserialize;

use super::{TYPES, parse};
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::testing::Failures;

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Case {
    source: String,
    #[serde(rename = "type")]
    diagram: Option<String>,
    mermaid_type: Option<String>,
    #[serde(default)]
    parsed: bool,
    error: Option<CaseError>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct CaseError {
    code: String,
    message: String,
    #[serde(rename = "type")]
    diagram: Option<String>,
    mermaid_type: Option<String>,
}

/// Reads each text as a whole, with parse and then layout, as Pele's parse and render read
/// it: the types it finds, as Pele and as Mermaid name them, or its error, which holds them
/// once the type is known. Texts of the types limu is built without are left out.
#[test]
fn diagrams_match_pele() {
    let f: Fixture = load("index");
    let mut failures = Failures::new();
    for c in &f.cases {
        if prepare(&c.source, &ParseOptions::default())
            .is_ok_and(|p| !TYPES.contains(&p.found.diagram))
        {
            continue;
        }
        let d = parse(&c.source, &ParseOptions::default());
        let parsed = d.is_ok();
        let got = d.and_then(|d| d.layout(&Options::default()).map(|_| d));
        let ok = match (&got, &c.error) {
            (Err(e), Some(w)) => {
                parsed == c.parsed
                    && e.code.as_str() == w.code
                    && e.message == w.message
                    && e.diagram.map(|d| d.name()) == w.diagram.as_deref()
                    && e.mermaid_type == w.mermaid_type.as_deref()
            }
            (Ok(d), None) => {
                Some(d.diagram_type().name()) == c.diagram.as_deref()
                    && Some(d.mermaid_type()) == c.mermaid_type.as_deref()
            }
            _ => false,
        };
        failures.check(ok, || {
            format!(
                "{:?}: {:?}\nPele {c:?}",
                c.source,
                got.as_ref().map(|d| (d.diagram_type(), d.mermaid_type()))
            )
        });
    }
    failures.done("diagrams", 100);
}

/// A host lays one diagram out from as many threads as it likes.
#[test]
fn diagrams_are_send_and_sync() {
    fn shared<T: Send + Sync>() {}
    shared::<super::Diagram>();
}
