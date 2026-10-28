//! What an agent flow's model reports about it, as Pele's diagnostics.ts names it. Of
//! Mermaid's vocabulary for agent flows, a parser raises only these: shapes an agent flow does
//! not draw, and containers that would hold each other. Pele keeps the rest for tools that
//! check what a diagram means, and limu has no use for them.

/// The id of a diagnostic a parser raises, Pele's DiagnosticId of those in EMITTED.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiagnosticId {
    ShapeUnsupported,
    ShapeRemoved,
    ContainmentViolation,
}

impl DiagnosticId {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            DiagnosticId::ShapeUnsupported => "SHAPE_UNSUPPORTED",
            DiagnosticId::ShapeRemoved => "SHAPE_REMOVED",
            DiagnosticId::ContainmentViolation => "CONTAINMENT_VIOLATION",
        }
    }
}
