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
//! print!("{}", limu_ascii::render(&scene, &limu_ascii::Options::default()));
//! ```
//!
//! Lines are drawn in box-drawing characters, and where they meet in a cell they are joined
//! in one, as "┬". A curve is drawn in steps across and down. A node with a label whose
//! outline curves or slants, as a stadium or a diamond, is drawn as the box it lies in, its
//! corners rounded where it curves, and its sides pointed where it comes to a point at
//! either side. A shape too small to outline in cells is drawn as a dot, and a marker as an
//! arrow or a mark like it. Labels are set a
//! line to a row, across, or down where they are turned 30 degrees or more. A grid of cells has no
//! colors, fills or images, so a fill only hides what it covers.
//!
//! A diagram is as wide as its layout makes it. Most diagrams fit the width a layout is
//! given, but some can't be drawn narrower, and nothing is cut off or wrapped to fit.

mod grid;
#[cfg(test)]
mod tests;

use std::collections::HashSet;

use limu_scene::style::{Color, Ink, Look};
use limu_scene::text::Measurer;
use limu_scene::{
    Align, Canvas, Grid as Spacing, Item, Kind, MarkerType, Outline, Pen, Placed, Point, Polyline,
    Rect, Run, Scene, Step,
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

/// A scene as text, a line for each row of cells, with no spaces at the ends of lines.
pub fn render(scene: &Scene, o: &Options) -> String {
    let cells = o.cells;
    let cols = (scene.width / cells.width).round().max(0.0) as usize + 1;
    let rows = (scene.height / cells.height).round().max(0.0) as usize + 1;
    let look = Look::default();
    let mut c = TextCanvas {
        grid: Grid::new(cols, rows),
        o,
        bg: look.theme.bg,
        turns: Vec::new(),
        ends: Vec::new(),
        dots: HashSet::new(),
    };
    limu_scene::draw_with(scene, &look, &mut c, |p, it, s| {
        let c = p.canvas();
        if let Some(sides) = c.boxed(it) {
            c.node(union(it.bounds, it.label_bounds()), sides);
        } else if it.label.is_some() && !matches!(it.kind, Kind::Edge(_)) && c.tiny(it.bounds) {
            // A shape too small for its label, as a sequence number's circle, is left to
            // the label.
        } else {
            p.parts(it, s);
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
                    stroke: Stroke {
                        dashed: s.dash.iter().any(|&d| d > 0.0),
                        heavy: s.stroke_width >= 3.0,
                    },
                });
            }
        }
        if let Some(placed) = &it.label
            && s.text.visible()
        {
            c.label(placed);
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
        c.grid.put(at, end.mark);
    }
    if o.compact {
        c.grid.squeeze();
    }
    c.grid.write(o.charset)
}

