//! The markers at the ends of edges: how far an edge stops short of its end for each, and the
//! outline it is drawn with.

use crate::geom::Point;
use crate::path::Path;
use crate::scene::{Painting, Part};
use crate::style::Join;

/// The length of an arrowhead.
pub const MARKER_SIZE: f64 = 8.0;

/// The kind of marker at an end of an edge. Its name is Mermaid's, under the diagram's
/// prefix where a diagram has markers of its own: "arrow_point", "class.extension",
/// "er.zero_or_more".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum MarkerType {
    /// A filled arrowhead, "arrow_point".
    ArrowPoint,
    /// A filled dot, "arrow_circle".
    ArrowCircle,
    /// A cross, "arrow_cross".
    ArrowCross,
    /// A class diagram's hollow triangle of an extension.
    ClassExtension,
    /// A class diagram's filled diamond of a composition.
    ClassComposition,
    /// A class diagram's hollow diamond of an aggregation.
    ClassAggregation,
    /// A class diagram's ring of a lollipop.
    ClassLollipop,
    /// A class diagram's arrowhead of a dependency.
    ClassDependency,
    /// An entity relationship diagram's two bars of exactly one.
    ErOnlyOne,
    /// An entity relationship diagram's bar and ring of zero or one.
    ErZeroOrOne,
    /// An entity relationship diagram's crow's foot and bar of one or more.
    ErOneOrMore,
    /// An entity relationship diagram's crow's foot and ring of zero or more.
    ErZeroOrMore,
    /// An entity relationship diagram's hollow diamond of a parent.
    ErMdParent,
    /// A sequence diagram's open head of two strokes.
    SequenceOpen,
    /// A sequence diagram's half head, a solid triangle, on the left of the line.
    SequenceHalfLeft,
    /// A sequence diagram's half head, a solid triangle, on the right of the line.
    SequenceHalfRight,
    /// A sequence diagram's half head, a single stroke, on the left of the line.
    SequenceStickLeft,
    /// A sequence diagram's half head, a single stroke, on the right of the line.
    SequenceStickRight,
    /// A use case diagram's hollow triangle of a generalization.
    UsecaseExtension,
    /// A requirement diagram's circled plus of a containment.
    RequirementContains,
    /// A requirement diagram's open arrowhead.
    RequirementArrow,
}

impl MarkerType {
    /// Every type of marker.
    pub const ALL: &[MarkerType] = &[
        MarkerType::ArrowPoint,
        MarkerType::ArrowCircle,
        MarkerType::ArrowCross,
        MarkerType::ClassExtension,
        MarkerType::ClassComposition,
        MarkerType::ClassAggregation,
        MarkerType::ClassLollipop,
        MarkerType::ClassDependency,
        MarkerType::ErOnlyOne,
        MarkerType::ErZeroOrOne,
        MarkerType::ErOneOrMore,
        MarkerType::ErZeroOrMore,
        MarkerType::ErMdParent,
        MarkerType::SequenceOpen,
        MarkerType::SequenceHalfLeft,
        MarkerType::SequenceHalfRight,
        MarkerType::SequenceStickLeft,
        MarkerType::SequenceStickRight,
        MarkerType::UsecaseExtension,
        MarkerType::RequirementContains,
        MarkerType::RequirementArrow,
    ];

