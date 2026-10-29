//! An architecture diagram laid out as a scene, as Pele's renderArchitecture in render.ts
//! draws it.
//!
//! An icon the host can draw is a picture of the service or the group. One of the five Pele
//! knows itself, database, server, disk, internet and cloud, is an item of its own, its
//! outline stroked in the color of text. A name neither knows draws nothing, as Pele draws an
//! empty icon.

use std::collections::HashMap;

use super::ArchitectureDb;
use super::icons::{GLYPH_STROKE, builtin_icon};
use super::layout::{DX, DY, place_on_grid, side_index};
use crate::common::{self, tighten, title_room};
use crate::draw::{label_patch, label_patch_box};
use crate::js::chars::{len16, trim_end};
use crate::js::math::{by, hypot, max, min, round};
use crate::layout::Options;
use crate::scene::style::{Cap, Join, Paint, Style};
use crate::scene::text::{Face, Label, Span};
use crate::scene::{
    Edge, Item, Kind, Marker, MarkerType, Painting, Part, Path, Picture, PictureSource, Placed,
    Point, Rect, Scene, Shape,
};
use crate::text::{Setting, decode_entities, layout_label};
use crate::value::{Map, Value};

/// The roles of the items of an architecture diagram: its title above the diagram, each
/// group's box with its title, its id the group's, each edge, its id its ends' ids joined by
/// a hyphen, with its label on a patch of the background, each service's square with its
/// title beneath it, the text a service shows in place of an icon, each junction's dot, and
/// an icon Pele draws itself, its id its service's or group's and its shape its name, right
/// after the item it belongs to.
pub(crate) const ROLE_TITLE: &str = "architecture.title";
pub(crate) const ROLE_GROUP: &str = "architecture.group";
pub(crate) const ROLE_EDGE: &str = "architecture.edge";
pub(crate) const ROLE_EDGE_LABEL: &str = "architecture.edge-label";
pub(crate) const ROLE_SERVICE: &str = "architecture.service";
pub(crate) const ROLE_ICON_TEXT: &str = "architecture.icon-text";
pub(crate) const ROLE_JUNCTION: &str = "architecture.junction";
pub(crate) const ROLE_ICON: &str = "architecture.icon";

// The measures of Pele's render.ts: how far an edge leaves a node straight, the most its
// corners are rounded by, the room between a service's icon and its title, and the radius of
// a junction's dot.
const STUB: f64 = 12.0;
const CORNER: f64 = 6.0;
const LABEL_GAP: f64 = 4.0;
const DOT: f64 = 3.0;

/// A number of the diagram's section of the configuration, held between lo and hi, or
/// fallback, as Pele's setting reads it.
fn setting(config: &Map, key: &str, fallback: f64, lo: f64, hi: f64) -> f64 {
    let section = config.get("architecture").and_then(Value::as_object);
    match section.and_then(|s| s.get(key)) {
        Some(&Value::Number(v)) if v.is_finite() => min(hi, max(lo, v)),
        _ => fallback,
    }
}

/// Maps the grid lines in use to consecutive indexes, so empty rows and columns take no room,
/// as Pele's rank does.
fn rank(mut values: Vec<i64>) -> HashMap<i64, usize> {
    values.sort();
    let mut out = HashMap::new();
    for v in values {
        let next = out.len();
        out.entry(v).or_insert(next);
    }
    out
}

/// The end of the first n UTF-16 code units of s, in bytes, and whether the n units end
/// inside a character, between the two of a surrogate pair. The end then takes the whole
/// character.
fn prefix16(s: &str, n: usize) -> (usize, bool) {
    let mut units = 0;
    for (i, c) in s.char_indices() {
        if units >= n {
            return (i, false);
        }
        units += c.len_utf16();
        if units > n {
            return (i + c.len_utf8(), true);
        }
    }
    (s.len(), false)
}

