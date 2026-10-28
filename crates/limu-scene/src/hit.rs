//! Finding the item of a scene at a point.

use crate::flatten::Polyline;
use crate::geom::{Point, Rect};
use crate::scene::{Item, Outline, Painting, Scene};

impl Scene {
    /// The index of the item at a point, in the scene's units: the one drawn last of those
    /// there, and so on top. An item is there when its bounds hold the point, or the box of
    /// its label. An edge, and an item whose parts are all lines, is there instead when one
    /// of its lines passes within slop of the point, or one of its markers holds it.
    pub fn item_at(&self, at: Point, slop: f64) -> Option<usize> {
        self.items.iter().rposition(|it| hits(it, at, slop))
    }
}

fn hits(it: &Item, at: Point, slop: f64) -> bool {
    if label_holds(it, at) {
        return true;
    }
    let edge = it.edge();
    let lines = !it.parts.is_empty() && it.parts.iter().all(|p| p.paint == Painting::StrokeOnly);
    if edge.is_none() && !lines {
        return it.bounds.contains(at) || it.parts.iter().any(|p| bounds(&p.outline).contains(at));
    }
    for part in &it.parts {
        let near = match &part.outline {
            Outline::Box { rect, .. } => near(*rect, at, slop),
            Outline::Path(p) => p.flatten(0.5).iter().any(|l| line_near(l, at, slop)),
        };
        if near {
            return true;
        }
    }
    let Some(e) = edge else { return false };
    let marked = e.ends.iter().flatten().any(|m| {
        m.outline()
            .iter()
            .any(|p| near(bounds(&p.outline), at, slop))
    });
    marked
        || it.parts.is_empty()
            && e.route
                .windows(2)
                .any(|w| segment_near(w[0], w[1], at, slop))
}

/// The box an outline lies in.
fn bounds(o: &Outline) -> Rect {
    match o {
        Outline::Path(p) => p.bounds(),
        Outline::Box { rect, .. } => *rect,
    }
}

/// Whether the box of an item's label, turned with it, holds a point.
fn label_holds(it: &Item, at: Point) -> bool {
    let Some(p) = &it.label else { return false };
    let l = &p.label;
    if l.is_empty() {
        return false;
    }
    let mut at = at;
    if p.rotate != 0.0 {
        // The point turned back about the label's, into the label's own axes.
        let (sin, cos) = (-p.rotate * std::f64::consts::PI / 180.0).sin_cos();
        let (dx, dy) = (at.x - p.at.x, at.y - p.at.y);
        at = Point::new(p.at.x + dx * cos - dy * sin, p.at.y + dx * sin + dy * cos);
    }
    p.upright().contains(at)
}

/// Whether a point is within slop of a box.
fn near(b: Rect, at: Point, slop: f64) -> bool {
    Rect::new(b.x - slop, b.y - slop, b.w + 2.0 * slop, b.h + 2.0 * slop).contains(at)
}

fn line_near(l: &Polyline, at: Point, slop: f64) -> bool {
    let n = l.points.len();
    l.points
        .windows(2)
        .any(|w| segment_near(w[0], w[1], at, slop))
        || l.closed && n > 1 && segment_near(l.points[n - 1], l.points[0], at, slop)
}

/// Whether a point is within slop of the line from a to b.
fn segment_near(a: Point, b: Point, at: Point, slop: f64) -> bool {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l = dx * dx + dy * dy;
    let t = if l > 0.0 {
        (((at.x - a.x) * dx + (at.y - a.y) * dy) / l).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a.x + dx * t - at.x).hypot(a.y + dy * t - at.y) <= slop
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marker::{Marker, MarkerType};
    use crate::path::Path;
    use crate::scene::{Edge, Kind, Part, Placed};
    use crate::text::{Label, Span};

    #[test]
    fn item_at() {
        let mut node = Item::new(Kind::Node, "x.node");
        node.bounds = Rect::new(0.0, 0.0, 40.0, 20.0);
        let mut line = Path::new();
        line.move_to(40.0, 10.0).line_to(100.0, 10.0);
        let mut edge = Item::new(
            Kind::Edge(Edge {
                route: vec![],
                ends: [
                    None,
                    Some(Marker {
                        kind: MarkerType::ArrowPoint,
                        at: Point::new(100.0, 10.0),
                        dx: 1.0,
                        dy: 0.0,
                    }),
                ],
            }),
            "x.edge",
        );
        edge.bounds = Rect::new(40.0, 0.0, 60.0, 20.0);
        edge.parts = vec![Part::path(line, Painting::StrokeOnly)];
        let mut title = Item::new(Kind::Text, "x.title");
        let label = Label {
            lines: vec![vec![Span {
                text: "t".into(),
                ..Span::default()
            }]],
            width: 20.0,
            height: 10.0,
            ..Label::default()
        };
        title.label = Some(Placed {
            rotate: -90.0,
            ..Placed::new(label, Point::new(120.0, 50.0))
        });
        let sc = Scene {
            items: vec![node, edge, title],
            ..Scene::default()
        };
        assert_eq!(sc.item_at(Point::new(10.0, 10.0), 2.0), Some(0));
        // Over the node, by the edge's line, the edge drawn later is on top.
        assert_eq!(sc.item_at(Point::new(39.0, 11.0), 2.0), Some(1));
        // An edge is there along its line, not over all its bounds.
        assert_eq!(sc.item_at(Point::new(70.0, 11.5), 2.0), Some(1));
        assert_eq!(sc.item_at(Point::new(70.0, 18.0), 2.0), None);
        assert_eq!(
            sc.item_at(Point::new(95.0, 13.0), 0.0),
            Some(1),
            "on the marker"
        );
        // A turned label is there as it is turned: upright, 10 wide and 20 tall.
        assert_eq!(sc.item_at(Point::new(120.0, 58.0), 0.0), Some(2));
        assert_eq!(sc.item_at(Point::new(128.0, 50.0), 0.0), None);
    }
}
