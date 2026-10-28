//! The default drawing, drawn by tiny-skia in the Liberation fonts, compared with golden
//! images. A golden image is written anew where UPDATE_GOLDEN is set. What a test draws is
//! written into target/golden for a look when it differs.

use limu_scene::style::{Cap, Color, Paint, Style};
use limu_scene::text::{self, Face, Label, Span};
use limu_scene::{
    Edge, Item, Kind, Marker, MarkerType, Painting, Part, Path, Picture, PictureSource, Placed,
    Point, Rect, Scene,
};
use limu_tiny_skia::{Fonts, Options, render, tiny_skia};

fn fonts() -> Fonts {
    let mut f = Fonts::new(limu_fixtures::font("LiberationSans-Regular.ttf")).unwrap();
    for (face, name) in [
        (Face::BOLD, "LiberationSans-Bold.ttf"),
        (Face::ITALIC, "LiberationSans-Italic.ttf"),
        (Face::BOLD | Face::ITALIC, "LiberationSans-BoldItalic.ttf"),
        (Face::MONO, "LiberationMono-Regular.ttf"),
    ] {
        f.set(face, limu_fixtures::font(name)).unwrap();
    }
    f
}

/// A label of a line of spans, each a text and its face, measured by the fonts.
fn label(fonts: &Fonts, spans: &[(&str, Face)], size: f64) -> Label {
    let line: Vec<Span> = spans
        .iter()
        .map(|&(t, face)| Span {
            text: t.into(),
            face,
            width: text::width(fonts, t, size, face),
            ..Span::default()
        })
        .collect();
    let width = line.iter().map(|s| s.width).sum();
    Label {
        lines: vec![line],
        widths: vec![width],
        width,
        height: size * 1.5,
        size,
        line_height: size * 1.5,
    }
}

/// A small flowchart drawn by hand: a group holding a box with a link, a diamond, a dashed
/// edge between them with its label, a note at half opacity, a title turned on its side
/// with a halo, and an icon.
fn sample(fonts: &Fonts) -> Scene {
    let mut group = Item::new(Kind::Group, "flowchart.cluster");
    group.bounds = Rect::new(8.0, 8.0, 224.0, 204.0);
    group.parts = vec![Part::rect(group.bounds, true, Painting::FillAndStroke)];
    group.label = Some(Placed::new(
        label(fonts, &[("Group & co", Face::REGULAR)], 16.0),
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
            route: vec![],
            ends: [None, Some(tip)],
        }),
        "flowchart.edge",
    );
    edge.parts = vec![Part::path(path, Painting::StrokeOnly)];
    edge.style = Style {
        stroke: Some(Paint::LINE),
        fill: Some(Paint::NONE),
        cap: Some(Cap::Round),
        dash: Some(vec![3.0]),
        ..Style::default()
    };

    let yes = label(fonts, &[("Yes", Face::ITALIC)], 14.0);
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
        label(fonts, &[("Start", Face::UNDERLINE)], 16.0),
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
        fonts,
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

    let mut title = Item::new(Kind::Text, "flowchart.title");
    title.label = Some(Placed {
        rotate: -90.0,
        ..Placed::new(
            label(fonts, &[("Sideways", Face::BOLD)], 16.0),
            Point::new(20.0, 120.0),
        )
    });
    title.style = Style {
        halo: Some(Paint::ACCENT),
        halo_width: Some(4.0),
        ..Style::default()
    };

    let mut icon = Item::new(Kind::Mark, "flowchart.icon");
    icon.picture = Some(Picture {
        source: PictureSource::Icon("ring".into()),
        bounds: Rect::new(190.0, 180.0, 24.0, 24.0),
    });

    Scene {
        diagram: "flowchart".into(),
        width: 240.0,
        height: 220.0,
        font_size: 16.0,
        items: vec![group, edge, edge_label, a, b, note, title, icon],
        ..Scene::default()
    }
}

fn options(fonts: &Fonts) -> Options<'_> {
    Options {
        fonts: Some(fonts),
        scale: 2.0,
        icons: Some(Box::new(|name| {
            (name == "ring").then(|| {
                let mut p = Path::new();
                p.ellipse(12.0, 12.0, 9.0, 9.0);
                let mut dot = Path::new();
                dot.ellipse(12.0, 12.0, 3.0, 3.0);
                vec![
                    Part::path(p, Painting::StrokeOnly),
                    Part::path(dot, Painting::FillOnly),
                ]
            })
        })),
        ..Options::default()
    }
}

/// Compares a pixmap with a golden image. Channels may differ by a little, as tiny-skia's
/// arithmetic does from one processor to another, and a few pixels by more.
fn golden(name: &str, got: &tiny_skia::Pixmap) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let path = dir.join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(&dir).unwrap();
        got.save_png(&path).unwrap();
        return;
    }
    let want =
        tiny_skia::Pixmap::load_png(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let off = if (want.width(), want.height()) != (got.width(), got.height()) {
        usize::MAX
    } else {
        want.data()
            .chunks(4)
            .zip(got.data().chunks(4))
            .filter(|(a, b)| a.iter().zip(*b).any(|(x, y)| x.abs_diff(*y) > 8))
            .count()
    };
    if off > want.data().len() / 4 / 1000 {
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/golden");
        std::fs::create_dir_all(&out).unwrap();
        got.save_png(out.join(name)).unwrap();
        panic!("{name}: {off} pixels differ; what was drawn is in target/golden/{name}");
    }
}

#[test]
fn sample_png() {
    let fonts = fonts();
    let got = render(&sample(&fonts), &options(&fonts)).unwrap();
    assert_eq!((got.width(), got.height()), (480, 440));
    golden("sample.png", &got);
}

#[test]
fn dark() {
    let fonts = fonts();
    let o = Options {
        look: limu_scene::style::Look::new(limu_scene::style::Theme::KOHU_DARK),
        ..options(&fonts)
    };
    golden("sample-dark.png", &render(&sample(&fonts), &o).unwrap());
}

// Liberation Sans is as wide as Arial, which Pele measures text in.
#[test]
fn measures_as_arial() {
    let fonts = fonts();
    let w = text::width(&fonts, "Start", 16.0, Face::REGULAR);
    assert!((w - 33.792).abs() < 0.01, "{w}");
    let w = text::width(&fonts, "Start", 16.0, Face::SUB);
    assert!((w - 33.792 / 1.2).abs() < 0.01, "{w}");
}
