//! Paths of lines and curves.

use std::f64::consts::PI;

use crate::geom::{Point, Rect};

/// A step of a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Op {
    /// To a point, starting a figure.
    MoveTo,
    /// To a point, in a straight line.
    LineTo,
    /// Through a control point to a point.
    QuadTo,
    /// Through two control points to a point.
    CubeTo,
    /// Back to where the figure began.
    Close,
}

impl Op {
    /// How many numbers the step takes.
    pub const fn args(self) -> usize {
        match self {
            Op::MoveTo | Op::LineTo => 2,
            Op::QuadTo => 4,
            Op::CubeTo => 6,
            Op::Close => 0,
        }
    }
}

/// A step of a path with its points, as [`Path::steps`] gives them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// To a point, starting a figure.
    MoveTo(Point),
    /// To a point, in a straight line.
    LineTo(Point),
    /// Through a control point to a point.
    QuadTo(Point, Point),
    /// Through two control points to a point.
    CubeTo(Point, Point, Point),
    /// Back to where the figure began.
    Close,
}

/// Lines and curves: the steps, and the numbers they take, in turn. Arcs are made of cubic
/// curves when they are added, so a backend draws only the four kinds of step. A path is
/// built by its methods, which keep its steps and numbers together.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Path {
    // The steps.
    pub(crate) ops: Vec<Op>,
    // The numbers the steps take, x and y in turn, the steps' after one another.
    pub(crate) args: Vec<f64>,
}

/// 4/3 (√2 − 1), how far along a quarter of a circle's tangents the control points of the
/// cubic curve that draws it are.
const KAPPA: f64 = 0.5522847498307936;

impl Path {
    /// An empty path.
    pub const fn new() -> Path {
        Path {
            ops: Vec::new(),
            args: Vec::new(),
        }
    }

    fn add(&mut self, op: Op, args: &[f64]) -> &mut Path {
        self.ops.push(op);
        self.args.extend_from_slice(args);
        self
    }

    /// Starts a figure at x, y.
    pub fn move_to(&mut self, x: f64, y: f64) -> &mut Path {
        self.add(Op::MoveTo, &[x, y])
    }

    /// Adds a straight line to x, y.
    pub fn line_to(&mut self, x: f64, y: f64) -> &mut Path {
        self.add(Op::LineTo, &[x, y])
    }

