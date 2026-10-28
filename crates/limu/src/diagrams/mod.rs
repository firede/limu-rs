//! The diagrams, each as Pele's src/diagrams has it: a parser that reads its text into a
//! model, and a layout that makes a scene of the model.

// A swimlane diagram is a flowchart laid out its own way, and is read and drawn by the
// flowchart's code.
#[cfg(feature = "class")]
pub(crate) mod class;
#[cfg(feature = "er")]
pub(crate) mod er;
#[cfg(any(feature = "flowchart", feature = "swimlane"))]
pub(crate) mod flowchart;
#[cfg(feature = "info")]
pub(crate) mod info;
#[cfg(feature = "requirement")]
pub(crate) mod requirement;
#[cfg(feature = "swimlane")]
pub(crate) mod swimlane;
