//! The diagrams, each as Pele's src/diagrams has it: a parser that reads its text into a
//! model, and a layout that makes a scene of the model.

#[cfg(feature = "agentflow")]
pub(crate) mod agentflow;
#[cfg(feature = "class")]
pub(crate) mod class;
#[cfg(feature = "er")]
pub(crate) mod er;
// A swimlane diagram is a flowchart laid out its own way, and is read and drawn by the
// flowchart's code. An agent flow draws its nodes in the flowchart's shapes.
#[cfg(any(feature = "agentflow", feature = "flowchart", feature = "swimlane"))]
pub(crate) mod flowchart;
#[cfg(feature = "info")]
pub(crate) mod info;
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
#[cfg(feature = "requirement")]
pub(crate) mod requirement;
#[cfg(feature = "state")]
pub(crate) mod state;
#[cfg(feature = "swimlane")]
pub(crate) mod swimlane;
#[cfg(feature = "timeline")]
pub(crate) mod timeline;
#[cfg(feature = "usecase")]
pub(crate) mod usecase;