    /// The type's name.
    pub fn name(self) -> &'static str {
        match self {
            MarkerType::ArrowPoint => "arrow_point",
            MarkerType::ArrowCircle => "arrow_circle",
            MarkerType::ArrowCross => "arrow_cross",
            MarkerType::ClassExtension => "class.extension",
            MarkerType::ClassComposition => "class.composition",
            MarkerType::ClassAggregation => "class.aggregation",
            MarkerType::ClassLollipop => "class.lollipop",
            MarkerType::ClassDependency => "class.dependency",
            MarkerType::ErOnlyOne => "er.only_one",
            MarkerType::ErZeroOrOne => "er.zero_or_one",
            MarkerType::ErOneOrMore => "er.one_or_more",
            MarkerType::ErZeroOrMore => "er.zero_or_more",
            MarkerType::ErMdParent => "er.md_parent",
            MarkerType::SequenceOpen => "sequence.open",
            MarkerType::SequenceHalfLeft => "sequence.half_left",
            MarkerType::SequenceHalfRight => "sequence.half_right",
            MarkerType::SequenceStickLeft => "sequence.stick_left",
            MarkerType::SequenceStickRight => "sequence.stick_right",
            MarkerType::UsecaseExtension => "usecase.extension",
            MarkerType::RequirementContains => "requirement.contains",
            MarkerType::RequirementArrow => "requirement.arrow",
        }
    }

    /// The type of a name, if it names one.
    pub fn from_name(name: &str) -> Option<MarkerType> {
        MarkerType::ALL.iter().copied().find(|t| t.name() == name)
    }

    /// How far an edge stops short of its end to leave room for a marker of the type.
    pub fn trim(self) -> f64 {
        match self {
            MarkerType::ArrowPoint | MarkerType::ClassDependency => MARKER_SIZE - 1.0,
            MarkerType::ArrowCircle => MARKER_SIZE,
            MarkerType::ArrowCross => 4.0,
            MarkerType::ClassExtension => CLASS_TRIANGLE,
            MarkerType::ClassComposition | MarkerType::ClassAggregation => CLASS_DIAMOND,
            MarkerType::ClassLollipop => 2.0 * CLASS_RING,
            MarkerType::UsecaseExtension => 12.0,
            MarkerType::RequirementContains => 2.0 * REQUIREMENT_CONTAINS_RADIUS,
            _ => 0.0,
        }
    }

    /// How a marker of the type turns the corners of its outline when they are stroked, over
    /// whatever the edge's style says, as Pele sets it on the marker itself. Where it is none,
    /// the marker's corners are the edge's. Its ends are always the edge's, as Pele's take
    /// theirs from the group the edges are drawn in.
    pub fn join(self) -> Option<Join> {
        match self {
            MarkerType::ClassExtension
            | MarkerType::ClassComposition
            | MarkerType::ClassAggregation
            | MarkerType::SequenceOpen
            | MarkerType::UsecaseExtension
            | MarkerType::RequirementArrow => Some(Join::Round),
            _ => None,
        }
    }
}

/// What an edge has at an end: its type, where its tip is, and the way it points, as a unit
/// vector.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Marker {
    /// The type of marker.
    pub kind: MarkerType,
    /// Where its tip is.
    pub at: Point,
    /// The way it points, across.
    pub dx: f64,
    /// The way it points, down.
    pub dy: f64,
}

// The measures of the markers of class diagrams.
const CLASS_TRIANGLE: f64 = 12.0; // the length of an extension's triangle
const CLASS_DIAMOND: f64 = 16.0; // the length of a diamond
const CLASS_HALF: f64 = 6.0; // half the width of a triangle, and one more than a diamond's
const CLASS_RING: f64 = 5.0; // the radius of a lollipop's ring

// The measures of Pele's erMarker: a bar's half length, a foot's half width at the box and
// its length, and a ring's radius.
const ER_BAR: f64 = 5.0;
const ER_FOOT: f64 = 6.0;
const ER_FOOT_LENGTH: f64 = 10.0;
const ER_RING: f64 = 3.5;

// The length of a sequence diagram's head and how far its points are from the line.
const SEQ_HEAD: f64 = MARKER_SIZE;
const SEQ_SPREAD: f64 = 4.5;
const SEQ_STICK: f64 = 5.0;

// The radius of the circle of a requirement diagram's containment, and the length and half
// the width of its open arrowhead.
const REQUIREMENT_CONTAINS_RADIUS: f64 = 6.0;
const REQUIREMENT_ARROW_LENGTH: f64 = 9.0;
const REQUIREMENT_ARROW_HALF: f64 = 4.5;

