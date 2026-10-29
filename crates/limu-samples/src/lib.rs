//! The scenes every backend of limu-rs draws in its tests, so that each backend is checked on
//! the same things and their drawings can be set side by side.
//!
//! There are two kinds. The scenes of [`primitives`] are built by hand, each to try one
//! thing a canvas does: pens, markers, paintings, text, layers, paths and pictures. The
//! scenes of [`diagrams`] are one diagram of each type, laid out by limu. Their sources are
//! examples of Pele's documentation, most of them taken from Mermaid's, both under the MIT
//! License.
//!
//! Text is measured as Arial, as limu measures it by default. Liberation Sans is as wide, so
//! a backend that draws in it fills the room laid out for its text.

use limu::text::Arial;
use limu_scene::style::{Cap, Color, Join, Paint, Style};
use limu_scene::text::{self, Face, Label, Span};
use limu_scene::{
    Align, Edge, Item, Kind, Marker, MarkerType, Painting, Part, Path, Picture, PictureSource,
    Placed, Point, Rect, Scene,
};

/// A scene a backend draws in its tests, and its name, which names its golden files.
pub struct Sample {
    /// The name.
    pub name: &'static str,
    /// The scene.
    pub scene: Scene,
}

/// Every sample, the primitives first.
pub fn all() -> Vec<Sample> {
    let mut all = primitives();
    all.extend(diagrams());
    all
}

/// The scenes built by hand, each trying one thing a canvas does, after a small flowchart
/// that tries a little of each.
pub fn primitives() -> Vec<Sample> {
    vec![
        Sample {
            name: "sample",
            scene: sample(),
        },
        Sample {
            name: "pens",
            scene: pens(),
        },
        Sample {
            name: "markers",
            scene: markers(),
        },
        Sample {
            name: "paintings",
            scene: paintings(),
        },
        Sample {
            name: "text",
            scene: texts(),
        },
        Sample {
            name: "layers",
            scene: layers(),
        },
        Sample {
            name: "paths",
            scene: paths(),
        },
        Sample {
            name: "pictures",
            scene: pictures(),
        },
    ]
}

macro_rules! sources {
    ($($name:literal),* $(,)?) => {
        [$(($name, include_str!(concat!("../samples/", $name, ".mmd")))),*]
    };
}

/// The sources of the diagrams, by the names of their samples.
const SOURCES: [(&str, &str); 33] = sources!(
    "agentflow",
    "architecture",
    "block",
    "c4",
    "class",
    "cynefin",
    "er",
    "eventmodeling",
    "flowchart",
    "gantt",
    "gitgraph",
    "info",
    "ishikawa",
    "journey",
    "kanban",
    "mindmap",
    "packet",
    "pie",
    "quadrant",
    "radar",
    "railroad",
    "requirement",
    "sankey",
    "sequence",
    "state",
    "swimlane",
    "timeline",
    "treemap",
    "treeview",
    "usecase",
    "venn",
    "wardley",
    "xychart",
);

/// One diagram of each type, laid out by limu with its default options.
pub fn diagrams() -> Vec<Sample> {
    diagrams_with(&limu::layout::Options::default())
}

/// One diagram of each type, laid out by limu with options, as a backend that measures text
/// its own way lays them out.
pub fn diagrams_with(o: &limu::layout::Options<'_>) -> Vec<Sample> {
    SOURCES
        .iter()
        .map(|&(name, src)| Sample {
            name,
            scene: limu::layout(src, o).unwrap_or_else(|e| panic!("{name}: {e}")),
        })
        .collect()
}

/// The parts of an icon the samples draw, on a box of 24 by 24, as a canvas's icons give
/// them. There is one, "ring". A sample also asks for "missing", which a canvas has no icon
/// of.
pub fn icon(name: &str) -> Option<Vec<Part>> {
    (name == "ring").then(|| {
        let mut ring = Path::new();
        ring.ellipse(12.0, 12.0, 9.0, 9.0);
        let mut dot = Path::new();
        dot.ellipse(12.0, 12.0, 3.0, 3.0);
        vec![
            Part::path(ring, Painting::StrokeOnly),
            Part::path(dot, Painting::FillOnly),
        ]
    })
}

