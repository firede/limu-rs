//! What layout does to the items and paths of a scene, which hosts have no need of.

use crate::scene::{Item, Kind, Outline, Path, Point, Rect, Scene};

/// Moves an item by dx, dy: its bounds, parts, route, markers, label and picture.
pub(crate) fn move_by(it: &mut Item, dx: f64, dy: f64) {
    let point = |p: &mut Point| {
        p.x += dx;
        p.y += dy;
    };
    it.bounds = it.bounds.moved(dx, dy);
    for part in &mut it.parts {
        match &mut part.outline {
            Outline::Path(p) => p.translate(dx, dy),
            Outline::Box { rect, .. } => *rect = rect.moved(dx, dy),
        }
    }
    if let Kind::Edge(e) = &mut it.kind {
        e.route.iter_mut().for_each(point);
        for m in e.ends.iter_mut().flatten() {
            point(&mut m.at);
        }
    }
    if let Some(l) = &mut it.label {
        point(&mut l.at);
    }
    if let Some(p) = &mut it.picture {
        p.bounds = p.bounds.moved(dx, dy);
    }
}

/// A path's points and the control points of its curves, in order: the line its curves are
/// drawn about, which runs straight between them. It is the route of an edge whose layout
/// gives it none, as a self-loop: the curve out of its node and back.
pub(crate) fn corners(p: &Path) -> Vec<Point> {
    p.args()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&[x, y]| Point::new(x, y))
        .collect()
}

/// Whether every number that places or sizes something in a scene is finite: its size, and
/// the bounds, paths, routes, markers, labels and pictures of its items. A layout given
/// numbers at the edge of what an f64 holds, as a sankey diagram's flow of 5e-324, can work
/// out NaN where Pele does too. limu gives an error for such a scene rather than hand it to
/// a backend.
pub(crate) fn finite(sc: &Scene) -> bool {
    let ok = |vs: &[f64]| vs.iter().all(|v| v.is_finite());
    let rect = |r: &Rect| ok(&[r.x, r.y, r.w, r.h]);
    let point = |p: &Point| ok(&[p.x, p.y]);
    if !ok(&[sc.width, sc.height, sc.font_size]) {
        return false;
    }
    sc.items.iter().all(|it| {
        rect(&it.bounds)
            && it.parts.iter().all(|p| match &p.outline {
                Outline::Path(p) => ok(p.args()),
                Outline::Box { rect: r, .. } => rect(r),
            })
            && it.edge().is_none_or(|e| {
                e.route.iter().all(point)
                    && e.ends
                        .iter()
                        .flatten()
                        .all(|m| point(&m.at) && ok(&[m.dx, m.dy]))
            })
            && it.label.as_ref().is_none_or(|p| {
                let l = &p.label;
                point(&p.at)
                    && ok(&[p.rotate, l.width, l.height, l.size, l.line_height])
                    && ok(&l.widths)
                    && l.lines.iter().flatten().all(|s| s.width.is_finite())
            })
            && it.picture.as_ref().is_none_or(|p| rect(&p.bounds))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::text::Label;
    use crate::scene::{Edge, Marker, MarkerType, Painting, Part, Picture, PictureSource, Placed};

    fn path(f: impl FnOnce(&mut Path) -> &mut Path) -> Path {
        let mut p = Path::new();
        f(&mut p);
        p
    }

    #[test]
    fn moves_an_item() {
        let mut it = Item::new(
            Kind::Edge(Edge {
                route: vec![Point::new(1.0, 1.0), Point::new(2.0, 2.0)],
                ends: [
                    Some(Marker {
                        kind: MarkerType::ArrowPoint,
                        at: Point::new(5.0, 5.0),
                        dx: 1.0,
                        dy: 0.0,
                    }),
                    None,
                ],
            }),
            "x.edge",
        );
        it.bounds = Rect::new(0.0, 0.0, 10.0, 10.0);
        it.parts = vec![
            Part::path(
                path(|p| p.move_to(1.0, 2.0).line_to(3.0, 4.0)),
                Painting::StrokeOnly,
            ),
            Part::rect(
                Rect::new(1.0, 1.0, 2.0, 2.0),
                false,
                Painting::FillAndStroke,
            ),
        ];
        it.label = Some(Placed::new(Label::default(), Point::new(5.0, 5.0)));
        it.picture = Some(Picture {
            source: PictureSource::Icon("fa:fa-car".into()),
            bounds: Rect::new(1.0, 1.0, 2.0, 2.0),
        });
        move_by(&mut it, 10.0, 20.0);
        assert_eq!(it.bounds, Rect::new(10.0, 20.0, 10.0, 10.0));
        assert_eq!(it.label.as_ref().unwrap().at, Point::new(15.0, 25.0));
        let e = it.edge().unwrap();
        assert_eq!(e.ends[0].unwrap().at, Point::new(15.0, 25.0));
        assert_eq!(e.route, [Point::new(11.0, 21.0), Point::new(12.0, 22.0)]);
        assert_eq!(
            it.parts[0].outline,
            Outline::Path(path(|p| p.move_to(11.0, 22.0).line_to(13.0, 24.0)))
        );
        assert_eq!(
            it.parts[1].outline,
            Outline::Box {
                rect: Rect::new(11.0, 21.0, 2.0, 2.0),
                rounded: false
            }
        );
        assert_eq!(it.picture.unwrap().bounds, Rect::new(11.0, 21.0, 2.0, 2.0));
    }

    #[test]
    fn finds_numbers_that_are_not_finite() {
        let mut it = Item::new(Kind::Edge(Edge::default()), "x.edge");
        it.parts = vec![Part::path(
            path(|p| p.move_to(0.0, 0.0).line_to(1.0, 1.0)),
            Painting::StrokeOnly,
        )];
        let mut sc = Scene {
            items: vec![it],
            ..Scene::default()
        };
        assert!(finite(&sc));
        sc.items[0].kind = Kind::Edge(Edge {
            route: vec![Point::new(f64::NAN, 0.0)],
            ends: [None, None],
        });
        assert!(!finite(&sc));
        sc.items[0].kind = Kind::Edge(Edge::default());
        sc.height = f64::INFINITY;
        assert!(!finite(&sc));
    }
}
