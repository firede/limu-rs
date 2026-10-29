//! Draws limu's scenes as text in a grid of cells, for terminals.
//!
//! A diagram is laid out for cells by measuring its text with [`Cells`], which counts the
//! cells a terminal gives each character, setting it at the size whose lines are a row high,
//! and snapping it to the cells' grid:
//!
//! ```
//! let cells = limu_ascii::Cells::default();
//! let o = limu::layout::Options {
//!     measurer: &cells,
//!     font_size: cells.font_size(),
//!     snap: Some(cells.grid()),
//!     // A terminal of 80 columns. A diagram that can draw itself narrower does, turning to
//!     // run down where that is narrower, however wide the terminal is.
//!     max_width: 80.0 * cells.width,
//!     direction_breakpoint: f64::INFINITY,
//!     ..limu::layout::Options::default()
//! };
//! let scene = limu::layout("flowchart LR\n  a --> b", &o).unwrap();
//! print!("{}", limu_ascii::render(&scene, &limu_ascii::Options::default()).unwrap());
//! ```
//!
//! Lines are drawn in box-drawing characters, and where they meet in a cell they are joined
//! in one, as "┬". An edge is drawn in lines across and down. A node with a label whose
//! outline curves or slants, as a stadium or a diamond, is drawn as the box it lies in, its
//! corners rounded where it curves, and its sides pointed where it comes to a point at
//! either side. A shape too small to outline in cells is drawn as a dot, or as an arrowhead
//! where it is a triangle, and a marker as an arrow or a mark like it. The ends of an entity
//! relationship are written as how many, as "0..*". A grid of cells has no colors, fills or
//! images, so a fill only hides what it covers, where it can't be seen through, and a
//! picture in a node, as an icon, is left out.
//!
//! Labels are set last, a line to a row, and every line of every label is set whole. Text
//! breaks lines down where it must, but never lines across or other text. It moves a row
//! for room, and where there is none, a row is put in for it. A label turned 30 degrees or
//! more, as an axis's title, is set across.
//!
//! A diagram is as wide as its layout makes it. Most diagrams fit the width a layout is
//! given, but some can't be drawn narrower, and nothing is cut off or wrapped to fit.
//!
//! Some diagrams say what they say in where things are and how large they are, as a pie's
//! slices, a chart's bars and a Gantt chart's tasks do, or in lines too close together for
//! cells to tell apart, as a git graph's branches. Drawn in cells, they would mislead, and
//! [`render`] gives [`Unsupported`] for them rather than draw them.

mod grid;
#[cfg(test)]
mod tests;

use std::collections::HashSet;

use limu_scene::style::{Color, Ink, Look, Resolved};
use limu_scene::text::Measurer;
use limu_scene::{
    Align, Canvas, Grid as Spacing, Item, Kind, MarkerType, Outline, Painting, Pen, Point,
    Polyline, Rect, Run, Scene, Step,
};
use unicode_width::UnicodeWidthStr;

use crate::grid::{Grid, Stroke};

/// The size of a terminal's cells in the units a diagram is laid out in, and the measurer of
/// text in them.
///
/// The default is 6 across and 12 down, half as wide as high, as a terminal's cells about
/// are. Text is laid out at the size whose lines, 1.5 times as high as it, are a row, and the
/// room Pele leaves about text and between parts of a shape, which it gives in pixels and
/// does not scale with the text, is then about a row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cells {
    /// The width of a cell.
    pub width: f64,
    /// The height of a cell.
    pub height: f64,
}

impl Default for Cells {
    fn default() -> Cells {
        Cells {
            width: 6.0,
            height: 12.0,
        }
    }
}

impl Cells {
    /// The size to lay text out at, whose lines are a row high: two thirds of a row.
    pub fn font_size(&self) -> f64 {
        self.height / 1.5
    }

    /// The grid of the cells, to snap a layout to.
    pub fn grid(&self) -> Spacing {
        Spacing {
            x: self.width,
            y: self.height,
        }
    }
}

impl Measurer for Cells {
    /// The cells the text takes in a terminal, each a cell's width, whatever its size and
    /// face. A character of East Asian scripts takes two.
    fn width(&self, s: &str, _size: f64, _f: limu_scene::text::Face) -> f64 {
        s.width() as f64 * self.width
    }
}

/// The characters lines are drawn in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Charset {
    /// Unicode's box-drawing characters and its arrows.
    #[default]
    Unicode,
    /// ASCII's "-", "|", "+" and letters for arrows, for terminals that have no more.
    Ascii,
}

/// How a scene is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    /// The cells, which should be those the scene was laid out for.
    pub cells: Cells,
    /// The characters lines and markers are drawn in.
    pub charset: Charset,
    /// Whether the room the layout leaves, which it gives in pixels, is cut to a cell, as
    /// it is by default. Each run of rows that nothing but straight lines down passes
    /// through, and of columns that nothing but straight lines across passes through, is
    /// cut to one, and the empty rows and columns about the diagram are cut away. Without
    /// it, every cell is where the scene puts it.
    pub compact: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            cells: Cells::default(),
            charset: Charset::default(),
            compact: true,
        }
    }
}

/// The diagrams that can't be drawn in cells without misleading, by the names scenes give
/// them.
const UNSUPPORTED: [&str; 8] = [
    "gantt", "gitGraph", "packet", "pie", "radar", "sankey", "treeView", "xychart",
];

/// The diagrams whose boxes are as large as the room they are given, as a block's columns or
/// a treemap's values, and not as their text, which keep their size in cells.
const SIZED: [&str; 2] = ["block", "treemap"];

/// A diagram that can't be drawn in cells without misleading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unsupported {
    /// The kind of diagram, as its scene names it.
    pub diagram: String,
}

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a {} diagram can't be drawn as text", self.diagram)
    }
}

impl std::error::Error for Unsupported {}

