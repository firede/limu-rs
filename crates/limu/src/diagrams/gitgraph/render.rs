//! A git graph laid out as a scene, as Pele's renderGit in render.ts draws it.

use std::collections::HashMap;

use super::GitModel;
use super::db::{CHERRY_PICK, GitCommit, HIGHLIGHT, MERGE, REVERSE};
use crate::common::{plain_label, setting, title_label, turn_to_fit};
use crate::js::math::{by, max, min, round};
use crate::layout::Options;
use crate::scene::style::{Cap, Join, Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{Edge, Item, Kind, Painting, Part, Path, Placed, Point, Rect, Scene, Shape};
use crate::text::decode_entities;
use crate::value::{Map, Value};

/// The roles of the items of a git graph: its title, a branch's dashed lane and its name, a
/// connector between commits, a commit, a merge, the mark drawn over a commit's dot, a
/// commit's id, a tag and the hole in it.
pub(crate) const ROLE_TITLE: &str = "gitgraph.title";
pub(crate) const ROLE_LANE: &str = "gitgraph.lane";
pub(crate) const ROLE_BRANCH: &str = "gitgraph.branch";
pub(crate) const ROLE_EDGE: &str = "gitgraph.edge";
pub(crate) const ROLE_COMMIT: &str = "gitgraph.commit";
pub(crate) const ROLE_MERGE: &str = "gitgraph.merge";
pub(crate) const ROLE_COMMIT_MARK: &str = "gitgraph.commit-mark";
pub(crate) const ROLE_COMMIT_LABEL: &str = "gitgraph.commit-label";
pub(crate) const ROLE_TAG: &str = "gitgraph.tag";
pub(crate) const ROLE_TAG_HOLE: &str = "gitgraph.tag-hole";

// The measures of Pele's git graph render.
const DOT: f64 = 7.0;
const STEP: f64 = 44.0;
const LANE: f64 = 44.0;
// Shorter than a step, so a curve between lanes never runs through a commit on a lane it
// crosses.
const CURVE: f64 = 36.0;
const GAP: f64 = 8.0;
const LABEL_GAP: f64 = 20.0;
const TAIL: f64 = 20.0;
const NOTCH: f64 = 7.0;
const TAG_GAP: f64 = 6.0;
const HALF: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// The names of the types of commit, as Pele's CLASSES gives them.
const CLASSES: [&str; 5] = ["normal", "reverse", "highlight", "merge", "cherry-pick"];

/// A commit as it is laid out, Pele's View.
struct View<'a> {
    commit: &'a GitCommit,
    index: usize,
    lane: usize,
    m: f64,
    label: Option<Label>,
    tags: Vec<Label>,
}

/// A setting of the diagram's configuration that is a boolean, or fallback, as Pele's flag
/// reads it.
fn flag(config: &Map, key: &str, fallback: bool) -> bool {
    let git = config.get("gitGraph").and_then(Value::as_object);
    match git.and_then(|g| g.get(key)) {
        Some(&Value::Bool(b)) => b,
        _ => fallback,
    }
}

fn tag_width(tag: &Label) -> f64 {
    NOTCH + 4.0 + tag.width + 6.0
}

/// Lays a git graph out as a scene, as Pele's renderGit draws it: commits in the order they
/// were made along the main axis, each on the lane of its branch, joined to their parents. A
/// graph that runs left to right turns to run down where the host's width calls for it.
pub(crate) fn layout(model: &GitModel, config: &Map, o: &Options) -> Scene {
    if model.direction != "LR" {
        return draw(model, config, o, model.direction);
    }
    turn_to_fit(o, |down| {
        draw(model, config, o, if down { "TB" } else { "LR" })
    })
}

/// A commit's label turned about a point, to be placed once the graph is moved to where it
/// is on the page.
struct Turned {
    item: usize,
    angle: f64,
    // The point it turns about.
    ax: f64,
    ay: f64,
    // The middle of the label, across, before it turns.
    cx: f64,
    back: Rect,
}

