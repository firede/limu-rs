//! Where a venn diagram's circles go and where what is written in its regions goes, as
//! Pele's layout.ts places them.

use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;

use super::db::{VennSubset, sort16};
use crate::js::math::{acos, atan2, cos, hypot, max, min, sin};
use crate::scene::Path;

const TAU: f64 = PI * 2.0;

/// A set's circle, Pele's Circle.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Circle {
    pub(crate) id: String,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) r: f64,
}

/// Where the content of a region goes, Pele's Spot: the point inside all of the circles it
/// is inside and outside every other that is farthest from their outlines, and how far that
/// is. A margin of 0 or less means the circles as placed leave the region empty.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Spot {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) margin: f64,
}

/// For every overlap of three or more sets, the pairwise overlaps it implies and the diagram
/// does not state, a quarter of the smaller set each, so that the circles are drawn
/// overlapping, after the subsets given, as Pele's ensurePairwiseSubsets adds them.
fn ensure_pairwise_subsets(subsets: &[VennSubset]) -> Vec<VennSubset> {
    let sorted = |sets: &[String]| {
        let mut s = sets.to_vec();
        sort16(&mut s);
        s
    };
    let mut present: HashSet<Vec<String>> = HashSet::new();
    let mut sizes: HashMap<&str, f64> = HashMap::new();
    for subset in subsets {
        present.insert(sorted(&subset.sets));
        if subset.sets.len() == 1 {
            sizes.insert(&subset.sets[0], subset.size);
        }
    }
    let mut out = subsets.to_vec();
    for subset in subsets {
        if subset.sets.len() < 3 {
            continue;
        }
        let members = sorted(&subset.sets);
        for i in 0..members.len() - 1 {
            for j in i + 1..members.len() {
                let pair = vec![members[i].clone(), members[j].clone()];
                if !present.insert(pair.clone()) {
                    continue;
                }
                let size = match (sizes.get(pair[0].as_str()), sizes.get(pair[1].as_str())) {
                    (Some(&a), Some(&b)) => min(a, b) / 4.0,
                    _ => 2.5,
                };
                out.push(VennSubset {
                    sets: pair,
                    size,
                    label: Some(String::new()),
                });
            }
        }
    }
    out
}

/// The area two circles share when their centers are d apart, as Pele's lensArea gives it.
fn lens_area(r1: f64, r2: f64, d: f64) -> f64 {
    if d >= r1 + r2 {
        return 0.0;
    }
    let small = min(r1, r2);
    if d <= (r1 - r2).abs() {
        return PI * small * small;
    }
    let a1 = r1 * r1 * acos((d * d + r1 * r1 - r2 * r2) / (2.0 * d * r1));
    let a2 = r2 * r2 * acos((d * d + r2 * r2 - r1 * r1) / (2.0 * d * r2));
    let a3 = 0.5
        * max(
            (-d + r1 + r2) * (d + r1 - r2) * (d - r1 + r2) * (d + r1 + r2),
            0.0,
        )
        .sqrt();
    a1 + a2 - a3
}

/// How far apart two circles sit for their overlap to have the given area, as Pele's
/// distanceForOverlap finds it. The area falls as the distance grows, so bisection finds it.
fn distance_for_overlap(r1: f64, r2: f64, area: f64) -> f64 {
    let mut low = (r1 - r2).abs();
    let mut high = r1 + r2;
    if !(area > 0.0) {
        return high;
    }
    // V8 works out Math.min(r1, r2) ** 2 as a product.
    let m = min(r1, r2);
    if area >= PI * (m * m) {
        return low;
    }
    for _ in 0..48 {
        let middle = (low + high) / 2.0;
        if lens_area(r1, r2, middle) > area {
            low = middle;
        } else {
            high = middle;
        }
    }
    (low + high) / 2.0
}

/// How far a point is from the nearest outline it must stay within or without, as Pele's
/// margin finds it. It gives up, with some value below floor, as soon as the point cannot
/// reach floor.
fn margin(
    circles: &[Circle],
    inside: &[usize],
    outside: &[usize],
    x: f64,
    y: f64,
    floor: f64,
) -> f64 {
    let mut best = f64::INFINITY;
    for &i in inside {
        let c = &circles[i];
        let (dx, dy) = (x - c.x, y - c.y);
        let room = c.r - (dx * dx + dy * dy).sqrt();
        if room < best {
            if room < floor {
                return room;
            }
            best = room;
        }
    }
    for &i in outside {
        let c = &circles[i];
        let (dx, dy) = (x - c.x, y - c.y);
        let room = (dx * dx + dy * dy).sqrt() - c.r;
        if room < best {
            if room < floor {
                return room;
            }
            best = room;
        }
    }
    best
}