/// A scene as text, a line for each row of cells, with no spaces at the ends of lines, or
/// [`Unsupported`] for a diagram that can't be drawn in cells without misleading.
pub fn render(scene: &Scene, o: &Options) -> Result<String, Unsupported> {
    if UNSUPPORTED.contains(&scene.diagram.as_str()) {
        return Err(Unsupported {
            diagram: scene.diagram.clone(),
        });
    }
    let cells = o.cells;
    let cols = (scene.width / cells.width).round().max(0.0) as usize + 1;
    let rows = (scene.height / cells.height).round().max(0.0) as usize + 1;
    let look = Look::default();
    let mut c = TextCanvas {
        grid: Grid::new(cols, rows),
        o,
        bg: look.theme.bg,
        width: scene.width,
        turns: Vec::new(),
        ends: Vec::new(),
        labels: Vec::new(),
        dots: HashSet::new(),
        nodes: Vec::new(),
        boxes: Vec::new(),
    };
    c.nodes = scene
        .items
        .iter()
        .filter(|it| matches!(it.kind, Kind::Node) && !it.bounds.is_empty())
        .map(|it| {
            let b = it.bounds;
            (
                c.cell(Point::new(b.x, b.y)),
                c.cell(Point::new(b.x + b.w, b.y + b.h)),
            )
        })
        .collect();
    // A line across or down, as a card's priority, which marks a node rather than being in
    // it.
    let line = |it: &Item| {
        (it.bounds.w == 0.0 || it.bounds.h == 0.0)
            && it.parts.iter().all(|p| match &p.outline {
                Outline::Path(path) => path
                    .steps()
                    .all(|s| matches!(s, Step::MoveTo(_) | Step::LineTo(_))),
                Outline::Box { .. } => false,
            })
    };
    // The nodes with more than text in them, as a quadrant chart's quadrants with their
    // points, whose room is not only about text.
    let crowded: Vec<Rect> = scene
        .items
        .iter()
        .filter(|it| matches!(it.kind, Kind::Node) && !it.bounds.is_empty())
        .map(|it| it.bounds)
        .filter(|b| {
            scene.items.iter().any(|x| {
                !matches!(x.kind, Kind::Text | Kind::Edge(_) | Kind::Group)
                    && x.bounds != *b
                    && !x.parts.is_empty()
                    && !line(x)
                    && b.x <= x.bounds.x
                    && b.y <= x.bounds.y
                    && x.bounds.x + x.bounds.w <= b.x + b.w
                    && x.bounds.y + x.bounds.h <= b.y + b.h
            })
        })
        .collect();
    // A box's room across is about its text where the text is set in its middle. Text set
    // against a side, as a card's on a Kanban board, is in a box as wide as others.
    let across = |it: &Item| it.label.as_ref().is_some_and(|l| l.align == Align::Middle);
    limu_scene::draw_with(scene, &look, &mut c, |p, it, s| {
        let c = p.canvas();
        // A shape too small for its label, as a sequence number's circle.
        let small = it.label.is_some()
            && !matches!(it.kind, Kind::Edge(_))
            && !it.parts.is_empty()
            && c.tiny(it.bounds);
        // The middle of the box a node is drawn as, which its label is set in.
        let mut boxed = None;
        if let Some(sides) = c.boxed(it) {
            // The label has room across inside the box's sides, which take a cell each,
            // and a cell between them and it. A label's box runs from the middle of a cell
            // to the middle of a cell, the first and last of its text.
            let l = it.label_bounds();
            let cw = cells.width;
            let (x, n) = ((l.x / cw).ceil(), (l.w / cw).round());
            let room = Rect::new((x - 2.0) * cw, l.y, (n + 3.0) * cw, l.h);
            let b = union(it.bounds, room);
            // The label is set across in the middle of the box's cells, and down where a
            // box too low for its sides has its row.
            let ((c0, r0), (c1, r1)) = c.node(b, sides);
            if !crowded.contains(&it.bounds) {
                c.boxes.push(Frame {
                    corners: [(c0, r0), (c1, r1)],
                    across: across(it),
                });
            }
            boxed = Some(Point::new((c0 + c1) as f64 * cw / 2.0, b.center().y));
        } else if c.picture(it) {
            // A picture in a node, as a service's icon or a task's face, is more than cells
            // can draw, and what it says is in the node's text or nowhere a grid of cells
            // can show, as a color.
        } else if small {
            // A shape too small for its label, as a sequence number's circle, is left to
            // the label.
        } else if let Kind::Edge(_) = it.kind
            && it
                .parts
                .iter()
                .all(|p| p.paint == Painting::StrokeOnly && matches!(p.outline, Outline::Path(_)))
        {
            if s.stroke.visible() && s.stroke_width > 0.0 {
                for part in &it.parts {
                    if let Outline::Path(path) = &part.outline {
                        c.route(path, line_stroke(s));
                    }
                }
            }
        } else if matches!(it.kind, Kind::Text) && !s.stroke.visible() {
            // A label's ground hides the lines under it, but text set in cells breaks them
            // where it is itself, and the ground, laid out about text a row high, may be a
            // row off it.
        } else {
            p.parts(it, s);
            // A node drawn as boxes, as a class's, is drawn in about what is in them, and
            // a group's box about the things in it, as wide as they are and not as its title.
            let c = p.canvas();
            let node = matches!(it.kind, Kind::Node)
                && it.label.is_some()
                && !crowded.contains(&it.bounds);
            if (node || matches!(it.kind, Kind::Group)) && s.stroke.visible() {
                for part in &it.parts {
                    if let Outline::Box { rect, .. } = part.outline
                        && !c.tiny(rect)
                    {
                        let corners = [
                            c.cell(Point::new(rect.x, rect.y)),
                            c.cell(Point::new(rect.x + rect.w, rect.y + rect.h)),
                        ];
                        c.boxes.push(Frame {
                            corners,
                            across: !node || across(it),
                        });
                    }
                }
            }
        }
        let c = p.canvas();
        if let Kind::Edge(e) = &it.kind {
            // An edge's line stops short of a marker's tip, by the marker's length, which
            // may be a cell or more. It is drawn on to the tip.
            let path = it.parts.iter().find_map(|p| match &p.outline {
                Outline::Path(p) if !p.is_empty() => Some(p),
                _ => None,
            });
            let stops = match path {
                Some(p) => [p.steps().next().map(first), Some(p.last())],
                None => [e.route.first().copied(), e.route.last().copied()],
            };
            for (m, stop) in e.ends.iter().zip(stops) {
                let (Some(m), Some(stop)) = (m, stop) else {
                    continue;
                };
                let way = (m.dx / cells.width, m.dy / cells.height);
                let back = if way.0.abs() >= way.1.abs() {
                    (-(way.0.signum() as i64), 0)
                } else {
                    (0, -(way.1.signum() as i64))
                };
                c.ends.push(End {
                    line: c.cell(stop),
                    tip: c.cell(m.at),
                    back,
                    mark: marker(m.kind, way, o.charset),
                    stroke: line_stroke(s),
                });
            }
        }
        if let Some(placed) = &it.label
            && s.text.visible()
        {
            let b = it.bounds;
            let inside = matches!(it.kind, Kind::Node | Kind::Group)
                && s.stroke.visible()
                && !b.is_empty()
                && (b.x..=b.x + b.w).contains(&placed.at.x)
                && (b.y..=b.y + b.h).contains(&placed.at.y);
            let lines = placed
                .label
                .lines
                .iter()
                .map(|spans| spans.iter().map(|s| s.text.as_str()).collect())
                .filter(|l: &String| !l.trim().is_empty())
                .collect();
            // A label in the middle of a node, or about it, as rounding to cells may have
            // moved it, is set in the middle of the box drawn.
            let about = |b: Rect| {
                let m = b.center();
                let (c0, r0) = c.cell(Point::new(b.x, b.y));
                let (c1, r1) = c.cell(Point::new(b.x + b.w, b.y + b.h));
                ((m.x - placed.at.x).abs() <= cells.width
                    && (m.y - placed.at.y).abs() <= cells.height)
                    .then(|| {
                        Point::new(
                            (c0 + c1) as f64 * cells.width / 2.0,
                            (r0 + r1) as f64 * cells.height / 2.0,
                        )
                    })
            };
            let middle = match boxed {
                Some(mid) => Some(mid),
                None if matches!(it.kind, Kind::Node) && !b.is_empty() => about(b),
                None => None,
            };
            let middle = middle.filter(|_| placed.align == Align::Middle && placed.rotate == 0.0);
            c.labels.push(Text {
                lines,
                at: middle.unwrap_or(placed.at),
                align: placed.align,
                rotate: placed.rotate,
                within: inside.then_some(b),
                own: small,
                // Text kept in the middle of a box drawn about it, which a shape too small to
                // draw, or one without an outline, has none of.
                middle: middle.is_some() && (boxed.is_some() || inside && !small),
            });
        }
    });
    // Markers are drawn last, so that a shape drawn after an edge does not hide its arrow.
    // One whose tip is on a dot, as a state diagram's end, is drawn a cell back from it.
    for end in std::mem::take(&mut c.ends) {
        let at = if c.dots.contains(&end.tip) {
            (end.tip.0 + end.back.0, end.tip.1 + end.back.1)
        } else {
            end.tip
        };
        c.grid.line(end.line, at, end.stroke);
        // The cells either side of the tip, across the line.
        let side = (end.back.1.abs(), end.back.0.abs());
        match end.mark {
            Mark::Tip(tip) => {
                c.grid.put(at, tip);
            }
            Mark::Back(back, tip) => {
                c.grid.put((at.0 + end.back.0, at.1 + end.back.1), back);
                c.grid.put(at, tip);
            }
            Mark::Across(before, tip, after) => {
                c.grid.put((at.0 - side.0, at.1 - side.1), before);
                c.grid.put(at, tip);
                c.grid.put((at.0 + side.0, at.1 + side.1), after);
            }
            Mark::Beside(text) => c.beside(at, end.back, text),
        }
    }
    // Labels are set after everything else, so that nothing drawn later breaks them.
    for text in std::mem::take(&mut c.labels) {
        c.label(&text);
    }
    if o.compact {
        // Boxes are drawn in about their text once the room between things is cut, so that
        // what was room inside them, and goes, is not left as room between them.
        let mut boxes = std::mem::take(&mut c.boxes);
        if SIZED.contains(&scene.diagram.as_str()) {
            boxes.clear();
        }
        let mut corners: Vec<(i64, i64)> = boxes
            .iter()
            .flat_map(|f| f.corners.map(|at| c.grid.at(at)))
            .collect();
        c.grid.squeeze(&mut corners);
        // A box in another is drawn after it, and is drawn in first, so that the one it is
        // in comes in about it as it is then.
        for (ab, f) in corners.chunks(2).zip(&boxes).rev() {
            c.grid.tighten(ab[0], ab[1], f.across);
        }
        c.grid.squeeze(&mut []);
    }
    c.grid.center();
    Ok(c.grid.write(o.charset))
}

