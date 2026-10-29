//! A diagram laid out: a list of items, each a thing of the diagram, where it is and what it
//! is, in the order they are drawn, but not how they look.
//!
//! An item says what it is by its [`Kind`], its [`Role`], as "flowchart.node", and, if it has
//! an outline, by its [`Shape`], as "diam". Its colors are tokens of the theme, settled only
//! when it is drawn, so a scene is drawn in any theme without being laid out again.
//!
//! An item also carries what a plain backend needs to draw it: its outline as parts, paths
//! and boxes, for an edge the markers at its ends, and a label laid out. A backend with a
//! look of its own may draw an item its own way from its role, shape, bounds and, for an
//! edge, its route instead.

use std::borrow::Cow;
use std::fmt;

use crate::geom::{Point, Rect};
use crate::marker::Marker;
use crate::path::{Op, Path, Step};
use crate::style::Style;
use crate::text::Label;

/// A diagram laid out. Every number in a scene that `limu` gives that places or sizes
/// something is finite. Where Pele would draw NaN, layout gives an error instead.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Scene {
    /// The kind of diagram: "flowchart", "sequence"… A scene `limu` lays out names it as
    /// its `DiagramType::name` does, which `DiagramType::from_name` reads back.
    pub diagram: String,
    /// How wide the diagram is.
    pub width: f64,
    /// How tall the diagram is.
    pub height: f64,
    /// The size of text with no size of its own.
    pub font_size: f64,
    /// The diagram's title for assistive technology, its accTitle.
    pub title: Option<String>,
    /// The diagram's description for assistive technology, its accDescr.
    pub desc: Option<String>,
    /// The items, in the order they are drawn.
    pub items: Vec<Item>,
}

/// The sort of thing an item is, with what only an edge has.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Kind {
    /// A shape, most often with a label: a flowchart's node, an actor.
    Node,
    /// A box that holds others: a subgraph, a loop, a lane.
    Group,
    /// A line from one thing to another, with markers at its ends.
    Edge(Edge),
    /// A label on its own: a title, an edge's label, an axis's.
    Text,
    /// A shape with no label: a lifeline, a tick, a slice of a pie.
    Mark,
}

impl Kind {
    /// The kind's name: "node", "group", "edge", "text" or "mark".
    pub fn name(&self) -> &'static str {
        match self {
            Kind::Node => "node",
            Kind::Group => "group",
            Kind::Edge(_) => "edge",
            Kind::Text => "text",
            Kind::Mark => "mark",
        }
    }

    /// The edge, if the item is one.
    pub fn edge(&self) -> Option<&Edge> {
        match self {
            Kind::Edge(e) => Some(e),
            _ => None,
        }
    }
}

/// What an edge has that no other item has: the route it takes and the markers at its ends.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Edge {
    /// The route the edge takes, as the corners of a line that runs straight between them.
    /// Its curves are in the item's parts. Every edge has a route. A self-loop's, which no
    /// layout routes, is the points and control points its curve is drawn about.
    pub route: Vec<Point>,
    /// The markers at its start and its end, where it has them.
    pub ends: [Option<Marker>; 2],
}