/// An image, its pixels in rows from the top, each red, green, blue and alpha, not
/// premultiplied.
pub struct Image {
    /// How many pixels wide it is.
    pub width: u32,
    /// How many pixels high it is.
    pub height: u32,
    /// The pixels.
    pub rgba: Vec<u8>,
}

/// The address of the image the samples draw.
pub const IMAGE: &str = "checker.png";

/// The image of an address, as a canvas's images give it, or none for an address other than
/// [`IMAGE`]. It is 16 by 12, a quarter in each of red, green, blue and gold, so that an image
/// turned or flipped shows it, in squares of 4 that alternate with a lighter shade.
pub fn image(src: &str) -> Option<Image> {
    if src != IMAGE {
        return None;
    }
    let (width, height) = (16, 12);
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let [r, g, b] = match (x < width / 2, y < height / 2) {
                (true, true) => [220, 50, 50],
                (false, true) => [50, 160, 70],
                (true, false) => [50, 90, 200],
                (false, false) => [230, 180, 40],
            };
            let light = (x / 4 + y / 4) % 2 == 1;
            let shade = |c: u8| if light { c / 2 + 128 } else { c };
            rgba.extend([shade(r), shade(g), shade(b), 255]);
        }
    }
    Some(Image {
        width,
        height,
        rgba,
    })
}

/// A label of one line of spans, each a text and its face.
fn label(spans: &[(&str, Face)], size: f64) -> Label {
    lines(&[spans], size)
}

/// A label of lines of spans, measured as Arial, its lines 1.5 times its size apart.
fn lines(lines: &[&[(&str, Face)]], size: f64) -> Label {
    let lines: Vec<Vec<Span>> = lines
        .iter()
        .map(|spans| {
            spans
                .iter()
                .map(|&(t, face)| Span {
                    text: t.into(),
                    face,
                    width: text::width(&Arial, t, size, face),
                    ..Span::default()
                })
                .collect()
        })
        .collect();
    let widths: Vec<f64> = lines
        .iter()
        .map(|l| l.iter().map(|s| s.width).sum())
        .collect();
    let line_height = size * 1.5;
    Label {
        width: widths.iter().copied().fold(0.0, f64::max),
        height: line_height * lines.len() as f64,
        lines,
        widths,
        size,
        line_height,
    }
}

/// A mark of one part, a path painted so, in a style.
fn mark(path: Path, paint: Painting, style: Style) -> Item {
    let mut it = Item::new(Kind::Mark, "sample.mark");
    it.bounds = path.bounds();
    it.parts = vec![Part::path(path, paint)];
    it.style = style;
    it
}

/// A line from a to b, stroked in a style.
fn line(a: (f64, f64), b: (f64, f64), style: Style) -> Item {
    let mut p = Path::new();
    p.move_to(a.0, a.1).line_to(b.0, b.1);
    mark(p, Painting::StrokeOnly, style)
}

/// A dot of radius 2 in the foreground's color, to show a point.
fn dot(at: Point) -> Item {
    let mut p = Path::new();
    p.ellipse(at.x, at.y, 2.0, 2.0);
    mark(
        p,
        Painting::FillOnly,
        Style {
            fill: Some(Paint::FG),
            ..Style::default()
        },
    )
}

/// A text item of a label, placed about a point, aligned and turned so.
fn words(label: Label, at: Point, align: Align, rotate: f64, style: Style) -> Item {
    let mut it = Item::new(Kind::Text, "sample.text");
    it.label = Some(Placed {
        align,
        rotate,
        ..Placed::new(label, at)
    });
    it.style = style;
    it
}

/// The style of a line stroked in the theme's line color, w wide.
fn pen(w: f64) -> Style {
    Style {
        stroke: Some(Paint::LINE),
        fill: Some(Paint::NONE),
        stroke_width: Some(w),
        ..Style::default()
    }
}

