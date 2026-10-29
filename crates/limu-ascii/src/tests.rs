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
        .filter_map(|s| golden(&format!("{}{suffix}.txt", s.name), &render(&s.scene, o)).err())
        .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// The diagrams, laid out for cells in a terminal of 80 columns.
fn diagrams() -> Vec<Sample> {
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