impl Marker {
    /// The marker's shape, as Pele draws it: parts painted in the color of the edge's line.
    /// [`FillOnly`](Painting::FillOnly) fills a part in it and
    /// [`FillAndStroke`](Painting::FillAndStroke) strokes it too.
    /// [`StrokeOnly`](Painting::StrokeOnly) strokes it only, and [`Hollow`](Painting::Hollow)
    /// fills it with the background and strokes it, so that it hides the line beneath.
    pub fn outline(&self) -> Vec<Part> {
        let (x, y, dx, dy) = (self.at.x, self.at.y, self.dx, self.dy);
        // The normal, a quarter turn to the left of the way the marker points.
        let (nx, ny) = (-dy, dx);
        match self.kind {
            // Pele's flowchart markers, and a class diagram's dependency, which is Pele's
            // arrow_point.
            MarkerType::ArrowPoint | MarkerType::ClassDependency => {
                let (bx, by) = (x - dx * MARKER_SIZE, y - dy * MARKER_SIZE);
                let half = MARKER_SIZE * 0.42;
                let (lx, ly) = (nx * half, ny * half);
                vec![Part::path(
                    polygon(&[(x, y), (bx + lx, by + ly), (bx - lx, by - ly)]),
                    Painting::FillOnly,
                )]
            }
            MarkerType::ArrowCircle => {
                vec![Part::path(
                    ellipse(x - dx * 4.0, y - dy * 4.0, 3.5, 3.5),
                    Painting::FillOnly,
                )]
            }
            MarkerType::ArrowCross => {
                let (cx, cy) = (x - dx * 4.0, y - dy * 4.0);
                let a = 3.5;
                let (ux, uy) = ((dx + nx) * a, (dy + ny) * a);
                let (vx, vy) = ((dx - nx) * a, (dy - ny) * a);
                let mut p = Path::new();
                p.move_to(cx - ux, cy - uy)
                    .line_to(cx + ux, cy + uy)
                    .move_to(cx - vx, cy - vy)
                    .line_to(cx + vx, cy + vy);
                vec![Part::path(p, Painting::StrokeOnly)]
            }
            // The markers of class diagrams, as Pele's diagrams/class/markers.ts draws them.
            // Pele draws them inside the group of the edge, which fills nothing and strokes in
            // the color of lines, so those it gives no fill of their own are strokes only:
            // their insides show what is behind them, as the line stops at their backs.
            MarkerType::ClassExtension => {
                let (bx, by) = (x - dx * CLASS_TRIANGLE, y - dy * CLASS_TRIANGLE);
                let p = polygon(&[
                    (x, y),
                    (bx + nx * CLASS_HALF, by + ny * CLASS_HALF),
                    (bx - nx * CLASS_HALF, by - ny * CLASS_HALF),
                ]);
                vec![Part::path(p, Painting::StrokeOnly)]
            }
            MarkerType::ClassComposition => {
                vec![Part::path(self.class_diamond(), Painting::FillAndStroke)]
            }
            MarkerType::ClassAggregation => {
                vec![Part::path(self.class_diamond(), Painting::StrokeOnly)]
            }
            MarkerType::ClassLollipop => {
                let p = ellipse(
                    x - dx * CLASS_RING,
                    y - dy * CLASS_RING,
                    CLASS_RING,
                    CLASS_RING,
                );
                vec![Part::path(p, Painting::StrokeOnly)]
            }
            // The crow's feet of entity relationship diagrams, as Pele's erMarker draws them:
            // bars and feet stroked in the color of the line, and rings and a diamond that
            // hide the line beneath them. A marker's tip is where the line meets the box, and
            // it reaches back along the line from there, at most 18. The line runs under the
            // marker to the box, so none trims it.
            MarkerType::ErOnlyOne => {
                let mut p = Path::new();
                self.er_bar(&mut p, 6.0);
                self.er_bar(&mut p, 10.0);
                vec![Part::path(p, Painting::StrokeOnly)]
            }
            MarkerType::ErZeroOrOne => {
                let mut p = Path::new();
                self.er_bar(&mut p, 6.0);
                vec![Part::path(p, Painting::StrokeOnly), self.er_ring(13.5)]
            }
            MarkerType::ErOneOrMore => {
                let mut p = Path::new();
                self.er_foot(&mut p);
                self.er_bar(&mut p, ER_FOOT_LENGTH + 3.5);
                vec![Part::path(p, Painting::StrokeOnly)]
            }
            MarkerType::ErZeroOrMore => {
                let mut p = Path::new();
                self.er_foot(&mut p);
                vec![
                    Part::path(p, Painting::StrokeOnly),
                    self.er_ring(ER_FOOT_LENGTH + ER_RING + 0.5),
                ]
            }
            MarkerType::ErMdParent => {
                let p = polygon(&[
                    self.er_at(0.0, 0.0),
                    self.er_at(6.0, 4.0),
                    self.er_at(12.0, 0.0),
                    self.er_at(6.0, -4.0),
                ]);
                vec![Part::path(p, Painting::Hollow)]
            }
            // The heads of sequence diagrams' messages that are not flowcharts', as Pele's
            // sequence render draws them: an open head of two strokes, and half heads, a
            // solid triangle or a single stroke, on one side of the line. A half head's side
            // is named as seen along the way it points: "left" is the side of the normal. The
            // line runs to the tip of all of them, so none trims it. The heads are drawn for
            // messages, which run across, so their points are off the line by the normal,
            // which is down or up.
            MarkerType::SequenceOpen => {
                let (bx, by) = (x - dx * SEQ_HEAD, y - dy * SEQ_HEAD);
                // The first point is above the line for a head that points either way across.
                let (sx, sy) = (nx * dx * SEQ_SPREAD, ny * dx * SEQ_SPREAD);
                let mut p = Path::new();
                p.move_to(bx - sx, by - sy)
                    .line_to(x, y)
                    .line_to(bx + sx, by + sy);
                vec![Part::path(p, Painting::StrokeOnly)]
            }
            MarkerType::SequenceHalfLeft => self.sequence_half(1.0, false),
            MarkerType::SequenceHalfRight => self.sequence_half(-1.0, false),
            MarkerType::SequenceStickLeft => self.sequence_half(1.0, true),
            MarkerType::SequenceStickRight => self.sequence_half(-1.0, true),
            // The hollow triangle of a use case diagram's generalization, with its tip at the
            // end of the line, as Pele's extension in its use case render draws it.
            MarkerType::UsecaseExtension => {
                let (length, half) = (12.0, 6.0);
                let (bx, by) = (x - dx * length, y - dy * length);
                let p = polygon(&[
                    (x, y),
                    (bx - dy * half, by + dx * half),
                    (bx + dy * half, by - dx * half),
                ]);
                vec![Part::path(p, Painting::Hollow)]
            }
            // The markers of requirement diagrams, as Pele's requirement render draws them:
            // both stroked in the color of the edge's line, filled with nothing.
            MarkerType::RequirementContains => {
                // A circled plus whose rim touches the tip, the circle beyond it. An edge
                // stops short of its end by the circle's width, so that it ends at the far
                // side of the rim.
                let r = REQUIREMENT_CONTAINS_RADIUS;
                let (cx, cy) = (x - dx * r, y - dy * r);
                let mut plus = Path::new();
                plus.move_to(cx - dx * r, cy - dy * r)
                    .line_to(cx + dx * r, cy + dy * r)
                    .move_to(cx + dy * r, cy - dx * r)
                    .line_to(cx - dy * r, cy + dx * r);
                vec![
                    Part::path(ellipse(cx, cy, r, r), Painting::StrokeOnly),
                    Part::path(plus, Painting::StrokeOnly),
                ]
            }
            MarkerType::RequirementArrow => {
                // An open arrowhead with its tip at the end of the edge, as Pele's arrowMarker
                // draws it. The edge runs on to the tip.
                let (bx, by) = (
                    x - dx * REQUIREMENT_ARROW_LENGTH,
                    y - dy * REQUIREMENT_ARROW_LENGTH,
                );
                let (h, mut p) = (REQUIREMENT_ARROW_HALF, Path::new());
                p.move_to(bx - dy * h, by + dx * h)
                    .line_to(x, y)
                    .line_to(bx + dy * h, by - dx * h);
                vec![Part::path(p, Painting::StrokeOnly)]
            }
        }
    }

