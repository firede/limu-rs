//! Preprocessing checked against what Pele makes of the same cases, in the oracle's parse
//! fixture.

use limu_fixtures::load;
use serde::Deserialize;

use super::{encode_entities, preprocess};
use crate::detect::detect_diagram;
use crate::testing::{Enc, same, sorted};

#[derive(Deserialize)]
struct Parse {
    preprocessed: Vec<Case>,
    encoded: Vec<Encoded>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    source: String,
    #[serde(default)]
    text: String,
    with_comments: Option<String>,
    title: Option<String>,
    config: Option<Enc>,
    #[serde(default)]
    line_offset: usize,
    #[serde(rename = "type")]
    diagram: Option<String>,
    mermaid_type: Option<String>,
    error: Option<PeleError>,
}

#[derive(Deserialize, Debug)]
struct PeleError {
    code: String,
    message: String,
    #[serde(rename = "type")]
    diagram: Option<String>,
    line: usize,
    column: usize,
    snippet: String,
}

#[derive(Deserialize)]
struct Encoded {
    text: String,
    encoded: String,
}

#[test]
fn preprocess_matches_pele() {
    let p: Parse = load("parse");
    assert!(
        p.preprocessed.len() >= 1000,
        "only {} cases",
        p.preprocessed.len()
    );
    let mut failed = 0;
    for c in &p.preprocessed {
        let got = preprocess(&c.source, None);
        let ok = match (&got, &c.error) {
            (Err(e), Some(w)) => {
                e.code.as_str() == w.code
                    && e.message == w.message
                    && e.diagram.map(|d| d.name()) == w.diagram.as_deref()
                    && (e.line, e.column) == (w.line, w.column)
                    && e.snippet == w.snippet
            }
            (Ok(got), None) => {
                let found = detect_diagram(&got.text);
                let config = crate::Value::Object(got.config.clone());
                got.text == c.text
                    && got.with_comments == *c.with_comments.as_ref().unwrap_or(&c.text)
                    && got.title == c.title
                    && got.line_offset == c.line_offset
                    && c.config
                        .as_ref()
                        .is_some_and(|w| same(&sorted(&config), &w.0))
                    && found.map(|f| f.diagram.name()) == c.diagram.as_deref()
                    && found.map(|f| f.mermaid_type) == c.mermaid_type.as_deref()
            }
            _ => false,
        };
        if !ok {
            failed += 1;
            if failed <= 10 {
                eprintln!(
                    "preprocess({:?})\n got {got:?}\nPele text {:?}, with comments {:?}, title {:?}, config {:?}, offset {}, type {:?}, error {:?}",
                    c.source,
                    c.text,
                    c.with_comments,
                    c.title,
                    c.config,
                    c.line_offset,
                    c.diagram,
                    c.error
                );
            }
        }
    }
    assert_eq!(failed, 0, "{failed} of {} differ", p.preprocessed.len());
}

#[test]
fn encode_entities_matches_pele() {
    let p: Parse = load("parse");
    assert!(p.encoded.len() >= 1000, "only {} cases", p.encoded.len());
    for c in &p.encoded {
        assert_eq!(
            encode_entities(&c.text, None),
            c.encoded,
            "encode_entities({:?})",
            c.text
        );
    }
}