/// A name of the scene's: a role, or a shape.
macro_rules! name {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "serde", serde(transparent))]
        pub struct $name(Cow<'static, str>);

        impl $name {
            /// The name of a string the program holds, without copying it.
            pub const fn new(name: &'static str) -> $name {
                $name(Cow::Borrowed(name))
            }

            /// The name, as a string.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&'static str> for $name {
            fn from(name: &'static str) -> $name {
                $name::new(name)
            }
        }

        impl From<String> for $name {
            fn from(name: String) -> $name {
                $name(Cow::Owned(name))
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl PartialEq<str> for $name {
            fn eq(&self, other: &str) -> bool {
                *self.0 == *other
            }
        }

        impl PartialEq<&str> for $name {
            fn eq(&self, other: &&str) -> bool {
                *self.0 == **other
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

name! {
    /// Which part of which diagram an item is, as "flowchart.node" or "sequence.lifeline".
    /// Styles are given by role. Each diagram's module in `limu` names its roles.
    Role
}

name! {
    /// An item's outline, by Mermaid's name: "rect", "rounded", "diam", "cyl"…
    Shape
}

/// A thing of a diagram, as it is drawn: a node, a group, an edge, a label on its own or a
/// mark.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Item {
    /// What sort of thing it is.
    pub kind: Kind,
    /// Which part of which diagram it is.
    pub role: Role,
    /// Its outline's name, where it has one.
    pub shape: Option<Shape>,
    /// The id the diagram's text gave it, if any.
    pub id: Option<String>,
    /// The classes the diagram's author gave the item, by name: with class statements and
    /// classDef's default, with :::name, or in a list of classes where the diagram takes one,
    /// as a kanban card or a tree view's entry does. A class's style is already in
    /// [`author`](Item::author).
    pub class: Vec<String>,
    /// What the diagram says the item is beyond its role, which Pele writes among the classes
    /// of what it draws: a C4 element's kind ("person", "system_db"…) and "external", a C4
    /// relation's kind ("rel", "birel"…) and a deployment node's ("node", "nodeL",
    /// "nodeR"), a gantt task's state ("done", "active", "crit", "milestone",
    /// "unscheduled"), the branch a gantt section or a mindmap's node or edge is in
    /// ("section-0"…), a wardley component's sourcing ("build", "buy", "outsource",
    /// "market"), "reset" on an event modeling frame that resets, and "clickable", which
    /// Mermaid gives what has a click. The SVG writer writes the tags and then the classes as
    /// the element's classes, for a host's CSS.
    pub tags: Vec<String>,
    /// Where the item is. It is the box of its shape for a node or a group, and the box its
    /// path lies in for an edge, the control points of its curves included. For a mark it is
    /// the box of what it draws. For a text item it is the box the diagram sets its label in,
    /// as an edge label's patch, or else the box its label is drawn in.
    pub bounds: Rect,
    /// The pieces of its outline. A backend that draws the shape its own way ignores them.
    pub parts: Vec<Part>,
    /// Its label, laid out and placed.
    pub label: Option<Placed>,
    /// An image or an icon the item shows, as a flowchart's node may.
    pub picture: Option<Picture>,
    /// The diagram's own style for the item.
    pub style: Style,
    /// What the author set with classDef and style lines.
    pub author: Style,
    /// Where the item leads, when it is a link.
    pub link: Option<String>,
    /// The text shown when the item is pointed at.
    pub tooltip: Option<String>,
}

impl Item {
    /// An item of a kind and a role, with nothing else.
    pub fn new(kind: Kind, role: impl Into<Role>) -> Item {
        Item {
            kind,
            role: role.into(),
            shape: None,
            id: None,
            class: Vec::new(),
            tags: Vec::new(),
            bounds: Rect::default(),
            parts: Vec::new(),
            label: None,
            picture: None,
            style: Style::default(),
            author: Style::default(),
            link: None,
            tooltip: None,
        }
    }

    /// The edge, if the item is one.
    pub fn edge(&self) -> Option<&Edge> {
        self.kind.edge()
    }

    /// The box the item's label is drawn in, or an empty one for an item with no label.
    pub fn label_bounds(&self) -> Rect {
        match &self.label {
            Some(p) => p.bounds(),
            None => Rect::default(),
        }
    }
}

/// A label laid out, and where it is drawn: about a point, centered on it or with its lines
/// starting or ending there, as its alignment says, and turned by a number of degrees about
/// it.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Placed {
    /// The label.
    pub label: Label,
    /// The point it is drawn about.
    pub at: Point,
    /// Where its lines are about the point, across.
    pub align: Align,
    /// How many degrees it is turned about the point, clockwise.
    pub rotate: f64,
}

impl Placed {
    /// A label drawn centered on a point, not turned.
    pub fn new(label: Label, at: Point) -> Placed {
        Placed {
            label,
            at,
            align: Align::Middle,
            rotate: 0.0,
        }
    }

    /// The box the label is drawn in: the box the turned label lies in. It is empty for an
    /// empty label.
    pub fn bounds(&self) -> Rect {
        if self.label.is_empty() {
            return Rect::default();
        }
        let r = self.upright();
        if self.rotate == 0.0 {
            return r;
        }
        let (sin, cos) = (self.rotate * std::f64::consts::PI / 180.0).sin_cos();
        let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
        let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for (px, py) in [
            (r.x, r.y),
            (r.x + r.w, r.y),
            (r.x, r.y + r.h),
            (r.x + r.w, r.y + r.h),
        ] {
            let (dx, dy) = (px - self.at.x, py - self.at.y);
            let (tx, ty) = (
                self.at.x + dx * cos - dy * sin,
                self.at.y + dx * sin + dy * cos,
            );
            (x0, y0, x1, y1) = (x0.min(tx), y0.min(ty), x1.max(tx), y1.max(ty));
        }
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    }
}

impl Placed {
    /// The box the label is drawn in before it is turned.
    pub(crate) fn upright(&self) -> Rect {
        let l = &self.label;
        let x = match self.align {
            Align::Middle => self.at.x - l.width / 2.0,
            Align::Start => self.at.x,
            Align::End => self.at.x - l.width,
        };
        Rect::new(x, self.at.y - l.height / 2.0, l.width, l.height)
    }
}

/// Where a label's lines are about the point it is drawn at, across. Down, the label is
/// always centered on it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Align {
    /// Each line centered on the point.
    #[default]
    Middle,
    /// Each line starting at it.
    Start,
    /// Each line ending at it.
    End,
}

/// An image, or an icon by name, drawn in a box.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Picture {
    /// What is drawn.
    pub source: PictureSource,
    /// The box it is drawn in.
    pub bounds: Rect,
}

/// What a picture draws.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PictureSource {
    /// An image, by its address, checked as the diagram's options allow.
    Image {
        /// The image's address.
        src: String,
        /// Whether the image keeps its proportions in the box. It fills it otherwise.
        fit: bool,
    },
    /// An icon, by its name, as "fa:fa-car", drawn in the color of text.
    Icon(String),
}