    /// The outline of a composition's or an aggregation's diamond, its tip at the marker's.
    fn class_diamond(&self) -> Path {
        let (x, y, dx, dy, nx, ny) = (self.at.x, self.at.y, self.dx, self.dy, -self.dy, self.dx);
        let (mx, my) = (x - dx * CLASS_DIAMOND / 2.0, y - dy * CLASS_DIAMOND / 2.0);
        let half = CLASS_HALF - 1.0;
        polygon(&[
            (x, y),
            (mx + nx * half, my + ny * half),
            (x - dx * CLASS_DIAMOND, y - dy * CLASS_DIAMOND),
            (mx - nx * half, my - ny * half),
        ])
    }

    /// The point back along the line from the marker's tip and side across it, to the right
    /// of the way it points, as Pele's erMarker places its points.
    fn er_at(&self, back: f64, side: f64) -> (f64, f64) {
        let (x, y, dx, dy) = (self.at.x, self.at.y, self.dx, self.dy);
        (x - dx * back - dy * side, y - dy * back + dx * side)
    }

    /// Adds a bar across the line, back from the tip.
    fn er_bar(&self, p: &mut Path, back: f64) {
        let (a, b) = (self.er_at(back, -ER_BAR), self.er_at(back, ER_BAR));
        p.move_to(a.0, a.1).line_to(b.0, b.1);
    }