/// The canvas of a grid of cells.
struct TextCanvas<'a> {
    grid: Grid,
    o: &'a Options,
    bg: Color,
    // The width of the scene.
    width: f64,
    // The turns begun, each by degrees about a point.
    turns: Vec<(f64, Point)>,
    // The markers of edges, drawn after the rest.
    ends: Vec<End>,
    // The labels, set last.
    labels: Vec<Text>,
    // The cells dots are drawn in.
    dots: HashSet<(i64, i64)>,
    // The cells of the corners of the nodes' boxes, top left and bottom right, which edges
    // go around.
    nodes: Vec<((i64, i64), (i64, i64))>,
    // The boxes nodes are drawn as.
    boxes: Vec<Frame>,
}

impl TextCanvas<'_> {
    /// A point turned as the turns begun turn it.
    fn turned(&self, p: Point) -> Point {
        self.turns.iter().rev().fold(p, |p, &(deg, about)| {
            let (sin, cos) = deg.to_radians().sin_cos();
            let (x, y) = (p.x - about.x, p.y - about.y);
            Point::new(about.x + x * cos - y * sin, about.y + x * sin + y * cos)
        })
    }

    /// The cell a point is in. A cell is centered on a point of the grid.
    fn cell(&self, p: Point) -> (i64, i64) {
        let p = self.turned(p);
        (
            (p.x / self.o.cells.width).round() as i64,
            (p.y / self.o.cells.height).round() as i64,
        )
    }

    /// Draws a dot in a cell, filled or not. A filled dot in an empty one is a dot with a
    /// ring, as a state diagram's end.
    fn dot(&mut self, at: (i64, i64), filled: bool) {
        let ring = self.grid.glyph(at) == Some('○');
        let ch = match (self.o.charset, filled) {
            (Charset::Unicode, true) if ring => '◉',
            (Charset::Unicode, true) => '●',
            (Charset::Unicode, false) => '○',
            (Charset::Ascii, true) => '*',
            (Charset::Ascii, false) => 'o',
        };
        self.grid.put(at, ch);
        self.dots.insert(at);
    }

    /// Whether a fill covers what is under it with a color, rather than the background's.
    fn solid(&self, fill: Option<&Ink>) -> bool {
        fill.is_some_and(|f| f.color != self.bg)
    }

    /// Whether an item is a picture in a node: a mark with a shape of its own inside a
    /// node's box, which is not drawn as a dot.
    fn picture(&self, it: &Item) -> bool {
        if !matches!(it.kind, Kind::Mark) || it.shape.is_none() {
            return false;
        }
        // A shape drawn as a dot, as a point on a chart, is drawn.
        let tol = self.o.cells.width.min(self.o.cells.height) / 4.0;
        let dot = it.parts.iter().all(|p| match &p.outline {
            Outline::Path(path) => path
                .flatten(tol)
                .iter()
                .all(|l| l.closed && bounds(&l.points).is_some_and(|b| self.tiny(b))),
            Outline::Box { rect, .. } => self.tiny(*rect),
        });
        if dot {
            return false;
        }
        let b = it.bounds;
        let ((c0, r0), (c1, r1)) = (
            self.cell(Point::new(b.x, b.y)),
            self.cell(Point::new(b.x + b.w, b.y + b.h)),
        );
        self.nodes
            .iter()
            .any(|&((x0, y0), (x1, y1))| x0 <= c0 && c1 <= x1 && y0 <= r0 && r1 <= y1)
    }

    /// Whether a box, in the scene's units, is too small to outline in cells.
    fn tiny(&self, r: Rect) -> bool {
        r.w <= 4.0 * self.o.cells.width && r.h <= 2.0 * self.o.cells.height
    }

    /// Empties the cells inside polylines by the nonzero rule, those whose middles they
    /// wind about.
    fn cover(&mut self, lines: &[Polyline]) {
        let pts: Vec<Point> = lines
            .iter()
            .flat_map(|l| l.points.iter().copied())
            .collect();
        let Some(b) = bounds(&pts) else { return };
        let (cw, ch) = (self.o.cells.width, self.o.cells.height);
        let (c0, c1) = ((b.x / cw).floor() as i64, ((b.x + b.w) / cw).ceil() as i64);
        let (r0, r1) = ((b.y / ch).floor() as i64, ((b.y + b.h) / ch).ceil() as i64);
        for r in r0..=r1 {
            for c in c0..=c1 {
                let p = Point::new(c as f64 * cw, r as f64 * ch);
                if lines.iter().map(|l| winding(&l.points, p)).sum::<i32>() != 0 {
                    self.grid.clear((c, r));
                }
            }
        }
    }

    /// How a node is drawn as the box it lies in: whether its corners are rounded and its
    /// left and right sides pointed. A node is, where it has a label and its outline curves
    /// or slants and is too large for a dot. Otherwise none.
    fn boxed(&self, it: &Item) -> Option<Sides> {
        if !matches!(it.kind, Kind::Node) || it.label.is_none() || self.tiny(it.bounds) {
            return None;
        }
        let mut round = false;
        let mut slants = false;
        let mut pts = Vec::new();
        for part in &it.parts {
            let Outline::Path(p) = &part.outline else {
                continue;
            };
            let mut last: Option<Point> = None;
            for step in p.steps() {
                match step {
                    Step::QuadTo(..) | Step::CubeTo(..) => round = true,
                    Step::LineTo(b) => {
                        slants |= last
                            .is_some_and(|a| (b.x - a.x).abs() > 0.5 && (b.y - a.y).abs() > 0.5);
                    }
                    _ => {}
                }
                if let Step::MoveTo(b) | Step::LineTo(b) = step {
                    last = Some(b);
                    pts.push(b);
                }
            }
        }
        if !(round || slants) {
            return None;
        }
        // A side comes to a point where the points furthest out on it are all about its
        // middle, as a diamond's and a hexagon's are.
        let b = it.bounds;
        let pointed = |x: f64| {
            let out: Vec<&Point> = pts.iter().filter(|p| (p.x - x).abs() < 0.5).collect();
            !out.is_empty()
                && out
                    .iter()
                    .all(|p| (p.y - (b.y + b.h / 2.0)).abs() < b.h / 6.0)
        };
        Some(Sides {
            round,
            left: !round && pointed(b.x),
            right: !round && pointed(b.x + b.w),
        })
    }

    /// Draws a node as a box, emptied inside, and gives the cells of its corners, top left
    /// and bottom right.
    fn node(&mut self, b: Rect, sides: Sides) -> ((i64, i64), (i64, i64)) {
        let pen = Pen {
            ink: Ink::NONE,
            width: 1.0,
            dash: &[],
            cap: Default::default(),
            join: Default::default(),
        };
        let radius = if sides.round { 1.0 } else { 0.0 };
        let ground = Ink {
            color: self.bg,
            ..Ink::NONE
        };
        self.rect(b, radius, Some(&ground), Some(&pen));
        let (c0, r0) = self.cell(Point::new(b.x, b.y));
        let (c1, r1) = self.cell(Point::new(b.x + b.w, b.y + b.h));
        let r = middle((r0 + r1) as f64 / 2.0, 1.0);
        if sides.left && r > r0 && r < r1 {
            self.grid.side((c0, r), '<');
        }
        if sides.right && r > r0 && r < r1 {
            self.grid.side((c1, r), '>');
        }
        ((c0, r0), (c1, r1))
    }

    /// Draws an edge's path in cells, in lines across and down. Steps that follow a curve
    /// cell by cell are hard to follow in cells, and text renderers of diagrams draw lines
    /// across and down, as trees are drawn in text.
    ///
    /// Each stretch of the path that runs one way across and one way down is drawn as a
    /// line the way it leaves, a line the other way, and a line the way it arrives, either
    /// of the first and last of which may be none. Of the ways to draw it, the one taken
    /// goes through the fewest cells of nodes, then runs along the fewest outlines of
    /// shapes, as a group's sides, then keeps nearest the path, then leaves and arrives the ways the path does,
    /// then turns nearest halfway. Labels are set by
    /// the path, and lines near it pass by them.
    fn route(&mut self, path: &limu_scene::Path, s: Stroke) {
        let tol = self.o.cells.width.min(self.o.cells.height) / 4.0;
        for line in path.flatten(tol) {
            let pts: Vec<Point> = line.points.iter().map(|&p| self.turned(p)).collect();
            let pts = self.clip(&pts);
            for stretch in stretches(&pts) {
                let (a, b) = (self.cell(stretch[0]), self.cell(stretch[stretch.len() - 1]));
                let down = |p: &[Point]| {
                    let (dx, dy) = (p[1].x - p[0].x, p[1].y - p[0].y);
                    dy.abs() / self.o.cells.height > dx.abs() / self.o.cells.width
                };
                let n = stretch.len();
                let ways = (down(&stretch[..2]), down(&stretch[n - 2..]));
                let mut from = a;
                for to in self.corners(stretch, a, b, ways).into_iter().chain([b]) {
                    self.grid.line(from, to, s);
                    from = to;
                }
            }
        }
    }

    /// A polyline without the stretches at its ends that are inside nodes, as an edge's
    /// that starts at a side of a diamond, which is drawn as the box it lies in. It is cut
    /// to the nearest half cell. A polyline all inside nodes is kept whole.
    fn clip(&self, pts: &[Point]) -> Vec<Point> {
        let step = self.o.cells.width.min(self.o.cells.height) / 2.0;
        let mut fine = Vec::new();
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let n = ((b.x - a.x).hypot(b.y - a.y) / step).ceil().max(1.0) as usize;
            for i in 0..n {
                let t = i as f64 / n as f64;
                fine.push(Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
            }
        }
        fine.extend(pts.last());
        let inside = |p: &Point| {
            let (c, r) = self.cell(*p);
            self.nodes
                .iter()
                .any(|&((c0, r0), (c1, r1))| c0 < c && c < c1 && r0 < r && r < r1)
        };
        let start = fine.iter().position(|p| !inside(p));
        let end = fine.iter().rposition(|p| !inside(p));
        match (start, end) {
            (Some(s), Some(e)) if e > s => fine[s..=e].to_vec(),
            _ => pts.to_vec(),
        }
    }

    /// The corners of the lines across and down from one cell to another, as
    /// [`TextCanvas::route`] picks them, given whether the path leaves and arrives down.
    fn corners(
        &self,
        stretch: &[Point],
        a: (i64, i64),
        b: (i64, i64),
        ways: (bool, bool),
    ) -> Vec<(i64, i64)> {
        if a.0 == b.0 || a.1 == b.1 {
            return Vec::new();
        }
        // Each way, by the row it goes across in, or the column it goes down in.
        let across = (a.1.min(b.1)..=a.1.max(b.1)).map(|m| {
            let ways = (m != a.1, m != b.1);
            (vec![(a.0, m), (b.0, m)], ways, (2 * m - a.1 - b.1).abs())
        });
        let down = (a.0.min(b.0)..=a.0.max(b.0)).map(|m| {
            let ways = (m == a.0, m == b.0);
            (vec![(m, a.1), (m, b.1)], ways, (2 * m - a.0 - b.0).abs())
        });
        // How far the path is from the lines, in cells, summed over its points.
        let (cw, ch) = (self.o.cells.width, self.o.cells.height);
        let pts: Vec<(f64, f64)> = stretch.iter().map(|p| (p.x / cw, p.y / ch)).collect();
        let off_path = |corners: &[(i64, i64)]| {
            let mut route = vec![a];
            route.extend(corners);
            route.push(b);
            pts.iter()
                .map(|&p| {
                    route
                        .windows(2)
                        .map(|w| apart(p, w[0], w[1]))
                        .fold(f64::INFINITY, f64::min)
                })
                .sum::<f64>()
        };
        across
            .chain(down)
            .map(|(corners, w, off)| {
                let missed = (w.0 != ways.0) as u8 + (w.1 != ways.1) as u8;
                let key = (
                    self.through(a, &corners, b),
                    off_path(&corners),
                    missed,
                    off,
                );
                (corners, key)
            })
            .min_by(|(_, x), (_, y)| {
                (x.0.cmp(&y.0))
                    .then(x.1.total_cmp(&y.1))
                    .then(x.2.cmp(&y.2))
                    .then(x.3.cmp(&y.3))
            })
            .map(|(corners, _)| corners)
            .unwrap_or_default()
    }

    /// How many cells of nodes lines across and down through corners pass through, between
    /// the cells they start and end in, and how many they run along the outlines of shapes
    /// drawn already.
    fn through(&self, a: (i64, i64), corners: &[(i64, i64)], b: (i64, i64)) -> (usize, usize) {
        let mut cells = Vec::new();
        let mut from = a;
        for &to in corners.iter().chain([&b]) {
            let (dc, dr) = ((to.0 - from.0).signum(), (to.1 - from.1).signum());
            let mut at = from;
            while at != to {
                at = (at.0 + dc, at.1 + dr);
                cells.push((at, dc != 0));
            }
            from = to;
        }
        cells.pop();
        let nodes = cells
            .iter()
            .filter(|&&((c, r), _)| {
                self.nodes
                    .iter()
                    .any(|&((c0, r0), (c1, r1))| (c0..=c1).contains(&c) && (r0..=r1).contains(&r))
            })
            .count();
        let along = cells
            .iter()
            .filter(|&&(at, across)| self.grid.along(self.grid.at(at), across))
            .count();
        (nodes, along)
    }

    /// Draws polylines in cells.
    fn stroke(&mut self, lines: &[Polyline], s: Stroke) {
        let (cw, ch) = (self.o.cells.width, self.o.cells.height);
        let at = |p: &Point| ((p.x / cw).round() as i64, (p.y / ch).round() as i64);
        for l in lines {
            let mut cells: Vec<(i64, i64)> = l.points.iter().map(at).collect();
            if l.closed
                && let Some(&first) = cells.first()
            {
                cells.push(first);
            }
            for w in cells.windows(2) {
                self.grid.line(w[0], w[1], s);
            }
        }
    }

    /// Sets text beside an edge's line, a cell back from its tip: to the right of a line
    /// down, and above a line across, ending before the tip or starting after it.
    fn beside(&mut self, tip: (i64, i64), back: (i64, i64), text: &str) {
        let (cw, ch) = (self.o.cells.width, self.o.cells.height);
        let (c, r) = (tip.0 + back.0, tip.1 + back.1);
        let (at, align) = match back {
            (0, _) => (
                Point::new((c as f64 + 0.5) * cw, r as f64 * ch),
                Align::Start,
            ),
            (1, _) => (
                Point::new((c as f64 - 0.5) * cw, (r - 1) as f64 * ch),
                Align::Start,
            ),
            _ => (
                Point::new((c as f64 + 0.5) * cw, (r - 1) as f64 * ch),
                Align::End,
            ),
        };
        self.labels.push(Text {
            lines: vec![text.to_string()],
            at,
            align,
            rotate: 0.0,
            within: None,
            own: false,
            middle: false,
        });
    }

    /// Sets a label's lines in cells about its point: a line to a row, centered on it, or
    /// starting or ending there as the label is aligned.
    ///
    /// Text is set where it breaks no more than lines down, and no other text, a row up or
    /// down from there if it must, and inside the box it is in. Where there is no such room,
    /// a row is put in, inside the box, where it splits no other text if it can, and the
    /// label is set again. The label of a shape too small to draw, as a sequence number's
    /// circle, stands for the shape, and breaks any line but no text.
    fn label(&mut self, text: &Text) {
        let lines = &text.lines;
        if lines.is_empty() {
            return;
        }
        let (cw, ch) = (self.o.cells.width, self.o.cells.height);
        let at = text.at;
        let m = lines.len() as i64;
        // Text can't be turned in cells. A label turned far enough that it would run into
        // what is beside it, as an axis's title, is set across about the point it is turned
        // about, running out from the diagram there: leftward in its left half, and
        // rightward in its right half.
        let turn = text.rotate.abs() % 180.0;
        let turned = (30.0..=150.0).contains(&turn);
        let cols: Vec<(i64, i64)> = lines
            .iter()
            .map(|line| {
                let n = line.width() as f64;
                let w = n * cw;
                let x = match text.align {
                    _ if turned && at.x < self.width / 2.0 => at.x - w / 2.0,
                    _ if turned => at.x + w / 2.0,
                    Align::Middle => at.x,
                    Align::Start => at.x + w / 2.0,
                    Align::End => at.x - w / 2.0,
                };
                (middle(x / cw, n), n as i64)
            })
            .collect();
        // The row of the first line in the scene, each line a row below the one before.
        let first = middle(at.y / ch - (m - 1) as f64 / 2.0, 1.0);
        // The rows of the box's sides above and below, which text stays between. A box too
        // low to have sides, drawn in brackets on a row, has the text on that row.
        let sides = text.within.and_then(|b| {
            let (_, r0) = self.cell(Point::new(b.x, b.y));
            let (_, r1) = self.cell(Point::new(b.x, b.y + b.h));
            (r1 - r0 > 1).then_some((r0, r1))
        });
        // Sets the label a number of rows down from where its first line is, which may grow
        // the grid.
        let set = |g: &mut Grid, shift: i64| {
            for (i, (&(c, n), line)) in cols.iter().zip(lines).enumerate() {
                let r = g.at((0, first)).1 + shift + i as i64;
                let at = g.fit((g.at((c, 0)).0, r), n);
                g.text(at, line);
                if text.middle {
                    g.middle(at, n);
                }
            }
            let top = g.at((0, first)).1 + shift;
            g.block(top, top + m - 1);
        };
        // How far down from where its first line is the label fits in a grid, if it does.
        let fits = |g: &Grid| {
            let top = g.at((0, first)).1;
            let inside = sides.map(|(r0, r1)| (g.at((0, r0)).1, g.at((0, r1)).1));
            let shifts: &[i64] = if text.own { &[0] } else { &[0, -1, 1] };
            shifts.iter().copied().find(|&shift| {
                cols.iter().enumerate().all(|(i, &(c, n))| {
                    let r = top + shift + i as i64;
                    inside.is_none_or(|(r0, r1)| r0 < r && r < r1)
                        && g.free((g.at((c, 0)).0, r), n, text.own)
                })
            })
        };
        for _ in 0..=m {
            if let Some(shift) = fits(&self.grid) {
                set(&mut self.grid, shift);
                return;
            }
            // A row is put in before a row of the label's or the one after it, inside its
            // box if it is in one: where the label then fits, if it does anywhere, where it
            // splits the fewest texts, and then the lowest.
            let mut rows: Vec<i64> = (first..=first + m)
                .filter(|&r| sides.is_none_or(|(r0, r1)| r0 < r && r <= r1))
                .collect();
            rows.sort_by_key(|&r| (self.grid.splits(r), std::cmp::Reverse(r)));
            let room = |&r: &i64| {
                let mut g = self.grid.clone();
                g.put_row_in(r);
                fits(&g).is_some()
            };
            let before = rows
                .iter()
                .find(|r| room(r))
                .or(rows.first())
                .copied()
                .unwrap_or(first + m);
            self.grid.put_row_in(before);
        }
        // There is no room to be made, as where text is set over text above and below.
        set(&mut self.grid, 0);
    }
}