/// A piece of an item's outline, and how it is painted in the item's style.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Part {
    /// Its outline.
    pub outline: Outline,
    /// How it is painted.
    pub paint: Painting,
}

impl Part {
    /// A part outlined by a path.
    pub fn path(path: Path, paint: Painting) -> Part {
        Part {
            outline: Outline::Path(path),
            paint,
        }
    }

    /// A part that is a box, its corners rounded by the style's radius when rounded is set.
    pub fn rect(rect: Rect, rounded: bool, paint: Painting) -> Part {
        Part {
            outline: Outline::Box { rect, rounded },
            paint,
        }
    }
}

/// The outline of a part: a path, or a box. A box whose corners take the theme's radius stays
/// a box, so that the radius stays a matter of style.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Outline {
    /// A path.
    Path(Path),
    /// A box.
    Box {
        /// Where the box is.
        rect: Rect,
        /// Whether its corners are rounded by the style's radius.
        rounded: bool,
    },
}

/// How a part is painted in its item's style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Painting {
    /// Fills the part and strokes its outline.
    #[default]
    FillAndStroke,
    /// Strokes the part only: an edge, or a detail on a shape, as the lid of a cylinder.
    StrokeOnly,
    /// Fills the part and does not stroke it: the body of a datastore, which lines close at
    /// its top and bottom only.
    FillOnly,
    /// Fills the part in the theme's color of lines, and strokes it: the dot of a final
    /// state, the bar of a fork.
    Solid,
    /// Fills the part with the theme's background, and strokes it: the ring of a final
    /// state, which hides what is beneath it.
    Hollow,
    /// Fills the part with the theme's background and does not stroke it: a stripe of an
    /// entity's table. When the item's author set a fill, it is the background at 60%
    /// opacity, laid over the parts drawn before it, as Pele draws an entity's stripes.
    Shade,
    /// Fills the part in the color of the item's stroke without its opacity, as a marker is
    /// filled, and does not stroke it: the dot of a central connection on a sequence
    /// diagram's message.
    Dot,
}

/// The spacing of a grid a scene is snapped to, across and down. A terminal's cells are a
/// grid whose spacings differ.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Grid {
    /// The spacing across.
    pub x: f64,
    /// The spacing down.
    pub y: f64,
}

impl Grid {
    /// A grid of squares of a side.
    pub const fn square(side: f64) -> Grid {
        Grid { x: side, y: side }
    }
}

