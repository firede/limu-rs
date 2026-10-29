//! Reads a Mermaid diagram from standard input and draws it as text for a terminal.
//!
//! ```sh
//! echo 'flowchart LR; a --> b' | cargo run -p limu-ascii --example terminal
//! echo 'flowchart LR; a --> b' | cargo run -p limu-ascii --example terminal -- --ascii 60
//! ```
//!
//! `--ascii` draws in ASCII alone, for terminals without box-drawing characters, and a number
//! is how many columns the terminal has, 80 where none is given.

use std::io::Read;
use std::process::ExitCode;

use limu_ascii::{Cells, Charset, Options};

fn main() -> ExitCode {
    // An error is written as its message, as a diagram's errors say what went wrong.
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut charset = Charset::Unicode;
    let mut columns = 80.0;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--ascii" => charset = Charset::Ascii,
            n => columns = n.parse()?,
        }
    }
    let mut src = String::new();
    std::io::stdin().read_to_string(&mut src)?;
    // The diagram is laid out for cells: its text measured in them, set a row high, and
    // snapped to their grid.
    let cells = Cells::default();
    let layout = limu::layout::Options {
        measurer: &cells,
        font_size: cells.font_size(),
        snap: Some(cells.grid()),
        max_width: columns * cells.width,
        direction_breakpoint: f64::INFINITY,
        ..limu::layout::Options::default()
    };
    let scene = limu::layout(&src, &layout)?;
    let text = limu_ascii::render(
        &scene,
        &Options {
            charset,
            ..Options::default()
        },
    )?;
    print!("{text}");
    Ok(())
}