impl Canvas for TextCanvas<'_> {
    fn begin_rotate(&mut self, degrees: f64, about: Point) {
        self.turns.push((degrees, about));
    }

    fn end_rotate(&mut self) {
        self.turns.pop();
    }

    fn path(&mut self, path: &limu_scene::Path, fill: Option<&Ink>, pen: Option<&Pen<'_>>) {
        let tol = self.o.cells.width.min(self.o.cells.height) / 4.0;
        let mut lines = path.flatten(tol);
        for l in &mut lines {
            for p in &mut l.points {
                *p = self.turned(*p);
            }
        }
        // Each closed shape too small to outline is drawn as a character: a triangle as an
        // arrowhead pointing the way it does, and anything else as a dot.
        if lines.iter().all(|l| l.closed) {
            let filled = self.solid(fill);
            lines.retain(|l| {
                let Some(b) = bounds(&l.points) else {
                    return false;
                };
                if !self.tiny(b) {
                    return true;
                }
                let at = self.cell(Point::new(b.x + b.w / 2.0, b.y + b.h / 2.0));
                match arrowhead(&l.points) {
                    Some(way) => {
                        if let Mark::Tip(ch) = marker(MarkerType::ArrowPoint, way, self.o.charset) {
                            self.grid.put(at, ch);
                        }
                    }
                    None => self.dot(at, filled),
                }
                false
            });
            if lines.is_empty() {
                return;
            }
        }
        if hides(fill) {
            self.cover(&lines);
        }
        if let Some(pen) = pen {
            self.stroke(&lines, stroke(pen));
        }
    }

    fn rect(&mut self, rect: Rect, radius: f64, fill: Option<&Ink>, pen: Option<&Pen<'_>>) {
        if !self.turns.is_empty() {
            let mut p = limu_scene::Path::new();
            p.round_rect(rect, radius);
            self.path(&p, fill, pen);
            return;
        }
        if self.tiny(rect) {
            let filled = self.solid(fill);
            self.dot(self.cell(rect.center()), filled);
            return;
        }
        let (c0, r0) = self.cell(Point::new(rect.x, rect.y));
        let (c1, r1) = self.cell(Point::new(rect.x + rect.w, rect.y + rect.h));
        // A box too narrow to have room inside, as a sequence diagram's activation, is a
        // heavy line down its middle. A middle is found as a label's is, so that a box and
        // its label share a row. One too low, as a branch's name on a git graph, is
        // bracketed on the row of its middle, and one that is only filled hides what it
        // covers there.
        let mid = {
            let p = self.turned(rect.center());
            (
                middle(p.x / self.o.cells.width, 1.0),
                middle(p.y / self.o.cells.height, 1.0),
            )
        };
        if c1 - c0 <= 2 && r1 > r0 {
            if pen.is_some() {
                let heavy = Stroke {
                    heavy: true,
                    outline: true,
                    ..Stroke::default()
                };
                self.grid.line((mid.0, r0), (mid.0, r1), heavy);
            }
            return;
        }
        if r1 - r0 <= 1 {
            let r = mid.1;
            if hides(fill) {
                for c in c0 + 1..c1 {
                    self.grid.clear((c, r));
                }
            }
            if pen.is_some() && c1 > c0 {
                self.grid.side((c0, r), '[');
                self.grid.side((c1, r), ']');
            }
            return;
        }
        if hides(fill) {
            // What is inside is hidden, and with it the lines from the sides into it.
            for r in r0 + 1..r1 {
                for c in c0 + 1..c1 {
                    self.grid.clear((c, r));
                }
                self.grid.cut((c0, r), (1, 0));
                self.grid.cut((c1, r), (-1, 0));
            }
            for c in c0 + 1..c1 {
                self.grid.cut((c, r0), (0, 1));
                self.grid.cut((c, r1), (0, -1));
            }
        }
        if let Some(pen) = pen {
            let s = stroke(pen);
            self.grid.line((c0, r0), (c1, r0), s);
            self.grid.line((c1, r0), (c1, r1), s);
            self.grid.line((c1, r1), (c0, r1), s);
            self.grid.line((c0, r1), (c0, r0), s);
            if radius > 0.0 {
                for at in [(c0, r0), (c1, r0), (c1, r1), (c0, r1)] {
                    self.grid.round(at);
                }
            }
        }
    }

    // Markers are drawn from the edge's ends, as characters.
    fn marker(&mut self, _: &limu_scene::Path, _: Option<&Ink>, _: Option<&Pen<'_>>) {}

    fn text(&mut self, run: &Run<'_>) {
        let s: String = run.spans.iter().map(|s| s.text.as_str()).collect();
        let n = s.width() as f64;
        let cw = self.o.cells.width;
        // The middle of the line is a little above its baseline.
        let p = self.turned(Point::new(run.at.x, run.at.y - run.size * 0.35));
        let x = match run.anchor {
            Align::Middle => p.x,
            Align::Start => p.x + n * cw / 2.0,
            Align::End => p.x - n * cw / 2.0,
        };
        let r = middle(p.y / self.o.cells.height, 1.0);
        self.grid.text((middle(x / cw, n), r), &s);
    }
}

