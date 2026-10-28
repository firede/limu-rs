//! The shapes checked against what Pele makes of the same cases, in the oracle's draw
//! fixture.

use limu_fixtures::{Num, load};
use serde::Deserialize;

use super::*;
use crate::js::math;
use crate::scene::{Marker, MarkerType};
use crate::scene::{Outline, Painting, Part, Point, Rect, Step};
use crate::testing::{Failures, pele_d, pele_num};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Draw {
    names: Vec<String>,
    size: Vec<SizeCase>,
    inset: Vec<InsetCase>,
    span: Vec<SpanCase>,
    has_label: Vec<HasLabelCase>,
    ends: Vec<EndCase>,
    routes: Vec<RouteCase>,
    loops: Vec<LoopCase>,
    reach: Vec<ReachCase>,
    titles: Vec<TitleCase>,
    outlines: Vec<OutlineCase>,
    markers: Vec<MarkerCase>,
    patches: Vec<PatchCase>,
    struck: Vec<StruckCase>,
}

#[derive(Deserialize)]
struct SizeCase {
    shape: String,
    tw: f64,
    th: f64,
    w: Num,
    h: Num,
    dy: Num,
}

#[derive(Deserialize)]
struct InsetCase {
    shape: String,
    w: f64,
    h: f64,
    side: usize,
    t: f64,
    v: Num,
}

#[derive(Deserialize)]
struct SpanCase {
    shape: String,
    w: f64,
    h: f64,
    sideways: bool,
    v: Num,
}