fn scene(name: &str, width: f64, height: f64, items: Vec<Item>) -> Scene {
    Scene {
        diagram: name.into(),
        width,
        height,
        font_size: 16.0,
        items,
        ..Scene::default()
    }
}

/// A small flowchart: a group holding a box with an author's fill, a diamond, a dashed edge
/// between them with its label, a note at half opacity, a title turned on its side with a
/// halo, and an icon.
fn sample() -> Scene {
    let mut group = Item::new(Kind::Group, "flowchart.cluster");
    group.bounds = Rect::new(8.0, 8.0, 224.0, 204.0);
    group.parts = vec![Part::rect(group.bounds, true, Painting::FillAndStroke)];
    group.label = Some(Placed::new(
        label(&[("Group & co", Face::REGULAR)], 16.0),
        Point::new(120.0, 24.0),
    ));
    group.style = Style {
        fill: Some(Paint::SURFACE_ALT.alpha(0.5)),
        stroke: Some(Paint::BORDER),
        text: Some(Paint::MUTED),
        ..Style::default()
    };

    let mut path = Path::new();
    path.move_to(100.0, 80.0)
        .cube_to(100.0, 100.0, 100.0, 100.0, 100.0, 113.0);
    let tip = Marker {
        kind: MarkerType::ArrowPoint,
        at: Point::new(100.0, 120.0),
        dx: 0.0,
        dy: 1.0,
    };
    let mut edge = Item::new(
        Kind::Edge(Edge {
            route: vec![Point::new(100.0, 80.0), Point::new(100.0, 120.0)],
            ends: [None, Some(tip)],
        }),
        "flowchart.edge",
    );
    edge.parts = vec![Part::path(path, Painting::StrokeOnly)];
    edge.style = Style {
        cap: Some(Cap::Round),
        dash: Some(vec![3.0]),
        ..pen(1.0)
    };

    let yes = label(&[("Yes", Face::ITALIC)], 14.0);
    let mut edge_label = Item::new(Kind::Text, "flowchart.edge-label");
    let patch = Rect::new(
        100.0 - yes.width / 2.0 - 4.0,
        100.0 - yes.height / 2.0,
        yes.width + 8.0,
        yes.height,
    );
    edge_label.parts = vec![Part::rect(patch, true, Painting::FillAndStroke)];
    edge_label.bounds = patch;
    edge_label.label = Some(Placed::new(yes, Point::new(100.0, 100.0)));
    edge_label.style = Style {
        fill: Some(Paint::BG),
        stroke: Some(Paint::NONE),
        radius: Some(3.0),
        ..Style::default()
    };

    let mut a = Item::new(Kind::Node, "flowchart.node");
    a.bounds = Rect::new(60.0, 40.0, 80.0, 40.0);
    a.parts = vec![Part::rect(a.bounds, true, Painting::FillAndStroke)];
    a.label = Some(Placed::new(
        label(&[("Start", Face::UNDERLINE)], 16.0),
        Point::new(100.0, 60.0),
    ));
    a.style = Style {
        fill: Some(Paint::SURFACE),
        stroke: Some(Paint::BORDER),
        ..Style::default()
    };
    a.author = Style {
        fill: Some(Paint::own(Color::hex("#f9f"))),
        stroke_width: Some(2.0),
        ..Style::default()
    };

    let mut b = Item::new(Kind::Node, "flowchart.node");
    b.bounds = Rect::new(30.0, 120.0, 140.0, 70.0);
    let mut diamond = Path::new();
    diamond.polygon(&[
        (100.0, 120.0),
        (170.0, 155.0),
        (100.0, 190.0),
        (30.0, 155.0),
    ]);
    b.parts = vec![Part::path(diamond, Painting::FillAndStroke)];
    let is = label(
        &[
            ("Is it ", Face::REGULAR),
            ("ready", Face::BOLD),
            ("x", Face::MONO),
            ("2", Face::SUP),
        ],
        16.0,
    );
    b.label = Some(Placed::new(is, Point::new(100.0, 155.0)));
    b.style = Style {
        fill: Some(Paint::SURFACE),
        stroke: Some(Paint::BORDER),
        ..Style::default()
    };

    let mut note = Item::new(Kind::Node, "flowchart.node");
    note.bounds = Rect::new(150.0, 30.0, 60.0, 60.0);
    note.parts = vec![
        Part::rect(note.bounds, false, Painting::FillAndStroke),
        Part::rect(
            Rect::new(170.0, 50.0, 60.0, 30.0),
            false,
            Painting::FillAndStroke,
        ),
    ];
    note.style = Style {
        fill: Some(Paint::ACCENT),
        stroke: Some(Paint::LINE),
        opacity: Some(0.5),
        ..Style::default()
    };

    let title = words(
        label(&[("Sideways", Face::BOLD)], 16.0),
        Point::new(20.0, 120.0),
        Align::Middle,
        -90.0,
        Style {
            halo: Some(Paint::ACCENT),
            halo_width: Some(4.0),
            ..Style::default()
        },
    );

    let mut icon = Item::new(Kind::Mark, "flowchart.icon");
    icon.picture = Some(Picture {
        source: PictureSource::Icon("ring".into()),
        bounds: Rect::new(190.0, 180.0, 24.0, 24.0),
    });

    scene(
        "flowchart",
        240.0,
        220.0,
        vec![group, edge, edge_label, a, b, note, title, icon],
    )
}

