//! Diagrams read as a whole against what Pele's parse and render make of the same texts, in
//! the oracle's index fixture.

use limu_fixtures::load;
use serde::Deserialize;

use super::{TYPES, parse};
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::Kind;
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

/// Every text item a diagram lays out has bounds, whether or not its layout gave it a box.
#[test]
fn text_items_have_bounds() {
    let sources = [
        "flowchart LR\n  A --> B",
        "pie title Pets\n  \"Dogs\" : 3\n  \"Cats\" : 2",
        "info",
        "journey\n  title My day\n  section Work\n    Code: 5: Me",
        "timeline\n  title History\n  2020 : One",
    ];
    for src in sources {
        // A type limu is built without is left out.
        if !crate::supports(src) {
            continue;
        }
        let sc = crate::layout(src, &Options::default()).expect("a diagram");
        for it in sc.items.iter().filter(|it| matches!(it.kind, Kind::Text)) {
            assert!(
                !it.bounds.is_unset(),
                "{src:?}: {} has no bounds",
                it.role.as_str()
            );
        }
    }
}

#[test]
fn names_read_back() {
    for &t in TYPES {
        assert_eq!(crate::DiagramType::from_name(t.name()), Some(t));
    }
    assert_eq!(crate::DiagramType::from_name("nothing"), None);
}

/// The roles of the items laid out from every case of the fixtures of rendering.
fn laid_out_roles() -> std::collections::BTreeSet<String> {
    #[derive(Deserialize)]
    struct Case {
        src: String,
        #[serde(rename = "maxWidth")]
        max_width: Option<f64>,
    }
    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }
    let mut roles = std::collections::BTreeSet::new();
    let mut names: Vec<String> = std::fs::read_dir(limu_fixtures::dir())
        .expect("the fixtures")
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            Some(name.strip_suffix("-render.json.gz")?.to_string())
        })
        .collect();
    names.sort();
    for name in names {
        for c in load::<Fixture>(&format!("{name}-render")).cases {
            let mut o = Options::default();
            if let Some(w) = c.max_width {
                o.max_width = w;
            }
            if let Ok(sc) = crate::layout(&c.src, &o) {
                roles.extend(sc.items.iter().map(|it| it.role.as_str().to_string()));
            }
        }
    }
    roles
}

/// Every role laid out is one `roles` names, so that a host finds each there.
#[test]
fn roles_are_named() {
    let named: Vec<&str> = [
        #[cfg(feature = "agentflow")]
        crate::roles::agentflow::ALL,
        #[cfg(feature = "architecture")]
        crate::roles::architecture::ALL,
        #[cfg(feature = "block")]
        crate::roles::block::ALL,
        #[cfg(feature = "c4")]
        crate::roles::c4::ALL,
        #[cfg(feature = "class")]
        crate::roles::class::ALL,
        #[cfg(feature = "cynefin")]
        crate::roles::cynefin::ALL,
        #[cfg(feature = "er")]
        crate::roles::er::ALL,
        #[cfg(feature = "eventmodeling")]
        crate::roles::eventmodeling::ALL,
        #[cfg(feature = "flowchart")]
        crate::roles::flowchart::ALL,
        #[cfg(feature = "gantt")]
        crate::roles::gantt::ALL,
        #[cfg(feature = "gitgraph")]
        crate::roles::gitgraph::ALL,
        #[cfg(feature = "info")]
        crate::roles::info::ALL,
        #[cfg(feature = "ishikawa")]
        crate::roles::ishikawa::ALL,
        #[cfg(feature = "journey")]
        crate::roles::journey::ALL,
        #[cfg(feature = "kanban")]
        crate::roles::kanban::ALL,
        #[cfg(feature = "mindmap")]
        crate::roles::mindmap::ALL,
        #[cfg(feature = "packet")]
        crate::roles::packet::ALL,
        #[cfg(feature = "pie")]
        crate::roles::pie::ALL,
        #[cfg(feature = "quadrant")]
        crate::roles::quadrant::ALL,
        #[cfg(feature = "radar")]
        crate::roles::radar::ALL,
        #[cfg(feature = "railroad")]
        crate::roles::railroad::ALL,
        #[cfg(feature = "requirement")]
        crate::roles::requirement::ALL,
        #[cfg(feature = "sankey")]
        crate::roles::sankey::ALL,
        #[cfg(feature = "sequence")]
        crate::roles::sequence::ALL,
        #[cfg(feature = "state")]
        crate::roles::state::ALL,
        #[cfg(feature = "swimlane")]
        crate::roles::swimlane::ALL,
        #[cfg(feature = "timeline")]
        crate::roles::timeline::ALL,
        #[cfg(feature = "treemap")]
        crate::roles::treemap::ALL,
        #[cfg(feature = "treeview")]
        crate::roles::treeview::ALL,
        #[cfg(feature = "usecase")]
        crate::roles::usecase::ALL,
        #[cfg(feature = "venn")]
        crate::roles::venn::ALL,
        #[cfg(feature = "wardley")]
        crate::roles::wardley::ALL,
        #[cfg(feature = "xychart")]
        crate::roles::xychart::ALL,
    ]
    .concat();
    let missing: Vec<String> = laid_out_roles()
        .into_iter()
        .filter(|r| !named.contains(&r.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "roles laid out that roles names not: {missing:?}"
    );
}
