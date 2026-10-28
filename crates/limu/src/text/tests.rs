//! Labels and widths checked against what Pele makes of the same cases, in the oracle's text
//! fixture.

use limu_fixtures::{Num, load};
use serde::Deserialize;

use super::{Arial, Setting, layout_label};
use crate::scene::text::{self, Face, Label};

#[derive(Deserialize)]
struct Text {
    measure: Vec<Measure>,
    label: Vec<LabelCase>,
}

#[derive(Deserialize)]
struct Measure {
    text: String,
    size: f64,
    face: u8,
    width: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LabelCase {
    text: String,
    markdown: bool,
    size: f64,
    max_width: Num,
    face: u8,
    #[serde(default)]
    icons: Vec<String>,
    label: PeleLabel,
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

fn same(got: &Label, want: &PeleLabel) -> bool {
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
fn arial_matches_pele() {
    let p: Text = load("text");
    for c in p.measure {
        let got = text::width(&Arial, &c.text, c.size, Face::from_bits(c.face));
        assert_eq!(got, c.width, "width({:?}, {}, {})", c.text, c.size, c.face);
    }
}

#[test]
fn labels_match_pele() {
    let p: Text = load("text");
    assert!(p.label.len() >= 100, "only {} cases", p.label.len());
    let mut failed = 0;
    for c in &p.label {
        let icons = |name: &str| c.icons.iter().any(|i| i == name);
        let set = Setting {
            measurer: &Arial,
            icons: &icons,
        };
        let got = layout_label(
            &c.text,
            c.markdown,
            &set,
            c.size,
            c.max_width.0,
            Face::from_bits(c.face),
        );
        if !same(&got, &c.label) {
            failed += 1;
            if failed <= 10 {
                eprintln!(
                    "layout_label({:?}, markdown {}, size {}, max {}, face {})\n got {got:?}\nPele {:?}",
                    c.text, c.markdown, c.size, c.max_width.0, c.face, c.label
                );
            }
        }
    }
    assert_eq!(failed, 0, "{failed} of {} differ", p.label.len());
}