/// Lines in each cap between guides at their ends, zigzags in each join, the one in mitres
/// with a turn too sharp for SVG's limit, dashes, widths, and a line at a part of its
/// opacity across another.
fn pens() -> Scene {
    let mut items = Vec::new();
    let guide = Style {
        stroke: Some(Paint::MUTED),
        ..pen(1.0)
    };
    items.push(line((24.0, 12.0), (24.0, 84.0), guide.clone()));
    items.push(line((136.0, 12.0), (136.0, 84.0), guide));
    for (i, cap) in [Cap::Butt, Cap::Round, Cap::Square].into_iter().enumerate() {
        let y = 24.0 + 24.0 * i as f64;
        items.push(line(
            (24.0, y),
            (136.0, y),
            Style {
                cap: Some(cap),
                ..pen(10.0)
            },
        ));
    }
    for (i, join) in [Join::Miter, Join::Round, Join::Bevel]
        .into_iter()
        .enumerate()
    {
        let x = 24.0 + 96.0 * i as f64;
        let mut p = Path::new();
        p.move_to(x, 150.0)
            .line_to(x + 25.0, 100.0)
            .line_to(x + 50.0, 150.0)
            .line_to(x + 50.0, 100.0);
        items.push(mark(
            p,
            Painting::StrokeOnly,
            Style {
                join: Some(join),
                ..pen(8.0)
            },
        ));
    }
    for (i, (dash, cap, w)) in [
        (vec![3.0], Cap::Butt, 2.0),
        (vec![8.0, 4.0], Cap::Butt, 2.0),
        (vec![8.0, 4.0, 2.0, 4.0], Cap::Butt, 2.0),
        (vec![0.1, 8.0], Cap::Round, 4.0),
    ]
    .into_iter()
    .enumerate()
    {
        let y = 176.0 + 16.0 * i as f64;
        items.push(line(
            (24.0, y),
            (296.0, y),
            Style {
                dash: Some(dash),
                cap: Some(cap),
                ..pen(w)
            },
        ));
    }
    for (i, w) in [0.5, 1.0, 3.0].into_iter().enumerate() {
        let x = 24.0 + 96.0 * i as f64;
        items.push(line((x, 248.0), (x + 80.0, 248.0), pen(w)));
    }
    items.push(line(
        (160.0, 262.0),
        (160.0, 290.0),
        Style {
            stroke: Some(Paint::FG),
            ..pen(2.0)
        },
    ));
    items.push(line(
        (24.0, 276.0),
        (296.0, 276.0),
        Style {
            stroke: Some(Paint::ACCENT),
            stroke_opacity: Some(0.4),
            ..pen(12.0)
        },
    ));
    scene("pens", 320.0, 300.0, items)
}

