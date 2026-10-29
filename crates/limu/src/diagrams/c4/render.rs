//! A C4 diagram laid out as a scene, as Pele's renderC4 in render.ts draws it.

use std::collections::HashSet;

use super::db::{C4Db, ElementRef};
use super::layout::{HEAD_LEFT, HEAD_TOP, Layout, bracketed, layout_c4};
use super::route::{
    Box, HEAD, LABEL_PAD, LANE, Lanes, Route, crosses, cyl_ry, queue_rx, route, truthy,
};
use crate::common::{setting, tighten, title_label, title_room};
use crate::css::{declarations, resolve_style};
use crate::detect::DiagramType;
use crate::draw::{Fill, casing, draw_shape, on_fills};
use crate::errors::{Error, ErrorCode};
use crate::js::math::{max, min, round};
use crate::layout::Options;
use crate::scene::style::{Cap, Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{
    Align, Edge, Item, Kind, MARKER_SIZE, Marker, MarkerType, Painting, Part, Path, Placed, Point,
    Rect, Scene,
};
use crate::text::{Setting, decode_entities, layout_label};
use crate::url::{link_url, sanitize_url};
use crate::value::Map;

/// The roles of the items of a C4 diagram: its title, a boundary and a deployment node, the
/// name, type and description of either, a relation and the casing under one, a shape, its
/// type, technology and description, and a relation's label and technology.
pub(crate) const ROLE_TITLE: &str = "c4.title";
pub(crate) const ROLE_BOUNDARY: &str = "c4.boundary";
pub(crate) const ROLE_DEPLOYMENT: &str = "c4.deployment";
pub(crate) const ROLE_BOUNDARY_LABEL: &str = "c4.boundary-label";
pub(crate) const ROLE_BOUNDARY_TYPE: &str = "c4.boundary-type";
pub(crate) const ROLE_BOUNDARY_DESCR: &str = "c4.boundary-descr";
pub(crate) const ROLE_REL: &str = "c4.rel";
pub(crate) const ROLE_CASING: &str = "c4.casing";
pub(crate) const ROLE_ELEMENT: &str = "c4.element";
pub(crate) const ROLE_ELEMENT_TYPE: &str = "c4.element-type";
pub(crate) const ROLE_ELEMENT_TECHN: &str = "c4.element-techn";
pub(crate) const ROLE_ELEMENT_DESCR: &str = "c4.element-descr";
pub(crate) const ROLE_REL_LABEL: &str = "c4.rel-label";
pub(crate) const ROLE_REL_TECHN: &str = "c4.rel-techn";

const PAIR_SHIFT: f64 = 7.0;
const NARROW_GAP: f64 = 36.0;
const WIDEN: f64 = 24.0;
const MAX_OFFSET: f64 = 2000.0;
// A label may cover this much of a shape's edge, where there is no text.
const EDGE: f64 = 8.0;
// Places along a relation to try for its label, nearest the middle first.
const SLIDE: [f64; 7] = [0.5, 0.4, 0.6, 0.3, 0.7, 0.2, 0.8];
// Above this many box comparisons, labels stay at the middle and lines are not steered
// around shapes.
const CARE_BUDGET: f64 = 2000000.0;
/// How much the corners of a relation label's patch are rounded.
pub(crate) const PATCH_RADIUS: f64 = 3.0;

/// The declarations of the colors UpdateElementStyle and UpdateRelStyle give, as Pele's
/// userStyle writes them.
fn user_decls(fill: Option<&str>, stroke: Option<&str>, color: Option<&str>) -> Vec<String> {
    let mut decls = Vec::new();
    for (prop, value) in [("fill", fill), ("stroke", stroke), ("color", color)] {
        if let Some(v) = value.filter(|v| !v.is_empty()) {
            decls.push(format!("{prop}:{v}"));
        }
    }
    decls
}

/// The author's style of the colors UpdateElementStyle and UpdateRelStyle give, as Pele's
/// userStyle declares them.
fn user_style(fill: Option<&str>, stroke: Option<&str>, color: Option<&str>) -> Style {
    let decls = user_decls(fill, stroke, color);
    if decls.is_empty() {
        return Style::default();
    }
    declarations(&decls).style
}

/// The fill under the texts of a shape or a boundary whose author's declarations are decls,
/// where Pele's inkFill picks a color for them on it, and none elsewhere.
fn text_ground(decls: &[String]) -> Option<Paint> {
    let r = resolve_style(decls);
    if !r.inks_text(&r) {
        return None;
    }
    declarations(decls).style.fill
}

/// Puts back the `#name;` that entity encoding hid in an address as ﬂ°°name¶ß, as Pele's
/// link does, so that the check sees what a browser would: /ﬂ°°?/g becomes # and /¶ß/g
/// becomes ;.
fn unhide(u: &str) -> String {
    const OPEN: &str = "ﬂ°";
    const DEGREE: &str = "°";
    const CLOSE: &str = "¶ß";
    if !u.contains(CLOSE) {
        return u.to_string();
    }
    let mut out = String::with_capacity(u.len());
    let mut rest = u;
    while let Some(c) = rest.chars().next() {
        if let Some(r) = rest.strip_prefix(OPEN) {
            rest = r.strip_prefix(DEGREE).unwrap_or(r);
            out.push('#');
        } else if let Some(r) = rest.strip_prefix(CLOSE) {
            rest = r;
            out.push(';');
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// Where a link the diagram gives leads, as Pele's link and linked check it, or none.
fn link(u: Option<&str>, o: &Options) -> Option<String> {
    let u = u.filter(|u| !u.is_empty())?;
    link_url(&sanitize_url(&unhide(u)), &o.schemes())
}

/// A semantic error of a C4 diagram.
fn fail(message: String) -> Error {
    let mut e = Error::new(message, ErrorCode::Semantic);
    e.diagram = Some(DiagramType::C4);
    e
}

/// The ends of each relation, as Pele's draw finds them: the shapes or the boundaries it
/// joins. A relation between things the diagram does not draw is an error.
fn rel_ends(db: &C4Db, grid: &Layout) -> Result<Vec<[ElementRef; 2]>, Error> {
    let end = |attr: bool, alias: &str| {
        (!attr)
            .then(|| db.get_c4_shape(alias))
            .flatten()
            .filter(|e| grid.boxes.contains_key(e))
    };
    db.rels
        .iter()
        .map(|rel| {
            match (
                end(rel.from_attr.is_some(), &rel.from),
                end(rel.to_attr.is_some(), &rel.to),
            ) {
                (Some(a), Some(b)) => Ok([a, b]),
                _ => Err(fail(format!(
                    "C4 rel \"{}\" -> \"{}\" references an unknown shape or boundary",
                    rel.from, rel.to
                ))),
            }
        })
        .collect()
}

/// Lays out a parsed C4 diagram as a scene, as Pele's renderC4 draws it: on Mermaid's grid
/// with room to spare, and, when that does not fit the width the host has, tightly, with the
/// gap between columns that leaves the fewest relations without a lane of their own. A
/// relation between things the diagram does not have is an error.
pub(crate) fn layout(db: &C4Db, _config: &Map, o: &Options) -> Result<Scene, Error> {
    let set = setting(o);
    let size = o.font_size;
    let small = round(size * 0.8125);
    let loose = layout_c4(db, &set, size, small, None, NARROW_GAP);
    let ends = rel_ends(db, &loose)?;
    let mut loose = Some(loose);
    Ok(tighten(o, |tight| {
        if !tight {
            let grid = loose
                .take()
                .unwrap_or_else(|| layout_c4(db, &set, size, small, None, NARROW_GAP));
            return draw(db, o, &set, grid, &ends, false).0;
        }
        let pad = o.pad();
        let width = if o.max_width > 0.0 {
            o.max_width
        } else {
            320.0
        };
        let room = Some(max(80.0, width - 2.0 * pad));
        let lay = |gap: f64| {
            let grid = layout_c4(db, &set, size, small, room, gap);
            draw(db, o, &set, grid, &ends, true)
        };
        let (mut best, mut squeezed) = lay(NARROW_GAP);
        let mut gap = NARROW_GAP + WIDEN;
        while squeezed > 0 && gap <= NARROW_GAP + 3.0 * WIDEN {
            let (wider, left) = lay(gap);
            if left < squeezed {
                (best, squeezed) = (wider, left);
            }
            gap += WIDEN;
        }
        best
    }))
}

/// The bounds of the drawing, which the grow of Pele's draw widens to hold a box.
struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl Bounds {
    fn grow(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        self.min_x = min(self.min_x, x0);
        self.min_y = min(self.min_y, y0);
        self.max_x = max(self.max_x, x1);
        self.max_y = max(self.max_y, y1);
    }
}

/// How much of a box the taken boxes cover, as the covered of Pele's draw gives it.
fn covered(taken: &[f64], x0: f64, y0: f64, x1: f64, y1: f64) -> f64 {
    let mut area = 0.0;
    for t in taken.chunks(4) {
        let w = min(x1, t[2]) - max(x0, t[0]);
        if w <= 0.0 {
            continue;
        }
        let h = min(y1, t[3]) - max(y0, t[1]);
        if h > 0.0 {
            area += w * h;
        }
    }
    area
}

/// A relation's offset kept within MAX_OFFSET, and 0 for none, as the offset of Pele's draw
/// gives it.
fn offset(v: Option<f64>) -> f64 {
    match v {
        Some(v) if truthy(v) => max(-MAX_OFFSET, min(MAX_OFFSET, v)),
        _ => 0.0,
    }
}

/// A label laid out at a size and in a face, with the room it may take.
fn label(set: &Setting, raw: &str, size: f64, room: f64, face: Face) -> Label {
    layout_label(raw, false, set, size, room, face)
}

/// A text item of a role with an id, its label placed at a point, in a style of its own and
/// the author's color of text.
fn text_item(role: &'static str, id: &str, placed: Placed, own: Style, author: &Style) -> Item {
    let mut it = Item::new(Kind::Text, role);
    it.id = Some(id.to_string());
    it.label = Some(placed);
    it.style = own;
    it.author.text = author.text;
    it
}

/// Lays the diagram out once from its grid, as Pele's draw does, and gives how many routes
/// found no lane of their own. The scene holds every place on the page, where Pele draws all
/// but its title in a group moved by ox, oy, and the inside of each shape about its middle.
/// Each number is moved as Pele's SVG moves it, the moves added up from the outermost in, so
/// that the scene holds what Pele draws to the bit.
fn draw(
    db: &C4Db,
    o: &Options,
    set: &Setting,
    mut grid: Layout,
    ends: &[[ElementRef; 2]],
    tight: bool,
) -> (Scene, usize) {
    let size = o.font_size;
    let small = round(size * 0.8125);
    let pad = o.pad();
    let root = grid.order[0];
    let mut bounds = Bounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: grid.groups[root].b.w,
        max_y: grid.groups[root].b.h,
    };

    let mut pairs = HashSet::new();
    let ends: Vec<[Box; 2]> = ends
        .iter()
        .map(|[a, b]| [grid.boxes[a], grid.boxes[b]])
        .collect();
    for [a, b] in &ends {
        pairs.insert((a.id, b.id));
    }

    // What a relation's label should not cover: shapes, arrowheads, boundary headings, and
    // the labels placed before it.
    let mut nodes: Vec<Box> = Vec::new();
    let mut taken: Vec<f64> = Vec::new();
    for &at in &grid.order {
        for n in &grid.groups[at].nodes {
            let b = n.b;
            nodes.push(b);
            taken.extend([b.x + EDGE, b.y + EDGE, b.x + b.w - EDGE, b.y + b.h - EDGE]);
        }
    }
    let rels = db.rels.len() as f64;
    let careful = rels * (nodes.len() as f64 + grid.order.len() as f64 + 3.0 * rels) <= CARE_BUDGET;

    // Every line is drawn before any label is placed, so that labels can keep off the
    // arrowheads. Lines, labels and boundaries are made where Pele works them out, before
    // the group they are drawn in is moved, and moved with it at the end.
    let trim = MarkerType::ArrowPoint.trim();
    // The boundaries whose author paints them a fill, as Pele gathers its fills: a relation
    // inside one is drawn in the color that stands out on it, and one that crosses the
    // border of one is laid on a casing.
    let mut fills = Vec::new();
    for &at in &grid.order[1..] {
        let g = &grid.groups[at].b;
        let decls = user_decls(db.boundaries[at].el.bg_color.as_deref(), None, None);
        if resolve_style(&decls).paints() {
            fills.push(Fill {
                bounds: [g.x, g.y, g.x + g.w, g.y + g.h],
                decls,
            });
        }
    }
    let mut lanes = Lanes::default();
    for &at in &grid.order[1..] {
        lanes.frame(&grid.groups[at].b);
    }
    let others = careful.then_some(nodes.as_slice());
    let mut routes: Vec<Route> = Vec::with_capacity(db.rels.len());
    let mut edges = Vec::new();
    let mut casings = Vec::new();
    for (rel, [a, b]) in db.rels.iter().zip(&ends) {
        let start_arrow = rel.r#type == "birel" || rel.r#type == "rel_b";
        let end_arrow = rel.r#type != "rel_b";
        let shift = if a.id != b.id && pairs.contains(&(b.id, a.id)) {
            PAIR_SHIFT
        } else {
            0.0
        };
        let st = if start_arrow { trim } else { 0.0 };
        let et = if end_arrow { trim } else { 0.0 };
        let r = route(a, b, shift, others, &mut lanes, st, et);
        bounds.grow(
            min(r.x0, r.x1),
            min(r.y0, r.y1),
            max(r.x0, r.x1),
            max(r.y0, r.y1),
        );
        if start_arrow && careful {
            taken.extend([
                r.sx - MARKER_SIZE,
                r.sy - MARKER_SIZE,
                r.sx + MARKER_SIZE,
                r.sy + MARKER_SIZE,
            ]);
        }
        if end_arrow && careful {
            taken.extend([
                r.ex - MARKER_SIZE,
                r.ey - MARKER_SIZE,
                r.ex + MARKER_SIZE,
                r.ey + MARKER_SIZE,
            ]);
        }
        let id = format!("{}-{}", rel.from, rel.to);
        let mut route: Vec<Point> = r.points.chunks(2).map(|p| Point::new(p[0], p[1])).collect();
        if route.is_empty() {
            // A self-loop, which the layout gives no route: the corners its curve is drawn
            // about.
            route = crate::items::corners(&r.path);
        }
        let marker = |at: (f64, f64), dx: f64, dy: f64| Marker {
            kind: MarkerType::ArrowPoint,
            at: Point::new(at.0, at.1),
            dx,
            dy,
        };
        let marks = [
            start_arrow.then(|| marker((r.sx, r.sy), r.sdx, r.sdy)),
            end_arrow.then(|| marker((r.ex, r.ey), r.edx, r.edy)),
        ];
        let mut it = Item::new(Kind::Edge(Edge { route, ends: marks }), ROLE_REL);
        it.id = Some(id.clone());
        it.tags = vec![rel.r#type.clone()];
        it.bounds = r.path.bounds();
        it.parts = vec![Part::path(r.path.clone(), Painting::StrokeOnly)];
        // A line ends round, as Pele draws it in the group of edges, which sets round caps.
        it.style = Style {
            stroke: Some(Paint::LINE),
            fill: Some(Paint::NONE),
            cap: Some(Cap::Round),
            ..Style::default()
        };
        it.author = user_style(None, rel.line_color.as_deref(), None);
        // A relation inside one fill is drawn in the color of its labels, and one that
        // crosses the border of a fill is laid on a casing, as Pele's overFills finds.
        let line_decls = user_decls(None, rel.line_color.as_deref(), None);
        let line = resolve_style(&line_decls).line;
        let (ground, cased) = on_fills(&r.path, &[r.sx, r.sy, r.ex, r.ey], &fills, &line);
        it.style.ground = ground;
        if cased && let Some(mut c) = casing(ROLE_CASING, &r.path, &marks, &line_decls, false) {
            c.id = Some(id);
            casings.push(c);
        }
        edges.push(it);
        routes.push(r);
    }

    // A boundary's heading goes at the left, the middle or the right of its top edge: the
    // first of them no line crosses.
    for &at in &grid.order {
        let g = &mut grid.groups[at];
        if !careful || g.head_w == 0.0 {
            continue;
        }
        let top = g.b.y + HEAD_TOP;
        let bottom = g.b.y + g.head - LANE;
        let mut fewest = f64::INFINITY;
        for x in [
            HEAD_LEFT,
            (g.b.w - g.head_w) / 2.0,
            g.b.w - HEAD_LEFT - g.head_w,
        ] {
            let count = routes
                .iter()
                .filter(|r| {
                    crosses(
                        &r.points,
                        g.b.x + x - 4.0,
                        top,
                        g.b.x + x + g.head_w + 4.0,
                        bottom,
                    )
                })
                .count() as f64;
            if count < fewest {
                fewest = count;
                g.head_x = x;
            }
            if count == 0.0 {
                break;
            }
        }
        taken.extend([
            g.b.x + g.head_x - 4.0,
            top,
            g.b.x + g.head_x + g.head_w + 4.0,
            bottom,
        ]);
    }

    let mut labels = Vec::new();
    for (index, (rel, r)) in db.rels.iter().zip(&routes).enumerate() {
        let id = format!("{}-{}", rel.from, rel.to);
        let author = user_style(None, None, rel.text_color.as_deref());
        let raw = if db.c4_type.as_deref() == Some("C4Dynamic") {
            format!("{}: {}", index + 1, rel.label)
        } else {
            rel.label.clone()
        };
        let text = label(set, &raw, small, r.room, Face::REGULAR);
        let techn = label(
            set,
            &bracketed(rel.techn.as_deref()),
            small,
            r.room,
            Face::ITALIC,
        );
        let lw = max(text.width, techn.width) + LABEL_PAD;
        let lh = text.height + techn.height;
        if lh == 0.0 {
            continue;
        }

        // The label sits on the line, or just beside it when another relation runs the
        // opposite way.
        let beside = r.nx.abs() * (lw / 2.0) + r.ny.abs() * (lh / 2.0) + 2.0;
        let aside = if truthy(r.lean) {
            max(0.0, min(r.lean, beside - 14.0))
        } else {
            beside
        };
        // Manual offsets are tuned to the original spacing. Compact labels are placed beside
        // their routed arrows so an offset cannot leave them floating outside a boundary.
        let (off_x, off_y) = if tight {
            (0.0, 0.0)
        } else {
            (offset(rel.offset_x), offset(rel.offset_y))
        };
        let mut dx = r.nx * aside + off_x;
        let mut dy = r.ny * aside + off_y;
        let mut at = 0.5;
        if careful {
            // Slide along the line to the spot that covers least. Offsets from the diagram
            // are kept unless a spot on the line itself covers less than any they lead to:
            // they are often tuned for Mermaid's spacing.
            let mut least = f64::INFINITY;
            let moved = dx != r.nx * aside || dy != r.ny * aside;
            let passes = if moved { 2 } else { 1 };
            let mut pass = 0;
            while pass < passes && least > 0.0 {
                let (px, py) = if pass > 0 {
                    (r.nx * aside, r.ny * aside)
                } else {
                    (dx, dy)
                };
                for t in SLIDE {
                    let cx = r.x0 + (r.x1 - r.x0) * t + px;
                    let cy = r.y0 + (r.y1 - r.y0) * t + py;
                    let area = covered(
                        &taken,
                        cx - lw / 2.0,
                        cy - lh / 2.0,
                        cx + lw / 2.0,
                        cy + lh / 2.0,
                    );
                    if area < least {
                        least = area;
                        at = t;
                        dx = px;
                        dy = py;
                    }
                    if area == 0.0 {
                        break;
                    }
                }
                pass += 1;
            }
        }
        let cx = r.x0 + (r.x1 - r.x0) * at + dx;
        let top = r.y0 + (r.y1 - r.y0) * at + dy - lh / 2.0;
        if careful {
            taken.extend([cx - lw / 2.0, top, cx + lw / 2.0, top + lh]);
        }
        bounds.grow(cx - lw / 2.0, top, cx + lw / 2.0, top + lh);
        let href = link(rel.link.as_deref(), o);
        let patch = Rect::new(cx - lw / 2.0, top, lw, lh);
        let mut back = Path::new();
        back.round_rect(patch, PATCH_RADIUS);
        let mut it = Item::new(Kind::Text, ROLE_REL_LABEL);
        it.id = Some(id.clone());
        it.bounds = patch;
        it.parts = vec![Part::path(back, Painting::FillOnly)];
        let text_h = text.height;
        if !text.is_empty() {
            it.label = Some(Placed::new(text, Point::new(cx, top + text_h / 2.0)));
        }
        it.style = Style {
            fill: Some(Paint::BG),
            stroke: Some(Paint::NONE),
            text: Some(Paint::FG),
            ..Style::default()
        };
        it.author.text = author.text;
        it.link = href.clone();
        it.tooltip = rel
            .descr
            .as_deref()
            .filter(|d| !d.is_empty())
            .map(decode_entities);
        labels.push(it);
        if !techn.is_empty() {
            let at = Point::new(cx, top + text_h + techn.height / 2.0);
            let own = Style {
                text: Some(Paint::MUTED),
                ..Style::default()
            };
            let mut it = text_item(ROLE_REL_TECHN, &id, Placed::new(techn, at), own, &author);
            it.link = href;
            labels.push(it);
        }
    }

    let mut clusters = Vec::new();
    for &at in &grid.order[1..] {
        let g = &grid.groups[at];
        let el = &db.boundaries[at].el;
        let colors = (
            el.bg_color.as_deref(),
            el.border_color.as_deref(),
            el.font_color.as_deref(),
        );
        let author = user_style(colors.0, colors.1, colors.2);
        // The heading is set on the boundary's fill, in the color that stands out on a fill
        // the author gives it, as Pele's inkFill picks it.
        let ground = text_ground(&user_decls(colors.0, colors.1, colors.2));
        let frame = Rect::new(g.b.x, g.b.y, g.b.w, g.b.h);
        let mut it = Item::new(Kind::Group, ROLE_BOUNDARY);
        it.shape = Some("rect".into());
        it.id = Some(el.alias.clone());
        it.bounds = frame;
        it.parts = vec![Part::rect(frame, true, Painting::FillAndStroke)];
        it.style = Style {
            fill: Some(Paint::NONE),
            stroke: Some(Paint::BORDER),
            dash: Some(vec![6.0, 4.0]),
            ..Style::default()
        };
        it.author = Style {
            fill: author.fill,
            stroke: author.stroke,
            ..Style::default()
        };
        if let Some(node_type) = db.boundaries[at]
            .node_type
            .as_deref()
            .filter(|t| !t.is_empty())
        {
            it.role = ROLE_DEPLOYMENT.into();
            it.tags = vec![node_type.to_string()];
            it.style.dash = None;
        }
        clusters.push(it);
        let href = link(el.link.as_deref(), o);
        let x = g.b.x + g.head_x;
        let mut y = g.b.y + HEAD_TOP;
        for (k, role) in [ROLE_BOUNDARY_LABEL, ROLE_BOUNDARY_TYPE, ROLE_BOUNDARY_DESCR]
            .into_iter()
            .enumerate()
        {
            let l = &g.labels[k];
            let h = l.height;
            if !l.is_empty() {
                let own = Style {
                    text: Some(if k == 0 { Paint::FG } else { Paint::MUTED }),
                    text_ground: ground,
                    ..Style::default()
                };
                let mut placed = Placed::new(l.clone(), Point::new(x, y + h / 2.0));
                placed.align = Align::Start;
                let mut it = text_item(role, &el.alias, placed, own, &author);
                it.link = href.clone();
                clusters.push(it);
            }
            y += h;
        }
    }

    let room = if tight { title_room(o) } else { 4000.0 };
    let title = title_label(db.title.as_deref(), set, size, room);
    let title_height = if title.height > 0.0 {
        title.height + 12.0
    } else {
        0.0
    };
    let Bounds {
        min_x,
        min_y,
        max_x,
        max_y,
    } = bounds;
    let inner = max(max_x - min_x, title.width);
    let width = (inner + 2.0 * pad).ceil();
    let height = (max_y - min_y + title_height + 2.0 * pad).ceil();
    let ox = pad - min_x + (inner - (max_x - min_x)) / 2.0;
    let oy = pad + title_height - min_y;

    let mut shapes = Vec::new();
    for &at in &grid.order {
        for n in &grid.groups[at].nodes {
            let shape = &db.shapes[n.shape];
            let el = &shape.el;
            let external = shape.kind.starts_with("external");
            let colors = (
                el.bg_color.as_deref(),
                el.border_color.as_deref(),
                el.font_color.as_deref(),
            );
            let decls = user_decls(colors.0, colors.1, colors.2);
            let author = user_style(colors.0, colors.1, colors.2);
            // The texts are set on the shape's fill, in the color that stands out on a fill
            // the author gives it, as Pele's inkFill picks it: the name on the shape's own,
            // and the rest on the fill under them.
            let resolved = resolve_style(&decls);
            let inks = resolved.inks_text(&resolved);
            let ground = text_ground(&decls);
            let Box { w, h, form, .. } = n.b;
            // The middle of the shape, which Pele draws its inside about.
            let nx = ox + (n.b.x + w / 2.0);
            let ny = oy + (n.b.y + h / 2.0);
            let dx = if form == "h-cyl" {
                -queue_rx(h) / 2.0
            } else {
                0.0
            };
            let dy = match form {
                "person" => HEAD / 2.0,
                "cyl" => cyl_ry(w) / 2.0,
                _ => 0.0,
            };
            let mut own = Style {
                fill: Some(Paint::SURFACE),
                stroke: Some(Paint::BORDER),
                text: Some(Paint::FG),
                on_fill: inks,
                ..Style::default()
            };
            if external {
                own.fill = Some(Paint::BG);
                own.dash = Some(vec![4.0, 3.0]);
            }
            let href = link(el.link.as_deref(), o);
            let mut it = Item::new(Kind::Node, ROLE_ELEMENT);
            it.shape = Some(form.into());
            it.id = Some(el.alias.clone());
            it.tags = vec![shape.kind.clone()];
            if external {
                it.tags.push("external".into());
            }
            it.bounds = Rect::new(nx - w / 2.0, ny - h / 2.0, w, h);
            it.parts = draw_shape(form, nx, ny, w, h);
            it.style = own;
            it.author = author.clone();
            it.link = href.clone();
            let labels = &n.labels;
            let mut y = dy - n.text_h / 2.0;
            let mut ys = [0.0; 4];
            for (k, l) in labels.iter().enumerate() {
                ys[k] = y + l.height / 2.0;
                y += l.height;
                if k == 2 {
                    y += 4.0;
                }
            }
            if !labels[1].is_empty() {
                let at = Point::new(nx + dx, ny + ys[1]);
                it.label = Some(Placed::new(labels[1].clone(), at));
            }
            shapes.push(it);
            for k in [0, 2, 3] {
                if labels[k].is_empty() {
                    continue;
                }
                let (role, text) = match k {
                    0 => (ROLE_ELEMENT_TYPE, Paint::MUTED),
                    2 => (ROLE_ELEMENT_TECHN, Paint::MUTED),
                    _ => (ROLE_ELEMENT_DESCR, Paint::FG),
                };
                let own = Style {
                    text: Some(text),
                    text_ground: ground,
                    ..Style::default()
                };
                let at = Point::new(nx + dx, ny + ys[k]);
                let placed = Placed::new(labels[k].clone(), at);
                let mut it = text_item(role, &el.alias, placed, own, &author);
                it.link = href.clone();
                shapes.push(it);
            }
        }
    }

    let mut sc = Scene {
        diagram: "c4".into(),
        width,
        height,
        font_size: size,
        title: db.acc_title.clone(),
        desc: db.acc_descr.clone(),
        items: Vec::new(),
    };
    if !title.is_empty() {
        let at = Point::new(width / 2.0, pad + title.height / 2.0);
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        it.label = Some(Placed::new(title, at));
        it.style.text = Some(Paint::FG);
        sc.items.push(it);
    }
    for layer in [clusters, casings, edges] {
        for mut it in layer {
            crate::items::move_by(&mut it, ox, oy);
            sc.items.push(it);
        }
    }
    sc.items.extend(shapes);
    for mut it in labels {
        crate::items::move_by(&mut it, ox, oy);
        sc.items.push(it);
    }
    (sc, lanes.squeezed)
}
