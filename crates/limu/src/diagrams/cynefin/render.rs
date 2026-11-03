//! A Cynefin diagram laid out as a scene, as Pele's renderCynefin in render.ts draws it.

use std::collections::HashMap;

use super::CynefinModel;
use super::boundaries::{
    FOLD, HORIZON, WaveOptions, backward, cliff, curve_path, ellipse_path, forward, resolve_seed,
    wave,
};
use crate::common;
use crate::js::math::{hypot, max, min, or1, round};
use crate::layout::Options;
use crate::scene::style::{Cap, Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{
    Edge, Item, Kind, Marker, MarkerType, Painting, Part, Path, Placed, Point, Rect, Scene, Shape,
};
use crate::text::layout_label;
use crate::value::{Map, Value};

/// The roles of the items of a Cynefin diagram, in the order Pele draws them: its title, the
/// tint of each of the four quadrants, its id its domain's, each of the three dashed wavy
/// lines between them and the S between Clear and Chaotic, the ellipse of the background that
/// hides what is under Confusion, and a region again for its tint and outline. Each domain is
/// a group, its id its name and its label its name as a heading, with a description for its
/// decision model and its kind of practice, and an item for each item it lists, its id the
/// item's text, and an overflow for the count of those Confusion leaves out. Each transition
/// is an edge, its id its ends joined by a hyphen, with a transition label for its label.
pub(crate) const ROLE_TITLE: &str = "cynefin.title";
pub(crate) const ROLE_REGION: &str = "cynefin.region";
pub(crate) const ROLE_BOUNDARY: &str = "cynefin.boundary";
pub(crate) const ROLE_CLIFF: &str = "cynefin.cliff";
pub(crate) const ROLE_BACKDROP: &str = "cynefin.backdrop";
pub(crate) const ROLE_DOMAIN: &str = "cynefin.domain";
pub(crate) const ROLE_DESCRIPTION: &str = "cynefin.description";
pub(crate) const ROLE_ITEM: &str = "cynefin.item";
pub(crate) const ROLE_OVERFLOW: &str = "cynefin.overflow";
pub(crate) const ROLE_TRANSITION: &str = "cynefin.transition";
pub(crate) const ROLE_TRANSITION_LABEL: &str = "cynefin.transition-label";

/// The domains drawn in the quadrants, as Pele's QUADRANTS: each its name, its column and
/// row, and the series of its tint.
const QUADRANTS: [(&str, f64, f64, usize); 4] = [
    ("complex", 0.0, 0.0, 0),
    ("complicated", 1.0, 0.0, 1),
    ("chaotic", 0.0, 1.0, 3),
    ("clear", 1.0, 1.0, 2),
];
const CONFUSION_COLOR: usize = 4;

/// Each domain's name, its decision model and its kind of practice, as Pele's META.
fn meta(name: &str) -> [&'static str; 3] {
    match name {
        "complex" => ["Complex", "Probe → Sense → Respond", "Emergent Practices"],
        "complicated" => ["Complicated", "Sense → Analyse → Respond", "Good Practices"],
        "clear" => ["Clear", "Sense → Categorise → Respond", "Best Practices"],
        "chaotic" => ["Chaotic", "Act → Sense → Respond", "Novel Practices"],
        _ => ["Confusion", "", "Disorder"],
    }
}

// The measures of Pele's render.ts.
const MAX_CONFUSION_ITEMS: usize = 3;
const ITEM_MAX_WIDTH: f64 = 200.0;
const ITEM_PAD_X: f64 = 10.0;
const ITEM_PAD_Y: f64 = 4.0;
const ITEM_GAP: f64 = 6.0;
const HEAD_GAP: f64 = 10.0;
const MARGIN: f64 = 20.0;
const CLEARANCE: f64 = 10.0;
const STEPS: usize = 48;

/// The id Pele hashes for the seed of the boundaries where the configuration gives none: the
/// id prefix of its render, "pele" unless the host names another. limu has no ids, so the
/// boundaries are drawn as Pele draws them by default.
const DEFAULT_ID: &str = "pele";

/// The place of Confusion among the blocks, after the four quadrants.
const CONFUSION: usize = 4;

/// An item of a domain, Pele's Badge.
struct Badge {
    text: String,
    label: Label,
    w: f64,
    h: f64,
    overflow: bool,
}

/// What a domain draws, Pele's Block: its name, its descriptions and its items, the size they
/// take, and the point they are centered on.
struct Block {
    name: &'static str,
    title: Label,
    subtitles: Vec<Label>,
    badges: Vec<Badge>,
    w: f64,
    h: f64,
    cx: f64,
    cy: f64,
}

/// A number of the diagram's section of the configuration, between lo and hi, or fallback,
/// as Pele's setting reads it.
fn setting(config: &Map, key: &str, fallback: f64, lo: f64, hi: f64) -> f64 {
    let section = config.get("cynefin").and_then(Value::as_object);
    match section.and_then(|s| s.get(key)) {
        Some(&Value::Number(v)) if v >= lo && v <= hi => v,
        _ => fallback,
    }
}

/// Lays a Cynefin diagram out as a scene, as Pele's renderCynefin draws it: the four
/// quadrants, as large as the configuration asks or their domains need, with wavy boundaries
/// between them and the cliff between Clear and Chaotic, Confusion in an ellipse in the
/// middle, each domain's name, descriptions and items, and the transitions, curves that bow
/// away from the middle.
pub(crate) fn layout(model: &CynefinModel, config: &Map, o: &Options) -> Scene {
    let size = o.font_size;
    let small = round(size * 0.8125);
    let item_size = round(size * 0.875);
    let set = common::setting(o);
    let pad = o.pad();
    let section = config.get("cynefin").and_then(Value::as_object);
    let show_descriptions =
        section.and_then(|s| s.get("showDomainDescriptions")) != Some(&Value::Bool(false));
    let amplitude = setting(config, "boundaryAmplitude", 8.0, 0.0, 50.0);
    let seed = resolve_seed(setting(config, "seed", 0.0, -1e15, 1e15), DEFAULT_ID);
    let item_line = round(item_size * 1.5);

    let badge = |text: String, overflow: bool| {
        let label = layout_label(
            &text,
            false,
            &set,
            item_size,
            ITEM_MAX_WIDTH,
            Face::default(),
        );
        let w = label.width + 2.0 * ITEM_PAD_X;
        let h = max(label.height, item_line) + 2.0 * ITEM_PAD_Y;
        Badge {
            text,
            label,
            w,
            h,
            overflow,
        }
    };
    let block = |name: &'static str| {
        let about = meta(name);
        let title = layout_label(about[0], false, &set, size, 4000.0, Face::BOLD);
        let mut subtitles = Vec::new();
        if show_descriptions {
            for text in [about[1], about[2]] {
                if !text.is_empty() {
                    subtitles.push(layout_label(text, false, &set, small, 4000.0, Face::ITALIC));
                }
            }
        }
        let items: &[String] = model.domains.get(name).map_or(&[], |d| &d.items);
        let shown = if name == "confusion" && items.len() > MAX_CONFUSION_ITEMS {
            MAX_CONFUSION_ITEMS
        } else {
            items.len()
        };
        let mut badges: Vec<Badge> = items[..shown]
            .iter()
            .map(|item| badge(item.clone(), false))
            .collect();
        if shown < items.len() {
            badges.push(badge(format!("+{} more", items.len() - shown), true));
        }
        let mut w = title.width;
        let mut h = title.height;
        for s in &subtitles {
            w = max(w, s.width);
            h += s.height;
        }
        if !badges.is_empty() {
            h += HEAD_GAP - ITEM_GAP;
        }
        for b in &badges {
            w = max(w, b.w);
            h += ITEM_GAP + b.h;
        }
        Block {
            name,
            title,
            subtitles,
            badges,
            w,
            h,
            cx: 0.0,
            cy: 0.0,
        }
    };

    let mut blocks: Vec<Block> = QUADRANTS.iter().map(|q| block(q.0)).collect();
    blocks.push(block("confusion"));
    let confusion = &blocks[CONFUSION];
    let rx = max(88.0, confusion.w * 0.71 + 8.0);
    let ry = max(64.0, confusion.h * 0.71 + 6.0);

    let mut quad_w = setting(config, "width", 640.0, 100.0, 10000.0) / 2.0;
    let mut quad_h = setting(config, "height", 440.0, 100.0, 10000.0) / 2.0;
    for b in &blocks[..CONFUSION] {
        quad_w = max(quad_w, b.w + 2.0 * MARGIN);
        quad_h = max(quad_h, b.h + 2.0 * MARGIN);
    }
    // A block sits in the middle of its quadrant, so its inner corner must stay clear of the
    // ellipse.
    for b in &blocks[..CONFUSION] {
        let dx = (quad_w - b.w) / 2.0;
        let dy = (quad_h - b.h) / 2.0;
        let (ex, ey) = (dx / (rx + CLEARANCE), dy / (ry + CLEARANCE));
        if ex * ex + ey * ey >= 1.0 {
            continue;
        }
        let wider = b.w + 2.0 * (rx + CLEARANCE);
        let taller = b.h + 2.0 * (ry + CLEARANCE);
        if wider * quad_h <= quad_w * taller {
            quad_w = max(quad_w, wider);
        } else {
            quad_h = max(quad_h, taller);
        }
    }

    let raw = model.title.as_deref().unwrap_or("");
    let title = layout_label(raw, false, &set, size, 4000.0, Face::BOLD);
    let title_height = if title.height > 0.0 {
        title.height + 12.0
    } else {
        0.0
    };
    let width = 2.0 * quad_w;
    let height = 2.0 * quad_h;
    let x0 = pad;
    let y0 = pad + title_height;
    let cx = x0 + quad_w;
    let cy = y0 + quad_h;
    let right = x0 + width;
    let bottom = y0 + height;

    let pinned = |o: WaveOptions, pin_start: bool, pin_end: bool| WaveOptions {
        segments: 4,
        pin_start,
        pin_end,
        ..o
    };
    let fold = wave(y0, cy, cx, seed, amplitude, pinned(FOLD, false, true));
    let west = wave(
        x0,
        cx,
        cy,
        seed + 100.0,
        amplitude,
        pinned(HORIZON, false, true),
    );
    let east = wave(
        cx,
        right,
        cy,
        seed + 200.0,
        amplitude,
        pinned(HORIZON, true, false),
    );
    let drop = cliff(cx, cy, bottom, width * 0.03);
    let last = |curve: &[f64]| (curve[curve.len() - 2], curve[curve.len() - 1]);

    let mut items = Vec::new();
    if !title.is_empty() {
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        let at = Point::new(x0 + width / 2.0, pad + title.height / 2.0);
        it.label = Some(Placed::new(title.clone(), at));
        items.push(it);
    }

    // Each quadrant's tint is bounded by the page and by the lines between it and its
    // neighbours, followed one way or the other.
    let region = |name: &str| {
        let mut p = Path::new();
        match name {
            "complex" => {
                p.move_to(x0, y0).line_to(fold[0], fold[1]);
                forward(&mut p, &fold);
                backward(&mut p, &west);
            }
            "complicated" => {
                let (ex, ey) = last(&east);
                p.move_to(fold[0], fold[1])
                    .line_to(right, y0)
                    .line_to(ex, ey);
                backward(&mut p, &east);
                backward(&mut p, &fold);
            }
            "chaotic" => {
                p.move_to(west[0], west[1]);
                forward(&mut p, &west);
                forward(&mut p, &drop);
                p.line_to(x0, bottom);
            }
            _ => {
                let (dx, dy) = last(&drop);
                p.move_to(east[0], east[1]);
                forward(&mut p, &east);
                p.line_to(right, bottom).line_to(dx, dy);
                backward(&mut p, &drop);
            }
        }
        p.close();
        p
    };
    for &(name, _, _, color) in &QUADRANTS {
        let p = region(name);
        let mut it = Item::new(Kind::Mark, ROLE_REGION);
        it.id = Some(name.to_string());
        it.bounds = p.bounds();
        it.parts = vec![Part::path(p, Painting::FillOnly)];
        it.style = Style {
            fill: Some(Paint::series(color)),
            fill_opacity: Some(0.14),
            ..Style::default()
        };
        items.push(it);
    }
    let stroke = |role: &'static str, curve: &[f64], style: Style| {
        let p = curve_path(curve);
        let mut it = Item::new(Kind::Mark, role);
        it.bounds = p.bounds();
        it.parts = vec![Part::path(p, Painting::StrokeOnly)];
        it.style = style;
        it
    };
    let boundary = Style {
        stroke: Some(Paint::BORDER),
        dash: Some(vec![6.0, 3.0]),
        ..Style::default()
    };
    items.push(stroke(ROLE_BOUNDARY, &fold, boundary.clone()));
    items.push(stroke(ROLE_BOUNDARY, &west, boundary.clone()));
    items.push(stroke(ROLE_BOUNDARY, &east, boundary));
    let cliff_style = Style {
        stroke: Some(Paint::LINE),
        stroke_width: Some(2.0),
        dash: Some(Vec::new()),
        cap: Some(Cap::Round),
        ..Style::default()
    };
    items.push(stroke(ROLE_CLIFF, &drop, cliff_style));

    let oval = ellipse_path(cx, cy, rx, ry);
    let oval_box = Rect::new(cx - rx, cy - ry, 2.0 * rx, 2.0 * ry);
    let mut it = Item::new(Kind::Mark, ROLE_BACKDROP);
    it.shape = Some(Shape::new("ellipse"));
    it.bounds = oval_box;
    it.parts = vec![Part::path(oval.clone(), Painting::FillOnly)];
    it.style.fill = Some(Paint::BG);
    items.push(it);
    let mut it = Item::new(Kind::Mark, ROLE_REGION);
    it.shape = Some(Shape::new("ellipse"));
    it.id = Some("confusion".to_string());
    it.bounds = oval_box;
    it.parts = vec![Part::path(oval, Painting::FillAndStroke)];
    it.style = Style {
        fill: Some(Paint::series(CONFUSION_COLOR)),
        fill_opacity: Some(0.18),
        stroke: Some(Paint::BORDER),
        dash: Some(vec![4.0, 2.0]),
        ..Style::default()
    };
    items.push(it);

    for (index, &(_, col, row, _)) in QUADRANTS.iter().enumerate() {
        let b = &mut blocks[index];
        b.cx = x0 + quad_w * (col + 0.5);
        b.cy = y0 + quad_h * (row + 0.5);
        draw_block(&mut items, b);
    }
    blocks[CONFUSION].cx = cx;
    blocks[CONFUSION].cy = cy;
    draw_block(&mut items, &blocks[CONFUSION]);

    let inside = |k: usize, (x, y): (f64, f64)| {
        let b = &blocks[k];
        if k == CONFUSION {
            let (ex, ey) = ((x - cx) / (rx + 4.0), (y - cy) / (ry + 4.0));
            return ex * ex + ey * ey <= 1.0;
        }
        (x - b.cx).abs() <= b.w / 2.0 + 8.0 && (y - b.cy).abs() <= b.h / 2.0 + 8.0
    };
    let place = |name: &str| blocks.iter().position(|b| b.name == name);

    let mut seen: HashMap<String, f64> = HashMap::new();
    for transition in &model.transitions {
        let (Some(fk), Some(tk)) = (place(&transition.from), place(&transition.to)) else {
            continue;
        };
        if fk == tk {
            continue;
        }
        let (from, to) = (&blocks[fk], &blocks[tk]);
        let key = if transition.from < transition.to {
            format!("{} {}", transition.from, transition.to)
        } else {
            format!("{} {}", transition.to, transition.from)
        };
        let repeat = seen.get(&key).copied().unwrap_or(0.0);
        seen.insert(key, repeat + 1.0);

        let dx = to.cx - from.cx;
        let dy = to.cy - from.cy;
        let len = or1(hypot(dx, dy));
        let mx = (from.cx + to.cx) / 2.0;
        let my = (from.cy + to.cy) / 2.0;
        // Curves bow away from the middle of the diagram, which keeps them off the ellipse.
        let outward = if -dy * (mx - cx) + dx * (my - cy) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        let nx = (-dy / len) * outward;
        let ny = (dx / len) * outward;
        // A diagonal runs through the middle, so it bows far enough to pass the ellipse.
        let diagonal = fk != CONFUSION && tk != CONFUSION && dx.abs() > 1.0 && dy.abs() > 1.0;
        let bow = if diagonal {
            2.0 * (hypot(rx * nx, ry * ny) + 16.0)
        } else {
            len * 0.15
        } + repeat * 56.0;
        let px = mx + nx * bow;
        let py = my + ny * bow;
        let point = |t: f64| {
            let u = 1.0 - t;
            (
                u * u * from.cx + 2.0 * u * t * px + t * t * to.cx,
                u * u * from.cy + 2.0 * u * t * py + t * t * to.cy,
            )
        };
        let crossing = |k: usize, mut lo: f64, mut hi: f64, leaving: bool| {
            for _ in 0..16 {
                let mid = (lo + hi) / 2.0;
                if inside(k, point(mid)) == leaving {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            (lo + hi) / 2.0
        };
        let steps = STEPS as f64;
        let mut i = 1;
        while i <= STEPS && inside(fk, point(i as f64 / steps)) {
            i += 1;
        }
        if i > STEPS {
            continue;
        }
        let t0 = crossing(fk, (i - 1) as f64 / steps, i as f64 / steps, true);
        while i <= STEPS && !inside(tk, point(i as f64 / steps)) {
            i += 1;
        }
        let t1 = if i > STEPS {
            1.0
        } else {
            crossing(tk, (i - 1) as f64 / steps, i as f64 / steps, false)
        };

        let (sx, sy) = point(t0);
        let (mut ex, mut ey) = point(t1);
        let blossom = |a: f64, b: f64, c: f64| {
            (1.0 - t0) * (1.0 - t1) * a + ((1.0 - t0) * t1 + t0 * (1.0 - t1)) * b + t0 * t1 * c
        };
        let qx = blossom(from.cx, px, to.cx);
        let qy = blossom(from.cy, py, to.cy);
        let el = or1(hypot(ex - qx, ey - qy));
        let ux = (ex - qx) / el;
        let uy = (ey - qy) / el;
        let head = Marker {
            kind: MarkerType::ArrowPoint,
            at: Point::new(ex, ey),
            dx: ux,
            dy: uy,
        };
        ex -= ux * MarkerType::ArrowPoint.trim();
        ey -= uy * MarkerType::ArrowPoint.trim();

        let id = format!("{}-{}", transition.from, transition.to);
        let mut p = Path::new();
        p.move_to(sx, sy).quad_to(qx, qy, ex, ey);
        let route = vec![Point::new(sx, sy), Point::new(ex, ey)];
        let mut it = Item::new(
            Kind::Edge(Edge {
                route,
                ends: [None, Some(head)],
            }),
            ROLE_TRANSITION,
        );
        it.id = Some(id.clone());
        it.bounds = p.bounds();
        it.parts = vec![Part::path(p, Painting::StrokeOnly)];
        it.style.stroke = Some(Paint::LINE);
        items.push(it);

        if let Some(text) = &transition.label {
            let label = layout_label(text, false, &set, small, ITEM_MAX_WIDTH, Face::default());
            let (lx, ly) = point((t0 + t1) / 2.0);
            let away = (nx.abs() * label.width) / 2.0 + (ny.abs() * label.height) / 2.0 + 6.0;
            let half = label.width / 2.0 + 4.0;
            let tx = min(max(lx + nx * away, x0 + half), max(right - half, x0 + half));
            if !label.is_empty() {
                let mut it = Item::new(Kind::Text, ROLE_TRANSITION_LABEL);
                it.id = Some(id);
                it.label = Some(Placed::new(label, Point::new(tx, ly + ny * away)));
                items.push(it);
            }
        }
    }

    Scene {
        diagram: "cynefin".into(),
        width: (max(width, title.width) + 2.0 * pad).ceil(),
        height: (height + title_height + 2.0 * pad).ceil(),
        font_size: size,
        title: model.acc_title.clone(),
        desc: model.acc_descr.clone(),
        items,
    }
}

/// Adds what a domain draws, as Pele's drawBlock draws it: its name as a heading, its
/// descriptions, and its items one under another, each in a rounded box.
fn draw_block(items: &mut Vec<Item>, b: &Block) {
    let mut y = b.cy - b.h / 2.0;
    let mut it = Item::new(Kind::Group, ROLE_DOMAIN);
    it.id = Some(b.name.to_string());
    it.bounds = Rect::new(b.cx - b.w / 2.0, y, b.w, b.h);
    let at = Point::new(b.cx, y + b.title.height / 2.0);
    it.label = Some(Placed::new(b.title.clone(), at));
    items.push(it);
    y += b.title.height;
    for s in &b.subtitles {
        let mut it = Item::new(Kind::Text, ROLE_DESCRIPTION);
        it.id = Some(b.name.to_string());
        it.label = Some(Placed::new(s.clone(), Point::new(b.cx, y + s.height / 2.0)));
        it.style.text = Some(Paint::MUTED);
        items.push(it);
        y += s.height;
    }
    y += HEAD_GAP - ITEM_GAP;
    for item in &b.badges {
        y += ITEM_GAP;
        let r = Rect::new(b.cx - item.w / 2.0, y, item.w, item.h);
        let role = if item.overflow {
            ROLE_OVERFLOW
        } else {
            ROLE_ITEM
        };
        let mut it = Item::new(Kind::Node, role);
        it.shape = Some(Shape::new("rounded"));
        it.bounds = r;
        it.parts = vec![Part::rect(r, true, Painting::FillAndStroke)];
        it.style = Style {
            fill: Some(Paint::BG),
            stroke: Some(Paint::BORDER),
            ..Style::default()
        };
        if item.overflow {
            it.style.dash = Some(vec![3.0, 2.0]);
            it.style.text = Some(Paint::MUTED);
        } else {
            it.id = Some(item.text.clone());
        }
        if !item.label.is_empty() {
            let at = Point::new(b.cx, y + item.h / 2.0);
            it.label = Some(Placed::new(item.label.clone(), at));
        }
        items.push(it);
        y += item.h;
    }
}
