//! The diagrams, each as Pele's src/diagrams has it: a parser that reads its text into a
//! model, and a layout that makes a scene of the model.

#[cfg(feature = "agentflow")]
pub(crate) mod agentflow;
#[cfg(feature = "architecture")]
pub(crate) mod architecture;
#[cfg(feature = "c4")]
pub(crate) mod c4;
#[cfg(feature = "class")]
pub(crate) mod class;
#[cfg(feature = "cynefin")]
pub(crate) mod cynefin;
#[cfg(feature = "er")]
pub(crate) mod er;
// A swimlane diagram is a flowchart laid out its own way, and is read and drawn by the
// flowchart's code. An agent flow draws its nodes in the flowchart's shapes.
#[cfg(any(feature = "agentflow", feature = "flowchart", feature = "swimlane"))]
pub(crate) mod flowchart;
#[cfg(feature = "gantt")]
pub(crate) mod gantt;
#[cfg(feature = "gitgraph")]
pub(crate) mod gitgraph;
#[cfg(feature = "info")]
pub(crate) mod info;
#[cfg(feature = "ishikawa")]
pub(crate) mod ishikawa;
#[cfg(feature = "journey")]
pub(crate) mod journey;
#[cfg(feature = "kanban")]
pub(crate) mod kanban;
#[cfg(feature = "mindmap")]
pub(crate) mod mindmap;
#[cfg(feature = "packet")]
pub(crate) mod packet;
#[cfg(feature = "pie")]
pub(crate) mod pie;
#[cfg(feature = "quadrant")]
pub(crate) mod quadrant;
#[cfg(feature = "radar")]
pub(crate) mod radar;
#[cfg(feature = "requirement")]
pub(crate) mod requirement;
#[cfg(feature = "sankey")]
pub(crate) mod sankey;
#[cfg(feature = "sequence")]
pub(crate) mod sequence;
#[cfg(feature = "state")]
pub(crate) mod state;
#[cfg(feature = "swimlane")]
pub(crate) mod swimlane;
#[cfg(feature = "timeline")]
pub(crate) mod timeline;
#[cfg(feature = "treemap")]
pub(crate) mod treemap;
#[cfg(feature = "treeview")]
pub(crate) mod treeview;
#[cfg(feature = "usecase")]
pub(crate) mod usecase;
#[cfg(feature = "venn")]
pub(crate) mod venn;
#[cfg(feature = "xychart")]
pub(crate) mod xychart;
