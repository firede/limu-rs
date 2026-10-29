//! Reads a Mermaid diagram from standard input and writes it as SVG to standard output.
//!
//! ```sh
//! echo 'flowchart LR; a --> b' | cargo run -p limu --example svg > diagram.svg
//! ```
//!
//! Text is measured as Arial, as limu measures it by default, and the SVG names the font
//! "sans-serif" unless a page sets `--limu-font`.

use std::io::Read;
use std::process::ExitCode;

use limu::scene::svg;

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
    let mut src = String::new();
    std::io::stdin().read_to_string(&mut src)?;
    let scene = limu::layout(&src, &limu::layout::Options::default())?;
    print!("{}", svg::render(&scene, &svg::Options::default()));
    Ok(())
}