/// An edge ending in each marker, and two edges with a marker at both ends, one of them
/// slanting.
fn markers() -> Scene {
    let edge = |from: Point, to: Point, start: Option<MarkerType>, end: Option<MarkerType>| {
        let len = (to.x - from.x).hypot(to.y - from.y);
        let (ux, uy) = ((to.x - from.x) / len, (to.y - from.y) / len);
        let mut p = Path::new();
        p.move_to(from.x, from.y).line_to(to.x, to.y);
        let mut it = Item::new(
            Kind::Edge(Edge {
                route: vec![from, to],
                ends: [
                    start.map(|kind| Marker {
                        kind,
                        at: from,
                        dx: -ux,
                        dy: -uy,
                    }),
                    end.map(|kind| Marker {
                        kind,
                        at: to,
                        dx: ux,
                        dy: uy,
                    }),
                ],
            }),
            "sample.edge",
        );
        it.bounds = p.bounds();
        it.parts = vec![Part::path(p, Painting::StrokeOnly)];
        it.style = pen(1.5);
        it
    };
    let mut items = Vec::new();
    for (i, kind) in MarkerType::ALL.into_iter().enumerate() {
        let x = 24.0 + 110.0 * (i % 3) as f64;
        let y = 24.0 + 30.0 * (i / 3) as f64;
        items.push(edge(
            Point::new(x, y),
            Point::new(x + 70.0, y),
            None,
            Some(kind),
        ));
    }
    items.push(edge(
        Point::new(24.0, 250.0),
        Point::new(124.0, 250.0),
        Some(MarkerType::ArrowPoint),
        Some(MarkerType::ArrowPoint),
    ));
    items.push(edge(
        Point::new(160.0, 266.0),
        Point::new(280.0, 226.0),
        Some(MarkerType::ClassComposition),
        Some(MarkerType::ErZeroOrMore),
    ));
    scene("markers", 340.0, 280.0, items)
}

/// On a band of the theme's alternate surface, so that what is filled with the background
/// shows, a circle and a rounded box in each painting, with the painting's name under them,
/// and last a circle under a box shaded where the author set a fill.
fn paintings() -> Scene {
    let mut band = Path::new();
    band.polygon(&[(12.0, 12.0), (600.0, 12.0), (600.0, 120.0), (12.0, 120.0)]);
    let mut items = vec![mark(
        band,
        Painting::FillOnly,
        Style {
            fill: Some(Paint::SURFACE_ALT),
            ..Style::default()
        },
    )];
    let style = Style {
        fill: Some(Paint::ACCENT),
        stroke: Some(Paint::FG),
        stroke_width: Some(2.0),
        radius: Some(8.0),
        ..Style::default()
    };
    for (i, (paint, name)) in [
        (Painting::FillAndStroke, "FillAndStroke"),
        (Painting::StrokeOnly, "StrokeOnly"),
        (Painting::FillOnly, "FillOnly"),
        (Painting::Solid, "Solid"),
        (Painting::Hollow, "Hollow"),
        (Painting::Shade, "Shade"),
        (Painting::Dot, "Dot"),
    ]
    .into_iter()
    .enumerate()
    {
        let x = 24.0 + 72.0 * i as f64;
        let mut circle = Path::new();
        circle.ellipse(x + 24.0, 40.0, 20.0, 20.0);
        let mut it = Item::new(Kind::Node, "sample.node");
        it.bounds = Rect::new(x, 20.0, 48.0, 100.0);
        it.parts = vec![
            Part::path(circle, paint),
            Part::rect(Rect::new(x, 76.0, 48.0, 36.0), true, paint),
        ];
        it.style = style.clone();
        items.push(it);
        items.push(words(
            label(&[(name, Face::REGULAR)], 10.0),
            Point::new(x + 24.0, 132.0),
            Align::Middle,
            0.0,
            Style::default(),
        ));
    }
    let x = 24.0 + 72.0 * 7.0;
    let mut circle = Path::new();
    circle.ellipse(x + 24.0, 70.0, 20.0, 20.0);
    let mut it = Item::new(Kind::Node, "sample.node");
    it.bounds = Rect::new(x, 20.0, 48.0, 100.0);
    it.parts = vec![
        Part::path(circle, Painting::FillAndStroke),
        Part::rect(Rect::new(x, 60.0, 48.0, 36.0), true, Painting::Shade),
    ];
    it.style = style;
    it.author = Style {
        fill: Some(Paint::own(Color::hex("#f9f"))),
        ..Style::default()
    };
    items.push(it);
    items.push(words(
        label(&[("Shade, own fill", Face::REGULAR)], 10.0),
        Point::new(x + 24.0, 132.0),
        Align::Middle,
        0.0,
        Style::default(),
    ));
    scene("paintings", 612.0, 146.0, items)
}

