//! What the diagrams share, checked against what Pele makes of the same cases, in the
//! oracle's common fixture.

use limu_fixtures::load;
use serde::Deserialize;

use super::{fit_label, literal_label, parse_generic_types, plain_label};
use crate::scene::text::{Face, Label};
use crate::testing::Failures;
use crate::text::{Arial, Setting};

#[derive(Deserialize)]
struct Fixture {
    generics: Vec<Generic>,
    labels: Vec<Labels>,
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

#[derive(Deserialize)]
struct Labels {
    text: String,
    size: f64,
    max: f64,
    face: u8,
    plain: PeleLabel,
    fit: PeleLabel,
    literal: PeleLabel,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct PeleLabel {
    lines: Vec<Vec<PeleSpan>>,
    widths: Vec<f64>,
    width: f64,
    height: f64,
    size: f64,
    line_height: f64,
}

#[derive(Deserialize, Debug)]
struct PeleSpan {
    text: String,
    style: u8,
    icon: Option<String>,
    width: f64,
}

/// Whether a label is the one Pele laid out, to the last bit of each number.
fn same_label(got: &Label, want: &PeleLabel) -> bool {
    got.width == want.width
        && got.height == want.height
        && got.size == want.size
        && got.line_height == want.line_height
        && got.widths == want.widths
        && got.lines.len() == want.lines.len()
        && got.lines.iter().zip(&want.lines).all(|(g, w)| {
            g.len() == w.len()
                && g.iter().zip(w).all(|(g, w)| {
                    g.text == w.text
                        && g.face.bits() == w.style
                        && g.icon == w.icon
                        && g.width == w.width
                })
        })
}

#[test]
fn labels_match_pele() {
    let f: Fixture = load("common");
    let set = Setting {
        measurer: &Arial,
        icons: &|_| false,
    };
    let mut failures = Failures::new();
    for c in &f.labels {
        let face = Face::from_bits(c.face);
        for (name, got, want) in [
            (
                "plain_label",
                plain_label(&c.text, &set, c.size, c.max, face),
                &c.plain,
            ),
            (
                "fit_label",
                fit_label(&c.text, &set, c.size, c.max, face),
                &c.fit,
            ),
            (
                "literal_label",
                literal_label(&c.text, &set, c.size, c.max, face),
                &c.literal,
            ),
        ] {
            failures.check(same_label(&got, want), || {
                format!(
                    "{name}({:?}, size {}, max {}, face {})\n got {got:?}\nPele {want:?}",
                    c.text, c.size, c.max, c.face
                )
            });
        }
    }
    failures.done("labels", 1500);
}
