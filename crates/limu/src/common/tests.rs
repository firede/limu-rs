//! What the diagrams share, checked against what Pele makes of the same cases, in the
//! oracle's common fixture.

use limu_fixtures::load;
use serde::Deserialize;

use super::parse_generic_types;
use crate::testing::Failures;

#[derive(Deserialize)]
struct Fixture {
    generics: Vec<Generic>,
}

#[derive(Deserialize)]
struct Generic {
    input: String,
    output: String,
}

#[test]
fn generics_match_pele() {
    let f: Fixture = load("common");
    let mut failures = Failures::new();
    for c in &f.generics {
        let got = parse_generic_types(&c.input);
        failures.check(got == c.output, || {
            format!("{:?}: {got:?}, Pele {:?}", c.input, c.output)
        });
    }
    failures.done("generics", 1000);
}