/// Finds the roomiest point of a region by sampling the smallest of its circles on a grid
/// and then walking uphill from the best sample, as Pele's findSpot does. Ties go to the
/// point nearest the middle of the region, so a symmetric arrangement gets symmetric labels.
pub(crate) fn find_spot(circles: &[Circle], inside: &[usize], outside: &[usize]) -> Spot {
    let (mut cx, mut cy) = (0.0, 0.0);
    let mut smallest = &circles[inside[0]];
    for &i in inside {
        let c = &circles[i];
        cx += c.x / inside.len() as f64;
        cy += c.y / inside.len() as f64;
        if c.r < smallest.r {
            smallest = c;
        }
    }
    let r = smallest.r;
    let (mut x, mut y) = (cx, cy);
    let mut best = margin(circles, inside, outside, x, y, f64::NEG_INFINITY);
    let mut off = 0.0;
    let mut consider = |px: f64, py: f64, x: &mut f64, y: &mut f64| {
        let m = margin(circles, inside, outside, px, py, best - r * 1e-9);
        if !(m > best - r * 1e-9) {
            return false;
        }
        let distance = hypot(px - cx, py - cy);
        if m > best + r * 1e-9 || distance < off - r * 1e-9 {
            best = m;
            off = distance;
            *x = px;
            *y = py;
            return true;
        }
        false
    };
    const GRID: i32 = 10;
    let grid = f64::from(GRID);
    for gy in -GRID..=GRID {
        for gx in -GRID..=GRID {
            if gx * gx + gy * gy <= GRID * GRID {
                let px = smallest.x + (f64::from(gx) * r) / grid;
                let py = smallest.y + (f64::from(gy) * r) / grid;
                consider(px, py, &mut x, &mut y);
            }
        }
    }
    let mut step = r / grid;
    let mut round = 0;
    while round < 40 && step > r * 1e-6 {
        let mut moved = false;
        for k in 0..8 {
            let angle = (f64::from(k) * TAU) / 8.0;
            let (px, py) = (x + cos(angle) * step, y + sin(angle) * step);
            if consider(px, py, &mut x, &mut y) {
                moved = true;
            }
        }
        if !moved {
            step /= 2.0;
        }
        round += 1;
    }
    Spot { x, y, margin: best }
}

