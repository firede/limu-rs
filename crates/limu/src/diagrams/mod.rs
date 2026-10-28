//! The diagrams, each as Pele's src/diagrams has it: a parser that reads its text into a
//! model, and a layout that makes a scene of the model.

// A swimlane diagram is a flowchart laid out its own way, and is read and drawn by the
// flowchart's code.
#[cfg(any(feature = "flowchart", feature = "swimlane"))]
pub(crate) mod flowchart;
#[cfg(feature = "swimlane")]
pub(crate) mod swimlane;
