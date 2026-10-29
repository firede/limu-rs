//! Reads a Mermaid diagram from standard input and draws it as a PNG, in a font the host
//! gives, at twice its size.
//!
//! ```sh
//! echo 'flowchart LR; a --> b' |
//!     cargo run -p limu-tiny-skia --example png -- /path/to/font.ttf diagram.png
//! ```
//!
//! The diagram is laid out with the font it is drawn in, so its text fills the room it is
//! given. Bold, italic and monospace text is drawn in the one font, as the example gives no
//! others.

use std::io::Read;
use std::process::ExitCode;

use limu_tiny_skia::{Fonts, Options};

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
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [font, out] = args.as_slice() else {
        return Err("usage: png <font.ttf> <out.png>".into());
    };
    let fonts = Fonts::new(std::fs::read(font)?)?;
    let mut src = String::new();
    std::io::stdin().read_to_string(&mut src)?;
    let layout = limu::layout::Options {
        measurer: &fonts,
        ..limu::layout::Options::default()
    };
    let scene = limu::layout(&src, &layout)?;
    let o = Options {
        fonts: Some(&fonts),
        scale: 2.0,
        ..Options::default()
    };
    let pixmap = limu_tiny_skia::render(&scene, &o).ok_or("the diagram is too large to draw")?;
    pixmap.save_png(out)?;
    Ok(())
}