/// Cuts a label down to a box, ending what is left with an ellipsis, as Pele's fit does.
/// Where its search measures text that ends between the two halves of a surrogate pair, it
/// measures the whole character, which Pele's measurer gives the same width.
fn fit(mut label: Label, max_width: f64, max_lines: usize, set: &Setting) -> Label {
    let cut = label.lines.len() > max_lines;
    if !cut && label.width <= max_width {
        return label;
    }
    let dots = set.width("…", label.size, Face::default());
    let mut lines = Vec::new();
    let mut widths = Vec::new();
    let mut widest = 0.0;
    for i in 0..max_lines.min(label.lines.len()) {
        let mut line = label.lines[i].clone();
        let mut width = label.widths[i];
        if width > max_width || (cut && i == max_lines - 1) {
            loop {
                let Some(span) = line.last_mut() else {
                    line.push(Span {
                        text: "…".into(),
                        face: Face::default(),
                        icon: None,
                        width: dots,
                    });
                    width = dots;
                    break;
                };
                let rest = width - span.width;
                if span.icon.is_none() && rest + dots < max_width {
                    let (mut lo, mut hi) = (0, len16(&span.text));
                    while lo < hi {
                        let mid = (lo + hi + 1) >> 1;
                        let (end, _) = prefix16(&span.text, mid);
                        let w = set.width(&span.text[..end], label.size, span.face);
                        if rest + w + dots <= max_width {
                            lo = mid;
                        } else {
                            hi = mid - 1;
                        }
                    }
                    let (mut end, split) = prefix16(&span.text, lo);
                    if split {
                        end = prefix16(&span.text, lo - 1).0;
                    }
                    span.text = format!("{}…", trim_end(&span.text[..end]));
                    span.width = set.width(&span.text, label.size, span.face);
                    width = rest + span.width;
                    break;
                }
                line.pop();
                width = rest;
            }
        }
        lines.push(line);
        widths.push(width);
        widest = max(widest, width);
    }
    label.height = lines.len() as f64 * label.line_height;
    label.lines = lines;
    label.widths = widths;
    label.width = widest;
    label
}

/// Where an edge leaves or reaches a node, Pele's Port: the point, the way out of the side,
/// the box the route has to stay out of when it cannot leave straight, how far short of the
/// point an arrowhead stops (the radius of a junction's dot), and whether a junction is
/// reached through the border of its group, so that the line carries on to the junction.
#[derive(Clone, Copy)]
struct Port {
    x: f64,
    y: f64,
    dx: f64,
    dy: f64,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    gap: f64,
    wire: bool,
}

/// An orthogonal route from one port to another, each left along its side's normal, as
/// Pele's route gives it.
fn route(s: &Port, e: &Port) -> Vec<f64> {
    let (sx, sy, sdx, sdy) = (s.x, s.y, s.dx, s.dy);
    let (ex, ey, edx, edy) = (e.x, e.y, e.dx, e.dy);
    let s1x = sx + sdx * STUB;
    let s1y = sy + sdy * STUB;
    let e1x = ex + edx * STUB;
    let e1y = ey + edy * STUB;
    if sdx * edx + sdy * edy == 0.0 {
        let cx = if sdx != 0.0 { ex } else { sx };
        let cy = if sdx != 0.0 { sy } else { ey };
        if (cx - sx) * sdx + (cy - sy) * sdy > 0.0 && (cx - ex) * edx + (cy - ey) * edy > 0.0 {
            return vec![sx, sy, cx, cy, ex, ey];
        }
        return if sdx != 0.0 {
            vec![sx, sy, s1x, s1y, s1x, e1y, e1x, e1y, ex, ey]
        } else {
            vec![sx, sy, s1x, s1y, e1x, s1y, e1x, e1y, ex, ey]
        };
    }
    let sideways = sdx != 0.0;
    let offset = if sideways { ey - sy } else { ex - sx };
    if sdx == -edx && sdy == -edy {
        let ahead = (ex - sx) * sdx + (ey - sy) * sdy;
        if ahead > 0.0 {
            if offset.abs() < 0.5 {
                return vec![sx, sy, ex, ey];
            }
            // Turn midway between the two boxes, which is clear of both titles, when there is
            // such a place.
            if sideways {
                let lo = if sdx > 0.0 { s.x1 } else { e.x1 };
                let hi = if sdx > 0.0 { e.x0 } else { s.x0 };
                let mx = if lo < hi {
                    (lo + hi) / 2.0
                } else {
                    (sx + ex) / 2.0
                };
                return vec![sx, sy, mx, sy, mx, ey, ex, ey];
            }
            let lo = if sdy > 0.0 { s.y1 } else { e.y1 };
            let hi = if sdy > 0.0 { e.y0 } else { s.y0 };
            let my = if lo < hi {
                (lo + hi) / 2.0
            } else {
                (sy + ey) / 2.0
            };
            return vec![sx, sy, sx, my, ex, my, ex, ey];
        }
    } else if offset.abs() >= 1.0 {
        if sideways {
            let x = if sdx > 0.0 {
                max(sx, ex) + STUB
            } else {
                min(sx, ex) - STUB
            };
            return vec![sx, sy, x, sy, x, ey, ex, ey];
        }
        let y = if sdy > 0.0 {
            max(sy, ey) + STUB
        } else {
            min(sy, ey) - STUB
        };
        return vec![sx, sy, sx, y, ex, y, ex, ey];
    }
    // The straight way is blocked by one of the two nodes: go between them, or around both.
    if sideways {
        let lane = if s.y1 + STUB <= e.y0 {
            (s.y1 + e.y0) / 2.0
        } else if e.y1 + STUB <= s.y0 {
            (e.y1 + s.y0) / 2.0
        } else {
            max(s.y1, e.y1) + STUB
        };
        return vec![sx, sy, s1x, sy, s1x, lane, e1x, lane, e1x, ey, ex, ey];
    }
    let lane = if s.x1 + STUB <= e.x0 {
        (s.x1 + e.x0) / 2.0
    } else if e.x1 + STUB <= s.x0 {
        (e.x1 + s.x0) / 2.0
    } else {
        max(s.x1, e.x1) + STUB
    };
    vec![sx, sy, sx, s1y, lane, s1y, lane, e1y, ex, e1y, ex, ey]
}

