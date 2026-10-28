//! What lies on a fill a style sets, as Pele's src/svg/edges.ts finds it: whether an edge
//! runs inside one fill, or crosses the border of one. A diagram looks for it among the
//! groups whose author gives them a fill, and records it in the scene. The style then picks
//! the color the edge is drawn in on that fill, or lays a casing under it.

use crate::css::{declarations, declared, declares, inks, resolve_style};
use crate::js::math::round;
use crate::scene::style::{Cap, Join, Paint, Style};
use crate::scene::{Item, Kind, Marker, Outline, Painting, Part, Path, Rect, Step};

/// A box a style fills, Pele's Fill: its left, top, right and bottom, and the declarations
/// its author gives it, whose style paints it. A diagram gathers its fills of the groups
/// whose author paints them.
#[derive(Clone, Debug)]
pub(crate) struct Fill {
    pub(crate) bounds: [f64; 4],
    pub(crate) decls: Vec<String>,
}

/// What Pele's overFills makes of an edge on fills: the ground to give the edge's style where
/// it lies inside one fill that ink picks a color on, unless the style of its line, line,
/// sets it a stroke, as edgeInk says, and whether it needs a casing, where it crosses the
/// border of a fill or lies inside one otherwise.
pub(crate) fn on_fills(
    path: &Path,
    tips: &[f64],
    fills: &[Fill],
    line: &str,
) -> (Option<Paint>, bool) {
    match fill_under(path, tips, fills) {
        Under::Crossing => (None, true),
        Under::None => (None, false),
        Under::Inside(at) => {
            let shape = resolve_style(&fills[at].decls).shape;
            if declares(line, "stroke") || !inks(&shape, "") {
                return (None, true);
            }
            (declarations(&fills[at].decls).style.fill, false)
        }
    }
}

/// A number as Pele's num writes it into a path and reads it back: rounded to two places, as
/// Pele's written does.
fn written(v: f64) -> f64 {
    let n = round(v * 100.0);
    if n == 0.0 { 0.0 } else { n / 100.0 }
}

/// The points a path passes through, as Pele's sample takes them from the numbers of its
/// path: each point a line runs to, and eight points along each curve, with NaN between the
/// figures. The path's numbers are taken as Pele writes them.
fn sample_path(p: &Path) -> Vec<f64> {
    let mut out = Vec::new();
    let (mut x, mut y) = (0.0, 0.0);
    for step in p.steps() {
        match step {
            Step::MoveTo(to) | Step::LineTo(to) => {
                if matches!(step, Step::MoveTo(_)) && !out.is_empty() {
                    out.extend([f64::NAN, f64::NAN]);
                }
                (x, y) = (written(to.x), written(to.y));
                out.extend([x, y]);
            }
            Step::QuadTo(c, to) | Step::CubeTo(c, _, to) => {
                let cubic = matches!(step, Step::CubeTo(..));
                let (x1, y1) = (written(c.x), written(c.y));
                let (x2, y2, x3, y3) = match step {
                    Step::CubeTo(_, c2, _) => {
                        (written(c2.x), written(c2.y), written(to.x), written(to.y))
                    }
                    _ => {
                        let (x2, y2) = (written(to.x), written(to.y));
                        (x2, y2, x2, y2)
                    }
                };
                let mut t = 0.125;
                while t <= 1.0 {
                    // De Casteljau's steps, in the order Pele takes them, which its rounding
                    // follows.
                    let mut ax = x + (x1 - x) * t;
                    let mut ay = y + (y1 - y) * t;
                    let mut bx = x1 + (x2 - x1) * t;
                    let mut by = y1 + (y2 - y1) * t;
                    if cubic {
                        let cx = x2 + (x3 - x2) * t;
                        let cy = y2 + (y3 - y2) * t;
                        ax += (bx - ax) * t;
                        ay += (by - ay) * t;
                        bx += (cx - bx) * t;
                        by += (cy - by) * t;
                    }
                    out.extend([ax + (bx - ax) * t, ay + (by - ay) * t]);
                    t += 0.125;
                }
                (x, y) = (x3, y3);
            }
            // A close adds no point, as Pele's Z, which has no numbers, adds none.
            Step::Close => {}
        }
    }
    out
}

/// Whether the segment from x, y along dx, dy passes into the box, by Liang and Barsky's
/// clipping, as Pele's clips says.
#[allow(clippy::too_many_arguments)]
fn clips(x: f64, y: f64, dx: f64, dy: f64, l: f64, t: f64, r: f64, b: f64) -> bool {
    let (mut lo, mut hi) = (0.0, 1.0);
    for (p, q) in [(-dx, x - l), (dx, r - x), (-dy, y - t), (dy, b - y)] {
        if p == 0.0 {
            if q < 0.0 {
                hi = -1.0;
            }
        } else if p < 0.0 {
            lo = crate::js::math::max(lo, q / p);
        } else {
            hi = crate::js::math::min(hi, q / p);
        }
    }
    lo < hi
}