/// Text set in cells: a label's lines, or a mark beside an edge.
struct Text {
    lines: Vec<String>,
    // The point the text is set about, turned by degrees about it.
    at: Point,
    align: Align,
    rotate: f64,
    // The box with an outline the text is in, which it stays in.
    within: Option<Rect>,
    // Whether the text stands for a shape too small to draw.
    own: bool,
    // Whether the text is set in the middle of a box, and kept there.
    middle: bool,
}

/// The box a node is drawn as, in cells.
struct Frame {
    // The cells of its corners, top left and bottom right.
    corners: [(i64, i64); 2],
    // Whether its sides may be drawn in across, as they may where its text is set in its
    // middle.
    across: bool,
}

/// A marker at an end of an edge, in cells.
struct End {
    // Where the edge's line stops.
    line: (i64, i64),
    // Where the marker's tip is.
    tip: (i64, i64),
    // A step back from the tip, against the way the marker points.
    back: (i64, i64),
    mark: Mark,
    stroke: Stroke,
}

/// How a marker is drawn in cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    /// A character at the tip.
    Tip(char),
    /// A character a cell back from the tip, and one at it.
    Back(char, char),
    /// A character at the tip, and one either side of it across the line.
    Across(char, char, char),
    /// Text beside the line, a cell back from the tip, which the line is drawn on to.
    Beside(&'static str),
}

