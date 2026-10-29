//! The type of a diagram, found from its first word, as Pele's src/detect.ts finds it.

use std::fmt;

use crate::js::chars::{is_space, is_word, space_run, unit};

/// A type of diagram, as Pele names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DiagramType {
    /// `flowchart` and `graph`.
    Flowchart,
    /// `sequenceDiagram`.
    Sequence,
    /// `classDiagram`.
    Class,
    /// `stateDiagram`.
    State,
    /// `erDiagram`.
    Er,
    /// `pie`.
    Pie,
    /// `gantt`.
    Gantt,
    /// `gitGraph`.
    GitGraph,
    /// `mindmap`.
    Mindmap,
    /// `timeline`.
    Timeline,
    /// `journey`.
    Journey,
    /// `xychart`.
    XyChart,
    /// `quadrantChart`.
    QuadrantChart,
    /// `requirementDiagram`.
    Requirement,
    /// `C4Context` and the other C4 diagrams.
    C4,
    /// `sankey`.
    Sankey,
    /// `block`.
    Block,
    /// `packet`.
    Packet,
    /// `kanban`.
    Kanban,
    /// `architecture`.
    Architecture,
    /// `radar-beta`.
    Radar,
    /// `treemap`.
    Treemap,
    /// `info`.
    Info,
    /// `agentflow-beta`.
    AgentFlow,
    /// `usecase-beta`.
    UseCase,
    /// `railroad-beta` and its dialects.
    Railroad,
    /// `treeView-beta`.
    TreeView,
    /// `venn-beta`.
    Venn,
    /// `cynefin-beta`.
    Cynefin,
    /// `wardley-beta`.
    Wardley,
    /// `swimlane-beta`.
    Swimlane,
    /// `ishikawa`.
    Ishikawa,
    /// `eventmodeling`.
    EventModeling,
}

impl DiagramType {
    /// The type a name names, as [`name`](DiagramType::name) gives it, as a scene's
    /// diagram is. Only a type limu reads is found.
    pub fn from_name(name: &str) -> Option<DiagramType> {
        crate::types().iter().copied().find(|t| t.name() == name)
    }

    /// The type as Pele names it, as "gitGraph", which is a scene's diagram.
    pub fn name(self) -> &'static str {
        use DiagramType::*;
        match self {
            Flowchart => "flowchart",
            Sequence => "sequence",
            Class => "class",
            State => "state",
            Er => "er",
            Pie => "pie",
            Gantt => "gantt",
            GitGraph => "gitGraph",
            Mindmap => "mindmap",
            Timeline => "timeline",
            Journey => "journey",
            XyChart => "xychart",
            QuadrantChart => "quadrantChart",
            Requirement => "requirement",
            C4 => "c4",
            Sankey => "sankey",
            Block => "block",
            Packet => "packet",
            Kanban => "kanban",
            Architecture => "architecture",
            Radar => "radar",
            Treemap => "treemap",
            Info => "info",
            AgentFlow => "agentflow",
            UseCase => "usecase",
            Railroad => "railroad",
            TreeView => "treeView",
            Venn => "venn",
            Cynefin => "cynefin",
            Wardley => "wardley",
            Swimlane => "swimlane",
            Ishikawa => "ishikawa",
            EventModeling => "eventmodeling",
        }
    }
}

impl fmt::Display for DiagramType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A diagram's type, as Pele and as Mermaid name it, as Pele's Detected is. Mermaid's name
/// is the type's but for "flowchart-v2", "flowchart-elk", "classDiagram", "stateDiagram" and
/// the railroad diagrams' "railroadEbnf", "railroadAbnf" and "railroadPeg".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Detected {
    pub(crate) diagram: DiagramType,
    pub(crate) mermaid_type: &'static str,
}

/// What `/^\s*word/` leaves after it, with fold for the i flag, which folds ASCII only.
fn starts_with<'a>(s: &'a str, word: &str, fold: bool) -> Option<&'a str> {
    let s = &s[space_run(s, 0)..];
    let head = s.get(..word.len())?;
    (head == word || fold && head.eq_ignore_ascii_case(word)).then(|| &s[word.len()..])
}

/// `\b` after a word character: the next is none, or there is no next.
fn word_ends(rest: &str) -> bool {
    rest.bytes().next().is_none_or(|c| !is_word(i32::from(c)))
}