/// Whether a line through points passes into the box, as Pele's enters says.
fn enters(points: &[f64], l: f64, t: f64, r: f64, b: f64) -> bool {
    let mut k = 0;
    while k + 2 < points.len() {
        let (x, y, x2, y2) = (points[k], points[k + 1], points[k + 2], points[k + 3]);
        k += 2;
        if x < l && x2 < l || x > r && x2 > r || y < t && y2 < t || y > b && y2 > b {
            continue;
        }
        if clips(x, y, x2 - x, y2 - y, l, t, r, b) {
            return true;
        }
    }
    false
}

/// Where an edge lies on fills, as Pele's fillUnder finds it.
enum Under {
    /// On none.
    None,
    /// Inside the fill at an index, the innermost that holds it.
    Inside(usize),
    /// Across the border of one, where Pele's gives the empty string.
    Crossing,
}

/// Of fills, the innermost that holds the whole of an edge's path and the tips of its
/// markers, given as x and y in turn, as Pele's fillUnder finds it. A fill the edge only
/// touches, within half a unit of its border, does not count.
fn fill_under(path: &Path, tips: &[f64], fills: &[Fill]) -> Under {
    if fills.is_empty() {
        return Under::None;
    }
    let points = sample_path(path);
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for &[x, y] in points
        .as_chunks::<2>()
        .0
        .iter()
        .chain(tips.as_chunks::<2>().0)
    {
        if x < min_x {
            min_x = x;
        }
        if x > max_x {
            max_x = x;
        }
        if y < min_y {
            min_y = y;
        }
        if y > max_y {
            max_y = y;
        }
    }
    let mut holder = Under::None;
    let mut least = f64::INFINITY;
    for (i, fill) in fills.iter().enumerate() {
        let [l, t, r, b] = fill.bounds;
        if max_x < l + 0.5 || min_x > r - 0.5 || max_y < t + 0.5 || min_y > b - 0.5 {
            continue;
        }
        let mut touched = enters(&points, l + 0.5, t + 0.5, r - 0.5, b - 0.5);
        for &[x, y] in tips.as_chunks::<2>().0 {
            if touched {
                break;
            }
            touched = x > l + 0.5 && x < r - 0.5 && y > t + 0.5 && y < b - 0.5;
        }
        if !touched {
            continue;
        }
        if min_x < l || max_x > r || min_y < t || max_y > b {
            return Under::Crossing;
        }
        let area = (r - l) * (b - t);
        if area < least {
            least = area;
            holder = Under::Inside(i);
        }
    }
    holder
}

/// How much wider than its edge a casing is, Pele's CASE.
const CASE: f64 = 4.0;

/// An item of a role that draws a casing under an edge that crosses the border of a fill, as
/// Pele's edgeCasing and casingsSvg draw it: its path and the outlines of its markers stroked
/// in the background, wider than the edge, with round ends and corners, so that the edge
/// stands out where it runs over the fill. Its width is the edge's, what the author's
/// declarations of the line, decls, give it or 2.5 where it is thick, and four more, and it
/// takes the author's opacities. It is none for an edge whose author strokes it in none or
/// transparent, written so, which needs none. A diagram puts its casings after its groups
/// and before its edges, as Pele does.
pub(crate) fn casing(
    role: &'static str,
    path: &Path,
    ends: &[Option<Marker>; 2],
    decls: &[String],
    thick: bool,
) -> Option<Item> {
    let line = resolve_style(decls).line;
    if matches!(declared(&line, "stroke"), Some("none" | "transparent")) {
        return None;
    }
    let author = declarations(decls).style;
    let width = if thick {
        2.5
    } else {
        author.stroke_width.unwrap_or(1.0)
    };
    let mut it = Item::new(Kind::Mark, role);
    it.parts
        .push(Part::path(path.clone(), Painting::StrokeOnly));
    it.style = Style {
        fill: Some(Paint::BG),
        stroke: Some(Paint::BG),
        stroke_width: Some(width + CASE),
        cap: Some(Cap::Round),
        join: Some(Join::Round),
        stroke_opacity: author.stroke_opacity,
        opacity: author.opacity,
        ..Style::default()
    };
    let mut bounds = path.bounds();
    for m in ends.iter().flatten() {
        for mut part in m.outline() {
            if part.paint != Painting::StrokeOnly {
                part.paint = Painting::FillAndStroke;
            }
            bounds = union(
                bounds,
                match &part.outline {
                    Outline::Path(p) => p.bounds(),
                    Outline::Box { rect, .. } => *rect,
                },
            );
            it.parts.push(part);
        }
    }
    it.bounds = bounds;
    Some(it)
}

/// The box about two boxes.
fn union(a: Rect, b: Rect) -> Rect {
    let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
    let (x1, y1) = ((a.x + a.w).max(b.x + b.w), (a.y + a.h).max(b.y + b.h));
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}