/// The point a path's first step goes to.
fn first(step: Step) -> Point {
    match step {
        Step::MoveTo(p) | Step::LineTo(p) | Step::QuadTo(_, p) | Step::CubeTo(_, _, p) => p,
        Step::Close => Point::default(),
    }
}

/// How a node drawn as a box looks.
#[derive(Clone, Copy, Debug)]
struct Sides {
    round: bool,
    left: bool,
    right: bool,
}

/// The way a triangle points, from the middle of its shortest side to the corner across
/// from it, where its points are a triangle's.
fn arrowhead(pts: &[Point]) -> Option<(f64, f64)> {
    let mut corners: Vec<Point> = Vec::new();
    for &p in pts {
        let far = |q: &Point| (p.x - q.x).abs() > 1e-6 || (p.y - q.y).abs() > 1e-6;
        if corners.last().is_none_or(far) && corners.first().is_none_or(far) {
            corners.push(p);
        }
    }
    let [a, b, c] = corners[..] else {
        return None;
    };
    let len = |p: Point, q: Point| (p.x - q.x).hypot(p.y - q.y);
    // Each corner, with the side across from it.
    let (tip, p, q) = [(a, b, c), (b, c, a), (c, a, b)]
        .into_iter()
        .min_by(|x, y| len(x.1, x.2).total_cmp(&len(y.1, y.2)))?;
    Some((tip.x - (p.x + q.x) / 2.0, tip.y - (p.y + q.y) / 2.0))
}