/// The faces and their mixes, sizes, the three alignments about a guide, lines centered on
/// a point, turns, a halo over a line, and a color of the text's own.
fn texts() -> Scene {
    let mut items = Vec::new();
    let none = Style::default;
    let faces: [&[(&str, Face)]; 8] = [
        &[("Regular", Face::REGULAR)],
        &[("Bold", Face::BOLD)],
        &[("Italic", Face::ITALIC)],
        &[("Bold italic", Face::BOLD | Face::ITALIC)],
        &[("Mono 0O1lI", Face::MONO)],
        &[("Underlined", Face::UNDERLINE)],
        &[
            ("H", Face::REGULAR),
            ("2", Face::SUB),
            ("O and x", Face::REGULAR),
            ("2", Face::SUP),
        ],
        &[
            ("Mixed ", Face::REGULAR),
            ("bold", Face::BOLD),
            (" and ", Face::REGULAR),
            ("mono", Face::MONO),
        ],
    ];
    for (i, spans) in faces.into_iter().enumerate() {
        let at = Point::new(24.0, 28.0 + 26.0 * i as f64);
        items.push(words(label(spans, 16.0), at, Align::Start, 0.0, none()));
    }
    items.push(words(
        label(&[("Small 10", Face::REGULAR)], 10.0),
        Point::new(24.0, 240.0),
        Align::Start,
        0.0,
        none(),
    ));
    items.push(words(
        label(&[("Large 24", Face::REGULAR)], 24.0),
        Point::new(24.0, 268.0),
        Align::Start,
        0.0,
        none(),
    ));

    items.push(line(
        (330.0, 14.0),
        (330.0, 106.0),
        Style {
            stroke: Some(Paint::MUTED),
            ..pen(1.0)
        },
    ));
    for (i, (align, name)) in [
        (Align::Start, "Start"),
        (Align::Middle, "Middle"),
        (Align::End, "End"),
    ]
    .into_iter()
    .enumerate()
    {
        let at = Point::new(330.0, 30.0 + 30.0 * i as f64);
        items.push(words(
            label(&[(name, Face::REGULAR)], 16.0),
            at,
            align,
            0.0,
            none(),
        ));
    }

    let three = lines(
        &[
            &[("Three lines", Face::REGULAR)],
            &[("centered on", Face::REGULAR)],
            &[("one point", Face::REGULAR)],
        ],
        14.0,
    );
    let at = Point::new(330.0, 150.0);
    items.push(words(three, at, Align::Middle, 0.0, none()));
    items.push(dot(at));

    for (x, turn, name) in [
        (250.0, -90.0, "Turned -90"),
        (330.0, 45.0, "Turned 45"),
        (430.0, 180.0, "Upside down"),
    ] {
        let at = Point::new(x, 230.0);
        items.push(words(
            label(&[(name, Face::REGULAR)], 14.0),
            at,
            Align::Middle,
            turn,
            none(),
        ));
        items.push(dot(at));
    }

    items.push(line(
        (24.0, 306.0),
        (200.0, 306.0),
        Style {
            stroke: Some(Paint::ACCENT),
            ..pen(6.0)
        },
    ));
    items.push(words(
        label(&[("Halo over a line", Face::BOLD)], 16.0),
        Point::new(112.0, 306.0),
        Align::Middle,
        0.0,
        Style {
            halo: Some(Paint::BG),
            halo_width: Some(4.0),
            ..Style::default()
        },
    ));
    items.push(words(
        label(&[("A color of its own", Face::REGULAR)], 16.0),
        Point::new(330.0, 306.0),
        Align::Middle,
        0.0,
        Style {
            text: Some(Paint::own(Color::hex("#c03"))),
            ..Style::default()
        },
    ));
    scene("text", 480.0, 324.0, items)
}