/// Drops repeated points and joins runs that continue in a straight line, as Pele's simplify
/// does.
fn simplify(p: &[f64]) -> Vec<f64> {
    let mut out = vec![p[0], p[1]];
    for &[x, y] in p[2..].as_chunks::<2>().0 {
        let n = out.len();
        let same_x = (x - out[n - 2]).abs() < 0.01;
        let same_y = (y - out[n - 1]).abs() < 0.01;
        if same_x && same_y {
            continue;
        }
        if n >= 4
            && ((same_x && (out[n - 4] - out[n - 2]).abs() < 0.01)
                || (same_y && (out[n - 3] - out[n - 1]).abs() < 0.01))
        {
            out[n - 2] = x;
            out[n - 1] = y;
        } else {
            out.extend([x, y]);
        }
    }
    out
}

/// The path through the points, trimmed at each end and with its corners rounded, as Pele's
/// pathData draws it.
fn path_data(p: &[f64], start_trim: f64, end_trim: f64) -> Path {
    let n = p.len() / 2;
    let mut xs: Vec<f64> = p.iter().step_by(2).copied().collect();
    let mut ys: Vec<f64> = p.iter().skip(1).step_by(2).copied().collect();
    let mut trim = |at: usize, toward: usize, amount: f64| {
        let len = hypot(xs[toward] - xs[at], ys[toward] - ys[at]);
        let t = min(amount, max(0.0, len - 1.0));
        if len > 0.0 && t > 0.0 {
            xs[at] += ((xs[toward] - xs[at]) / len) * t;
            ys[at] += ((ys[toward] - ys[at]) / len) * t;
        }
    };
    trim(0, 1, start_trim);
    trim(n - 1, n - 2, end_trim);
    let mut path = Path::new();
    path.move_to(xs[0], ys[0]);
    for i in 1..n - 1 {
        let before = hypot(xs[i] - xs[i - 1], ys[i] - ys[i - 1]);
        let after = hypot(xs[i + 1] - xs[i], ys[i + 1] - ys[i]);
        let r = min(min(CORNER, before / 2.0), after / 2.0);
        if r < 0.5 {
            path.line_to(xs[i], ys[i]);
            continue;
        }
        let in_x = xs[i] - ((xs[i] - xs[i - 1]) / before) * r;
        let in_y = ys[i] - ((ys[i] - ys[i - 1]) / before) * r;
        let out_x = xs[i] + ((xs[i + 1] - xs[i]) / after) * r;
        let out_y = ys[i] + ((ys[i + 1] - ys[i]) / after) * r;
        path.line_to(in_x, in_y).quad_to(xs[i], ys[i], out_x, out_y);
    }
    path.line_to(xs[n - 1], ys[n - 1]);
    path
}

/// Lays an architecture diagram out as a scene, as Pele's renderArchitecture draws it: the
/// services, junctions and groups on the grid place_on_grid gives them, the rows and columns
/// as wide as what is in them needs, and the edges in orthogonal routes between the sides
/// they name. A diagram wider than the width the host has is laid out again more tightly, and
/// kept so when that is narrower.
pub(crate) fn layout(model: &ArchitectureDb, config: &Map, o: &Options) -> Scene {
    tighten(o, |tight| draw(model, config, o, tight))
}