/// Whether a fill hides what is under it, as one does that can't be seen through. One that
/// can, as a Venn diagram's sets are filled, hides nothing.
fn hides(fill: Option<&Ink>) -> bool {
    fill.is_some_and(|f| f.color.a == 255)
}

/// The first of n cells centered on a place, in cells, the one before the middle where
/// there is no cell in the middle.
fn middle(at: f64, n: f64) -> i64 {
    (at - (n - 1.0) / 2.0 - 0.5).ceil() as i64
}

/// How an item's line is drawn in cells, as [`stroke`] draws a pen's.
fn line_stroke(s: &Resolved) -> Stroke {
    Stroke {
        dashed: s.dash.iter().any(|&d| d > 0.0),
        heavy: s.stroke_width >= 3.0,
        outline: false,
    }
}

/// How a pen's line is drawn in cells: dashed where it has a dash, and heavy where it is
/// thicker than most lines are, as a flowchart's thick edges.
fn stroke(pen: &Pen<'_>) -> Stroke {
    Stroke {
        dashed: pen.dash.iter().any(|&d| d > 0.0),
        heavy: pen.width >= 3.0,
        outline: true,
    }
}

/// How a marker pointing a way, which is in cells, is drawn.
///
/// The ends of an entity relationship are written as how many each side has, "1", "0..1",
/// "1..*" or "0..*", as a crow's foot can't be drawn in a cell without looking like an
/// arrow. In ASCII, a hollow triangle, as a class's extension, is written in two or three
/// characters, so that it is not read as an arrow.
fn marker(kind: MarkerType, (dx, dy): (f64, f64), charset: Charset) -> Mark {
    use MarkerType as M;
    // 0 right, 1 down, 2 left, 3 up.
    let way = if dx.abs() >= dy.abs() {
        if dx >= 0.0 { 0 } else { 2 }
    } else if dy >= 0.0 {
        1
    } else {
        3
    };
    let pick = |chars: [char; 4]| Mark::Tip(chars[way]);
    match (charset, kind) {
        (_, M::ErOnlyOne) => Mark::Beside("1"),
        (_, M::ErZeroOrOne) => Mark::Beside("0..1"),
        (_, M::ErOneOrMore) => Mark::Beside("1..*"),
        (_, M::ErZeroOrMore) => Mark::Beside("0..*"),
        (Charset::Ascii, M::ClassExtension | M::UsecaseExtension | M::RequirementArrow) => [
            Mark::Back('|', '>'),
            Mark::Across('\\', 'V', '/'),
            Mark::Back('|', '<'),
            Mark::Across('/', '_', '\\'),
        ][way],
        (Charset::Ascii, M::ArrowCircle | M::ClassAggregation | M::ClassLollipop) => Mark::Tip('o'),
        (Charset::Ascii, M::ArrowCross) => Mark::Tip('x'),
        (Charset::Ascii, M::ClassComposition | M::ErMdParent) => Mark::Tip('*'),
        (Charset::Ascii, M::RequirementContains) => Mark::Tip('+'),
        (Charset::Ascii, _) => pick(['>', 'v', '<', '^']),
        (_, M::ArrowCircle) => Mark::Tip('●'),
        (_, M::ArrowCross) => Mark::Tip('×'),
        (_, M::ClassExtension | M::UsecaseExtension | M::RequirementArrow) => {
            pick(['▷', '▽', '◁', '△'])
        }
        (_, M::ClassComposition | M::ErMdParent) => Mark::Tip('◆'),
        (_, M::ClassAggregation) => Mark::Tip('◇'),
        (_, M::ClassLollipop) => Mark::Tip('○'),
        (_, M::RequirementContains) => Mark::Tip('⊕'),
        (_, M::SequenceOpen | M::ClassDependency) => pick(['>', 'v', '<', '^']),
        _ => pick(['▶', '▼', '◀', '▲']),
    }
}