/// Over stripes, a box at half opacity, whose stroke hides the fill under it, beside one
/// whose fill and stroke are each at half opacity, so the fill shows under the stroke; a
/// label with a halo and an edge with a marker, each at half opacity.
fn layers() -> Scene {
    let mut items = Vec::new();
    for i in 0..6 {
        let x = 20.0 + 60.0 * f64::from(i);
        let mut p = Path::new();
        p.polygon(&[(x, 10.0), (x + 20.0, 10.0), (x + 20.0, 180.0), (x, 180.0)]);
        items.push(mark(
            p,
            Painting::FillOnly,
            Style {
                fill: Some(Paint::MUTED),
                ..Style::default()
            },
        ));
    }
    let boxed = |x: f64, style: Style| {
        let mut it = Item::new(Kind::Node, "sample.node");
        it.bounds = Rect::new(x, 30.0, 100.0, 60.0);
        it.parts = vec![Part::rect(it.bounds, false, Painting::FillAndStroke)];
        it.style = Style {
            fill: Some(Paint::ACCENT),
            stroke: Some(Paint::FG),
            stroke_width: Some(12.0),
            ..style
        };
        it
    };
    items.push(boxed(
        40.0,
        Style {
            opacity: Some(0.5),
            ..Style::default()
        },
    ));
    items.push(boxed(
        220.0,
        Style {
            fill_opacity: Some(0.5),
            stroke_opacity: Some(0.5),
            ..Style::default()
        },
    ));
    items.push(words(
        label(&[("Faded label", Face::BOLD)], 20.0),
        Point::new(90.0, 140.0),
        Align::Middle,
        0.0,
        Style {
            halo: Some(Paint::BG),
            halo_width: Some(6.0),
            opacity: Some(0.5),
            ..Style::default()
        },
    ));
    let (from, to) = (Point::new(220.0, 140.0), Point::new(340.0, 140.0));
    let mut p = Path::new();
    p.move_to(from.x, from.y).line_to(to.x, to.y);
    let mut edge = Item::new(
        Kind::Edge(Edge {
            route: vec![from, to],
            ends: [
                None,
                Some(Marker {
                    kind: MarkerType::ArrowPoint,
                    at: to,
                    dx: 1.0,
                    dy: 0.0,
                }),
            ],
        }),
        "sample.edge",
    );
    edge.bounds = p.bounds();
    edge.parts = vec![Part::path(p, Painting::StrokeOnly)];
    edge.style = Style {
        opacity: Some(0.5),
        ..pen(3.0)
    };
    items.push(edge);
    scene("layers", 380.0, 190.0, items)
}

