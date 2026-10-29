//! Paths as straight lines, and lines cut into dashes.

use crate::geom::Point;
use crate::path::{Path, Step};

/// Points joined by straight lines. A closed one is joined from its last point back to its
/// first as well.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Polyline {
    /// The points, in turn.
    pub points: Vec<Point>,
    /// Whether the last point is joined back to the first.
    pub closed: bool,
}

impl Path {
    /// The path as polylines, one for each of its figures, with its curves cut into straight
    /// lines that stray from them by no more than tol, or 0.1 when tol is not above zero. It
    /// is for backends that draw no curves, or no dashes, and for finding what is near a
    /// point. A figure of a single point is left out.
    pub fn flatten(&self, tol: f64) -> Vec<Polyline> {
        let tol = if tol > 0.0 { tol } else { 0.1 };
        let mut out = Vec::new();
        let mut cur: Vec<Point> = Vec::new();
        let mut at = Point::default(); // where the path is
        let flush = |cur: &mut Vec<Point>, out: &mut Vec<Polyline>, closed: bool| {
            if closed && cur.len() > 2 && cur.last() == cur.first() {
                cur.pop();
            }
            if cur.len() > 1 {
                out.push(Polyline {
                    points: std::mem::take(cur),
                    closed,
                });
            }
            cur.clear();
        };
        for step in self.steps() {
            // A step that draws with no figure begun, as one after a close, begins one where
            // the path is.
            if !matches!(step, Step::MoveTo(_) | Step::Close) && cur.is_empty() {
                cur.push(at);
            }
            match step {
                Step::MoveTo(p) => {
                    flush(&mut cur, &mut out, false);
                    at = p;
                    cur.push(p);
                }
                Step::LineTo(p) => {
                    at = p;
                    cur.push(p);
                }
                Step::QuadTo(c, end) => {
                    // A quadratic curve cut into n even steps strays from them by at most
                    // |p0 - 2p1 + p2| / (4 n²).
                    let n = steps(
                        dist(at.x - 2.0 * c.x + end.x, at.y - 2.0 * c.y + end.y) / 4.0,
                        tol,
                    );
                    for i in 1..=n {
                        let t = i as f64 / n as f64;
                        let u = 1.0 - t;
                        cur.push(Point::new(
                            u * u * at.x + 2.0 * u * t * c.x + t * t * end.x,
                            u * u * at.y + 2.0 * u * t * c.y + t * t * end.y,
                        ));
                    }
                    *cur.last_mut().unwrap() = end;
                    at = end;
                }
                Step::CubeTo(c1, c2, end) => {
                    // A cubic curve cut into n even steps strays from them by at most 3/4 of
                    // the larger of |p0 - 2p1 + p2| and |p1 - 2p2 + p3|, over n².
                    let dd = dist(at.x - 2.0 * c1.x + c2.x, at.y - 2.0 * c1.y + c2.y)
                        .max(dist(c1.x - 2.0 * c2.x + end.x, c1.y - 2.0 * c2.y + end.y));
                    let n = steps(dd * 3.0 / 4.0, tol);
                    for i in 1..=n {
                        let t = i as f64 / n as f64;
                        let u = 1.0 - t;
                        let (k0, k1, k2, k3) =
                            (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                        cur.push(Point::new(
                            k0 * at.x + k1 * c1.x + k2 * c2.x + k3 * end.x,
                            k0 * at.y + k1 * c1.y + k2 * c2.y + k3 * end.y,
                        ));
                    }
                    *cur.last_mut().unwrap() = end;
                    at = end;
                }
                Step::Close => {
                    if let Some(&first) = cur.first() {
                        at = first;
                        flush(&mut cur, &mut out, true);
                    }
                }
            }
        }
        flush(&mut cur, &mut out, false);
        out
    }
}

/// How many even steps keep a curve within tol of them, given its error over one step, err:
/// err / n² ≤ tol.
fn steps(err: f64, tol: f64) -> usize {
    let n = (err / tol).sqrt().ceil();
    if !(n >= 1.0) {
        1
    } else {
        n.min(256.0) as usize
    }
}

fn dist(dx: f64, dy: f64) -> f64 {
    dx.hypot(dy)
}

/// Cuts polylines into dashes, as SVG's stroke-dasharray and stroke-dashoffset do. The
/// pattern is the lengths of dashes and gaps in turn, taken twice over when it has an odd
/// number of them, and starts anew with each polyline, offset by offset along it. A dash goes
/// round the corners it meets. A dash of no length is a polyline of two points that are the
/// same, which a stroke with round caps draws as a dot. A pattern with a length below zero,
/// or none above it, leaves the polylines as they are, as SVG draws them solid. It is for a
/// backend whose platform draws no dashed strokes of its own.
pub fn dash(lines: &[Polyline], pattern: &[f64], offset: f64) -> Vec<Polyline> {
    let pattern: Vec<f64> = if pattern.len() % 2 == 1 {
        pattern.iter().chain(pattern).copied().collect()
    } else {
        pattern.to_vec()
    };
    let mut total = 0.0;
    for &l in &pattern {
        if !(l >= 0.0) || l.is_infinite() {
            return lines.to_vec();
        }
        total += l;
    }
    if !(total > 0.0) || !offset.is_finite() {
        return lines.to_vec();
    }
    let n = pattern.len();
    let mut out = Vec::new();
    for line in lines {
        let pts = &line.points;
        if pts.len() < 2 {
            continue;
        }
        // Where the pattern is at the start of the line: in its entry idx, with remain of it
        // to go.
        let mut o = offset % total;
        if o < 0.0 {
            o += total;
        }
        let mut idx = 0;
        while o > 0.0 && o >= pattern[idx] {
            o -= pattern[idx];
            idx = (idx + 1) % n;
        }
        let mut remain = pattern[idx] - o;
        let mut on = idx % 2 == 0;
        let mut dash: Vec<Point> = if on { vec![pts[0]] } else { Vec::new() };
        let segs = if line.closed {
            pts.len()
        } else {
            pts.len() - 1
        };
        for i in 0..segs {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            let length = dist(b.x - a.x, b.y - a.y);
            if length == 0.0 {
                continue;
            }
            let along = |pos: f64| {
                if pos >= length {
                    return b;
                }
                let k = pos / length;
                Point::new(a.x + (b.x - a.x) * k, a.y + (b.y - a.y) * k)
            };
            let mut pos = 0.0;
            loop {
                if remain <= 0.0 {
                    // The entry is done: a dash ends, or one begins.
                    if on {
                        if dash.len() == 1 {
                            dash.push(dash[0]);
                        }
                        out.push(Polyline {
                            points: std::mem::take(&mut dash),
                            closed: false,
                        });
                    }
                    idx = (idx + 1) % n;
                    remain = pattern[idx];
                    on = idx % 2 == 0;
                    if on {
                        dash = vec![along(pos)];
                    }
                    continue;
                }
                if pos >= length {
                    break;
                }
                let step = remain.min(length - pos);
                pos += step;
                remain -= step;
                if on {
                    dash.push(along(pos));
                }
            }
        }
        if on && dash.len() > 1 {
            out.push(Polyline {
                points: dash,
                closed: false,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn flatten() {
        let mut p = Path::new();
        p.move_to(0.0, 0.0)
            .line_to(10.0, 0.0)
            .move_to(5.0, 5.0)
            .move_to(0.0, 10.0)
            .line_to(10.0, 10.0)
            .line_to(10.0, 20.0)
            .close();
        let lines = p.flatten(0.1);
        assert_eq!(lines.len(), 2, "a lone point is left out");
        let l = &lines[0];
        assert!(
            !l.closed && l.points.len() == 2 && l.points[1] == Point::new(10.0, 0.0),
            "first {l:?}"
        );
        assert!(
            lines[1].closed && lines[1].points.len() == 3,
            "second {:?}",
            lines[1]
        );

        // A circle stays within the tolerance of itself, and closes without repeating its
        // start.
        let (r, tol) = (50.0, 0.1);
        let c = Path::new().ellipse(0.0, 0.0, r, r).flatten(tol);
        assert!(c.len() == 1 && c[0].closed, "circle {c:?}");
        let pts = &c[0].points;
        assert_ne!(pts[0], pts[pts.len() - 1], "the start is repeated");
        for (i, p) in pts.iter().enumerate() {
            let q = pts[(i + 1) % pts.len()];
            let mid = Point::new((p.x + q.x) / 2.0, (p.y + q.y) / 2.0);
            let d = r - mid.x.hypot(mid.y);
            assert!(d <= tol * 1.01, "strays {d} from the circle");
        }
        assert!(pts.len() <= 200, "{} points for a circle", pts.len());

        // A step after a close starts where the figure began.
        let mut p = Path::new();
        p.move_to(1.0, 1.0)
            .line_to(5.0, 1.0)
            .line_to(5.0, 5.0)
            .close()
            .line_to(0.0, 0.0);
        let l = p.flatten(0.0);
        assert!(
            l.len() == 2 && l[1].points[0] == Point::new(1.0, 1.0),
            "after close {l:?}"
        );
        // Curves end exactly where they say.
        let mut p = Path::new();
        p.move_to(0.0, 0.0).quad_to(10.0, 10.0, 20.0, 0.3);
        let q = p.flatten(0.01);
        assert_eq!(q[0].points.last(), Some(&Point::new(20.0, 0.3)));
        // The points are the same on every platform: fused, on arm64, as Go would fuse them,
        // this one's x would be 169.94974746830584.
        let o = Path::new().ellipse(165.0, 83.0, 7.0, 7.0).flatten(0.25);
        assert_eq!(
            o[0].points[2],
            Point::new(169.94974746830582, 87.94974746830583)
        );
    }

    #[test]
    fn dashes() {
        let line = [Polyline {
            points: vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0)],
            closed: false,
        }];
        let xs = |ls: Vec<Polyline>| -> Vec<(f64, f64)> {
            ls.iter()
                .map(|l| (l.points[0].x, l.points[l.points.len() - 1].x))
                .collect()
        };
        for (pattern, offset, want) in [
            (&[3.0, 2.0][..], 0.0, &[(0.0, 3.0), (5.0, 8.0)][..]),
            (&[3.0], 0.0, &[(0.0, 3.0), (6.0, 9.0)]), // 3 3
            (&[3.0, 2.0], 1.0, &[(0.0, 2.0), (4.0, 7.0), (9.0, 10.0)]),
            (&[3.0, 2.0], -1.0, &[(1.0, 4.0), (6.0, 9.0)]),
            (&[0.0, 5.0], 0.0, &[(0.0, 0.0), (5.0, 5.0), (10.0, 10.0)]), // dots
        ] {
            let got = xs(dash(&line, pattern, offset));
            assert!(
                got.len() == want.len()
                    && got
                        .iter()
                        .zip(want)
                        .all(|(g, w)| near(g.0, w.0) && near(g.1, w.1)),
                "{pattern:?} offset {offset}: {got:?}, want {want:?}"
            );
        }

        // A dash goes round a corner, and round the close of a closed line.
        let square = [Polyline {
            points: vec![
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 10.0),
                Point::new(0.0, 10.0),
            ],
            closed: true,
        }];
        let d = dash(&square, &[12.0, 3.0], 0.0);
        let first = &d[0].points;
        assert_eq!(
            first,
            &[
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 2.0)
            ]
        );
        assert_eq!(d[d.len() - 1].points.last(), Some(&Point::new(0.0, 0.0)));

        // Patterns SVG draws solid.
        for p in [&[][..], &[0.0, 0.0], &[3.0, -1.0]] {
            let got = dash(&line, p, 0.0);
            assert!(got.len() == 1 && got[0].points.len() == 2, "{p:?}: {got:?}");
        }
    }
}