/// Places one circle per set, as Pele's layoutCircles does, with areas in proportion to the
/// sizes of the sets and each pair as far apart as its overlap asks: one, two and three sets
/// exactly, more by relaxing a ring of circles toward those distances. Lengths are in units
/// of the square root of a size.
pub(crate) fn layout_circles(subsets: &[VennSubset]) -> Vec<Circle> {
    // The sizes of the sets by their ids, in the order the ids first came.
    let mut ids: Vec<&str> = Vec::new();
    let mut sizes: HashMap<&str, f64> = HashMap::new();
    for subset in subsets {
        for id in &subset.sets {
            if !sizes.contains_key(id.as_str()) {
                sizes.insert(id, 10.0);
                ids.push(id);
            }
        }
    }
    for subset in subsets {
        if subset.sets.len() == 1 {
            sizes.insert(&subset.sets[0], subset.size);
        }
    }

    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut circles: Vec<Circle> = Vec::new();
    let mut largest = 0.0;
    for id in &ids {
        let size = sizes[id];
        if size.is_finite() && size > largest {
            largest = size;
        }
    }
    if largest == 0.0 {
        largest = 10.0;
    }
    let widest = (largest / PI).sqrt();
    for &id in &ids {
        let size = sizes[id];
        index.insert(id, circles.len());
        // A set too small to hold its name, or with no size at all, still gets a circle that
        // shows.
        let r = if size.is_finite() && size > 0.0 {
            (size / PI).sqrt()
        } else {
            0.0
        };
        circles.push(Circle {
            id: id.to_string(),
            x: 0.0,
            y: 0.0,
            r: max(r, widest * 0.3),
        });
    }
    let n = circles.len();
    if n < 2 {
        return circles;
    }

    // The overlap each pair was given, by the indexes of its circles.
    let mut overlaps: HashMap<usize, f64> = HashMap::new();
    for subset in ensure_pairwise_subsets(subsets) {
        if subset.sets.len() != 2 {
            continue;
        }
        let a = index[subset.sets[0].as_str()];
        let b = index[subset.sets[1].as_str()];
        if a != b {
            overlaps.insert(a.min(b) * n + a.max(b), subset.size);
        }
    }
    // How far apart each overlapping pair should be. Pairs that do not overlap only have to
    // keep a gap.
    let wanted: HashMap<usize, f64> = overlaps
        .iter()
        .map(|(&key, &area)| {
            let d = distance_for_overlap(circles[key / n].r, circles[key % n].r, area);
            (key, d)
        })
        .collect();
    let gap = widest * 0.15;
    let distance = |circles: &[Circle], a: usize, b: usize| {
        wanted
            .get(&(a * n + b))
            .copied()
            .unwrap_or(circles[a].r + circles[b].r + gap)
    };

    if n == 2 {
        circles[1].x = distance(&circles, 0, 1);
        return circles;
    }

    if n == 3 {
        // Two sets side by side and the third below, the corners of a triangle whose sides
        // are the three distances. An overlap of all three has to exist, so the triangle
        // shrinks until it does.
        let mut ab = distance(&circles, 0, 1);
        let mut ac = distance(&circles, 0, 2);
        let mut bc = distance(&circles, 1, 2);
        let triple = subsets.iter().any(|s| distinct(&s.sets) == 3);
        for _ in 0..40 {
            let x = if ab > 0.0 {
                (ab * ab + ac * ac - bc * bc) / (2.0 * ab)
            } else {
                0.0
            };
            circles[1].x = ab;
            circles[2].x = max(-ac, min(ac, x));
            circles[2].y = max(ac * ac - x * x, 0.0).sqrt();
            if !triple || find_spot(&circles, &[0, 1, 2], &[]).margin > widest * 0.12 {
                break;
            }
            ab *= 0.94;
            ac *= 0.94;
            bc *= 0.94;
        }
        return circles;
    }

    let mut total = 0.0;
    for c in &circles {
        total += 2.0 * c.r + gap;
    }
    let rounds = 300.min(3_000_000 / (n * n));
    if n <= 64 {
        let ring = total / TAU;
        for (i, c) in circles.iter_mut().enumerate() {
            let angle = (i as f64 * TAU) / n as f64 - PI / 2.0;
            c.x = cos(angle) * ring;
            c.y = sin(angle) * ring;
        }
    } else {
        let columns = (n as f64).sqrt().ceil() as usize;
        let cell = 2.0 * widest + gap;
        for (i, c) in circles.iter_mut().enumerate() {
            c.x = (i % columns) as f64 * cell;
            c.y = (i / columns) as f64 * cell;
        }
    }
    let mut dx = vec![0.0; n];
    let mut dy = vec![0.0; n];
    let mut pulls = vec![0u32; n];
    for _ in 0..rounds {
        dx.fill(0.0);
        dy.fill(0.0);
        pulls.fill(0);
        for a in 0..n {
            for b in a + 1..n {
                let overlap = wanted.get(&(a * n + b)).copied();
                let apart = circles[a].r + circles[b].r + gap;
                let mut ux = circles[b].x - circles[a].x;
                let mut uy = circles[b].y - circles[a].y;
                let mut length = hypot(ux, uy);
                if overlap.is_none() && length >= apart {
                    continue;
                }
                if length < 1e-9 {
                    ux = cos((a + b) as f64);
                    uy = sin((a + b) as f64);
                    length = 1.0;
                }
                let step = (length - overlap.unwrap_or(apart)) / 2.0 / length;
                dx[a] += ux * step;
                dy[a] += uy * step;
                dx[b] -= ux * step;
                dy[b] -= uy * step;
                pulls[a] += 1;
                pulls[b] += 1;
            }
        }
        for i in 0..n {
            if pulls[i] == 0 {
                continue;
            }
            circles[i].x += dx[i] / f64::from(pulls[i]);
            circles[i].y += dy[i] / f64::from(pulls[i]);
        }
    }
    circles
}

/// The number of different ids in a list, the size of Pele's new Set of them.
fn distinct(ids: &[String]) -> usize {
    ids.iter().collect::<HashSet<_>>().len()
}