/// Curves of each kind filled and stroked: a quadratic and a cubic, left open, arcs, an
/// ellipse, rounded boxes, one rounded more than it has room for, and a star whose middle
/// the nonzero rule fills.
fn paths() -> Scene {
    let drawn = |p: Path| {
        mark(
            p,
            Painting::FillAndStroke,
            Style {
                fill: Some(Paint::SURFACE_ALT),
                stroke: Some(Paint::LINE),
                stroke_width: Some(2.0),
                ..Style::default()
            },
        )
    };
    let mut items = Vec::new();
    let mut p = Path::new();
    p.move_to(24.0, 90.0).quad_to(74.0, 10.0, 124.0, 90.0);
    items.push(drawn(p));
    let mut p = Path::new();
    p.move_to(150.0, 90.0)
        .cube_to(150.0, 10.0, 250.0, 90.0, 250.0, 10.0);
    items.push(drawn(p));
    let mut p = Path::new();
    p.move_to(280.0, 60.0)
        .arc_to(30.0, 20.0, 0.0, false, true, 340.0, 60.0)
        .arc_to(30.0, 20.0, 30.0, true, false, 400.0, 60.0);
    items.push(mark(p, Painting::StrokeOnly, pen(2.0)));
    let mut p = Path::new();
    p.ellipse(74.0, 150.0, 50.0, 30.0);
    items.push(drawn(p));
    let mut p = Path::new();
    p.round_rect(Rect::new(150.0, 120.0, 100.0, 60.0), 12.0);
    items.push(drawn(p));
    let mut p = Path::new();
    p.round_rect(Rect::new(270.0, 120.0, 40.0, 60.0), 40.0);
    items.push(drawn(p));
    let star: Vec<(f64, f64)> = (0..5)
        .map(|i| {
            let a = (f64::from(i) * 144.0 - 90.0).to_radians();
            (370.0 + 40.0 * a.cos(), 150.0 + 40.0 * a.sin())
        })
        .collect();
    let mut p = Path::new();
    p.polygon(&star);
    items.push(drawn(p));
    scene("paths", 430.0, 200.0, items)
}

/// The icon at three sizes and in the accent's color, an icon the canvas has not, in a
/// dashed outline, and the image kept in proportion, stretched, and at half opacity, each in
/// an outline of its box.
fn pictures() -> Scene {
    let mut items = Vec::new();
    let outline = |r: Rect| {
        let mut p = Path::new();
        p.polygon(&[
            (r.x, r.y),
            (r.x + r.w, r.y),
            (r.x + r.w, r.y + r.h),
            (r.x, r.y + r.h),
        ]);
        mark(
            p,
            Painting::StrokeOnly,
            Style {
                stroke: Some(Paint::MUTED),
                dash: Some(vec![2.0, 2.0]),
                ..pen(1.0)
            },
        )
    };
    let picture = |source: PictureSource, bounds: Rect, style: Style| {
        let mut it = Item::new(Kind::Mark, "sample.picture");
        it.bounds = bounds;
        it.picture = Some(Picture { source, bounds });
        it.style = style;
        it
    };
    let ring = || PictureSource::Icon("ring".into());
    let none = Style::default;
    items.push(picture(ring(), Rect::new(24.0, 28.0, 16.0, 16.0), none()));
    items.push(picture(ring(), Rect::new(56.0, 24.0, 24.0, 24.0), none()));
    items.push(picture(ring(), Rect::new(96.0, 12.0, 48.0, 48.0), none()));
    items.push(picture(
        ring(),
        Rect::new(160.0, 12.0, 48.0, 48.0),
        Style {
            text: Some(Paint::ACCENT),
            ..Style::default()
        },
    ));
    let missing = Rect::new(224.0, 12.0, 48.0, 48.0);
    items.push(outline(missing));
    items.push(picture(
        PictureSource::Icon("missing".into()),
        missing,
        none(),
    ));
    let image = |fit: bool| PictureSource::Image {
        src: IMAGE.into(),
        fit,
    };
    for (r, fit, opacity) in [
        (Rect::new(24.0, 80.0, 120.0, 50.0), true, 1.0),
        (Rect::new(160.0, 80.0, 80.0, 50.0), false, 1.0),
        (Rect::new(256.0, 80.0, 60.0, 50.0), true, 0.5),
    ] {
        items.push(outline(r));
        items.push(picture(
            image(fit),
            r,
            Style {
                opacity: Some(opacity),
                ..Style::default()
            },
        ));
    }
    scene("pictures", 340.0, 146.0, items)
}