/// Pele's draw.
fn draw(model: &GitModel, config: &Map, o: &Options, direction: &str) -> Scene {
    let size = o.font_size;
    let small = round(size * 0.75);
    let set = setting(o);
    let pad = o.pad();

    let show_branches = flag(config, "showBranches", true);
    let show_commit_label = flag(config, "showCommitLabel", true);
    let rotate = flag(config, "rotateCommitLabel", true);
    let parallel = flag(config, "parallelCommits", false);
    let vertical = direction != "LR";
    let flip = direction == "BT";

    let lane_of: HashMap<&str, usize> = model
        .lanes
        .iter()
        .enumerate()
        .map(|(i, name)| (name.as_str(), i))
        .collect();
    let lane_count = model.lanes.len();
    let medium = round(size * 0.875);
    // Mermaid writes branch names, tags, and commit ids as plain text, so tags in them show
    // as written.
    let names: Vec<Label> = model
        .lanes
        .iter()
        .map(|name| {
            let name = if show_branches { name.as_str() } else { "" };
            plain_label(name, &set, medium, 4000.0, Face::REGULAR)
        })
        .collect();

    let mut commits: Vec<&GitCommit> = model.commits.values().collect();
    commits.sort_by_key(|c| c.seq);
    let mut views: Vec<View> = Vec::with_capacity(commits.len());
    let mut by_id: HashMap<&str, usize> = HashMap::new();
    for commit in commits {
        let labelled = show_commit_label
            && commit.kind != CHERRY_PICK
            && (commit.kind != MERGE || commit.custom_id == Some(true));
        let tags = commit
            .tags
            .iter()
            .map(|tag| plain_label(tag, &set, small, 4000.0, Face::REGULAR))
            .filter(|l| !l.lines.is_empty())
            .collect();
        let view = View {
            commit,
            index: views.len(),
            lane: lane_of.get(commit.branch.as_str()).copied().unwrap_or(0),
            m: 0.0,
            label: labelled.then(|| plain_label(&commit.id, &set, small, 4000.0, Face::REGULAR)),
            tags,
        };
        by_id.insert(&commit.id, views.len());
        views.push(view);
    }

    // Room each commit needs: along the main axis on either side, and across it towards the
    // previous lane (lo) and the next one (hi). Tags sit on one side of a lane and ids on the
    // other.
    let mut half = vec![0.0; views.len()];
    let mut tag_half = vec![0.0; views.len()];
    let mut lo = vec![DOT; lane_count];
    let mut hi = vec![DOT; lane_count];
    for (k, view) in views.iter().enumerate() {
        let (mut tags_main, mut tags_cross) = (0.0, 0.0);
        for tag in &view.tags {
            let w = tag_width(tag);
            if vertical {
                tags_main = max(tags_main, tag.height);
                tags_cross += w + 4.0;
            } else {
                tags_main = max(tags_main, w);
                tags_cross += tag.height + 3.0;
            }
        }
        let (mut label_main, mut label_cross) = (0.0, 0.0);
        if let Some(label) = view.label.as_ref().filter(|l| !l.lines.is_empty()) {
            if rotate {
                label_cross = 3.0 + HALF * (label.width + label.height);
            } else {
                label_main = if vertical { label.height } else { label.width };
                label_cross = 6.0 + if vertical { label.width } else { label.height };
            }
        }
        half[k] = max(max(DOT, tags_main / 2.0), label_main / 2.0);
        tag_half[k] = tags_main / 2.0;
        let tag_side = DOT
            + if view.tags.is_empty() {
                0.0
            } else {
                TAG_GAP + tags_cross
            };
        let label_side = DOT + label_cross;
        let lane = view.lane;
        lo[lane] = max(lo[lane], if vertical { label_side } else { tag_side });
        hi[lane] = max(hi[lane], if vertical { tag_side } else { label_side });
    }

    // In a left-to-right graph the tags stand over their commit. A connector that comes down
    // from a lane above joins the commit's lane this far before the commit, clear of the
    // tags, and the commit is placed far enough along to leave room for that.
    let mut lead = vec![0.0; views.len()];
    if !vertical {
        for (k, view) in views.iter().enumerate() {
            if tag_half[k] == 0.0 {
                continue;
            }
            for id in &view.commit.parents {
                if let Some(&p) = by_id.get(id.as_str())
                    && views[p].lane < view.lane
                {
                    lead[k] = tag_half[k] + 4.0;
                }
            }
        }
    }
    let space = |p: usize, k: usize| -> f64 {
        let third = if lead[k] > 0.0 {
            half[p] + CURVE + lead[k]
        } else {
            0.0
        };
        max(max(STEP, half[p] + half[k] + GAP), third)
    };

    for k in 0..views.len() {
        if !parallel {
            views[k].m = if k == 0 {
                0.0
            } else {
                views[k - 1].m + space(k - 1, k)
            };
            continue;
        }
        // Mermaid's parallelCommits: a commit sits one step after its furthest parent, so
        // branches that grow side by side share positions instead of taking turns.
        let mut m = 0.0;
        for id in &views[k].commit.parents {
            let Some(&p) = by_id.get(id.as_str()) else {
                continue;
            };
            if views[p].index >= k {
                continue;
            }
            m = max(m, views[p].m + space(views[p].index, k));
        }
        views[k].m = m;
    }
    let mut end = 0.0;
    for view in &views {
        end = max(end, view.m);
    }

    let mut cross = vec![0.0; lane_count];
    for i in 0..lane_count {
        let name = &names[i];
        let reach = if name.lines.is_empty() {
            0.0
        } else if vertical {
            name.width / 2.0 + 8.0
        } else {
            name.height / 2.0 + 2.0
        };
        lo[i] = max(lo[i], reach);
        hi[i] = max(hi[i], reach);
        if i > 0 {
            cross[i] = cross[i - 1] + max(LANE, hi[i - 1] + lo[i] + GAP);
        }
    }

    let px = |m: f64, c: f64| if vertical { c } else { m };
    let py = |m: f64, c: f64| {
        if vertical {
            if flip { -m } else { m }
        } else {
            c
        }
    };

    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut grow = |x0: f64, y0: f64, x1: f64, y1: f64| {
        if x0 < min_x {
            min_x = x0;
        }
        if y0 < min_y {
            min_y = y0;
        }
        if x1 > max_x {
            max_x = x1;
        }
        if y1 > max_y {
            max_y = y1;
        }
    };

    let mut items: Vec<Item> = Vec::new();
    if show_branches {
        for i in 0..lane_count {
            let name = &names[i];
            let color = Paint::series(i);
            let c = cross[i];
            let w = name.width + 16.0;
            let h = name.height + 4.0;
            let along = if vertical { h } else { w };
            let from = if name.lines.is_empty() {
                -LABEL_GAP
            } else {
                -LABEL_GAP - along
            };
            let cx = px(from + along / 2.0, c);
            let cy = py(from + along / 2.0, c);
            let (x0, y0) = (px(-LABEL_GAP, c), py(-LABEL_GAP, c));
            let (x1, y1) = (px(end + TAIL, c), py(end + TAIL, c));
            grow(x0, y0, x0, y0);
            grow(x1, y1, x1, y1);
            let mut path = Path::new();
            path.move_to(x0, y0).line_to(x1, y1);
            let mut lane = Item::new(Kind::Mark, ROLE_LANE);
            lane.id = Some(model.lanes[i].clone());
            lane.bounds = path.bounds();
            lane.parts = vec![Part::path(path, Painting::StrokeOnly)];
            lane.style = Style {
                stroke: Some(color),
                dash: Some(vec![2.0, 4.0]),
                ..Style::default()
            };
            items.push(lane);
            if !name.lines.is_empty() {
                grow(cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0);
                let rect = Rect::new(cx - w / 2.0, cy - h / 2.0, w, h);
                let mut branch = Item::new(Kind::Node, ROLE_BRANCH);
                branch.id = Some(model.lanes[i].clone());
                branch.bounds = rect;
                branch.parts = vec![Part::rect(rect, true, Painting::FillOnly)];
                branch.label = Some(Placed::new(name.clone(), Point::new(cx, cy)));
                branch.style = Style {
                    fill: Some(color),
                    stroke: Some(Paint::NONE),
                    text: Some(Paint::BG),
                    ..Style::default()
                };
                items.push(branch);
            }
        }
    }

    // Positions of the commits on each lane, to tell whether a connector may run along it.
    let mut taken: Vec<Vec<f64>> = vec![Vec::new(); lane_count];
    for view in &views {
        taken[view.lane].push(view.m);
    }
    if parallel {
        for list in &mut taken {
            list.sort_by(|a, b| by(a - b));
        }
    }
    let busy = |lane: usize, from: f64, to: f64| -> bool {
        let list = &taken[lane];
        let (mut low, mut high) = (0, list.len());
        while low < high {
            let mid = (low + high) >> 1;
            if list[mid] <= from {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        low < list.len() && list[low] < to
    };

    let mut curves = Vec::new();
    let mut lines = Vec::new();
    for view in &views {
        let c2 = cross[view.lane];
        let m2 = view.m;
        for (j, id) in view.commit.parents.iter().enumerate() {
            let Some(&p) = by_id.get(id.as_str()) else {
                continue;
            };
            if p == view.index {
                continue;
            }
            let parent = &views[p];
            let c1 = cross[parent.lane];
            let m1 = parent.m;
            let mut e = EdgePath::new(&px, &py);
            e.move_to(m1, c1);
            if parent.lane == view.lane {
                e.line_to(m2, c2);
                lines.push(e.item(None, Paint::series(view.lane), false));
                continue;
            }
            let merged = j > 0 && view.commit.kind == MERGE;
            let picked = j > 0 && !merged;
            if m2 <= m1 {
                let mid = (m1 + m2) / 2.0;
                e.cube_to(mid, c1, mid, c2, m2, c2);
            } else if !merged && !busy(view.lane, m1, m2) {
                // Leaves the parent at once and runs along the child's lane.
                let s = min(CURVE, m2 - m1);
                e.cube_to(m1 + s / 2.0, c1, m1 + s / 2.0, c2, m1 + s, c2);
                e.line_to(m2, c2);
            } else {
                // Turns in just before the child, or before its tags when it comes from
                // their side.
                let to = if parent.lane < view.lane && m2 - lead[view.index] > m1 {
                    m2 - lead[view.index]
                } else {
                    m2
                };
                let mut s = min(CURVE, to - m1);
                let mut along = c1;
                if !merged || busy(parent.lane, m1, m2) {
                    // Neither lane is free to run along. Between two lanes, past the ids of
                    // one and before the tags of the next, a strip stays empty, and the
                    // connector takes that.
                    along = if c1 > c2 {
                        c2 + hi[view.lane] + GAP / 2.0
                    } else {
                        c2 - lo[view.lane] - GAP / 2.0
                    };
                    s = min(CURVE, (to - m1) / 2.0);
                    e.cube_to(m1 + s / 2.0, c1, m1 + s / 2.0, along, m1 + s, along);
                }
                e.line_to(to - s, along);
                e.cube_to(to - s / 2.0, along, to - s / 2.0, c2, to, c2);
                if to < m2 {
                    e.line_to(m2, c2);
                }
            }
            let (shape, color) = if merged {
                (Some("merge"), Paint::series(parent.lane))
            } else if picked {
                (Some("cherry-pick"), Paint::series(view.lane))
            } else {
                (None, Paint::series(view.lane))
            };
            curves.push(e.item(shape, color, picked));
        }
    }
    items.extend(curves);
    items.extend(lines);

    let mut turns: Vec<Turned> = Vec::new();
    for view in &views {
        let commit = view.commit;
        let x = px(view.m, cross[view.lane]);
        let y = py(view.m, cross[view.lane]);
        let kind = commit.custom_type.unwrap_or(commit.kind);
        grow(x - DOT - 1.0, y - DOT - 1.0, x + DOT + 1.0, y + DOT + 1.0);
        items.extend(symbol(commit, kind, x, y, Paint::series(view.lane)));

        if let Some(label) = view.label.as_ref().filter(|l| !l.lines.is_empty()) {
            let (w, h) = (label.width, label.height);
            let back = |cx: f64, cy: f64| {
                Rect::new(cx - w / 2.0 - 2.0, cy - h / 2.0 + 2.0, w + 4.0, h - 4.0)
            };
            let mut it = Item::new(Kind::Text, ROLE_COMMIT_LABEL);
            it.id = Some(commit.id.clone());
            it.style = Style {
                fill: Some(Paint::BG),
                stroke: Some(Paint::NONE),
                text: Some(Paint::MUTED),
                radius: Some(3.0),
                ..Style::default()
            };
            if rotate {
                // The text ends beside the commit and trails back the way the history came.
                let k = HALF * h / 2.0;
                let (ax, ay) = if vertical {
                    (x - DOT - 3.0 - k, y)
                } else {
                    (x + 3.0, y + DOT + 3.0 + k)
                };
                let reach = HALF * w + k;
                if flip {
                    grow(ax - reach, ay - reach, ax + k, ay + k);
                } else {
                    grow(ax - reach, ay - k, ax + k, ay + reach);
                }
                let angle = if flip { 45.0 } else { -45.0 };
                let r = back(ax - w / 2.0, ay);
                turns.push(Turned {
                    item: items.len(),
                    angle,
                    ax,
                    ay,
                    cx: ax - w / 2.0,
                    back: r,
                });
                it.bounds = r;
                it.label = Some(Placed {
                    rotate: angle,
                    ..Placed::new(label.clone(), Point::new(ax - w / 2.0, ay))
                });
            } else {
                let (cx, cy) = if vertical {
                    (x - DOT - 6.0 - w / 2.0, y)
                } else {
                    (x, y + DOT + 6.0 + h / 2.0)
                };
                grow(cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0);
                let r = back(cx, cy);
                it.bounds = r;
                it.parts = vec![Part::rect(r, true, Painting::FillOnly)];
                it.label = Some(Placed::new(label.clone(), Point::new(cx, cy)));
            }
            items.push(it);
        }

        // Beside the lane the tags form a row. Above it they stack, the first one furthest
        // out. Pele turns a tag's corners round.
        let mut offset = DOT + TAG_GAP;
        let order: Vec<usize> = if vertical {
            (0..view.tags.len()).collect()
        } else {
            (0..view.tags.len()).rev().collect()
        };
        for t in order {
            let tag = &view.tags[t];
            let w = tag_width(tag);
            let h = tag.height;
            let (left, cy) = if vertical {
                (x + offset, y)
            } else {
                (x - w / 2.0, y - offset - h / 2.0)
            };
            offset += if vertical { w + 4.0 } else { h + 3.0 };
            grow(left, cy - h / 2.0, left + w, cy + h / 2.0);
            let mut path = Path::new();
            path.move_to(left, cy)
                .line_to(left + NOTCH, cy - h / 2.0)
                .line_to(left + w, cy - h / 2.0)
                .line_to(left + w, cy + h / 2.0)
                .line_to(left + NOTCH, cy + h / 2.0)
                .close();
            let mut it = Item::new(Kind::Node, ROLE_TAG);
            it.id = Some(commit.id.clone());
            it.bounds = Rect::new(left, cy - h / 2.0, w, h);
            it.parts = vec![Part::path(path, Painting::FillAndStroke)];
            it.label = Some(Placed::new(
                tag.clone(),
                Point::new(left + NOTCH + 4.0 + tag.width / 2.0, cy),
            ));
            it.style = Style {
                fill: Some(Paint::SURFACE),
                stroke: Some(Paint::BORDER),
                text: Some(Paint::FG),
                join: Some(Join::Round),
                ..Style::default()
            };
            items.push(it);
            let mut hole = Item::new(Kind::Mark, ROLE_TAG_HOLE);
            hole.id = Some(commit.id.clone());
            hole.bounds = Rect::new(left + NOTCH - 1.5, cy - 1.5, 3.0, 3.0);
            hole.parts = vec![Part::path(
                circle(left + NOTCH, cy, 1.5),
                Painting::FillOnly,
            )];
            hole.style = Style {
                fill: Some(Paint::BORDER),
                stroke: Some(Paint::NONE),
                ..Style::default()
            };
            items.push(hole);
        }
    }

    if min_x > max_x {
        (min_x, min_y, max_x, max_y) = (0.0, 0.0, 0.0, 0.0);
    }
    let title = title_label(model.title.as_deref(), &set, size, 4000.0);
    let title_height = if title.height > 0.0 {
        title.height + 16.0
    } else {
        0.0
    };
    let inner = max(max_x - min_x, title.width);
    let width = (inner + 2.0 * pad).ceil();
    let height = (max_y - min_y + title_height + 2.0 * pad).ceil();
    let dx = pad - min_x + (inner - (max_x - min_x)) / 2.0;
    let dy = pad + title_height - min_y;

    for it in &mut items {
        crate::items::move_by(it, dx, dy);
    }
    for t in &turns {
        place(&mut items[t.item], t, dx, dy);
    }
    let mut sc = Scene {
        diagram: "gitGraph".into(),
        width,
        height,
        font_size: size,
        title: model.acc_title.clone(),
        desc: model.acc_descr.clone(),
        items: Vec::with_capacity(items.len() + 1),
    };
    if !title.lines.is_empty() {
        let at = Point::new(width / 2.0, pad + title.height / 2.0);
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        it.label = Some(Placed::new(title, at));
        it.style = Style {
            text: Some(Paint::FG),
            ..Style::default()
        };
        sc.items.push(it);
    }
    sc.items.extend(items);
    sc
}

/// Turns a commit's label about its point, once the graph has been moved by dx, dy to where
/// it is on the page: the label's middle, and the box behind it, which becomes a path.
fn place(it: &mut Item, t: &Turned, dx: f64, dy: f64) {
    let (ax, ay) = (t.ax + dx, t.ay + dy);
    let s = if t.angle < 0.0 { -HALF } else { HALF };
    let turn = |x: f64, y: f64| {
        let (vx, vy) = (x - ax, y - ay);
        (ax + vx * HALF - vy * s, ay + vx * s + vy * HALF)
    };
    if let Some(p) = &mut it.label {
        let (x, y) = turn(t.cx + dx, ay);
        p.at = Point::new(x, y);
    }
    let r = t.back.moved(dx, dy);
    let mut path = Path::new();
    path.round_rect(r, 3.0);
    path.map_points(|p| {
        let (x, y) = turn(p.x, p.y);
        Point::new(x, y)
    });
    it.bounds = path.bounds();
    it.parts = vec![Part::path(path, Painting::FillOnly)];
}

/// A circle about x, y.
fn circle(x: f64, y: f64, r: f64) -> Path {
    let mut p = Path::new();
    p.ellipse(x, y, r, r);
    p
}

/// The path of a connector, built by points along and across the lanes.
struct EdgePath<'a> {
    px: &'a dyn Fn(f64, f64) -> f64,
    py: &'a dyn Fn(f64, f64) -> f64,
    path: Path,
    route: Vec<Point>,
}

impl<'a> EdgePath<'a> {
    fn new(px: &'a dyn Fn(f64, f64) -> f64, py: &'a dyn Fn(f64, f64) -> f64) -> EdgePath<'a> {
        EdgePath {
            px,
            py,
            path: Path::new(),
            route: Vec::new(),
        }
    }

    fn point(&self, m: f64, c: f64) -> (f64, f64) {
        ((self.px)(m, c), (self.py)(m, c))
    }

    fn move_to(&mut self, m: f64, c: f64) {
        let (x, y) = self.point(m, c);
        self.path.move_to(x, y);
        self.route.push(Point::new(x, y));
    }

    fn line_to(&mut self, m: f64, c: f64) {
        let (x, y) = self.point(m, c);
        self.path.line_to(x, y);
        self.route.push(Point::new(x, y));
    }

    #[allow(clippy::too_many_arguments)]
    fn cube_to(&mut self, m1: f64, c1: f64, m2: f64, c2: f64, m: f64, c: f64) {
        let (x1, y1) = self.point(m1, c1);
        let (x2, y2) = self.point(m2, c2);
        let (x, y) = self.point(m, c);
        self.path.cube_to(x1, y1, x2, y2, x, y);
        self.route.push(Point::new(x, y));
    }

    /// The connector as an edge of the scene: its shape is "merge" for the line that brings
    /// in a merged branch, "cherry-pick" for the one from a picked commit, which is dashed,
    /// and none for others. Its ends are round, as Pele's group of edges makes them.
    fn item(self, shape: Option<&'static str>, color: Paint, dashed: bool) -> Item {
        let mut it = Item::new(
            Kind::Edge(Edge {
                route: self.route,
                ends: [None, None],
            }),
            ROLE_EDGE,
        );
        it.shape = shape.map(Shape::new);
        it.bounds = self.path.bounds();
        it.parts = vec![Part::path(self.path, Painting::StrokeOnly)];
        it.style = Style {
            stroke: Some(color),
            stroke_width: Some(2.0),
            cap: Some(Cap::Round),
            dash: dashed.then(|| vec![4.0, 3.0]),
            ..Style::default()
        };
        it
    }
}