/// How a detector tests a text.
enum Test {
    /// `/^\s*word/`.
    Prefix(&'static str),
    /// `/^\s*word/`, or with fold set `/^\s*word/i`, and then a test of what follows.
    Then(&'static str, bool, fn(&str) -> bool),
    /// `/^\s*(?:a|b|…)/`.
    Any(&'static [&'static str]),
}

impl Test {
    fn matches(&self, s: &str) -> bool {
        match *self {
            Test::Prefix(word) => starts_with(s, word, false).is_some(),
            Test::Then(word, fold, after) => starts_with(s, word, fold).is_some_and(after),
            Test::Any(words) => words.iter().any(|w| starts_with(s, w, false).is_some()),
        }
    }
}

fn always(_: &str) -> bool {
    true
}

use DiagramType as D;

/// Mermaid's detectors, in the order Mermaid registers them, as Pele's DETECTORS lists them.
/// The first match wins.
const DETECTORS: &[(DiagramType, &str, Test)] = &[
    (D::Flowchart, "flowchart-elk", Test::Prefix("flowchart-elk")),
    (D::Mindmap, "mindmap", Test::Prefix("mindmap")),
    (
        D::Architecture,
        "architecture",
        Test::Prefix("architecture"),
    ),
    (
        D::AgentFlow,
        "agentflow",
        Test::Then("agentflow-beta", false, word_ends),
    ),
    (
        D::C4,
        "c4",
        Test::Any(&[
            "C4Context",
            "C4Container",
            "C4Component",
            "C4Dynamic",
            "C4Deployment",
        ]),
    ),
    (D::Kanban, "kanban", Test::Prefix("kanban")),
    (D::Class, "classDiagram", Test::Prefix("classDiagram")),
    (D::Er, "er", Test::Prefix("erDiagram")),
    (D::Gantt, "gantt", Test::Prefix("gantt")),
    (D::Info, "info", Test::Prefix("info")),
    (D::Pie, "pie", Test::Prefix("pie")),
    // /^\s*requirement(Diagram)?/ is /^\s*requirement/.
    (D::Requirement, "requirement", Test::Prefix("requirement")),
    (D::Sequence, "sequence", Test::Prefix("sequenceDiagram")),
    (
        D::Swimlane,
        "swimlane",
        Test::Then("swimlane-beta", false, word_ends),
    ),
    (
        D::Flowchart,
        "flowchart-v2",
        Test::Any(&["graph", "flowchart"]),
    ),
    (D::Timeline, "timeline", Test::Prefix("timeline")),
    (D::GitGraph, "gitGraph", Test::Prefix("gitGraph")),
    (D::State, "stateDiagram", Test::Prefix("stateDiagram")),
    (D::Journey, "journey", Test::Prefix("journey")),
    (
        D::QuadrantChart,
        "quadrantChart",
        Test::Prefix("quadrantChart"),
    ),
    (D::Sankey, "sankey", Test::Prefix("sankey")),
    (D::Packet, "packet", Test::Prefix("packet")),
    (D::XyChart, "xychart", Test::Prefix("xychart")),
    (D::Block, "block", Test::Prefix("block")),
    (
        D::EventModeling,
        "eventmodeling",
        Test::Prefix("eventmodeling"),
    ),
    (D::TreeView, "treeView", Test::Prefix("treeView-beta")),
    (D::Radar, "radar", Test::Prefix("radar-beta")),
    // /^\s*ishikawa(-beta)?\b/i: the optional -beta gives way to the word's end.
    (
        D::Ishikawa,
        "ishikawa",
        Test::Then("ishikawa", true, word_ends),
    ),
    (D::Treemap, "treemap", Test::Prefix("treemap")),
    (
        D::Railroad,
        "railroad",
        Test::Then("railroad-beta", true, always),
    ),
    (
        D::Railroad,
        "railroadEbnf",
        Test::Then("railroad-ebnf-beta", true, always),
    ),
    (
        D::Railroad,
        "railroadAbnf",
        Test::Then("railroad-abnf-beta", true, always),
    ),
    (
        D::Railroad,
        "railroadPeg",
        Test::Then("railroad-peg-beta", true, always),
    ),
    (D::Venn, "venn", Test::Prefix("venn-beta")),
    (
        D::Wardley,
        "wardley",
        Test::Then("wardley-beta", true, always),
    ),
    // /^\s*cynefin-beta(?:[\s:]|$)/
    (
        D::Cynefin,
        "cynefin",
        Test::Then("cynefin-beta", false, |rest| {
            rest.is_empty() || rest.starts_with(':') || is_space(unit(rest, 0))
        }),
    ),
    // /^\s*usecase-beta(?:\s|$)/
    (
        D::UseCase,
        "usecase",
        Test::Then("usecase-beta", false, |rest| {
            rest.is_empty() || is_space(unit(rest, 0))
        }),
    ),
];

/// The type of diagram a preprocessed text is, as Pele's detectDiagram finds it.
pub(crate) fn detect_diagram(cleaned: &str) -> Option<Detected> {
    DETECTORS
        .iter()
        .find(|(_, _, test)| test.matches(cleaned))
        .map(|&(diagram, mermaid_type, _)| Detected {
            diagram,
            mermaid_type,
        })
}