#[derive(Deserialize)]
struct HasLabelCase {
    shape: String,
    v: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PeleNode {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    is_group: bool,
}

#[derive(Deserialize)]
struct EndCase {
    route: Vec<f64>,
    at: usize,
    node: PeleNode,
    shape: String,
    w: f64,
    h: f64,
    side: i32,
    out: Vec<Num>,
}

/// Pele's EdgePath, with its path as it writes it.
#[derive(Deserialize, Debug)]
struct PeleTips {
    d: String,
    sx: Num,
    sy: Num,
    sdx: Num,
    sdy: Num,
    ex: Num,
    ey: Num,
    edx: Num,
    edy: Num,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RouteCase {
    #[serde(flatten)]
    tips: PeleTips,
    route: Vec<f64>,
    curve: Option<String>,
    start_trim: f64,
    end_trim: f64,
    start_reach: Option<f64>,
    end_reach: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoopCase {
    #[serde(flatten)]
    tips: PeleTips,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    k: usize,
    sideways: bool,
    base: f64,
    trim: f64,
    label_w: f64,
    label_h: f64,
    label_x: Num,
    label_y: Num,
}

#[derive(Deserialize)]
struct ReachCase {
    adds: Vec<(usize, i32, String)>,
    of: Vec<(usize, i32, f64)>,
}

#[derive(Deserialize)]
struct TitleCase {
    node: PeleBox,
    routes: Vec<Vec<f64>>,
    marked: Vec<(f64, Vec<f64>)>,
    tried: Vec<Tried>,
}

#[derive(Deserialize)]
struct PeleBox {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Deserialize)]
struct Tried {
    width: f64,
    pad: f64,
    x: Num,
    probes: Vec<(f64, bool)>,
}

#[derive(Deserialize)]
struct StruckCase {
    parents: Vec<i32>,
    index: usize,
    tint: String,
}

#[derive(Deserialize)]
struct MarkerCase {
    #[serde(rename = "type")]
    kind: String,
    x: f64,
    y: f64,
    dx: f64,
    dy: f64,
    svg: String,
}

#[derive(Deserialize)]
struct PatchCase {
    width: f64,
    height: f64,
    x: f64,
    y: f64,
    svg: String,
}

#[derive(Deserialize)]
struct OutlineCase {
    shape: String,
    w: f64,
    h: f64,
    x: f64,
    y: f64,
    svg: String,
}

fn draw() -> Draw {
    load("draw")
}

#[test]
fn names_are_peles() {
    let names: Vec<&str> = shape_names().collect();
    assert_eq!(names, draw().names);
}

#[test]
fn shapes_match_pele() {
    let p = draw();
    let mut f = Failures::new();
    for c in &p.size {
        let got = shape_size(&c.shape, c.tw, c.th);
        f.check(
            Num::same(got.w, c.w.0) && Num::same(got.h, c.h.0) && Num::same(got.dy, c.dy.0),
            || {
                format!(
                    "shape_size({:?}, {}, {}) = {got:?}, Pele {:?} {:?} {:?}",
                    c.shape, c.tw, c.th, c.w, c.h, c.dy
                )
            },
        );
    }
    for c in &p.inset {
        let got = shape_inset(&c.shape, c.w, c.h, c.side, c.t);
        f.check(Num::same(got, c.v.0), || {
            format!(
                "shape_inset({:?}, {}, {}, {}, {}) = {got}, Pele {:?}",
                c.shape, c.w, c.h, c.side, c.t, c.v
            )
        });
    }
    for c in &p.span {
        let got = shape_span(&c.shape, c.w, c.h, c.sideways);
        f.check(Num::same(got, c.v.0), || {
            format!(
                "shape_span({:?}, {}, {}, {}) = {got}, Pele {:?}",
                c.shape, c.w, c.h, c.sideways, c.v
            )
        });
    }
    for c in &p.has_label {
        let got = shape_has_label(&c.shape);
        f.check(got == c.v, || {
            format!("shape_has_label({:?}) = {got}, Pele {}", c.shape, c.v)
        });
    }
    f.done("draw shapes", 4000);
}

#[test]
fn ends_match_pele() {
    let mut f = Failures::new();
    for c in draw().ends {
        let node = Node {
            x: c.node.x,
            y: c.node.y,
            w: c.node.w,
            h: c.node.h,
            group: c.node.is_group,
        };
        let side = end_side(&c.route, c.at, &node).map_or(-1, |s| s as i32);
        let mut route = c.route.clone();
        inset_route(&mut route, c.at, &c.shape, c.w, c.h, &node);
        let same =
            route.len() == c.out.len() && route.iter().zip(&c.out).all(|(&a, b)| Num::same(a, b.0));
        f.check(side == c.side && same, || {
            format!(
                "ends of {:?} at {} on {:?}: side {side}, {route:?}, Pele {} {:?}",
                c.route, c.at, c.shape, c.side, c.out
            )
        });
    }
    f.done("draw ends", 500);
}

/// Whether an edge's path, written as Pele writes it, and its tips are Pele's, to the bit.
fn same_tips(e: &EdgePath, want: &PeleTips) -> bool {
    let tips = [
        e.start.at.x,
        e.start.at.y,
        e.start.dx,
        e.start.dy,
        e.end.at.x,
        e.end.at.y,
        e.end.dx,
        e.end.dy,
    ];
    let pele = [
        want.sx, want.sy, want.sdx, want.sdy, want.ex, want.ey, want.edx, want.edy,
    ];
    // The fixture writes the -0 of a tip that points straight across or down as 0, as
    // JSON.stringify does, so the tips are compared as numbers.
    pele_d(&e.path) == want.d
        && tips
            .iter()
            .zip(pele)
            .all(|(&a, b)| a == b.0 || a.is_nan() && b.0.is_nan())
}

#[test]
fn route_paths_match_pele() {
    let mut f = Failures::new();
    for c in draw().routes {
        let curve = Curve::of(c.curve.as_deref().unwrap_or(""));
        let start_reach = c.start_reach.unwrap_or_else(|| own_reach(c.start_trim));
        let end_reach = c.end_reach.unwrap_or_else(|| own_reach(c.end_trim));
        let e = route_path(
            &c.route,
            curve,
            c.start_trim,
            c.end_trim,
            start_reach,
            end_reach,
        );
        f.check(same_tips(&e, &c.tips), || {
            format!(
                "route_path({:?}, {:?}, {}, {}, {start_reach}, {end_reach})\n got {} {:?} {:?}\nPele {:?}",
                c.route,
                c.curve,
                c.start_trim,
                c.end_trim,
                pele_d(&e.path),
                e.start,
                e.end,
                c.tips
            )
        });
    }
    f.done("draw routes", 2000);
}

#[test]
fn loop_paths_match_pele() {
    let mut f = Failures::new();
    for c in draw().loops {
        let (e, label) = loop_path(
            Point::new(c.x, c.y),
            c.w,
            c.h,
            c.k,
            c.sideways,
            c.base,
            c.trim,
            c.label_w,
            c.label_h,
        );
        let ok = same_tips(&e, &c.tips)
            && Num::same(label.x, c.label_x.0)
            && Num::same(label.y, c.label_y.0);
        f.check(ok, || {
            format!(
                "loop_path({}, {}, {}, {}, {}, {}, {}, {}): {} {:?} {:?} label {label:?}, Pele {:?} {:?} {:?}",
                c.x,
                c.y,
                c.w,
                c.h,
                c.k,
                c.sideways,
                c.base,
                c.trim,
                pele_d(&e.path),
                e.start,
                e.end,
                c.tips,
                c.label_x,
                c.label_y
            )
        });
    }
    f.done("draw loops", 200);
}

#[test]
fn side_reach_matches_pele() {
    let mut f = Failures::new();
    for c in draw().reach {
        let mut r = SideReach::default();
        for (node, side, kind) in &c.adds {
            r.add(*node, *side, MarkerType::from_name(kind));
        }
        for &(node, side, want) in &c.of {
            let got = r.of(node, side);
            f.check(Num::same(got, want), || {
                format!("of({node}, {side}) = {got}, Pele {want}")
            });
        }
    }
    f.done("draw reach", 500);
}

#[test]
fn markers_match_pele() {
    let mut f = Failures::new();
    for c in draw().markers {
        let tip = Tip {
            at: Point::new(c.x, c.y),
            dx: c.dx,
            dy: c.dy,
        };
        let parts = tip
            .marker(MarkerType::from_name(&c.kind))
            .map_or_else(Vec::new, |m: Marker| m.outline());
        let what = format!("{} at {}, {} along {}, {}", c.kind, c.x, c.y, c.dx, c.dy);
        if c.svg.is_empty() {
            f.check(parts.is_empty(), || {
                format!("{what}: {parts:?}, Pele draws none")
            });
            continue;
        }
        let [part] = parts.as_slice() else {
            f.check(false, || {
                format!("{what}: {} parts, Pele {}", parts.len(), c.svg)
            });
            continue;
        };
        let Outline::Path(path) = &part.outline else {
            f.check(false, || format!("{what}: {part:?}, Pele {}", c.svg));
            continue;
        };
        let a = attrs(&c.svg);
        let get = |k: &str| a.iter().find(|(n, _)| *n == k).map(|(_, v)| *v);
        let filled = part.paint == Painting::FillOnly;
        f.check(filled == (get("fill") == Some("C")), || {
            format!("{what}: {:?}, Pele {}", part.paint, c.svg)
        });
        if c.svg.starts_with("<circle") {
            let b = path.bounds();
            let n = |k: &str| numbers(get(k).unwrap())[0];
            let ok = (b.x + b.w / 2.0 - n("cx")).abs() <= 0.0051
                && (b.y + b.h / 2.0 - n("cy")).abs() <= 0.0051
                && (b.w / 2.0 - n("r")).abs() <= 1e-9;
            f.check(ok, || format!("{what}: {b:?}, Pele {}", c.svg));
            continue;
        }
        let got = pele_d(path);
        f.check(Some(got.as_str()) == get("d"), || {
            format!("{what}: {got}, Pele {}", c.svg)
        });
    }
    f.done("draw markers", 200);
}

#[test]
fn label_patches_match_pele() {
    let mut f = Failures::new();
    for c in draw().patches {
        let b = label_patch_box(c.width, c.height, c.x, c.y);
        let rect = &c.svg[c.svg.find("<rect ").unwrap() + "<rect ".len()..];
        let a = attrs(&rect[..rect.find('>').unwrap()]);
        let want: Vec<&str> = ["x", "y", "width", "height", "rx"]
            .iter()
            .map(|k| a.iter().find(|(n, _)| n == k).unwrap().1)
            .collect();
        let got = [
            pele_num(b.x),
            pele_num(b.y),
            pele_num(b.w),
            pele_num(b.h),
            "3".to_string(),
        ];
        f.check(got.iter().zip(&want).all(|(g, w)| g == w), || {
            format!(
                "label_patch_box({}, {}, {}, {}) = {got:?}, Pele {want:?}",
                c.width, c.height, c.x, c.y
            )
        });
        let Outline::Path(p) = label_patch(c.width, c.height, c.x, c.y).outline else {
            unreachable!()
        };
        let pb = p.bounds();
        let ok =
            (pb.x - b.x).abs() <= 1e-9 && (pb.w - b.w).abs() <= 1e-9 && (pb.h - b.h).abs() <= 1e-9;
        f.check(ok, || format!("patch path {pb:?}, box {b:?}"));
    }
    f.done("draw patches", 50);
}

#[test]
fn titles_match_pele() {
    let p = draw();
    let mut f = Failures::new();
    for c in &p.titles {
        let n = Node {
            x: c.node.x,
            y: c.node.y,
            w: c.node.w,
            h: c.node.h,
            group: false,
        };
        let mut cr = Crossings::default();
        for r in &c.routes {
            mark_crossings(&mut cr, r);
        }
        f.check(cr.len() == c.marked.len(), || {
            format!("{} crossings, Pele {}", cr.len(), c.marked.len())
        });
        for (key, xs) in &c.marked {
            let got = cr.at(*key);
            f.check(got == Some(xs.as_slice()), || {
                format!("crossings at {key}: {got:?}, Pele {xs:?}")
            });
        }
        for t in &c.tried {
            let got = cluster_title_x(&n, t.width, &cr, t.pad);
            f.check(Num::same(got, t.x.0), || {
                format!(
                    "cluster_title_x({n:?}, {}, {}) = {got}, Pele {:?}",
                    t.width, t.pad, t.x
                )
            });
            for &(x, want) in &t.probes {
                let got = title_crossed(&n, t.width, x, &cr);
                f.check(got == want, || {
                    format!("title_crossed({n:?}, {}, {x}) = {got}", t.width)
                });
            }
        }
    }
    for c in &p.struck {
        let parent = |i: usize| usize::try_from(c.parents[i]).ok();
        let got = pele_num(struck_tint(struck_depth(parent, c.index)));
        f.check(got == c.tint, || {
            format!(
                "struck tint of {:?} at {} = {got}, Pele {}",
                c.parents, c.index, c.tint
            )
        });
    }
    f.done("draw titles", 500);
}

// The outlines are compared number for number where Pele draws with lines and curves: a
// polygon, a rect with square corners and a path with no arc in it must be the steps of the
// part's path, and a rect with the theme's corners its box. The fixture has Pele's numbers
// in full, and a path's are taken to hundredths, as Pele writes them, since the parts take
// them as written. A circle, an ellipse, a rect with corners of its own and a path with an
// arc, which a Path draws in cubic curves that only come near, are compared as figures.
// Pele's markup and the parts are each flattened to lines through points no more than about
// a unit apart, and each part must have the paint of its element, the same bounding box, and
// every point near the other's lines.

type Lines = Vec<Vec<Point>>;

/// An element of Pele's markup.
struct Figure {
    paint: Painting,
    rounded: bool,
    /// The box of a rect.
    rect: Option<Rect>,
    lines: Lines,
    /// The element as the steps of a path, for one drawn with lines and curves only, which
    /// a part is to have exactly.
    steps: Option<Vec<Step>>,
}

/// How far apart the points of a flattened curve are, at most.
fn segments(n: f64) -> usize {
    64.max(n.ceil() as usize)
}

fn dist(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

fn quad(out: &mut Vec<Point>, a: Point, c: Point, b: Point) {
    let n = segments(dist(a, c) + dist(c, b));
    for i in 1..=n {
        let s = i as f64 / n as f64;
        let u = 1.0 - s;
        out.push(Point::new(
            u * u * a.x + 2.0 * u * s * c.x + s * s * b.x,
            u * u * a.y + 2.0 * u * s * c.y + s * s * b.y,
        ));
    }
}

fn cube(out: &mut Vec<Point>, a: Point, c1: Point, c2: Point, b: Point) {
    let n = segments(dist(a, c1) + dist(c1, c2) + dist(c2, b));
    for i in 1..=n {
        let s = i as f64 / n as f64;
        let u = 1.0 - s;
        out.push(Point::new(
            u * u * u * a.x + 3.0 * u * u * s * c1.x + 3.0 * u * s * s * c2.x + s * s * s * b.x,
            u * u * u * a.y + 3.0 * u * u * s * c1.y + 3.0 * u * s * s * c2.y + s * s * s * b.y,
        ));
    }
}

/// An SVG arc sampled on the ellipse itself, by the conversion of SVG's implementation notes,
/// F.6.5 and F.6.6, with no rotation.
#[allow(clippy::too_many_arguments)]
fn arc(out: &mut Vec<Point>, a: Point, rx: f64, ry: f64, large: bool, sweep: bool, b: Point) {
    if a == b {
        return;
    }
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 {
        out.push(b);
        return;
    }
    let (x1p, y1p) = ((a.x - b.x) / 2.0, (a.y - b.y) / 2.0);
    let l = x1p * x1p / (rx * rx) + y1p * y1p / (ry * ry);
    if l > 1.0 {
        (rx, ry) = (rx * l.sqrt(), ry * l.sqrt());
    }
    let sq = (rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p)
        / (rx * rx * y1p * y1p + ry * ry * x1p * x1p);
    let mut co = sq.max(0.0).sqrt();
    if large == sweep {
        co = -co;
    }
    let (cxp, cyp) = (co * rx * y1p / ry, -co * ry * x1p / rx);
    let (cx, cy) = (cxp + (a.x + b.x) / 2.0, cyp + (a.y + b.y) / 2.0);
    let t1 = ((y1p - cyp) / ry).atan2((x1p - cxp) / rx);
    let t2 = ((-y1p - cyp) / ry).atan2((-x1p - cxp) / rx);
    let mut dt = t2 - t1;
    if sweep && dt < 0.0 {
        dt += 2.0 * std::f64::consts::PI;
    } else if !sweep && dt > 0.0 {
        dt -= 2.0 * std::f64::consts::PI;
    }
    let n = segments(dt.abs() * rx.max(ry));
    for i in 1..=n {
        let t = t1 + dt * i as f64 / n as f64;
        out.push(Point::new(cx + rx * t.cos(), cy + ry * t.sin()));
    }
}

fn ellipse(cx: f64, cy: f64, rx: f64, ry: f64) -> Vec<Point> {
    let n = segments(2.0 * std::f64::consts::PI * rx.max(ry));
    (0..=n)
        .map(|i| {
            let t = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
            Point::new(cx + rx * t.cos(), cy + ry * t.sin())
        })
        .collect()
}

/// A rect as SVG draws it, with rx and ry each made no more than half the side.
fn svg_rect(x: f64, y: f64, w: f64, h: f64, rx: f64) -> Vec<Point> {
    let (rx, ry) = (rx.min(w / 2.0), rx.min(h / 2.0));
    let p = Point::new;
    if rx <= 0.0 {
        return vec![p(x, y), p(x + w, y), p(x + w, y + h), p(x, y + h), p(x, y)];
    }
    let mut out = vec![p(x + rx, y), p(x + w - rx, y)];
    arc(
        &mut out,
        p(x + w - rx, y),
        rx,
        ry,
        false,
        true,
        p(x + w, y + ry),
    );
    out.push(p(x + w, y + h - ry));
    arc(
        &mut out,
        p(x + w, y + h - ry),
        rx,
        ry,
        false,
        true,
        p(x + w - rx, y + h),
    );
    out.push(p(x + rx, y + h));
    arc(
        &mut out,
        p(x + rx, y + h),
        rx,
        ry,
        false,
        true,
        p(x, y + h - ry),
    );
    out.push(p(x, y + ry));
    arc(&mut out, p(x, y + ry), rx, ry, false, true, p(x + rx, y));
    out
}

/// The numbers in s, as SVG separates them.
fn numbers(s: &str) -> Vec<f64> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if !(c.is_ascii_digit() || c == b'-' || c == b'.') {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        let mut dot = c == b'.';
        while i < b.len() {
            let c = b[i];
            if c.is_ascii_digit() || c == b'.' && !dot {
                dot |= c == b'.';
            } else if (c == b'e' || c == b'E') && i + 1 < b.len() {
                i += 1;
                if b[i] == b'-' || b[i] == b'+' {
                    i += 1;
                }
                continue;
            } else {
                break;
            }
            i += 1;
        }
        out.push(
            s[start..i]
                .parse()
                .unwrap_or_else(|_| panic!("{:?} in {s:?}", &s[start..i])),
        );
    }
    out
}

/// Each of n to hundredths, as Pele's num writes it and SVG reads it back.
fn hundredths(n: Vec<f64>) -> Vec<f64> {
    n.into_iter()
        .map(|v| math::round(v * 100.0) / 100.0)
        .collect()
}

/// The commands of a path's d, each with its numbers to hundredths.
fn commands(d: &str) -> Vec<(u8, Vec<f64>)> {
    let starts: Vec<usize> = d
        .bytes()
        .enumerate()
        .filter(|(_, c)| b"MLHVCQTAZmlhvcqtaz".contains(c))
        .map(|(i, _)| i)
        .collect();
    starts
        .iter()
        .enumerate()
        .map(|(k, &i)| {
            let end = starts.get(k + 1).copied().unwrap_or(d.len());
            (d.as_bytes()[i], hundredths(numbers(&d[i + 1..end])))
        })
        .collect()
}

/// The d of a path about o, flattened, and as the steps of a path where it has no arc: its
/// numbers to hundredths, relative ones taken from where the path is and a T's control point
/// the last one's reflected, in the shape's frame, as SVG reads them, and then moved by o.
fn svg_path(d: &str, o: Point) -> (Lines, Option<Vec<Step>>) {
    let mut lines = Vec::new();
    let mut line: Vec<Point> = Vec::new();
    let mut steps = Some(Vec::new());
    let at_o = |p: Point| Point::new(o.x + p.x, o.y + p.y);
    let (mut at, mut start, mut ctrl) = (Point::default(), Point::default(), Point::default());
    let mut quad_at = false;
    for (op, n) in commands(d) {
        let r = if op >= b'a' { at } else { Point::default() };
        let want = match op | 0x20 {
            b'm' | b'l' | b't' => 2,
            b'h' | b'v' => 1,
            b'q' => 4,
            b'c' => 6,
            b'a' => 7,
            _ => 0,
        };
        assert_eq!(n.len(), want, "{} in {d:?}", op as char);
        let p = |i: usize| Point::new(r.x + n[i], r.y + n[i + 1]);
        let was_quad = quad_at;
        quad_at = false;
        let step = match op | 0x20 {
            b'm' => {
                if line.len() > 1 {
                    lines.push(std::mem::take(&mut line));
                }
                at = p(0);
                start = at;
                line = vec![at];
                Some(Step::MoveTo(at_o(at)))
            }
            b'l' | b'h' | b'v' => {
                at = match op | 0x20 {
                    b'l' => p(0),
                    b'h' => Point::new(r.x + n[0], at.y),
                    _ => Point::new(at.x, r.y + n[0]),
                };
                line.push(at);
                Some(Step::LineTo(at_o(at)))
            }
            b'q' | b't' => {
                ctrl = if op | 0x20 == b'q' {
                    p(0)
                } else if was_quad {
                    Point::new(2.0 * at.x - ctrl.x, 2.0 * at.y - ctrl.y)
                } else {
                    at
                };
                let b = if op | 0x20 == b'q' { p(2) } else { p(0) };
                quad(&mut line, at, ctrl, b);
                at = b;
                quad_at = true;
                Some(Step::QuadTo(at_o(ctrl), at_o(at)))
            }
            b'c' => {
                let (c1, c2, b) = (p(0), p(2), p(4));
                cube(&mut line, at, c1, c2, b);
                at = b;
                Some(Step::CubeTo(at_o(c1), at_o(c2), at_o(b)))
            }
            b'a' => {
                assert_eq!(n[2], 0.0, "a rotated arc in {d:?}");
                let b = p(5);
                arc(&mut line, at, n[0], n[1], n[3] != 0.0, n[4] != 0.0, b);
                at = b;
                None
            }
            _ => {
                line.push(start);
                at = start;
                Some(Step::Close)
            }
        };
        match step {
            Some(s) => {
                if let Some(steps) = &mut steps {
                    steps.push(s);
                }
            }
            None => steps = None,
        }
    }
    if line.len() > 1 {
        lines.push(line);
    }
    for l in &mut lines {
        for p in l {
            *p = at_o(*p);
        }
    }
    (lines, steps)
}

/// A polygon about o as the steps of a path.
fn polygon_steps(o: Point, xy: &[f64]) -> Vec<Step> {
    let p = |i: usize| Point::new(o.x + xy[i], o.y + xy[i + 1]);
    let mut out = vec![Step::MoveTo(p(0))];
    out.extend((2..xy.len() - 1).step_by(2).map(|i| Step::LineTo(p(i))));
    out.push(Step::Close);
    out
}

/// The attributes of a tag, in its order.
fn attrs(s: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(eq) = rest.find("=\"") {
        let name = rest[..eq].trim();
        let value_end = rest[eq + 2..].find('"').unwrap();
        out.push((name, &rest[eq + 2..eq + 2 + value_end]));
        rest = &rest[eq + 2 + value_end + 1..];
    }
    out
}

/// The elements of Pele's markup for a shape drawn about x, y.
fn pele_figures(svg: &str, x: f64, y: f64) -> Vec<Figure> {
    let mut out = Vec::new();
    let mut shift = vec![Point::new(x, y)];
    let mut rest = svg;
    while let Some(open) = rest.find('<') {
        let close = rest[open..].find('>').unwrap() + open;
        let tag = rest[open + 1..close].trim_end_matches('/');
        rest = &rest[close + 1..];
        let closing = tag.starts_with('/');
        let tag = tag.trim_start_matches('/');
        let (name, a) = tag.split_once(' ').unwrap_or((tag, ""));
        let a = attrs(a);
        let get = |k: &str| a.iter().find(|(n, _)| *n == k).map(|(_, v)| *v);
        // An attribute left out is 0, as SVG reads a circle's center.
        let num = |k: &str| get(k).map_or(0.0, |v| numbers(v)[0]);
        let at = *shift.last().unwrap();
        if name == "g" {
            if closing {
                shift.pop();
            } else {
                let n = numbers(get("transform").unwrap());
                shift.push(Point::new(at.x + n[0], at.y + n[1]));
            }
            continue;
        }
        let (fill, stroke) = (get("fill"), get("stroke"));
        let paint = match (fill, stroke) {
            (Some("none"), Some("none")) => continue,
            (Some("none"), _) => Painting::StrokeOnly,
            (Some("var(--_l)"), _) => Painting::Solid,
            (_, Some("none")) => Painting::FillOnly,
            _ => Painting::FillAndStroke,
        };
        let mut f = Figure {
            paint,
            rounded: false,
            rect: None,
            lines: Vec::new(),
            steps: None,
        };
        match name {
            "rect" => {
                let (x0, y0, w, h) = (num("x"), num("y"), num("width"), num("height"));
                f.rect = Some(Rect::new(at.x + x0, at.y + y0, w, h));
                let mut r = 0.0;
                if get("style").is_some_and(|s| s.starts_with("rx:var(--_r)")) {
                    f.rounded = true;
                } else if get("rx").is_none() {
                    f.steps = Some(polygon_steps(
                        at,
                        &[x0, y0, x0 + w, y0, x0 + w, y0 + h, x0, y0 + h],
                    ));
                } else {
                    r = num("rx");
                }
                f.lines = vec![svg_rect(at.x + x0, at.y + y0, w, h, r)];
            }
            "circle" => {
                let r = num("r");
                f.lines = vec![ellipse(at.x + num("cx"), at.y + num("cy"), r, r)];
            }
            "ellipse" => f.lines = vec![ellipse(at.x, at.y, num("rx"), num("ry"))],
            "polygon" => {
                let n = numbers(get("points").unwrap());
                let mut l: Vec<Point> = n
                    .chunks(2)
                    .map(|p| Point::new(at.x + p[0], at.y + p[1]))
                    .collect();
                l.push(l[0]);
                f.lines = vec![l];
                f.steps = Some(polygon_steps(at, &n));
            }
            "path" => (f.lines, f.steps) = svg_path(get("d").unwrap(), at),
            _ => panic!("element {name} in {svg}"),
        }
        out.push(f);
    }
    out
}

/// A part flattened, and the steps of its path, or of its box where it has none.
fn part_lines(p: &Part) -> (Lines, Vec<Step>) {
    let path = match &p.outline {
        Outline::Box { rect: b, .. } => {
            let pts = [
                (b.x, b.y),
                (b.x + b.w, b.y),
                (b.x + b.w, b.y + b.h),
                (b.x, b.y + b.h),
            ];
            let mut line: Vec<Point> = pts.iter().map(|&(x, y)| Point::new(x, y)).collect();
            line.push(line[0]);
            let xy: Vec<f64> = pts.iter().flat_map(|&(x, y)| [x, y]).collect();
            return (vec![line], polygon_steps(Point::default(), &xy));
        }
        Outline::Path(path) => path,
    };
    let (mut lines, mut line) = (Vec::new(), Vec::new());
    let (mut at, mut start) = (Point::default(), Point::default());
    for step in path.steps() {
        match step {
            Step::MoveTo(p) => {
                if line.len() > 1 {
                    lines.push(std::mem::take(&mut line));
                }
                (at, start) = (p, p);
                line = vec![p];
            }
            Step::LineTo(p) => {
                at = p;
                line.push(p);
            }
            Step::QuadTo(c, p) => {
                quad(&mut line, at, c, p);
                at = p;
            }
            Step::CubeTo(c1, c2, p) => {
                cube(&mut line, at, c1, c2, p);
                at = p;
            }
            Step::Close => {
                line.push(start);
                at = start;
            }
        }
    }
    if line.len() > 1 {
        lines.push(line);
    }
    (lines, path.steps().collect())
}

fn bounds(lines: &Lines) -> Rect {
    let (mut x0, mut y0, mut x1, mut y1) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for p in lines.iter().flatten() {
        (x0, y0, x1, y1) = (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y));
    }
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// The farthest any point of a lies from the lines of b.
fn farthest(a: &Lines, b: &Lines) -> f64 {
    let mut worst: f64 = 0.0;
    for &p in a.iter().flatten() {
        let mut best = f64::INFINITY;
        for l in b {
            for s in l.windows(2) {
                if best == 0.0 {
                    break;
                }
                best = best.min(to_segment(p, s[0], s[1]));
            }
        }
        worst = worst.max(best);
    }
    worst
}

fn to_segment(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l = dx * dx + dy * dy;
    let s = if l > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / l).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.x - a.x - s * dx).hypot(p.y - a.y - s * dy)
}