/// The icon of a service or a group in a square box, as Pele's iconSlot draws it: a picture
/// of the owner where the host draws it, or an item of its own for one of Pele's glyphs,
/// stroked round in the color of text, or nothing.
fn icon_slot(owner: &mut Item, name: &str, x: f64, y: f64, side: f64, o: &Options) -> Option<Item> {
    let bounds = Rect::new(x, y, side, side);
    if o.has_icon(&decode_entities(name)) {
        owner.picture = Some(Picture {
            source: PictureSource::Icon(decode_entities(name)),
            bounds,
        });
        return None;
    }
    let (name, outline) = builtin_icon(name)?;
    let mut it = Item::new(Kind::Mark, ROLE_ICON);
    it.shape = Some(Shape::new(name));
    it.id = owner.id.clone();
    it.bounds = bounds;
    let part = outline.scaled(bounds.w / 24.0, bounds.x, bounds.y);
    it.parts = vec![Part::path(part, Painting::StrokeOnly)];
    it.style = Style {
        stroke: Some(Paint::FG),
        fill: Some(Paint::NONE),
        stroke_width: Some(GLYPH_STROKE * bounds.w / 24.0),
        cap: Some(Cap::Round),
        join: Some(Join::Round),
        ..Style::default()
    };
    Some(it)
}