/// How far a point is from a line across or down between two cells, in cells.
fn apart((x, y): (f64, f64), a: (i64, i64), b: (i64, i64)) -> f64 {
    let (x0, x1) = (a.0.min(b.0) as f64, a.0.max(b.0) as f64);
    let (y0, y1) = (a.1.min(b.1) as f64, a.1.max(b.1) as f64);
    let dx = (x0 - x).max(x - x1).max(0.0);
    let dy = (y0 - y).max(y - y1).max(0.0);
    dx.hypot(dy)
}

/// A polyline cut where it turns back across or down, into stretches that each run one way
/// across and one way down, of two points or more.
fn stretches(pts: &[Point]) -> Vec<&[Point]> {
    let mut out = Vec::new();
    let mut start = 0;
    let (mut sx, mut sy) = (0.0, 0.0);
    for i in 1..pts.len() {
        let (dx, dy) = (pts[i].x - pts[i - 1].x, pts[i].y - pts[i - 1].y);
        let turns = |was: f64, d: f64| was * d < 0.0;
        if turns(sx, dx) || turns(sy, dy) {
            out.push(&pts[start..i]);
            start = i - 1;
            (sx, sy) = (0.0, 0.0);
        }
        if dx != 0.0 {
            sx = dx.signum();
        }
        if dy != 0.0 {
            sy = dy.signum();
        }
    }
    if pts.len() - start >= 2 {
        out.push(&pts[start..]);
    }
    out.retain(|s| s.len() >= 2);
    out
}

/// The box two boxes lie in, where the second is not empty.
fn union(a: Rect, b: Rect) -> Rect {
    if b.is_empty() {
        return a;
    }
    let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
    let (x1, y1) = ((a.x + a.w).max(b.x + b.w), (a.y + a.h).max(b.y + b.h));
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// The box points lie in, or none for no points.
fn bounds(pts: &[Point]) -> Option<Rect> {
    let first = pts.first()?;
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
    for p in pts {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
}

/// How many times a closed polygon winds about a point, counted as the nonzero rule counts.
fn winding(poly: &[Point], p: Point) -> i32 {
    let mut n = 0;
    for (i, a) in poly.iter().enumerate() {
        let b = poly[(i + 1) % poly.len()];
        let side = (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y);
        if a.y <= p.y {
            if b.y > p.y && side > 0.0 {
                n += 1;
            }
        } else if b.y <= p.y && side < 0.0 {
            n -= 1;
        }
    }
    n
}