/// A corner of a region: where the outlines of circles a and b cross.
#[derive(Clone, Copy)]
struct Corner {
    x: f64,
    y: f64,
    a: usize,
    b: usize,
}

/// The outline of the region all the given circles share, as Pele's regionPath writes it,
/// or none if they share none. Its sides are arcs, drawn with arc_to as the A commands Pele
/// writes draw them.
pub(crate) fn region_path(circles: &[Circle]) -> Option<Path> {
    let whole = |c: &Circle| {
        let mut p = Path::new();
        p.move_to(c.x - c.r, c.y)
            .arc_to(c.r, c.r, 0.0, true, true, c.x + c.r, c.y)
            .arc_to(c.r, c.r, 0.0, true, true, c.x - c.r, c.y)
            .close();
        p
    };
    if circles.len() == 1 {
        return Some(whole(&circles[0]));
    }
    let within = |x: f64, y: f64, skip: usize, other: usize| {
        circles.iter().enumerate().all(|(k, c)| {
            k == skip || k == other || !(hypot(x - c.x, y - c.y) > c.r * (1.0 + 1e-9))
        })
    };
    // The corners of the region: where two outlines cross inside all the other circles.
    let mut corners: Vec<Corner> = Vec::new();
    for i in 0..circles.len() {
        for j in i + 1..circles.len() {
            let (a, b) = (&circles[i], &circles[j]);
            let d = hypot(b.x - a.x, b.y - a.y);
            if d >= a.r + b.r || d <= (a.r - b.r).abs() || d == 0.0 {
                continue;
            }
            let along = (a.r * a.r - b.r * b.r + d * d) / (2.0 * d);
            let up = max(a.r * a.r - along * along, 0.0).sqrt();
            let mx = a.x + ((b.x - a.x) * along) / d;
            let my = a.y + ((b.y - a.y) * along) / d;
            for sign in [1.0, -1.0] {
                let x = mx + (sign * up * (b.y - a.y)) / d;
                let y = my - (sign * up * (b.x - a.x)) / d;
                if within(x, y, i, j) {
                    corners.push(Corner { x, y, a: i, b: j });
                }
            }
        }
    }
    if corners.len() < 2 {
        // No corners: either one circle lies inside all the others, or there is nothing
        // shared.
        let mut smallest = 0;
        for (k, c) in circles.iter().enumerate() {
            if c.r < circles[smallest].r {
                smallest = k;
            }
        }
        let s = &circles[smallest];
        for (k, c) in circles.iter().enumerate() {
            if k != smallest && hypot(c.x - s.x, c.y - s.y) + s.r > c.r * (1.0 + 1e-9) {
                return None;
            }
        }
        return Some(whole(s));
    }
    let (mut cx, mut cy) = (0.0, 0.0);
    for p in &corners {
        cx += p.x / corners.len() as f64;
        cy += p.y / corners.len() as f64;
    }
    // Pele sorts by the difference of the angles, which orders them as the angles do.
    corners.sort_by(|p, q| {
        let d = atan2(p.y - cy, p.x - cx) - atan2(q.y - cy, q.x - cx);
        d.partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut path = Path::new();
    for i in 0..corners.len() {
        let from = corners[i];
        let to = corners[(i + 1) % corners.len()];
        if i == 0 {
            path.move_to(from.x, from.y);
        }
        // The side between two corners is an arc of a circle that passes through both. Of
        // those, it is the one whose arc stays inside the others.
        let mut arc = None;
        let mut large = false;
        let mut narrowest = f64::INFINITY;
        for k in [from.a, from.b] {
            if k != to.a && k != to.b {
                continue;
            }
            let c = &circles[k];
            let start = atan2(from.y - c.y, from.x - c.x);
            let mut sweep = atan2(to.y - c.y, to.x - c.x) - start;
            if sweep <= 0.0 {
                sweep += TAU;
            }
            let mx = c.x + c.r * cos(start + sweep / 2.0);
            let my = c.y + c.r * sin(start + sweep / 2.0);
            let width = hypot(mx - (from.x + to.x) / 2.0, my - (from.y + to.y) / 2.0);
            if width < narrowest {
                narrowest = width;
                arc = Some(k);
                large = sweep > PI;
            }
        }
        match arc {
            None => {
                path.line_to(to.x, to.y);
            }
            Some(k) => {
                let r = circles[k].r;
                path.arc_to(r, r, 0.0, large, true, to.x, to.y);
            }
        }
    }
    path.close();
    Some(path)
}