impl Scene {
    /// Moves everything in the scene onto a grid: the corners of boxes, the points of paths
    /// and routes, the tips of markers and the points labels are drawn about. It is for a
    /// backend that draws on whole pixels or cells, after layout. A box's sides are snapped
    /// each on its own, so boxes that touch go on touching. The scene's size is rounded up.
    ///
    /// A path with straight lines in it has each point it passes through snapped, so that its
    /// lines meet the boxes, markers and lines they lead to, and each control point of a
    /// curve moved with the point it leans on, so that a rounded corner still turns from one
    /// line into the next. A path of curves alone, such as a circle, or one of a mark, such
    /// as an icon or an arrowhead, is moved as a whole, by what puts its first point on the
    /// grid, so that it keeps its shape, as rounding each of its points would not. A mark
    /// with no shape of its own whose lines all run straight across or down, such as an
    /// axis or a divider, keeps its shape with each point snapped, as a box does, and is
    /// snapped so, so that its corners are on the grid as the boxes about it are. A grid
    /// whose spacing is not above zero leaves the scene as it is.
    pub fn snap(&mut self, grid: Grid) {
        if !(grid.x > 0.0 && grid.y > 0.0) {
            return;
        }
        let ax = |v: f64| (v / grid.x).round() * grid.x;
        let ay = |v: f64| (v / grid.y).round() * grid.y;
        let rect = |r: Rect| {
            let (x0, y0) = (ax(r.x), ay(r.y));
            Rect::new(x0, y0, ax(r.x + r.w) - x0, ay(r.y + r.h) - y0)
        };
        let point = |p: &mut Point| *p = Point::new(ax(p.x), ay(p.y));
        self.width = (self.width / grid.x).ceil() * grid.x;
        self.height = (self.height / grid.y).ceil() * grid.y;
        for it in &mut self.items {
            it.bounds = rect(it.bounds);
            let mark = matches!(it.kind, Kind::Mark);
            let shaped = it.shape.is_some();
            for part in &mut it.parts {
                match &mut part.outline {
                    Outline::Box { rect: r, .. } => *r = rect(*r),
                    Outline::Path(p) => {
                        let through =
                            p.ops.contains(&Op::LineTo) && (!mark || !shaped && square(p));
                        snap_path(p, through, &ax, &ay);
                    }
                }
            }
            if let Kind::Edge(e) = &mut it.kind {
                e.route.iter_mut().for_each(point);
                for m in e.ends.iter_mut().flatten() {
                    point(&mut m.at);
                }
            }
            if let Some(l) = &mut it.label {
                point(&mut l.at);
            }
            if let Some(p) = &mut it.picture {
                p.bounds = rect(p.bounds);
            }
        }
    }
}

/// Whether a path is of lines alone, each running straight across or down.
fn square(p: &Path) -> bool {
    let mut at = Point::default();
    let mut start = at;
    p.steps().all(|step| {
        let (from, to) = match step {
            Step::MoveTo(q) => {
                (at, start) = (q, q);
                return true;
            }
            Step::LineTo(q) => (at, q),
            Step::Close => (at, start),
            Step::QuadTo(..) | Step::CubeTo(..) => return false,
        };
        at = to;
        from.x == to.x || from.y == to.y
    })
}

/// Puts a path on the grid ax and ay give, as [`Scene::snap`] says: through its points, when
/// through is set, and otherwise as a whole.
fn snap_path(p: &mut Path, through: bool, ax: &dyn Fn(f64) -> f64, ay: &dyn Fn(f64) -> f64) {
    let a = &mut p.args;
    if a.len() < 2 {
        return;
    }
    if !through {
        let (dx, dy) = (ax(a[0]) - a[0], ay(a[1]) - a[1]);
        let (x, y) = (ax(a[0]), ay(a[1]));
        for k in (2..a.len() - 1).step_by(2) {
            a[k] += dx;
            a[k + 1] += dy;
        }
        (a[0], a[1]) = (x, y);
        return;
    }
    // The point a step starts from, before and after it is snapped.
    let (mut x0, mut y0, mut sx0, mut sy0) = (0.0, 0.0, 0.0, 0.0);
    let mut k = 0;
    for op in &p.ops {
        let n = op.args();
        if n == 0 {
            continue;
        }
        let (x, y) = (a[k + n - 2], a[k + n - 1]);
        let (sx, sy) = (ax(x), ay(y));
        match op {
            Op::QuadTo => {
                // The control point moves along each axis with the end nearer to it on that
                // axis, as a corner's control point sits level with one line and in line with
                // the other.
                a[k] += nearer(a[k], x0, sx0 - x0, x, sx - x);
                a[k + 1] += nearer(a[k + 1], y0, sy0 - y0, y, sy - y);
            }
            Op::CubeTo => {
                a[k] += sx0 - x0;
                a[k + 1] += sy0 - y0;
                a[k + 2] += sx - x;
                a[k + 3] += sy - y;
            }
            _ => {}
        }
        (a[k + n - 2], a[k + n - 1]) = (sx, sy);
        (x0, y0, sx0, sy0) = (x, y, sx, sy);
        k += n;
    }
}

