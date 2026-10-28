//! A diagram's text read into a model, and laid out as a scene, as Pele's parse and render in
//! src/core.ts read and draw it.

use crate::detect::{Detected, DiagramType, detect_diagram};
use crate::diagrams;
use crate::errors::{Error, ErrorCode};
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::preprocess::preprocess;
use crate::scene::Scene;
use crate::value::Map;

/// The model of a diagram of a type, as its parser reads it.
#[derive(Clone, Debug)]
enum Model {
    #[cfg(feature = "flowchart")]
    Flowchart(diagrams::flowchart::FlowDb),
    #[cfg(feature = "swimlane")]
    Swimlane(diagrams::flowchart::FlowDb),
}

/// The types of diagram limu reads, in the order Pele's registry lists them.
const TYPES: &[DiagramType] = &[
    #[cfg(feature = "flowchart")]
    DiagramType::Flowchart,
    #[cfg(feature = "swimlane")]
    DiagramType::Swimlane,
];

/// The types of diagram limu reads, in the order Pele's registry lists them. Each is a
/// feature of the crate, and only those it is built with are here.
pub fn types() -> &'static [DiagramType] {
    TYPES
}

/// Whether limu reads the type of diagram a text is, as Pele's supports says: its type is
/// found, and it is one limu reads. Text whose front matter does not read is not supported.
/// It does not parse the diagram, which may still be in error.
pub fn supports(src: &str) -> bool {
    preprocess(src, None)
        .ok()
        .and_then(|pre| detect_diagram(&pre.text))
        .is_some_and(|found| TYPES.contains(&found.diagram))
}

/// A diagram's text read into a model, ready to be laid out. Laying it out changes neither
/// its model nor its configuration, so one diagram may be laid out many times, with other
/// options, and from many threads at once. A host that lays a diagram out again when its
/// width changes parses it once.
#[derive(Clone, Debug)]
pub struct Diagram {
    found: Detected,
    config: Map,
    title: Option<String>,
    model: Model,
}

/// Reads a diagram's text, as Pele's parse does: its front matter and directives, its type,
/// and then its body, with the parser of its type. The options hold the limits the text is
/// held to and the configuration beneath its own. What changes where things are is given to
/// [`Diagram::layout`]. Its errors are text of no type limu reads, text it cannot parse, and
/// text past a limit. An error found once the type is known holds it, as Pele's tagErrors
/// gives it, and so do the errors of layout.
// Built with no type of diagram, limu reads none, and what comes after the parsers and
// layouts is never reached.
#[cfg_attr(
    not(any(feature = "flowchart", feature = "swimlane")),
    allow(unreachable_code)
)]
pub fn parse(src: &str, options: &ParseOptions) -> Result<Diagram, Error> {
    let p = prepare(src, options)?;
    let model = match p.found.diagram {
        #[cfg(feature = "flowchart")]
        DiagramType::Flowchart => Model::Flowchart(p.parse_with(false, |s| {
            diagrams::flowchart::parse(s, &p.config, p.title.as_deref(), p.max_edges)
        })?),
        #[cfg(feature = "swimlane")]
        DiagramType::Swimlane => Model::Swimlane(p.parse_with(false, |s| {
            diagrams::swimlane::parse(s, &p.config, p.title.as_deref(), p.max_edges)
        })?),
        _ => {
            let message = format!("Diagram type \"{}\" is not supported yet.", p.found.diagram);
            return Err(Error::new(message, ErrorCode::UnsupportedDiagram).of(p.found));
        }
    };
    Ok(Diagram {
        found: p.found,
        config: p.config,
        title: p.title,
        model,
    })
}

impl Diagram {
    /// The diagram's type.
    pub fn diagram_type(&self) -> DiagramType {
        self.found.diagram
    }

    /// The diagram's type as Mermaid names it, as Pele's parse gives it: "flowchart-v2" for
    /// `graph TD`, where the type is a flowchart.
    pub fn mermaid_type(&self) -> &'static str {
        self.found.mermaid_type
    }

    /// The title the diagram's front matter gives it.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// The diagram's configuration: its own, over the configuration it was parsed with.
    pub fn config(&self) -> &Map {
        &self.config
    }

    /// Lays the diagram out as a scene, for a backend to draw, with the layout of its type,
    /// and snaps it to the options' grid. Its errors are those a layout gives, and one of a
    /// limit for a scene that would hold a number that is not finite, as numbers at the edge
    /// of what an f64 holds can give, where Pele draws NaN.
    #[cfg_attr(
        not(any(feature = "flowchart", feature = "swimlane")),
        allow(unreachable_code, unused_variables)
    )]
    pub fn layout(&self, o: &Options) -> Result<Scene, Error> {
        let mut sc: Scene = match self.model {
            #[cfg(feature = "flowchart")]
            Model::Flowchart(ref db) => diagrams::flowchart::layout(db, &self.config, o),
            #[cfg(feature = "swimlane")]
            Model::Swimlane(ref db) => diagrams::swimlane::layout(db, &self.config, o),
        };
        if let Some(grid) = o.snap {
            sc.snap(grid);
        }
        if !sc.is_finite() {
            let message = "Diagram cannot be laid out in finite numbers.";
            return Err(Error::new(message, ErrorCode::Limit).tagged(self.found));
        }
        Ok(sc)
    }
}

/// Reads a diagram's text and lays it out as a scene, for a backend to draw: [`parse`] with
/// the default limits and no configuration of the host's, and then [`Diagram::layout`]. Its
/// errors are theirs.
pub fn layout(src: &str, o: &Options) -> Result<Scene, Error> {
    parse(src, &ParseOptions::default())?.layout(o)
}

#[cfg(test)]
mod tests;