    /// Adds a quadratic curve through cx, cy to x, y.
    pub fn quad_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) -> &mut Path {
        self.add(Op::QuadTo, &[cx, cy, x, y])
    }

    /// Adds a cubic curve through c1x, c1y and c2x, c2y to x, y.
    pub fn cube_to(&mut self, c1x: f64, c1y: f64, c2x: f64, c2y: f64, x: f64, y: f64) -> &mut Path {
        self.add(Op::CubeTo, &[c1x, c1y, c2x, c2y, x, y])
    }

    /// Closes the figure, back to where it began.
    pub fn close(&mut self) -> &mut Path {
        self.add(Op::Close, &[])
    }

    /// Whether the path has no steps.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// The kinds of its steps, in turn.
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    /// The numbers its steps take, x and y in turn, the steps' after one another.
    pub fn args(&self) -> &[f64] {
        &self.args
    }

    /// The steps with their points, in turn. A path read from data whose steps take more
    /// numbers than it has ends at the first step without them.
    pub fn steps(&self) -> impl Iterator<Item = Step> + '_ {
        let mut at = 0;
        self.ops.iter().map_while(move |&op| {
            let a = self.args.get(at..at + op.args())?;
            at += op.args();
            let p = |i: usize| Point::new(a[i], a[i + 1]);
            match op {
                Op::MoveTo => Step::MoveTo(p(0)),
                Op::LineTo => Step::LineTo(p(0)),
                Op::QuadTo => Step::QuadTo(p(0), p(2)),
                Op::CubeTo => Step::CubeTo(p(0), p(2), p(4)),
                Op::Close => Step::Close,
            }
            .into()
        })
    }

    /// Where the path is: the end of its last step.
    pub fn last(&self) -> Point {
        let (mut start, mut at) = (Point::default(), Point::default());
        for step in self.steps() {
            match step {
                Step::MoveTo(p) => (start, at) = (p, p),
                Step::LineTo(p) | Step::QuadTo(_, p) | Step::CubeTo(_, _, p) => at = p,
                Step::Close => at = start,
            }
        }
        at
    }

    /// Adds a closed figure through the points.
    pub fn polygon(&mut self, points: &[(f64, f64)]) -> &mut Path {
        for (i, &(x, y)) in points.iter().enumerate() {
            if i == 0 {
                self.move_to(x, y);
            } else {
                self.line_to(x, y);
            }
        }
        self.close()
    }

    /// Adds a closed ellipse with its center at cx, cy, in four curves.
    pub fn ellipse(&mut self, cx: f64, cy: f64, rx: f64, ry: f64) -> &mut Path {
        let (kx, ky) = (rx * KAPPA, ry * KAPPA);
        self.move_to(cx + rx, cy);
        self.cube_to(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry);
        self.cube_to(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy);
        self.cube_to(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry);
        self.cube_to(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy);
        self.close()
    }

    /// Adds a closed box with its corners rounded by radius, which is held to half the box's
    /// smaller side.
    pub fn round_rect(&mut self, r: Rect, radius: f64) -> &mut Path {
        let radius = radius.min(r.w / 2.0).min(r.h / 2.0).max(0.0);
        let (x0, y0, x1, y1) = (r.x, r.y, r.x + r.w, r.y + r.h);
        if radius == 0.0 {
            return self.polygon(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
        }
        let c = radius * (1.0 - KAPPA);
        self.move_to(x0 + radius, y0);
        self.line_to(x1 - radius, y0);
        self.cube_to(x1 - c, y0, x1, y0 + c, x1, y0 + radius);
        self.line_to(x1, y1 - radius);
        self.cube_to(x1, y1 - c, x1 - c, y1, x1 - radius, y1);
        self.line_to(x0 + radius, y1);
        self.cube_to(x0 + c, y1, x0, y1 - c, x0, y1 - radius);
        self.line_to(x0, y0 + radius);
        self.cube_to(x0, y0 + c, x0 + c, y0, x0 + radius, y0);
        self.close()
    }

    /// Adds an elliptical arc from where the path is to x, y, as SVG's A command draws one:
    /// radii rx and ry, the ellipse turned by rot degrees, the large or the small way round,
    /// and clockwise on the screen when sweep is set. It is drawn as cubic curves, one for
    /// each quarter turn or less. An arc whose radii or ends place no ellipse is a line to its
    /// end.
    #[allow(clippy::too_many_arguments)]
    pub fn arc_to(
        &mut self,
        rx: f64,
        ry: f64,
        rot: f64,
        large: bool,
        sweep: bool,
        x: f64,
        y: f64,
    ) -> &mut Path {
        let from = self.last();
        let (x1, y1) = (from.x, from.y);
        if x1 == x && y1 == y {
            return self;
        }
        let (mut rx, mut ry) = (rx.abs(), ry.abs());
        if rx == 0.0 || ry == 0.0 {
            return self.line_to(x, y);
        }
        // The endpoint to center conversion of SVG's implementation notes, F.6.5.
        let (sin, cos) = (rot * PI / 180.0).sin_cos();
        let (dx, dy) = ((x1 - x) / 2.0, (y1 - y) / 2.0);
        let x1p = cos * dx + sin * dy;
        let y1p = -sin * dx + cos * dy;
        let l = x1p * x1p / (rx * rx) + y1p * y1p / (ry * ry);
        if l > 1.0 {
            let s = l.sqrt();
            rx *= s;
            ry *= s;
        }
        let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
        let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
        let mut co = (num / den).max(0.0).sqrt();
        if large == sweep {
            co = -co;
        }
        let (cxp, cyp) = (co * rx * y1p / ry, -co * ry * x1p / rx);
        let cx = cos * cxp - sin * cyp + (x1 + x) / 2.0;
        let cy = sin * cxp + cos * cyp + (y1 + y) / 2.0;
        let angle =
            |ux: f64, uy: f64, vx: f64, vy: f64| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
        let t1 = angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
        let mut dt = angle(
            (x1p - cxp) / rx,
            (y1p - cyp) / ry,
            (-x1p - cxp) / rx,
            (-y1p - cyp) / ry,
        );
        if !sweep && dt > 0.0 {
            dt -= 2.0 * PI;
        } else if sweep && dt < 0.0 {
            dt += 2.0 * PI;
        }
        if !cx.is_finite() || !cy.is_finite() || t1.is_nan() || !(dt.abs() <= 2.0 * PI) || dt == 0.0
        {
            // Radii or ends that are not finite numbers place no ellipse, and an arc of no
            // turn is a line; the path still reaches its end.
            return self.line_to(x, y);
        }
        // At most a quarter turn a curve. A turn of a quarter, or of a half, comes out of the
        // conversion a little over it as often as not: a quarter, as of a rounded corner, to
        // a part in 10^12 or less, and a half, whose center the conversion finds from the
        // square root of a difference near 0, to a part in 10^6 where its radius is small
        // beside its coordinates. A turn within a part in 10^4 of a whole number of quarters
        // is that many curves, each a little over a quarter at most, which strays from the
        // ellipse no more than a quarter does.
        let n = ((dt.abs() / (PI / 2.0) - 1e-4).ceil() as usize).max(1);
        let step = dt / n as f64;
        let k = 4.0 / 3.0 * (step / 4.0).tan();
        let point = |t: f64| {
            let (st, ct) = t.sin_cos();
            let (ex, ey) = (rx * ct, ry * st); // on the ellipse
            let (tx, ty) = (-rx * st, ry * ct); // along it
            (
                cos * ex - sin * ey + cx,
                sin * ex + cos * ey + cy,
                cos * tx - sin * ty,
                sin * tx + cos * ty,
            )
        };
        let mut t = t1;
        let (mut ax, mut ay, mut adx, mut ady) = point(t);
        for i in 0..n {
            t += step;
            let (mut bx, mut by, bdx, bdy) = point(t);
            if i == n - 1 {
                (bx, by) = (x, y);
            }
            self.cube_to(
                ax + k * adx,
                ay + k * ady,
                bx - k * bdx,
                by - k * bdy,
                bx,
                by,
            );
            (ax, ay, adx, ady) = (bx, by, bdx, bdy);
        }
        self
    }

    /// The box the path's points and control points lie in.
    pub fn bounds(&self) -> Rect {
        if self.args.len() < 2 {
            return Rect::default();
        }
        let (mut x0, mut y0) = (self.args[0], self.args[1]);
        let (mut x1, mut y1) = (x0, y0);
        for &[x, y] in self.args.as_chunks::<2>().0 {
            (x0, y0) = (x0.min(x), y0.min(y));
            (x1, y1) = (x1.max(x), y1.max(y));
        }
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    }

    /// The path scaled by s about the origin, then moved by dx, dy, as an icon drawn on a
    /// grid of its own is placed in a box.
    pub fn scaled(&self, s: f64, dx: f64, dy: f64) -> Path {
        let args = self
            .args
            .iter()
            .enumerate()
            .map(|(i, v)| v * s + if i % 2 == 0 { dx } else { dy })
            .collect();
        Path {
            ops: self.ops.clone(),
            args,
        }
    }

    /// Adds another path's steps after this one's.
    pub fn append(&mut self, other: &Path) -> &mut Path {
        self.ops.extend_from_slice(&other.ops);
        self.args.extend_from_slice(&other.args);
        self
    }

    /// Moves each of the path's points where a function takes it, as a turn does.
    pub fn map_points(&mut self, mut f: impl FnMut(Point) -> Point) {
        for [x, y] in self.args.as_chunks_mut::<2>().0 {
            let p = f(Point::new(*x, *y));
            (*x, *y) = (p.x, p.y);
        }
    }

    /// Moves the path by dx, dy.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        for [x, y] in self.args.as_chunks_mut::<2>().0 {
            *x += dx;
            *y += dy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn arc_to() {
        // From (10,0) to (-10,0) on a circle of radius 10 about the origin.
        for (sweep, mid) in [
            (true, Point::new(0.0, 10.0)),
            (false, Point::new(0.0, -10.0)),
        ] {
            let mut p = Path::new();
            p.move_to(10.0, 0.0)
                .arc_to(10.0, 10.0, 0.0, false, sweep, -10.0, 0.0);
            assert_eq!(p.ops.len() - 1, 2, "curves");
            let end = p.last();
            assert!(near(end.x, -10.0) && near(end.y, 0.0), "ends at {end:?}");
            let m = Point::new(p.args[6], p.args[7]);
            assert!(
                near(m.x, mid.x) && near(m.y, mid.y),
                "sweep {sweep} passes {m:?}"
            );
        }
        // Radii too small to reach are scaled up, as SVG does.
        let mut p = Path::new();
        p.move_to(0.0, 0.0)
            .arc_to(1.0, 1.0, 0.0, false, true, 10.0, 0.0);
        assert!(p.bounds().h >= 4.9, "bounds {:?}", p.bounds());
        let mut p = Path::new();
        p.move_to(0.0, 0.0)
            .arc_to(0.0, 5.0, 0.0, false, true, 4.0, 0.0);
        assert_eq!(p.ops[1], Op::LineTo, "a zero radius is a line");
        // A quarter turn is one curve, though the angle the conversion finds is a little over.
        for r in [1.0, 3.0, 4.5, 7.25, 10.0, 1e3] {
            for at in [(0.0, 0.0), (0.1, 0.2), (123.45, 678.9), (-3.3, 1e4)] {
                for sweep in [false, true] {
                    let mut p = Path::new();
                    p.move_to(at.0 + r, at.1)
                        .arc_to(r, r, 0.0, false, sweep, at.0, at.1 + r);
                    assert_eq!(
                        p.ops.len() - 1,
                        1,
                        "a quarter turn of radius {r} from {at:?}"
                    );
                    let mut p = Path::new();
                    p.move_to(at.0 + r, at.1)
                        .arc_to(r, r, 0.0, false, sweep, at.0 - r, at.1);
                    assert_eq!(p.ops.len() - 1, 2, "a half turn of radius {r} from {at:?}");
                }
            }
        }
        // What is not a finite number places no ellipse: the path goes on to its end in a line.
        for c in [
            [f64::NAN, 5.0, 4.0],
            [f64::INFINITY, 5.0, 4.0],
            [5.0, 5.0, f64::NAN],
        ] {
            let mut p = Path::new();
            p.move_to(0.0, 0.0)
                .arc_to(c[0], c[1], 0.0, false, true, c[2], 3.0);
            assert!(
                p.ops.len() == 2 && p.ops[1] == Op::LineTo && p.args[3] == 3.0,
                "arc_to({c:?}) {:?} {:?}",
                p.ops,
                p.args
            );
        }
    }

    #[test]
    fn shapes() {
        let b = Path::new().ellipse(5.0, 5.0, 4.0, 2.0).bounds();
        assert!(
            near(b.x, 1.0) && near(b.w, 8.0) && near(b.h, 4.0),
            "ellipse {b:?}"
        );
        let mut r = Path::new();
        r.round_rect(Rect::new(0.0, 0.0, 20.0, 10.0), 100.0);
        assert_eq!(r.bounds(), Rect::new(0.0, 0.0, 20.0, 10.0));
        assert_eq!(r.last(), Point::new(5.0, 0.0), "closes at");
    }

    #[test]
    fn scaled() {
        let mut p = Path::new();
        p.move_to(0.0, 0.0).line_to(24.0, 12.0).close();
        let mut want = Path::new();
        want.move_to(10.0, 20.0).line_to(22.0, 26.0).close();
        assert_eq!(p.scaled(0.5, 10.0, 20.0), want);
    }
}