/// The canvas of a grid of cells.
struct TextCanvas<'a> {
    grid: Grid,
    o: &'a Options,
    bg: Color,
    // The turns begun, each by degrees about a point.
    turns: Vec<(f64, Point)>,
    // The markers of edges, drawn last.
    ends: Vec<End>,
    // The cells dots are drawn in.
    dots: HashSet<(i64, i64)>,
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

    /// Draws a node as a box, emptied inside.
    fn node(&mut self, b: Rect, sides: Sides) {
        let pen = Pen {
            ink: Ink::NONE,
            width: 1.0,
            dash: &[],
            cap: Default::default(),
            join: Default::default(),
        };
        let radius = if sides.round { 1.0 } else { 0.0 };
        self.rect(b, radius, Some(&Ink::NONE), Some(&pen));
        let (c0, r0) = self.cell(Point::new(b.x, b.y));
        let (c1, r1) = self.cell(Point::new(b.x + b.w, b.y + b.h));
        let r = middle((r0 + r1) as f64 / 2.0, 1.0);
        if sides.left && r > r0 && r < r1 {
            self.grid.put((c0, r), '<');
        }
        if sides.right && r > r0 && r < r1 {
            self.grid.put((c1, r), '>');
        }
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

    /// Sets a label's lines in cells about its point: a line to a row, centered on it, or
    /// starting or ending there as the label is aligned. A label turned 30 degrees or more
    /// is set down, a character to a row, its lines side by side.
    fn label(&mut self, placed: &Placed) {
        let lines: Vec<String> = placed
            .label
            .lines
            .iter()
            .map(|spans| spans.iter().map(|s| s.text.as_str()).collect())
            .filter(|l: &String| !l.trim().is_empty())
            .collect();
        if lines.is_empty() {
            return;
        }
        let (cw, ch) = (self.o.cells.width, self.o.cells.height);
        let at = placed.at;
        let m = lines.len() as f64;
        // The middle of each line, in the scene's units, before the label is turned.
        let mids = lines.iter().enumerate().map(|(i, line)| {
            let w = line.width() as f64 * cw;
            let x = match placed.align {
                Align::Middle => at.x,
                Align::Start => at.x + w / 2.0,
                Align::End => at.x - w / 2.0,
            };
            Point::new(x, at.y + (i as f64 - (m - 1.0) / 2.0) * ch)
        });
        // Text can't be turned in cells. Turned far enough that it would run into what is
        // beside it, a line is set down, about where its middle is turned to.
        let turn = placed.rotate.abs() % 180.0;
        if (30.0..=150.0).contains(&turn) {
            let (sin, cos) = placed.rotate.to_radians().sin_cos();
            for (line, mid) in lines.iter().zip(mids) {
                let (x, y) = (mid.x - at.x, mid.y - at.y);
                let (x, y) = (at.x + x * cos - y * sin, at.y + x * sin + y * cos);
                let chars: Vec<char> = line.chars().collect();
                let c = (x / cw).round() as i64;
                let r0 = middle(y / ch, chars.len() as f64);
                for (j, &ch) in chars.iter().enumerate() {
                    self.grid.put((c, r0 + j as i64), ch);
                }
            }
            return;
        }
        let rows: Vec<((i64, i64), &String)> = lines
            .iter()
            .zip(mids)
            .map(|(line, mid)| {
                let at = (
                    middle(mid.x / cw, line.width() as f64),
                    middle(mid.y / ch, 1.0),
                );
                (at, line)
            })
            .collect();
        // A label between rows may be set a row off the room left for it, on a box's side.
        // It is moved a row down or up where that is clear of lines across.
        let clear = |shift: i64| {
            rows.iter().all(|&((c, r), line)| {
                (c..c + line.width() as i64).all(|c| !self.grid.across((c, r + shift)))
            })
        };
        let shift = [0, 1, -1].into_iter().find(|&s| clear(s)).unwrap_or(0);
        for ((c, r), line) in rows {
            self.grid.text((c, r + shift), line);
        }
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
        let pts: Vec<Point> = lines
            .iter()
            .flat_map(|l| l.points.iter().copied())
            .collect();
        let Some(b) = bounds(&pts) else { return };
        if lines.iter().all(|l| l.closed) && self.tiny(b) {
            let filled = self.solid(fill);
            let at = self.cell(Point::new(b.x + b.w / 2.0, b.y + b.h / 2.0));
            self.dot(at, filled);
            return;
        }
        if fill.is_some() {
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
                    ..Stroke::default()
                };
                self.grid.line((mid.0, r0), (mid.0, r1), heavy);
            }
            return;
        }
        if r1 - r0 <= 1 {
            let r = mid.1;
            if fill.is_some() {
                for c in c0 + 1..c1 {
                    self.grid.clear((c, r));
                }
            }
            if pen.is_some() && c1 > c0 {
                self.grid.put((c0, r), '[');
                self.grid.put((c1, r), ']');
            }
            return;
        }
        if fill.is_some() {
            for r in r0 + 1..r1 {
                for c in c0 + 1..c1 {
                    self.grid.clear((c, r));
                }
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

/// A marker at an end of an edge, in cells.
struct End {
    // Where the edge's line stops.
    line: (i64, i64),
    // Where the marker's tip is.
    tip: (i64, i64),
    // A step back from the tip, against the way the marker points.
    back: (i64, i64),
    mark: char,
    stroke: Stroke,
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

/// The first of n cells centered on a place, in cells, the one before the middle where
/// there is no cell in the middle.
fn middle(at: f64, n: f64) -> i64 {
    (at - (n - 1.0) / 2.0 - 0.5).ceil() as i64
}

/// How a pen's line is drawn in cells: dashed where it has a dash, and heavy where it is
/// thicker than most lines are, as a flowchart's thick edges.
fn stroke(pen: &Pen<'_>) -> Stroke {
    Stroke {
        dashed: pen.dash.iter().any(|&d| d > 0.0),
        heavy: pen.width >= 3.0,
    }
}

/// The character of a marker pointing a way, which is in cells.
fn marker(kind: MarkerType, (dx, dy): (f64, f64), charset: Charset) -> char {
    use MarkerType as M;
    // 0 right, 1 down, 2 left, 3 up.
    let way = if dx.abs() >= dy.abs() {
        if dx >= 0.0 { 0 } else { 2 }
    } else if dy >= 0.0 {
        1
    } else {
        3
    };
    let pick = |chars: [char; 4]| chars[way];
    match (charset, kind) {
        (
            Charset::Ascii,
            M::ArrowCircle | M::ClassAggregation | M::ClassLollipop | M::ErZeroOrOne,
        ) => 'o',
        (Charset::Ascii, M::ArrowCross) => 'x',
        (Charset::Ascii, M::ClassComposition | M::ErMdParent) => '*',
        (Charset::Ascii, M::ErOnlyOne | M::RequirementContains) => '+',
        (Charset::Ascii, M::ErOneOrMore | M::ErZeroOrMore) => pick(['<', '^', '>', 'v']),
        (Charset::Ascii, _) => pick(['>', 'v', '<', '^']),
        (_, M::ArrowCircle) => '●',
        (_, M::ArrowCross) => '×',
        (_, M::ClassExtension | M::UsecaseExtension | M::RequirementArrow) => {
            pick(['▷', '▽', '◁', '△'])
        }
        (_, M::ClassComposition | M::ErMdParent) => '◆',
        (_, M::ClassAggregation) => '◇',
        (_, M::ClassLollipop | M::ErZeroOrOne) => '○',
        (_, M::ErOnlyOne) => '┼',
        (_, M::ErOneOrMore | M::ErZeroOrMore) => pick(['<', '^', '>', 'v']),
        (_, M::RequirementContains) => '⊕',
        (_, M::SequenceOpen | M::ClassDependency) => pick(['>', 'v', '<', '^']),
        _ => pick(['▶', '▼', '◀', '▲']),
    }
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
