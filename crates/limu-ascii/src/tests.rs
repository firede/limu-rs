//! The samples every backend draws, drawn in cells and compared with golden text. A golden
//! file is written anew where UPDATE_GOLDEN is set, and into target/golden when it differs.
//! The diagrams are laid out for cells, in a terminal of 80 columns. The other samples are
//! laid out in pixels, and are drawn in cells of 8 by 16 of them.

use std::path::{Path, PathBuf};

use limu_samples::Sample;
use limu_scene::text::Face;

use super::*;

fn here() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Writes what was drawn into target/golden, and says where.
fn keep(name: &str, got: &str) -> PathBuf {
    let out = here().join("../../target/golden/ascii");
    std::fs::create_dir_all(&out).unwrap();
    let path = out.join(name);
    std::fs::write(&path, got).unwrap();
    path
}

fn golden(name: &str, got: &str) -> Result<(), String> {
    let path = here().join("tests/golden").join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, got).unwrap();
    }
    match std::fs::read_to_string(&path) {
        Ok(want) if want == got => Ok(()),
        Ok(_) => Err(format!(
            "{name}: differs from its golden file; what was drawn is in {}",
            keep(name, got).display()
        )),
        Err(_) => Err(format!(
            "{name}: has no golden file; what was drawn is in {}",
            keep(name, got).display()
        )),
    }
}

/// Draws samples and compares each with its golden file, named by the sample and a suffix.
fn compare(samples: Vec<Sample>, o: &Options, suffix: &str) {
    let failed: Vec<String> = samples
        .iter()
        .filter_map(|s| {
            let got = render(&s.scene, o).unwrap();
            golden(&format!("{}{suffix}.txt", s.name), &got).err()
        })
        .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// The diagrams that can be drawn as text, laid out for cells in a terminal of 80 columns.
fn diagrams() -> Vec<Sample> {
    all_diagrams()
        .into_iter()
        .filter(|s| !UNSUPPORTED.contains(&s.scene.diagram.as_str()))
        .collect()
}

fn all_diagrams() -> Vec<Sample> {
    let cells = Cells::default();
    limu_samples::diagrams_with(&limu::layout::Options {
        measurer: &cells,
        font_size: cells.font_size(),
        snap: Some(cells.grid()),
        max_width: 80.0 * cells.width,
        direction_breakpoint: f64::INFINITY,
        ..limu::layout::Options::default()
    })
}

/// The lines of the labels of a scene, each as one string.
fn label_lines(scene: &Scene) -> Vec<String> {
    scene
        .items
        .iter()
        .filter_map(|it| it.label.as_ref())
        .flat_map(|p| &p.label.lines)
        .map(|spans| spans.iter().map(|s| s.text.as_str()).collect::<String>())
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

#[test]
fn primitives() {
    let o = Options {
        cells: Cells {
            width: 8.0,
            height: 16.0,
        },
        ..Options::default()
    };
    compare(limu_samples::primitives(), &o, "");
}

#[test]
fn diagrams_in_cells() {
    compare(diagrams(), &Options::default(), "");
}

// Every line of every label is drawn whole, across, on a line of its own.
#[test]
fn keeps_every_label() {
    let mut lost = Vec::new();
    for s in diagrams() {
        let got = render(&s.scene, &Options::default()).unwrap();
        for line in label_lines(&s.scene) {
            if !got.contains(&line) {
                lost.push(format!("{}: {line:?}", s.name));
            }
        }
    }
    assert!(lost.is_empty(), "lost:\n{}", lost.join("\n"));
}

#[test]
fn refuses_what_would_mislead() {
    let refused: Vec<String> = all_diagrams()
        .iter()
        .filter_map(|s| render(&s.scene, &Options::default()).err())
        .map(|e| e.diagram)
        .collect();
    let mut want = UNSUPPORTED.map(String::from).to_vec();
    want.sort();
    let mut got = refused.clone();
    got.sort();
    assert_eq!(got, want);
    assert_eq!(
        Unsupported {
            diagram: "pie".into()
        }
        .to_string(),
        "a pie diagram can't be drawn as text"
    );
}

#[test]
fn ascii() {
    let o = Options {
        charset: Charset::Ascii,
        ..Options::default()
    };
    let some = diagrams()
        .into_iter()
        .filter(|s| ["flowchart", "sequence", "class"].contains(&s.name))
        .collect();
    compare(some, &o, "-ascii");
}

#[test]
fn measures_in_cells() {
    let cells = Cells::default();
    assert_eq!(cells.width("Start", 8.0, Face::BOLD), 30.0);
    assert_eq!(cells.width("中文", 20.0, Face::REGULAR), 24.0);
    assert_eq!(cells.width("e\u{301}", 8.0, Face::REGULAR), 6.0);
}

#[test]
fn centers_in_cells() {
    // Three cells about cell 5, and two about the line between 5 and 6, or about 5 itself,
    // where they start a cell before it.
    assert_eq!(middle(5.0, 3.0), 4);
    assert_eq!(middle(5.5, 2.0), 5);
    assert_eq!(middle(5.0, 2.0), 4);
    assert_eq!(middle(5.0, 1.0), 5);
}

// Without compacting, every cell is where the scene puts it.
#[test]
fn loose() {
    let o = Options {
        compact: false,
        ..Options::default()
    };
    let some = diagrams()
        .into_iter()
        .filter(|s| s.name == "flowchart")
        .collect();
    compare(some, &o, "-loose");
}