/// A commit drawn as its type, Pele's symbol: a dot, a dot crossed out for a reverse, a
/// square for a highlight, a ring about a dot for a merge, and a dot with a cherry for a
/// cherry-pick. The marks drawn over a dot, in the color of the background, are an item of
/// their own: a reverse's cross with round ends and a cherry-pick's stem with a round
/// corner, as Pele draws them. A merge's role is gitgraph.merge whatever type it is drawn
/// as.
fn symbol(c: &GitCommit, kind: u8, x: f64, y: f64, color: Paint) -> Vec<Item> {
    let role = if c.kind == MERGE {
        ROLE_MERGE
    } else {
        ROLE_COMMIT
    };
    let shape = CLASSES
        .get(usize::from(kind))
        .copied()
        .unwrap_or(CLASSES[0]);
    let mut it = Item::new(Kind::Node, role);
    it.shape = Some(Shape::new(shape));
    it.id = Some(c.id.clone());
    // Merges and cherry-picks carry a message Mermaid makes up. Only a written one is worth
    // showing.
    if c.kind < MERGE && !c.message.is_empty() {
        it.tooltip = Some(decode_entities(&c.message));
    }
    it.bounds = Rect::new(x - DOT, y - DOT, 2.0 * DOT, 2.0 * DOT);
    it.style = Style {
        fill: Some(color),
        stroke: Some(Paint::NONE),
        ..Style::default()
    };
    let dot = Part::path(circle(x, y, DOT), Painting::FillOnly);
    match kind {
        HIGHLIGHT => {
            let rect = Rect::new(
                x - DOT - 1.0,
                y - DOT - 1.0,
                2.0 * DOT + 2.0,
                2.0 * DOT + 2.0,
            );
            it.bounds = rect;
            it.parts = vec![Part::rect(rect, true, Painting::FillOnly)];
            it.style.radius = Some(2.0);
            vec![it]
        }
        MERGE => {
            it.style.stroke = Some(color);
            it.style.stroke_width = Some(2.0);
            it.parts = vec![
                Part::path(circle(x, y, DOT - 1.0), Painting::Hollow),
                Part::path(circle(x, y, 2.5), Painting::FillOnly),
            ];
            vec![it]
        }
        REVERSE => {
            it.parts = vec![dot];
            let mut cross = Path::new();
            cross
                .move_to(x - 3.0, y - 3.0)
                .line_to(x - 3.0 + 6.0, y - 3.0 + 6.0)
                .move_to(x - 3.0, y + 3.0)
                .line_to(x - 3.0 + 6.0, y + 3.0 - 6.0);
            let mut mark = Item::new(Kind::Mark, ROLE_COMMIT_MARK);
            mark.shape = Some(Shape::new("reverse"));
            mark.id = Some(c.id.clone());
            mark.bounds = cross.bounds();
            mark.parts = vec![Part::path(cross, Painting::StrokeOnly)];
            mark.style = Style {
                stroke: Some(Paint::BG),
                stroke_width: Some(1.5),
                cap: Some(Cap::Round),
                ..Style::default()
            };
            vec![it, mark]
        }
        CHERRY_PICK => {
            it.parts = vec![dot];
            let mut stem = Path::new();
            stem.move_to(x - 2.6, y + 1.0)
                .line_to(x + 0.8, y - 4.2)
                .line_to(x + 2.6, y + 1.0);
            let mut mark = Item::new(Kind::Mark, ROLE_COMMIT_MARK);
            mark.shape = Some(Shape::new("cherry-pick"));
            mark.id = Some(c.id.clone());
            mark.bounds = Rect::new(
                x - 2.6 - 1.8,
                y - 4.2,
                2.0 * 2.6 + 2.0 * 1.8,
                4.2 + 2.2 + 1.8,
            );
            mark.parts = vec![
                Part::path(stem, Painting::StrokeOnly),
                Part::path(circle(x - 2.6, y + 2.2, 1.8), Painting::FillOnly),
                Part::path(circle(x + 2.6, y + 2.2, 1.8), Painting::FillOnly),
            ];
            mark.style = Style {
                fill: Some(Paint::BG),
                stroke: Some(Paint::BG),
                join: Some(Join::Round),
                ..Style::default()
            };
            vec![it, mark]
        }
        _ => {
            it.parts = vec![dot];
            vec![it]
        }
    }
}
