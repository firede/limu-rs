//! The engine of limu's playground, a web page that lays a Mermaid diagram out as it is typed
//! and shows it as SVG, as text for a terminal, or as its scene. Built for WebAssembly, it is
//! called by the page through the functions the module exports, which pass text through the
//! module's memory, so the page needs no script but its own.
//!
//! `./scripts/playground.sh` builds the page into target/playground.

use std::time::{Duration, SystemTime};

use limu::layout::Options as Layout;
use limu_ascii::{Cells, Charset};
use limu_scene::Scene;
use limu_scene::style::{Look, Theme, Token};
use limu_scene::svg;
use serde_json::{Value, json};

#[cfg(target_arch = "wasm32")]
mod wasm;

/// What the page shows of a diagram.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// The diagram written as SVG.
    Svg,
    /// The diagram laid out for a terminal's cells and drawn as text.
    Text,
    /// The scene, written as JSON.
    Scene,
}

/// How the page asks a diagram to be shown.
#[derive(Clone, Debug)]
pub struct Request {
    /// What is shown.
    pub view: View,
    /// The theme the SVG is drawn in.
    pub theme: Theme,
    /// Whether the SVG's colors are written as colors, rather than as the `--limu-*`
    /// variables with the theme's colors to fall back on, which a page that holds the SVG
    /// sets to draw it in its own theme.
    pub literal: bool,
    /// Whether text is drawn in ASCII alone rather than in box-drawing characters.
    pub ascii: bool,
    /// How many columns text is laid out to fit.
    pub columns: f64,
    /// The time, in milliseconds since 1970 began, that a Gantt chart takes as today. The
    /// page gives it, as WebAssembly has no clock of its own.
    pub now: f64,
}

/// A diagram's source shown as a request asks, written as JSON for the page. It holds the
/// output, the type of the diagram, its size and the background it is drawn on, or the error
/// and where it is in the source, its line from 1 and its column from 1 in UTF-16 code units,
/// as a text area counts them, or 0 for an error with no place.
pub fn show(src: &str, r: &Request) -> String {
    match output(src, r) {
        Ok(v) => v,
        Err(e) => e,
    }
    .to_string()
}

fn output(src: &str, r: &Request) -> Result<Value, Value> {
    let now = Some(today(r.now));
    let scene = match r.view {
        View::Svg | View::Scene => limu::layout(
            src,
            &Layout {
                now,
                ..Layout::default()
            },
        ),
        View::Text => {
            // Laid out for cells, as limu-ascii's terminal example lays it out.
            let cells = Cells::default();
            limu::layout(
                src,
                &Layout {
                    measurer: &cells,
                    font_size: cells.font_size(),
                    snap: Some(cells.grid()),
                    max_width: r.columns.max(1.0) * cells.width,
                    direction_breakpoint: f64::INFINITY,
                    now,
                    ..Layout::default()
                },
            )
        }
    }
    .map_err(|e| {
        json!({
            "error": e.message,
            "line": e.line,
            "column": e.column,
        })
    })?;
    let output = match r.view {
        View::Svg => svg::render(
            &scene,
            &svg::Options {
                look: Look::new(r.theme.clone()),
                literal: r.literal,
                // Text is measured as Arial, and Liberation Sans is as wide.
                font_family: Some("Arial,Liberation Sans,sans-serif".into()),
                ..Default::default()
            },
        ),
        View::Text => limu_ascii::render(
            &scene,
            &limu_ascii::Options {
                charset: if r.ascii {
                    Charset::Ascii
                } else {
                    Charset::Unicode
                },
                ..Default::default()
            },
        )
        .map_err(|e| json!({ "error": e.to_string(), "line": 0, "column": 0 }))?,
        View::Scene => serde_json::to_string_pretty(&scene).expect("a scene written as JSON"),
    };
    Ok(described(&scene, output, &r.theme))
}

/// The output of a scene, with what the page says of it.
fn described(scene: &Scene, output: String, theme: &Theme) -> Value {
    json!({
        "output": output,
        "diagram": scene.diagram,
        "width": scene.width,
        "height": scene.height,
        "background": theme.color(Token::Bg).to_string(),
    })
}

/// The time a number of milliseconds since 1970 began, or then where the number is not a time.
fn today(ms: f64) -> SystemTime {
    if ms.is_finite() && ms >= 0.0 {
        SystemTime::UNIX_EPOCH + Duration::from_secs_f64(ms / 1000.0)
    } else {
        SystemTime::UNIX_EPOCH
    }
}

/// The samples' sources by their names, written as JSON for the page.
pub fn samples() -> String {
    let all: Vec<Value> = limu_samples::SOURCES
        .iter()
        .map(|&(name, src)| json!([name, src]))
        .collect();
    Value::from(all).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(view: View) -> Request {
        Request {
            view,
            theme: Theme::KOHU,
            literal: true,
            ascii: false,
            columns: 80.0,
            now: 0.0,
        }
    }

    fn shown(src: &str, view: View) -> Value {
        serde_json::from_str(&show(src, &request(view))).unwrap()
    }

    #[test]
    fn shows_every_view() {
        let src = "flowchart LR; a --> b";
        let svg = shown(src, View::Svg);
        assert!(svg["output"].as_str().unwrap().starts_with("<svg"));
        assert_eq!(svg["diagram"], "flowchart");
        assert_eq!(svg["background"], "#f1f6f5");
        assert!(
            shown(src, View::Text)["output"]
                .as_str()
                .unwrap()
                .contains('▶')
        );
        let scene: Value =
            serde_json::from_str(shown(src, View::Scene)["output"].as_str().unwrap()).unwrap();
        assert_eq!(scene["diagram"], "flowchart");
    }

    #[test]
    fn places_errors() {
        let e = shown("flowchart LR\n  a --> ", View::Svg);
        assert!(e["error"].is_string());
        assert_eq!(e["line"], 2);
        let e = shown("pie\n  \"a\": 1", View::Text);
        assert_eq!(e["error"], "a pie diagram can't be drawn as text");
        assert_eq!(e["line"], 0);
    }

    #[test]
    fn lays_out_every_sample() {
        let all: Vec<(String, String)> = serde_json::from_str(&samples()).unwrap();
        assert_eq!(all.len(), limu_samples::SOURCES.len());
        for (name, src) in &all {
            let v = shown(src, View::Svg);
            assert!(v["output"].is_string(), "{name}: {v}");
        }
    }
}
