//! Swimlane diagrams laid out as Pele lays them out, checked against what Pele draws.

use serde::Deserialize;

use super::{layout, parse};
use crate::detect::DiagramType;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::testing::Failures;
use crate::testing::pelesvg::{flow_lines, tags_last};

#[derive(Deserialize)]
struct Renders {
    cases: Vec<Render>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Render {
    src: String,
    max_width: Option<f64>,
    #[serde(default)]
    keep: bool,
    breakpoint: Option<f64>,
    #[serde(default)]
    unlimited: bool,
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

/// What Pele's render draws of swimlane diagrams, read back out of its SVG as a line for each
/// thing drawn, in the order drawn, lanes with their title bands and the turn of their
/// titles, against the same lines written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("swimlane-render");
    assert!(f.cases.len() >= 3000, "only {} cases", f.cases.len());
    let read = tags_last(&["clickable"]);
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let options = ParseOptions {
            limit: if c.unlimited {
                None
            } else {
                ParseOptions::default().limit
            },
            ..ParseOptions::default()
        };
        let p = prepare(&c.src, &options).expect("a swimlane diagram");
        assert_eq!(p.found.diagram, DiagramType::Swimlane, "{:?}", c.src);
        let db = p
            .parse_with(false, |s| {
                parse(s, &p.config, p.title.as_deref(), p.max_edges)
            })
            .expect("a swimlane diagram that parses");
        let mut o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            keep_direction: c.keep,
            ..Options::default()
        };
        if let Some(b) = c.breakpoint {
            o.direction_breakpoint = b;
        }
        let sc = layout(&db, &p.config, &o);
        assert_eq!(sc.diagram, "swimlane");
        assert!(
            sc.items
                .iter()
                .all(|it| it.role.as_str().starts_with("swimlane.")),
            "{:?} has items of other roles",
            c.src
        );
        let got = flow_lines(&sc);
        let diff = drawings.diff(k, &c.drawing, &got, Some(&read));
        failures.check(diff.is_none(), || {
            format!("{:?}, max {:?}: {}", c.src, c.max_width, diff.unwrap())
        });
    }
    failures.done("renders", 3000);
}