    /// Adds a crow's foot: two toes from the box that meet on the line.
    fn er_foot(&self, p: &mut Path) {
        let (a, b, c) = (
            self.er_at(0.0, -ER_FOOT),
            self.er_at(ER_FOOT_LENGTH, 0.0),
            self.er_at(0.0, ER_FOOT),
        );
        p.move_to(a.0, a.1).line_to(b.0, b.1).line_to(c.0, c.1);
    }

    /// A ring on the line, back from the tip.
    fn er_ring(&self, back: f64) -> Part {
        let (cx, cy) = (self.at.x - self.dx * back, self.at.y - self.dy * back);
        Part::path(ellipse(cx, cy, ER_RING, ER_RING), Painting::Hollow)
    }

    /// A half head on the side the sign of side says, a solid triangle or a single stroke.
    fn sequence_half(&self, side: f64, stick: bool) -> Vec<Part> {
        let (x, y, dx, dy, nx, ny) = (self.at.x, self.at.y, self.dx, self.dy, -self.dy, self.dx);
        let (bx, by) = (x - dx * SEQ_HEAD, y - dy * SEQ_HEAD);
        if stick {
            let mut p = Path::new();
            p.move_to(x, y)
                .line_to(bx + nx * side * SEQ_STICK, by + ny * side * SEQ_STICK);
            return vec![Part::path(p, Painting::StrokeOnly)];
        }
        let p = polygon(&[
            (x, y),
            (bx + nx * side * SEQ_SPREAD, by + ny * side * SEQ_SPREAD),
            (bx, by),
        ]);
        vec![Part::path(p, Painting::FillOnly)]
    }
}

/// A closed polygon through points.
fn polygon(points: &[(f64, f64)]) -> Path {
    let mut p = Path::new();
    p.polygon(points);
    p
}

/// An ellipse about cx, cy of radii rx and ry.
fn ellipse(cx: f64, cy: f64, rx: f64, ry: f64) -> Path {
    let mut p = Path::new();
    p.ellipse(cx, cy, rx, ry);
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::scene::Outline;

    #[test]
    fn markers() {
        let m = |kind| Marker {
            kind,
            at: Point::new(10.0, 0.0),
            dx: 1.0,
            dy: 0.0,
        };
        let parts = m(MarkerType::ArrowPoint).outline();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].paint, Painting::FillOnly);
        let Outline::Path(p) = &parts[0].outline else {
            panic!("arrow {parts:?}")
        };
        assert_eq!(p.bounds(), Rect::new(2.0, -3.36, 8.0, 6.72));
        let cross = m(MarkerType::ArrowCross).outline();
        assert!(
            cross.len() == 1 && cross[0].paint == Painting::StrokeOnly,
            "cross {cross:?}"
        );
        assert_eq!(MarkerType::ArrowPoint.trim(), 7.0);
        assert_eq!(MarkerType::ErOnlyOne.trim(), 0.0);
        for &t in MarkerType::ALL {
            assert_eq!(MarkerType::from_name(t.name()), Some(t));
            assert!(!m(t).outline().is_empty(), "{t:?} has no outline");
        }
        assert_eq!(MarkerType::from_name("arrow_open"), None);
    }
}