/// How far the start moved, d0, when v is no farther from v0 than from v1, and otherwise
/// how far the end moved, d1.
fn nearer(v: f64, v0: f64, d0: f64, v1: f64, d1: f64) -> f64 {
    if (v - v0).abs() <= (v - v1).abs() {
        d0
    } else {
        d1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marker::MarkerType;
    use crate::text::Span;

    fn near(a: Rect, b: Rect) -> bool {
        [(a.x, b.x), (a.y, b.y), (a.w, b.w), (a.h, b.h)]
            .iter()
            .all(|(p, q)| (p - q).abs() < 1e-9)
    }

    fn path(f: impl FnOnce(&mut Path) -> &mut Path) -> Path {
        let mut p = Path::new();
        f(&mut p);
        p
    }

    fn marker(x: f64, y: f64) -> Marker {
        Marker {
            kind: MarkerType::ArrowPoint,
            at: Point::new(x, y),
            dx: 1.0,
            dy: 0.0,
        }
    }

    #[test]
    fn label_bounds() {
        let label = Label {
            lines: vec![vec![Span {
                text: "x".into(),
                ..Span::default()
            }]],
            width: 20.0,
            height: 10.0,
            ..Label::default()
        };
        for (align, rotate, want) in [
            (Align::Middle, 0.0, Rect::new(90.0, 45.0, 20.0, 10.0)),
            (Align::Start, 0.0, Rect::new(100.0, 45.0, 20.0, 10.0)),
            (Align::End, 0.0, Rect::new(80.0, 45.0, 20.0, 10.0)),
            (Align::Middle, -90.0, Rect::new(95.0, 40.0, 10.0, 20.0)),
        ] {
            let p = Placed {
                label: label.clone(),
                at: Point::new(100.0, 50.0),
                align,
                rotate,
            };
            assert!(
                near(p.bounds(), want),
                "{align:?} {rotate}: {:?}, want {want:?}",
                p.bounds()
            );
        }
        assert_eq!(
            Item::new(Kind::Text, "x").label_bounds(),
            Rect::default(),
            "a box for no label"
        );
    }

    #[test]
    fn snap() {
        let mut node = Item::new(Kind::Node, "x.node");
        node.bounds = Rect::new(0.4, 0.4, 9.2, 3.3);
        node.parts = vec![Part::path(
            path(|p| p.move_to(0.4, 0.6).line_to(10.5, 3.2)),
            Painting::StrokeOnly,
        )];
        node.label = Some(Placed::new(Label::default(), Point::new(4.6, 2.5)));
        let edge = Item::new(
            Kind::Edge(Edge {
                route: vec![Point::new(1.2, 3.7)],
                ends: [Some(marker(2.49, 0.0)), None],
            }),
            "x.edge",
        );
        let mut sc = Scene {
            width: 10.2,
            height: 7.0,
            items: vec![node, edge],
            ..Scene::default()
        };
        sc.snap(Grid::square(1.0));
        assert_eq!((sc.width, sc.height), (11.0, 7.0));
        assert_eq!(sc.items[0].bounds, Rect::new(0.0, 0.0, 10.0, 4.0));
        assert_eq!(
            sc.items[0].parts[0].outline,
            Outline::Path(path(|p| p.move_to(0.0, 1.0).line_to(11.0, 3.0)))
        );
        let e = sc.items[1].edge().unwrap();
        assert_eq!(
            (e.route[0], e.ends[0].unwrap().at),
            (Point::new(1.0, 4.0), Point::new(2.0, 0.0))
        );

        // A path with curves moves as a whole, by what puts its first point on the grid.
        let circle = path(|p| p.ellipse(10.3, 20.6, 3.3, 3.3));
        let mut it = Item::new(Kind::Node, "x.node");
        it.parts = vec![Part::path(circle.clone(), Painting::FillAndStroke)];
        let mut sc = Scene {
            items: vec![it],
            ..Scene::default()
        };
        sc.snap(Grid::square(1.0));
        let Outline::Path(moved) = &sc.items[0].parts[0].outline else {
            unreachable!()
        };
        assert_eq!(
            (moved.args[0], moved.args[1]),
            (14.0, 21.0),
            "a circle's first point"
        );
        for k in (2..moved.args.len()).step_by(2) {
            let (dx, dy) = (
                moved.args[k] - circle.args[k],
                moved.args[k + 1] - circle.args[k + 1],
            );
            assert!(
                (dx - 0.4).abs() < 1e-9 && (dy - 0.4).abs() < 1e-9,
                "point {} moved by {dx},{dy}",
                k / 2
            );
        }

        // An edge's lines land on the grid, and a rounded corner's control points stay on the
        // lines it joins. A mark's path of the same steps moves as a whole, but one of lines
        // across and down alone has its points snapped, unless it has a shape of its own, as
        // an icon.
        let corner = path(|p| {
            p.move_to(0.4, 0.6)
                .line_to(10.3, 0.6)
                .cube_to(12.3, 0.6, 13.3, 1.6, 13.3, 3.6)
                .line_to(13.3, 20.2)
                .quad_to(13.3, 22.6, 15.7, 22.6)
        });
        let mut edge = Item::new(Kind::Edge(Edge::default()), "x.edge");
        edge.parts = vec![Part::path(corner.clone(), Painting::StrokeOnly)];
        let mut mark = Item::new(Kind::Mark, "x.mark");
        mark.parts = vec![Part::path(corner, Painting::StrokeOnly)];
        let square = path(|p| p.move_to(0.4, 0.6).line_to(0.4, 20.2).line_to(15.7, 20.2));
        let mut axis = Item::new(Kind::Mark, "x.axis");
        axis.parts = vec![Part::path(square.clone(), Painting::StrokeOnly)];
        let mut icon = Item::new(Kind::Mark, "x.icon");
        icon.shape = Some(Shape::new("icon"));
        icon.parts = vec![Part::path(square, Painting::StrokeOnly)];
        let mut sc = Scene {
            items: vec![edge, mark, axis, icon],
            ..Scene::default()
        };
        sc.snap(Grid::square(1.0));
        let args = |i: usize| match &sc.items[i].parts[0].outline {
            Outline::Path(p) => p.args.clone(),
            _ => unreachable!(),
        };
        let want = [
            0.0, 1.0, 10.0, 1.0, 12.0, 1.0, 13.0, 2.0, 13.0, 4.0, 13.0, 20.0, 13.0, 23.0, 16.0,
            23.0,
        ];
        assert_eq!(args(0), want, "an edge's corner");
        assert_eq!(
            args(2),
            [0.0, 1.0, 0.0, 20.0, 16.0, 20.0],
            "an axis's corner"
        );
        let icon = args(3);
        assert!(
            icon[0] == 0.0 && icon[1] == 1.0 && (icon[3] - 20.6).abs() < 1e-9,
            "an icon's path moved to {icon:?}"
        );
        let a = args(1);
        assert!(
            a[0] == 0.0
                && a[1] == 1.0
                && (a[10] - 12.9).abs() < 1e-9
                && (a[11] - 20.6).abs() < 1e-9,
            "a mark's path moved to {a:?}"
        );

        // A grid's spacings may differ, as a terminal's cells do.
        let mut it = Item::new(Kind::Node, "x.node");
        it.bounds = Rect::new(1.2, 1.2, 10.0, 10.0);
        let mut sc = Scene {
            width: 13.0,
            height: 13.0,
            items: vec![it],
            ..Scene::default()
        };
        sc.snap(Grid { x: 1.0, y: 2.0 });
        assert_eq!(sc.items[0].bounds, Rect::new(1.0, 2.0, 10.0, 10.0));
        assert_eq!((sc.width, sc.height), (13.0, 14.0));
    }

    #[test]
    fn names() {
        const NODE: Role = Role::new("flowchart.node");
        assert_eq!(NODE, "flowchart.node");
        assert_eq!(Role::from(String::from("flowchart.node")), NODE);
        assert_eq!(Shape::new("diam").to_string(), "diam");
    }
}
