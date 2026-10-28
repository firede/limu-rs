//! A diagram's text read into a model, and laid out as a scene, as Pele's parse and render in
//! src/core.ts read and draw it.

use crate::detect::{Detected, DiagramType, detect_diagram};
use crate::diagrams;
use crate::errors::{Error, ErrorCode};
use crate::layout::Options;
use crate::prepare::{ParseOptions, Prepared, prepare};
use crate::preprocess::preprocess;
use crate::scene::Scene;
use crate::value::Map;

/// The model of a diagram of a type, as its parser reads it.
// A diagram holds one model, made once each time a text is parsed, so the room the largest
// takes in every diagram matters less than a box for each would.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
enum Model {
    #[cfg(feature = "flowchart")]
    Flowchart(diagrams::flowchart::FlowDb),
    #[cfg(feature = "pie")]
    Pie(diagrams::pie::PieModel),
    #[cfg(feature = "er")]
    Er(diagrams::er::ErDb),
    #[cfg(feature = "class")]
    Class(diagrams::class::ClassDb),
    #[cfg(feature = "state")]
    State(diagrams::state::StateDb),
    #[cfg(feature = "packet")]
    Packet(diagrams::packet::PacketModel),
    #[cfg(feature = "info")]
    Info(diagrams::info::InfoModel),
    #[cfg(feature = "requirement")]
    Requirement(diagrams::requirement::RequirementDb),
    #[cfg(feature = "agentflow")]
    AgentFlow(diagrams::agentflow::AgentflowDb),
    #[cfg(feature = "swimlane")]
    Swimlane(diagrams::flowchart::FlowDb),
}

/// The types of diagram limu reads, in the order Pele's registry lists them.
const TYPES: &[DiagramType] = &[
    #[cfg(feature = "flowchart")]
    DiagramType::Flowchart,
    #[cfg(feature = "pie")]
    DiagramType::Pie,
    #[cfg(feature = "er")]
    DiagramType::Er,
    #[cfg(feature = "class")]
    DiagramType::Class,
    #[cfg(feature = "state")]
    DiagramType::State,
    #[cfg(feature = "requirement")]
    DiagramType::Requirement,
    #[cfg(feature = "packet")]
    DiagramType::Packet,
    #[cfg(feature = "info")]
    DiagramType::Info,
    #[cfg(feature = "agentflow")]
    DiagramType::AgentFlow,
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
pub fn parse(src: &str, options: &ParseOptions) -> Result<Diagram, Error> {
    let p = prepare(src, options)?;
    let model = Model::read(&p)?;
    Ok(Diagram {
        found: p.found,
        config: p.config,
        title: p.title,
        model,
    })
}

impl Model {
    /// Reads the body of a prepared text with the parser of its type. A type limu is built
    /// without is not supported.
    fn read(p: &Prepared) -> Result<Model, Error> {
        match p.found.diagram {
            #[cfg(feature = "flowchart")]
            DiagramType::Flowchart => p
                .parse_with(false, |s| {
                    diagrams::flowchart::parse(s, &p.config, p.title.as_deref(), p.max_edges)
                })
                .map(Model::Flowchart),
            #[cfg(feature = "pie")]
            DiagramType::Pie => p
                .parse_with(false, |s| diagrams::pie::parse(s, p.title.as_deref()))
                .map(Model::Pie),
            #[cfg(feature = "er")]
            DiagramType::Er => p
                .parse_with(false, |s| diagrams::er::parse(s, p.title.as_deref()))
                .map(Model::Er),
            #[cfg(feature = "class")]
            DiagramType::Class => p
                .parse_with(false, |s| diagrams::class::parse(s, p.title.as_deref()))
                .map(Model::Class),
            #[cfg(feature = "state")]
            DiagramType::State => p
                .parse_with(false, |s| diagrams::state::parse(s, p.title.as_deref()))
                .map(Model::State),
            #[cfg(feature = "packet")]
            DiagramType::Packet => p
                .parse_with(false, |s| {
                    diagrams::packet::parse(s, &p.config, p.title.as_deref())
                })
                .map(Model::Packet),
            #[cfg(feature = "info")]
            DiagramType::Info => p
                .parse_with(false, |s| diagrams::info::parse(s, p.title.as_deref()))
                .map(Model::Info),
            #[cfg(feature = "requirement")]
            DiagramType::Requirement => p
                .parse_with(false, |s| {
                    diagrams::requirement::parse(s, p.title.as_deref())
                })
                .map(Model::Requirement),
            #[cfg(feature = "agentflow")]
            DiagramType::AgentFlow => p
                .parse_with(true, |s| {
                    diagrams::agentflow::parse(
                        s,
                        &p.config,
                        p.title.as_deref(),
                        p.line_offset,
                        p.max_edges,
                    )
                })
                .map(Model::AgentFlow),
            #[cfg(feature = "swimlane")]
            DiagramType::Swimlane => p
                .parse_with(false, |s| {
                    diagrams::swimlane::parse(s, &p.config, p.title.as_deref(), p.max_edges)
                })
                .map(Model::Swimlane),
            _ => {
                let message = format!("Diagram type \"{}\" is not supported yet.", p.found.diagram);
                Err(Error::new(message, ErrorCode::UnsupportedDiagram).of(p.found))
            }
        }
    }

    /// Lays the model out with the layout of its type.
    // Built with no type of diagram, limu has no model, and nothing reads the configuration
    // or the options.
    #[allow(unused_variables)]
    fn layout(&self, config: &Map, o: &Options) -> Scene {
        match *self {
            #[cfg(feature = "flowchart")]
            Model::Flowchart(ref db) => diagrams::flowchart::layout(db, config, o),
            #[cfg(feature = "pie")]
            Model::Pie(ref m) => diagrams::pie::layout(m, config, o),
            #[cfg(feature = "er")]
            Model::Er(ref m) => diagrams::er::layout(m, config, o),
            #[cfg(feature = "class")]
            Model::Class(ref m) => diagrams::class::layout(m, config, o),
            #[cfg(feature = "state")]
            Model::State(ref m) => diagrams::state::layout(m, config, o),
            #[cfg(feature = "packet")]
            Model::Packet(ref m) => diagrams::packet::layout(m, config, o),
            #[cfg(feature = "info")]
            Model::Info(ref m) => diagrams::info::layout(m, config, o),
            #[cfg(feature = "requirement")]
            Model::Requirement(ref db) => diagrams::requirement::layout(db, config, o),
            #[cfg(feature = "agentflow")]
            Model::AgentFlow(ref m) => diagrams::agentflow::layout(m, config, o),
            #[cfg(feature = "swimlane")]
            Model::Swimlane(ref db) => diagrams::swimlane::layout(db, config, o),
        }
    }
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
    pub fn layout(&self, o: &Options) -> Result<Scene, Error> {
        let mut sc = self.model.layout(&self.config, o);
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