/// Lays the diagram out, tightly or not, as Pele's draw does.
fn draw(model: &ArchitectureDb, config: &Map, o: &Options, tight: bool) -> Scene {
    let icon_size = setting(config, "iconSize", 48.0, 8.0, 512.0);
    let size = setting(config, "fontSize", o.font_size, 4.0, 128.0);
    let group_pad = min(
        setting(config, "padding", 20.0, 0.0, 256.0),
        if tight { 12.0 } else { f64::INFINITY },
    );
    let small = round(size * 0.875);
    let set = common::setting(o);
    let pad = o.pad();
    let half = icon_size / 2.0;

    let nodes: Vec<_> = model.nodes.values().collect();
    let groups: Vec<_> = model.groups.values().collect();
    let n = nodes.len();
    let g_count = groups.len();
    let grid = place_on_grid(&nodes, &groups, &model.edges, &model.layout_hints);
    let index: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (node.id.as_str(), i))
        .collect();

    // A service is its icon square with the title beneath. A junction takes the same room.
    let mut labels = Vec::with_capacity(n);
    let mut box_w = vec![0.0; n];
    let mut box_h = vec![0.0; n];
    let wrap = if tight {
        max(icon_size, 64.0)
    } else {
        max(icon_size * 2.25, 96.0)
    };
    for (i, node) in nodes.iter().enumerate() {
        let raw = if node.junction {
            ""
        } else {
            node.title.as_deref().unwrap_or("")
        };
        let label = layout_label(raw, true, &set, size, wrap, Face::default());
        box_w[i] = max(icon_size, 2.0 * (label.width / 2.0).ceil());
        box_h[i] = icon_size
            + if label.height > 0.0 {
                LABEL_GAP + label.height
            } else {
                0.0
            };
        labels.push(label);
    }

    let mut col_values = grid.col.clone();
    let mut row_values = grid.row.clone();
    for g in 0..g_count {
        if grid.group_empty[g] {
            col_values.push(grid.group_col[g]);
            row_values.push(grid.group_row[g]);
        }
    }
    let col_of = rank(col_values);
    let row_of = rank(row_values);
    // A line no node or empty group is on reads as the first, as Pele's Int32Array keeps the
    // undefined it is given.
    let col_at = |c: i64| col_of.get(&c).copied().unwrap_or(0);
    let row_at = |r: i64| row_of.get(&r).copied().unwrap_or(0);
    let cols = col_of.len();
    let rows = row_of.len();
    let mut col_w = vec![0.0; cols];
    let mut row_h = vec![0.0; rows];
    let gap = if tight {
        max(24.0, 2.0 * STUB)
    } else {
        max(40.0, round(icon_size * 0.75))
    };
    let mut gap_x = vec![gap; cols];
    let mut gap_y = vec![gap; rows];
    let mut node_col = vec![0; n];
    let mut node_row = vec![0; n];
    for i in 0..n {
        let c = col_at(grid.col[i]);
        let r = row_at(grid.row[i]);
        node_col[i] = c;
        node_row[i] = r;
        col_w[c] = max(col_w[c], box_w[i]);
        row_h[r] = max(row_h[r], box_h[i]);
    }
    for g in 0..g_count {
        if !grid.group_empty[g] {
            continue;
        }
        let c = col_at(grid.group_col[g]);
        let r = row_at(grid.group_row[g]);
        col_w[c] = max(col_w[c], icon_size);
        row_h[r] = max(row_h[r], half);
    }

    // An edge label needs the gap its edge crosses to be wide enough to hold it.
    let edge_count = model.edges.len();
    let mut edge_labels = Vec::with_capacity(edge_count);
    let mut ends = Vec::with_capacity(edge_count);
    for edge in &model.edges {
        let a = index.get(edge.lhs_id.as_str()).copied();
        let b = index.get(edge.rhs_id.as_str()).copied();
        ends.push((a, b));
        let raw = edge.title.as_deref().unwrap_or("");
        let label = layout_label(raw, true, &set, small, 160.0, Face::default());
        let width = label.width;
        let height = label.height;
        edge_labels.push(label);
        let (Some(a), Some(b)) = (a, b) else {
            continue;
        };
        if width == 0.0 {
            continue;
        }
        if node_col[a] != node_col[b] {
            let c = node_col[a].min(node_col[b]);
            gap_x[c] = max(gap_x[c], width.ceil() + 28.0);
        } else {
            let c = node_col[a];
            col_w[c] = max(col_w[c], 2.0 * (width / 2.0).ceil() + 8.0);
            if node_row[a] != node_row[b] {
                let r = node_row[a].min(node_row[b]);
                gap_y[r] = max(gap_y[r], height.ceil() + 24.0);
            }
        }
    }

    // Room around each group, kept on the rows and columns at its edges. A group that shares
    // an edge with a group inside it stands that much further out. Children come after their
    // parents, so walking backwards settles every group before the one that holds it.
    let mut group_labels = Vec::with_capacity(g_count);
    let mut c0 = vec![0; g_count];
    let mut c1 = vec![0; g_count];
    let mut r0 = vec![0; g_count];
    let mut r1 = vec![0; g_count];
    let mut ins_l = vec![0.0; g_count];
    let mut ins_r = vec![0.0; g_count];
    let mut ins_t = vec![0.0; g_count];
    let mut ins_b = vec![0.0; g_count];
    let mut head_w = vec![0.0; g_count];
    let mut head_h = vec![0.0; g_count];
    let mut col_l = vec![0.0; cols];
    let mut col_r = vec![0.0; cols];
    let mut row_t = vec![0.0; rows];
    let mut row_b = vec![0.0; rows];
    let badge = round(size * 1.25);
    let group_wrap = if tight {
        max(80.0, title_room(o) / 2.0)
    } else {
        320.0
    };
    for (g, group) in groups.iter().enumerate() {
        c0[g] = col_at(grid.group_col[g]);
        c1[g] = col_at(grid.group_col[g] + grid.group_cols[g] - 1);
        r0[g] = row_at(grid.group_row[g]);
        r1[g] = row_at(grid.group_row[g] + grid.group_rows[g] - 1);
        let raw = group.title.as_deref().unwrap_or("");
        let label = layout_label(raw, true, &set, size, group_wrap, Face::default());
        let icon = if group.icon.is_some() { badge } else { 0.0 };
        head_w[g] = label.width
            + icon
            + if label.width > 0.0 && icon > 0.0 {
                6.0
            } else {
                0.0
            };
        head_h[g] = max(label.height, icon);
        group_labels.push(label);
    }
    for g in (0..g_count).rev() {
        let left = group_pad + ins_l[g];
        let mut right = group_pad + ins_r[g];
        let top = if head_h[g] > 0.0 {
            max(group_pad, head_h[g] + 12.0)
        } else {
            group_pad
        } + ins_t[g];
        let bottom = group_pad + ins_b[g];
        if head_w[g] > 0.0 {
            let mut inner = left + right;
            for c in c0[g]..=c1[g] {
                inner += col_w[c]
                    + if c < c1[g] {
                        col_r[c] + gap_x[c] + col_l[c + 1]
                    } else {
                        0.0
                    };
            }
            if head_w[g] + 20.0 > inner {
                right += (head_w[g] + 20.0 - inner).ceil();
            }
        }
        ins_l[g] = left;
        ins_r[g] = right;
        ins_t[g] = top;
        ins_b[g] = bottom;
        col_l[c0[g]] = max(col_l[c0[g]], left);
        col_r[c1[g]] = max(col_r[c1[g]], right);
        row_t[r0[g]] = max(row_t[r0[g]], top);
        row_b[r1[g]] = max(row_b[r1[g]], bottom);
        let Some(p) = grid.group_parent[g] else {
            continue;
        };
        if c0[p] == c0[g] {
            ins_l[p] = max(ins_l[p], left);
        }
        if c1[p] == c1[g] {
            ins_r[p] = max(ins_r[p], right);
        }
        if r0[p] == r0[g] {
            ins_t[p] = max(ins_t[p], top);
        }
        if r1[p] == r1[g] {
            ins_b[p] = max(ins_b[p], bottom);
        }
    }

    let mut xs = vec![0.0; cols];
    let mut ys = vec![0.0; rows];
    let mut inner_w = 0.0;
    for c in 0..cols {
        xs[c] = inner_w + col_l[c];
        inner_w = xs[c] + col_w[c] + col_r[c] + if c < cols - 1 { gap_x[c] } else { 0.0 };
    }
    let mut inner_h = 0.0;
    for r in 0..rows {
        ys[r] = inner_h + row_t[r];
        inner_h = ys[r] + row_h[r] + row_b[r] + if r < rows - 1 { gap_y[r] } else { 0.0 };
    }
    // What is drawn, which an edge that has to go around a node can push past the grid.
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (0.0, 0.0, inner_w, inner_h);

    let left = |g: usize| xs[c0[g]] - ins_l[g];
    let right = |g: usize| xs[c1[g]] + col_w[c1[g]] + ins_r[g];
    let top = |g: usize| ys[r0[g]] - ins_t[g];
    let bottom = |g: usize| ys[r1[g]] + row_h[r1[g]] + ins_b[g];
    let center_x = |i: usize| xs[node_col[i]] + col_w[node_col[i]] / 2.0;

    let port = |i: usize, side: usize, via_group: bool| {
        let dx = DX[side] as f64;
        let dy = DY[side] as f64;
        let cx = center_x(i);
        let y = ys[node_row[i]];
        let mut p = Port {
            x: cx,
            y: y + half,
            dx,
            dy,
            x0: cx - box_w[i] / 2.0,
            y0: y,
            x1: cx + box_w[i] / 2.0,
            y1: y + box_h[i],
            gap: 0.0,
            wire: false,
        };
        if nodes[i].junction {
            p.x0 = cx;
            p.x1 = cx;
            p.y0 = p.y;
            p.y1 = p.y;
            p.gap = DOT;
        } else if dx != 0.0 {
            p.x = cx + dx * half;
        } else {
            p.y = if dy < 0.0 {
                y
            } else {
                y + box_h[i] + if labels[i].height > 0.0 { 2.0 } else { 0.0 }
            };
        }
        let g = if via_group { grid.node_parent[i] } else { None };
        if let Some(g) = g {
            p.x0 = left(g);
            p.x1 = right(g);
            p.y0 = top(g);
            p.y1 = bottom(g);
            p.gap = 0.0;
            p.wire = nodes[i].junction;
            if dx != 0.0 {
                p.x = if dx < 0.0 { p.x0 } else { p.x1 };
            } else {
                p.y = if dy < 0.0 { p.y0 } else { p.y1 };
            }
        }
        p
    };

    let mut edge_items = Vec::new();
    let mut label_items = Vec::new();
    // Vertical runs of the edges, to keep group titles out from under them.
    let mut runs: Vec<f64> = Vec::new();
    let trim_length = MarkerType::ArrowPoint.trim();
    let edge_style = Style {
        stroke: Some(Paint::LINE),
        fill: Some(Paint::NONE),
        cap: Some(Cap::Round),
        join: Some(Join::Round),
        ..Style::default()
    };
    for (k, edge) in model.edges.iter().enumerate() {
        let (Some(a), Some(b)) = ends[k] else {
            continue;
        };
        let mut s = port(a, side_index(edge.lhs_dir), edge.lhs_group);
        let mut e = port(b, side_index(edge.rhs_dir), edge.rhs_group);
        if s.x == e.x && s.y == e.y && s.dx == e.dx {
            // An edge from a side back to the same side: part the two ends so it shows as a
            // loop.
            s.x -= 8.0 * s.dy.abs();
            s.y -= 8.0 * s.dx.abs();
            e.x += 8.0 * s.dy.abs();
            e.y += 8.0 * s.dx.abs();
        }
        let mut routed = route(&s, &e);
        if s.wire {
            routed.splice(0..0, [center_x(a), ys[node_row[a]] + half]);
        }
        if e.wire {
            routed.extend([center_x(b), ys[node_row[b]] + half]);
        }
        let points = simplify(&routed);
        if points.len() < 4 {
            continue;
        }
        let id = format!("{}-{}", edge.lhs_id, edge.rhs_id);
        let start_trim = if edge.lhs_into && !s.wire {
            trim_length + s.gap
        } else {
            0.0
        };
        let end_trim = if edge.rhs_into && !e.wire {
            trim_length + e.gap
        } else {
            0.0
        };
        let path = path_data(&points, start_trim, end_trim);
        let head = |p: &Port| Marker {
            kind: MarkerType::ArrowPoint,
            at: Point::new(p.x + p.dx * p.gap, p.y + p.dy * p.gap),
            dx: -p.dx,
            dy: -p.dy,
        };
        let route: Vec<Point> = points
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[x, y]| Point::new(x, y))
            .collect();
        let mut it = Item::new(
            Kind::Edge(Edge {
                route,
                ends: [
                    edge.lhs_into.then(|| head(&s)),
                    edge.rhs_into.then(|| head(&e)),
                ],
            }),
            ROLE_EDGE,
        );
        it.id = Some(id.clone());
        it.bounds = path.bounds();
        it.parts = vec![Part::path(path, Painting::StrokeOnly)];
        it.style = edge_style.clone();
        edge_items.push(it);

        let (mut best, mut best_length, mut flat, mut flat_length) = (0, -1.0, 0, -1.0);
        let label = &edge_labels[k];
        for &[x, y] in points.as_chunks::<2>().0 {
            min_x = min(min_x, x - 4.0);
            max_x = max(max_x, x + 4.0);
            min_y = min(min_y, y - 4.0);
            max_y = max(max_y, y + 4.0);
        }
        let mut i = 0;
        while i + 3 < points.len() {
            let horizontal = (points[i + 1] - points[i + 3]).abs() < 0.01;
            let length = (points[i] - points[i + 2]).abs() + (points[i + 1] - points[i + 3]).abs();
            if !horizontal {
                runs.extend([
                    points[i],
                    min(points[i + 1], points[i + 3]),
                    max(points[i + 1], points[i + 3]),
                ]);
            }
            if length > best_length {
                best = i;
                best_length = length;
            }
            if horizontal && length > flat_length {
                flat = i;
                flat_length = length;
            }
            i += 2;
        }
        if label.width > 0.0 {
            let at = if flat_length >= label.width + 8.0 {
                flat
            } else {
                best
            };
            let mut x = (points[at] + points[at + 2]) / 2.0;
            let mut y = (points[at + 1] + points[at + 3]) / 2.0;
            let w = label.width + 8.0;
            let upright = (points[at] - points[at + 2]).abs() < 0.01;
            // On a short run next to a node the label would land on the node, so it moves off
            // to the side.
            for p in [&s, &e] {
                if x + w / 2.0 <= p.x0
                    || x - w / 2.0 >= p.x1
                    || y + label.height / 2.0 <= p.y0
                    || y - label.height / 2.0 >= p.y1
                {
                    continue;
                }
                if upright {
                    x = if x >= (p.x0 + p.x1) / 2.0 {
                        p.x1 + w / 2.0 + 4.0
                    } else {
                        p.x0 - w / 2.0 - 4.0
                    };
                } else {
                    y = if y >= (p.y0 + p.y1) / 2.0 {
                        p.y1 + label.height / 2.0 + 4.0
                    } else {
                        p.y0 - label.height / 2.0 - 4.0
                    };
                }
            }
            min_x = min(min_x, x - w / 2.0);
            max_x = max(max_x, x + w / 2.0);
            min_y = min(min_y, y - label.height / 2.0);
            max_y = max(max_y, y + label.height / 2.0);
            let mut it = Item::new(Kind::Text, ROLE_EDGE_LABEL);
            it.id = Some(id);
            it.bounds = label_patch_box(label.width, label.height, x, y);
            it.parts = vec![label_patch(label.width, label.height, x, y)];
            it.label = Some(Placed::new(label.clone(), Point::new(x, y)));
            it.style = Style {
                fill: Some(Paint::BG),
                stroke: Some(Paint::NONE),
                text: Some(Paint::FG),
                ..Style::default()
            };
            label_items.push(it);
        }
    }

    let mut group_items = Vec::new();
    for (g, group) in groups.iter().enumerate() {
        let x = left(g);
        let y = top(g);
        let w = right(g) - x;
        let r = Rect::new(x, y, w, bottom(g) - y);
        let mut it = Item::new(Kind::Group, ROLE_GROUP);
        it.shape = Some(Shape::new("rect"));
        it.id = Some(group.id.clone());
        it.bounds = r;
        it.parts = vec![Part::rect(r, true, Painting::FillAndStroke)];
        it.style = Style {
            fill: Some(Paint::SURFACE_ALT),
            fill_opacity: Some(0.5),
            stroke: Some(Paint::BORDER),
            text: Some(Paint::MUTED),
            ..Style::default()
        };
        let mut icon = None;
        if head_w[g] > 0.0 {
            // The title starts at the left, or after the edges that come down through that
            // corner.
            let band = y + head_h[g] + 12.0;
            let mut crossings: Vec<f64> = runs
                .as_chunks::<3>()
                .0
                .iter()
                .filter(|&&[rx, lo, hi]| lo < band && hi > y && rx > x && rx < x + w)
                .map(|run| run[0])
                .collect();
            crossings.sort_by(|a, b| by(a - b));
            crossings.push(x + w - 4.0);
            let mut start = x + 10.0;
            for crossing in crossings {
                if crossing - 6.0 - start >= head_w[g] {
                    break;
                }
                start = max(start, crossing + 6.0);
            }
            if start + head_w[g] > x + w - 4.0 {
                start = x + 10.0;
            }
            let cy = y + 6.0 + head_h[g] / 2.0;
            let mut text_x = start;
            if let Some(name) = &group.icon {
                icon = icon_slot(&mut it, name, start, cy - badge / 2.0, badge, o);
                text_x += badge + 6.0;
            }
            let label = &group_labels[g];
            if !label.is_empty() {
                let at = Point::new(text_x + label.width / 2.0, cy);
                it.label = Some(Placed::new(label.clone(), at));
            }
        }
        group_items.push(it);
        group_items.extend(icon);
    }

    let mut node_items = Vec::new();
    let glyph = icon_size * 0.5;
    let text_size = max(6.0, round(size * 0.75));
    for (i, node) in nodes.iter().enumerate() {
        let (cx, cy) = (center_x(i), ys[node_row[i]]);
        if node.junction {
            let mut it = Item::new(Kind::Node, ROLE_JUNCTION);
            it.shape = Some(Shape::new("circle"));
            it.id = Some(node.id.clone());
            it.bounds = Rect::new(cx - DOT, cy + half - DOT, 2.0 * DOT, 2.0 * DOT);
            let mut dot = Path::new();
            dot.ellipse(cx, cy + half, DOT, DOT);
            it.parts = vec![Part::path(dot, Painting::FillOnly)];
            it.style.fill = Some(Paint::LINE);
            node_items.push(it);
            continue;
        }
        let r = Rect::new(cx - half, cy, icon_size, icon_size);
        let mut it = Item::new(Kind::Node, ROLE_SERVICE);
        it.shape = Some(Shape::new("rect"));
        it.id = Some(node.id.clone());
        it.bounds = r;
        it.parts = vec![Part::rect(r, true, Painting::FillAndStroke)];
        it.style = Style {
            fill: Some(Paint::SURFACE),
            stroke: Some(Paint::BORDER),
            text: Some(Paint::FG),
            ..Style::default()
        };
        let label = &labels[i];
        if !label.is_empty() {
            let at = Point::new(cx, cy + (icon_size + LABEL_GAP + label.height / 2.0));
            it.label = Some(Placed::new(label.clone(), at));
        }
        let mut inner = None;
        if let Some(name) = &node.icon {
            let (x, y) = (cx - glyph / 2.0, cy + (icon_size - glyph) / 2.0);
            inner = icon_slot(&mut it, name, x, y, glyph, o);
        } else if let Some(text) = node.icon_text.as_deref().filter(|t| !t.is_empty()) {
            let label = layout_label(
                text,
                false,
                &set,
                text_size,
                icon_size - 8.0,
                Face::default(),
            );
            let line_height = if label.line_height == 0.0 {
                1.0
            } else {
                label.line_height
            };
            let lines = max(1.0, ((icon_size - 4.0) / line_height).floor()) as usize;
            let label = fit(label, icon_size - 8.0, lines, &set);
            if !label.is_empty() {
                let mut text = Item::new(Kind::Text, ROLE_ICON_TEXT);
                text.id = Some(node.id.clone());
                text.label = Some(Placed::new(label, Point::new(cx, cy + half)));
                text.style.text = Some(Paint::FG);
                inner = Some(text);
            }
        }
        node_items.push(it);
        node_items.extend(inner);
    }

    let room = if tight { title_room(o) } else { 4000.0 };
    let raw = model.title.as_deref().unwrap_or("");
    let title = layout_label(raw, false, &set, size, room, Face::BOLD);
    let title_height = if title.height > 0.0 {
        title.height + 12.0
    } else {
        0.0
    };
    let width = (max(max_x - min_x, title.width) + 2.0 * pad).ceil();
    let height = (max_y - min_y + title_height + 2.0 * pad).ceil();
    let ox = round((width - (max_x - min_x)) / 2.0 - min_x);
    let oy = round(pad + title_height - min_y);

    let mut items = Vec::new();
    if !title.is_empty() {
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        let at = Point::new(width / 2.0, pad + title.height / 2.0);
        it.label = Some(Placed::new(title, at));
        it.style.text = Some(Paint::FG);
        items.push(it);
    }
    let body = items.len();
    items.extend(group_items);
    items.extend(edge_items);
    items.extend(label_items);
    items.extend(node_items);
    for it in &mut items[body..] {
        crate::items::move_by(it, ox, oy);
    }
    Scene {
        diagram: "architecture".into(),
        width,
        height,
        font_size: size,
        title: model.acc_title.clone(),
        desc: model.acc_descr.clone(),
        items,
    }
}