#[test]
fn outlines_match_pele() {
    let p = draw();
    let mut f = Failures::new();
    let mut seen = std::collections::HashSet::new();
    let (mut exact, mut all) = (0, 0);
    for c in &p.outlines {
        seen.insert(c.shape.as_str());
        // Four cubic curves stand for a circle to within 0.027% of its radius, and the numbers
        // of Pele's markup for a circle, an ellipse or a rect are in full, where a path's are
        // taken to hundredths.
        let tol = 0.05 + 0.00028 * c.w.max(c.h) / 2.0;
        let what = format!(
            "draw_shape({:?}, {}, {}, {}, {})",
            c.shape, c.x, c.y, c.w, c.h
        );
        let want = pele_figures(&c.svg, c.x, c.y);
        let parts = draw_shape(&c.shape, c.x, c.y, c.w, c.h);
        f.check(parts.len() == want.len(), || {
            format!(
                "{what}: {} parts, Pele {}: {}",
                parts.len(),
                want.len(),
                c.svg
            )
        });
        if parts.len() != want.len() {
            continue;
        }
        for (i, (part, w)) in parts.iter().zip(&want).enumerate() {
            let rounded = matches!(part.outline, Outline::Box { rounded: true, .. });
            f.check(part.paint == w.paint && rounded == w.rounded, || {
                format!(
                    "{what}: part {i} {:?} rounded {rounded}, Pele {:?} rounded {}",
                    part.paint, w.paint, w.rounded
                )
            });
            all += 1;
            let (lines, steps) = part_lines(part);
            if w.rounded {
                exact += 1;
                let ok = matches!(part.outline, Outline::Box { rect, .. } if Some(rect) == w.rect);
                f.check(ok, || {
                    format!("{what}: part {i} is {:?}, Pele {:?}", part.outline, w.rect)
                });
                continue;
            }
            if let Some(want_steps) = &w.steps {
                exact += 1;
                let ok = match part.outline {
                    Outline::Box { rect, .. } => Some(rect) == w.rect,
                    Outline::Path(_) => &steps == want_steps,
                };
                f.check(ok, || {
                    format!(
                        "{what}: part {i} is\n{steps:?},\nPele\n{want_steps:?}: {}",
                        c.svg
                    )
                });
                continue;
            }
            let (gb, wb) = (bounds(&lines), bounds(&w.lines));
            let near = (gb.x - wb.x).abs() <= tol
                && (gb.y - wb.y).abs() <= tol
                && (gb.x + gb.w - wb.x - wb.w).abs() <= tol
                && (gb.y + gb.h - wb.y - wb.h).abs() <= tol;
            f.check(near, || {
                format!("{what}: part {i} bounds {gb:?}, Pele {wb:?}: {}", c.svg)
            });
            let d = farthest(&lines, &w.lines).max(farthest(&w.lines, &lines));
            f.check(d <= tol, || {
                format!("{what}: part {i} is {d} from Pele's: {}", c.svg)
            });
        }
    }
    for name in shape_names() {
        f.check(seen.contains(name), || format!("no outline of {name:?}"));
    }
    eprintln!(
        "{} outlines, {exact} of their {all} parts compared number for number",
        p.outlines.len()
    );
    f.done("draw outlines", 800);
}
