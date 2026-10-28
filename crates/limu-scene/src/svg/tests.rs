use super::*;
use crate::marker::{Marker, MarkerType};
use crate::scene::{Edge, Kind, Outline, Painting, Part, Picture, PictureSource, Placed};
use crate::style::{Color, Style};
use crate::text::{Label, Span};

/// A label of lines of spans in the regular face, each span measured a size wide to a
/// character, lines 1.5 times the size apart.
fn label(lines: &[&[&str]], size: f64) -> Label {
    let lines: Vec<Vec<Span>> = lines
        .iter()
        .map(|l| {
            l.iter()
                .map(|t| Span {
                    text: t.to_string(),
                    width: t.chars().count() as f64 * size / 2.0,
                    ..Span::default()
                })
                .collect()
        })
        .collect();
    let widths: Vec<f64> = lines
        .iter()
        .map(|l| l.iter().map(|s| s.width).sum())
        .collect();
    Label {
        width: widths.iter().copied().fold(0.0, f64::max),
        height: lines.len() as f64 * size * 1.5,
        line_height: size * 1.5,
        size,
        lines,
        widths,
    }
}

fn own(s: &str) -> Paint {
    Paint::own(Color::hex(s))
}

/// A small flowchart laid out by hand: a group holding a box, a diamond, an edge between
/// them with its label, and a title.
fn sample() -> Scene {
    let mut group = Item::new(Kind::Group, "flowchart.cluster");
    group.id = Some("g1".into());
    group.bounds = Rect::new(8.0, 8.0, 224.0, 204.0);
    group.parts = vec![Part::rect(group.bounds, true, Painting::FillAndStroke)];
    group.label = Some(Placed::new(
        label(&[&["Group & co"]], 16.0),
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
    edge.id = Some("L_A_B_0".into());
    edge.parts = vec![Part::path(path, Painting::StrokeOnly)];
    edge.style = Style {
        stroke: Some(Paint::LINE),
        fill: Some(Paint::NONE),
        cap: Some(Cap::Round),
        ..Style::default()
    };

    let yes = label(&[&["Yes"]], 14.0);
    let mut edge_label = Item::new(Kind::Text, "flowchart.edge-label");
    edge_label.parts = vec![Part::rect(
        Rect::new(
            100.0 - yes.width / 2.0 - 4.0,
            100.0 - yes.height / 2.0,
            yes.width + 8.0,
            yes.height,
        ),
        true,
        Painting::FillAndStroke,
    )];
    edge_label.bounds = match edge_label.parts[0].outline {
        Outline::Box { rect, .. } => rect,
        _ => unreachable!(),
    };
    edge_label.label = Some(Placed::new(yes, Point::new(100.0, 100.0)));
    edge_label.style = Style {
        fill: Some(Paint::BG),
        stroke: Some(Paint::NONE),
        radius: Some(3.0),
        ..Style::default()
    };

    let mut a = Item::new(Kind::Node, "flowchart.node");
    a.shape = Some("rect".into());
    a.id = Some("A".into());
    a.class = vec!["hot".into()];
    a.bounds = Rect::new(60.0, 40.0, 80.0, 40.0);
    a.parts = vec![Part::rect(a.bounds, true, Painting::FillAndStroke)];
    a.label = Some(Placed::new(
        label(&[&["Start"]], 16.0),
        Point::new(100.0, 60.0),
    ));
    a.style = Style {
        fill: Some(Paint::SURFACE),
        stroke: Some(Paint::BORDER),
        ..Style::default()
    };
    a.author = Style {
        fill: Some(own("#f9f")),
        stroke_width: Some(2.0),
        ..Style::default()
    };
    a.link = Some("https://example.com/?a=1&b=2".into());
    a.tooltip = Some("Where it starts".into());

    let mut b = Item::new(Kind::Node, "flowchart.node");
    b.shape = Some("diam".into());
    b.id = Some("B".into());
    b.bounds = Rect::new(30.0, 120.0, 140.0, 70.0);
    let mut diamond = Path::new();
    diamond.polygon(&[
        (100.0, 120.0),
        (170.0, 155.0),
        (100.0, 190.0),
        (30.0, 155.0),
    ]);
    b.parts = vec![Part::path(diamond, Painting::FillAndStroke)];
    let mut is = label(&[&["Is it ", "ready", "?"]], 16.0);
    is.lines[0][1].face = Face::BOLD;
    b.label = Some(Placed::new(is, Point::new(100.0, 155.0)));
    b.style = Style {
        fill: Some(Paint::SURFACE),
        stroke: Some(Paint::BORDER),
        ..Style::default()
    };

    Scene {
        diagram: "flowchart".into(),
        width: 240.0,
        height: 220.0,
        font_size: 16.0,
        title: Some("A <sample>".into()),
        desc: None,
        items: vec![group, edge, edge_label, a, b],
    }
}

/// Compares what is written with a file of src/svg/testdata, or writes the file anew where
/// UPDATE_GOLDEN is set.
fn golden(name: &str, got: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/svg/testdata")
        .join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, got).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        got == want,
        "{name} differs; look at it, then run with UPDATE_GOLDEN=1\n got {got}\nwant {want}"
    );
}

#[test]
fn render_sample() {
    let got = render(&sample(), &Options::default());
    roxmltree::Document::parse(&got).expect("not XML");
    golden("sample.svg", &got);
}

#[test]
fn literal() {
    let theme = Theme {
        surface: Color::hex("#ddd"),
        ..Theme::default()
    };
    let got = render(
        &sample(),
        &Options {
            literal: true,
            look: Look::new(theme),
            ..Options::default()
        },
    );
    assert!(
        !got.replace("var(--limu-font", "").contains("var(--limu-"),
        "a variable was written"
    );
    assert!(
        got.contains(r##"fill="#dddddd""##),
        "the theme's surface was not used"
    );
    golden("sample-literal.svg", &got);
}

#[test]
fn overrides() {
    let mut look = Look::default();
    look.overrides.insert(
        "flowchart.node".into(),
        Style {
            fill: Some(Paint::ACCENT),
            dash: Some(vec![3.0, 2.0]),
            ..Style::default()
        },
    );
    let got = render(
        &sample(),
        &Options {
            look,
            ..Options::default()
        },
    );
    // The author's fill on A wins over the override, and B takes the override.
    assert!(
        got.contains(r##"fill="#ff99ff""##)
            && got.contains(r##"fill="var(--limu-accent,#5b7bd5)""##),
        "{got}"
    );
    assert_eq!(
        got.matches(r#"stroke-dasharray="3 2""#).count(),
        2,
        "dashes on both nodes"
    );
}

// A part painted shade is filled with the background, or over a fill its author set, with
// the background at 60%, and not stroked. A fill with an alpha of its own does not change
// that, as variables or as colors.
#[test]
fn shade() {
    let item = |author: Style| {
        let mut it = Item::new(Kind::Node, "er.entity");
        it.bounds = Rect::new(0.0, 0.0, 50.0, 30.0);
        it.parts = vec![
            Part::rect(it.bounds, false, Painting::FillAndStroke),
            Part::rect(Rect::new(0.0, 10.0, 50.0, 10.0), false, Painting::Shade),
        ];
        it.style = Style {
            fill: Some(Paint::SURFACE),
            stroke: Some(Paint::BORDER),
            ..Style::default()
        };
        it.author = author;
        it
    };
    let mut sc = Scene {
        width: 50.0,
        height: 30.0,
        font_size: 16.0,
        items: vec![item(Style::default())],
        ..Scene::default()
    };
    let got = render(&sc, &Options::default());
    assert!(
        got.contains(r##"height="10" fill="var(--limu-bg,#ffffff)"/>"##),
        "no stripe of the background:\n{got}"
    );
    for fill in ["#ff9966", "#ff996680"] {
        sc.items[0] = item(Style {
            fill: Some(own(fill)),
            ..Style::default()
        });
        let got = render(&sc, &Options::default());
        assert!(
            got.contains(r##"height="10" fill="var(--limu-bg,#ffffff)" fill-opacity="0.6"/>"##),
            "{fill}:\n{got}"
        );
        let got = render(
            &sc,
            &Options {
                literal: true,
                ..Options::default()
            },
        );
        assert!(
            got.contains(r##"height="10" fill="#ffffff" fill-opacity="0.6"/>"##),
            "{fill}, as a color:\n{got}"
        );
    }
}

// A color of the author's own with an alpha is written opaque, its alpha its opacity, and
// not twice: fill:#ff000080 is drawn at 0.5, not at 0.25.
#[test]
fn own_alpha() {
    let mut sc = sample();
    sc.items[3].author = Style {
        fill: Some(own("#ff000080")),
        stroke: Some(own("#00ff0040")),
        ..Style::default()
    };
    for literal in [false, true] {
        let got = render(
            &sc,
            &Options {
                literal,
                ..Options::default()
            },
        );
        assert!(
            got.contains(
                r##"fill="#ff0000" fill-opacity="0.5" stroke="#00ff00" stroke-opacity="0.25""##
            ),
            "literal {literal}: the author's alpha not written once:\n{got}"
        );
    }
}

// A style's fill and stroke opacities are written with its paints, times the paints' own. A
// marker is filled in the stroke's color alone and stroked at the stroke opacity.
#[test]
fn fill_opacity() {
    let mut sc = sample();
    sc.items[3].author = Style {
        fill: Some(own("#ff000080")),
        fill_opacity: Some(0.5),
        stroke_opacity: Some(0.25),
        ..Style::default()
    };
    let got = render(
        &sc,
        &Options {
            literal: true,
            ..Options::default()
        },
    );
    assert!(
        got.contains(r##"fill="#ff0000" fill-opacity="0.25""##)
            && got.contains(r#"stroke-opacity="0.25""#),
        "{got}"
    );

    let mut line = Path::new();
    line.move_to(0.0, 10.0).line_to(20.0, 10.0);
    let tip = Marker {
        kind: MarkerType::ArrowPoint,
        at: Point::new(20.0, 10.0),
        dx: 1.0,
        dy: 0.0,
    };
    let mut edge = Item::new(
        Kind::Edge(Edge {
            route: vec![],
            ends: [None, Some(tip)],
        }),
        "flowchart.edge",
    );
    edge.parts = vec![Part::path(line, Painting::StrokeOnly)];
    edge.style = Style {
        stroke: Some(own("#f00")),
        stroke_opacity: Some(0.25),
        ..Style::default()
    };
    let sc = Scene {
        width: 20.0,
        height: 20.0,
        items: vec![edge],
        ..Scene::default()
    };
    let got = render(
        &sc,
        &Options {
            literal: true,
            ..Options::default()
        },
    );
    assert!(
        got.contains(r##"class="limu-marker" d="M20,10L12,13.36L12,6.64Z" fill="#ff0000"/>"##),
        "{got}"
    );
}

// An item's tags and then its classes are written as the element's classes.
#[test]
fn tags_and_classes() {
    let mut it = Item::new(Kind::Node, "c4.element");
    it.tags = vec!["person".into(), "external".into()];
    it.class = vec!["mine".into()];
    let sc = Scene {
        width: 10.0,
        height: 10.0,
        items: vec![it],
        ..Scene::default()
    };
    let got = render(&sc, &Options::default());
    assert!(
        got.contains(r#"class="limu-node limu-c4-element person external mine""#),
        "{got}"
    );
}

fn icons<'a>() -> Option<IconMarkup<'a>> {
    Some(Box::new(|_| {
        Some(r#"<circle cx="12" cy="12" r="10"/>"#.into())
    }))
}

/// A label of an icon and a word, as layout would lay out "fa:fa-car Car".
fn icon_label(icon: &str, word: &str, size: f64) -> Label {
    let mut l = label(&[&["", word]], size);
    l.lines[0][0] = Span {
        icon: Some(icon.into()),
        width: size * 1.1,
        ..Span::default()
    };
    l.widths[0] += size * 1.1;
    l.width = l.widths[0];
    l
}

// An icon in a label is drawn in the color of the label's text, as an item's picture is, at
// its opacity.
#[test]
fn label_icon_color() {
    let mut it = Item::new(Kind::Node, "x.node");
    it.label = Some(Placed::new(
        icon_label("fa:fa-car", " Car", 16.0),
        Point::new(50.0, 20.0),
    ));
    it.style = Style {
        text: Some(Paint::MUTED.alpha(0.5)),
        ..Style::default()
    };
    it.picture = Some(Picture {
        source: PictureSource::Icon("fa:fa-tree".into()),
        bounds: Rect::new(0.0, 0.0, 20.0, 20.0),
    });
    let sc = Scene {
        diagram: "t".into(),
        width: 100.0,
        height: 40.0,
        font_size: 16.0,
        items: vec![it],
        ..Scene::default()
    };
    let got = render(
        &sc,
        &Options {
            icon_markup: icons(),
            ..Options::default()
        },
    );
    for icon in ["fa:fa-car", "fa:fa-tree"] {
        let at = got
            .find(&format!(r#"data-icon="{icon}""#))
            .unwrap_or_else(|| panic!("no icon {icon}:\n{got}"));
        let tag = &got[at..at + got[at..].find('>').unwrap()];
        assert!(
            tag.contains(r##" color="var(--limu-muted,#6e6e6e)" opacity="0.5" "##),
            "icon {icon}: {tag}"
        );
    }
}

// A label turned by its rotation is written in a group turned about its point, its halo and
// icons in it.
#[test]
fn rotated_icon() {
    let mut it = Item::new(Kind::Text, "swimlane.title");
    it.label = Some(Placed {
        rotate: -90.0,
        ..Placed::new(
            icon_label("fa:fa-user", " Customer", 16.0),
            Point::new(20.0, 100.0),
        )
    });
    it.style = Style {
        halo: Some(Paint::BG),
        halo_width: Some(4.0),
        ..Style::default()
    };
    let sc = Scene {
        diagram: "swimlane".into(),
        width: 60.0,
        height: 200.0,
        font_size: 16.0,
        items: vec![it],
        ..Scene::default()
    };
    let got = render(
        &sc,
        &Options {
            icon_markup: icons(),
            ..Options::default()
        },
    );
    let open = got
        .find(r#"<g transform="rotate(-90 20 100)">"#)
        .expect("no turn");
    let icon = got.find(r#"<svg class="limu-icon""#).expect("no icon");
    let end = got.rfind("</g></g>").expect("no end");
    assert!(
        open < icon && icon < end && got.matches("<text ").count() == 2,
        "{got}"
    );
}

// A hook writes a paint with its opacity, and keeps the default's label.
#[test]
fn hook_pieces() {
    let got = render_with(&sample(), &Options::default(), |p, it, s| {
        if it.id.as_deref() != Some("g1") {
            return p.item(it, s);
        }
        let w = p.canvas();
        w.raw(r#"<g class="mine"><circle"#);
        w.paint_attr("fill", &s.fill);
        w.raw(r#" opacity=""#);
        let o = w.opacity(s.fill.paint);
        w.num(o);
        w.raw(r#""/>"#);
        p.parts(it, s);
        p.label(it, s);
        p.canvas().raw("</g>");
    });
    let want = r##"<g class="mine"><circle fill="var(--limu-surface-alt,#e6e6e6)" fill-opacity="0.5" opacity="0.5"/><rect x="8" y="8" width="224" height="204" rx="4" fill="var(--limu-surface-alt,#e6e6e6)" fill-opacity="0.5" stroke="var(--limu-border,#8a8a8a)"/><text text-anchor="middle" xml:space="preserve" fill="var(--limu-muted,#6e6e6e)"><tspan x="120" y="29.6">Group &amp; co</tspan></text></g>"##;
    assert!(got.contains(want), "no {want} in\n{got}");
}

// A hook may draw an item in a way of its own, inside the item's group.
#[test]
fn hook() {
    let got = render_with(&sample(), &Options::default(), |p, it, s| {
        if it.shape.as_ref().is_some_and(|s| *s == "diam") {
            let w = p.canvas();
            w.raw(r#"<circle class="mine" fill=""#);
            w.paint(s.fill.paint);
            w.raw(r#""/>"#);
        } else {
            p.item(it, s);
        }
    });
    assert!(got.contains(r##"limu-shape-diam" data-id="B"><circle class="mine" fill="var(--limu-surface,#f3f3f3)"/></g>"##), "{got}");
    assert!(got.contains("limu-shape-rect"), "the rest not drawn");
}

// An item's opacity is the opacity of a layer of its pieces.
#[test]
fn opacity() {
    let mut sc = sample();
    sc.items[4].author.opacity = Some(0.5);
    let got = render(&sc, &Options::default());
    assert!(
        got.contains(r#"data-id="B"><g opacity="0.5"><path"#),
        "{got}"
    );
}

// What XML does not allow is not written, in text or in attributes, and what is written
// reads as XML.
#[test]
fn valid_xml() {
    let bad = "ab\u{FFFE}c\u{FFFF}de\u{1}f\u{7f}<&>\"";
    let mut it = Item::new(Kind::Node, "x.node");
    it.id = Some(bad.into());
    it.class = vec![bad.into()];
    it.tooltip = Some(bad.into());
    it.label = Some(Placed::new(label(&[&[bad]], 16.0), Point::new(5.0, 5.0)));
    let sc = Scene {
        diagram: bad.into(),
        title: Some(bad.into()),
        desc: Some(bad.into()),
        width: 10.0,
        height: 10.0,
        font_size: 16.0,
        items: vec![it],
    };
    let got = render(&sc, &Options::default());
    roxmltree::Document::parse(&got).unwrap_or_else(|e| panic!("{e} in\n{got}"));
}

// A cap and a join are written where they are set: an edge's on its line and on the markers
// it strokes, a marker's own join over the edge's, and an author's over the diagram's.
#[test]
fn cap_join() {
    let mut line = Path::new();
    line.move_to(0.0, 0.0)
        .line_to(40.0, 0.0)
        .line_to(40.0, 30.0);
    let ends = [
        Some(Marker {
            kind: MarkerType::ArrowCross,
            at: Point::new(0.0, 0.0),
            dx: -1.0,
            dy: 0.0,
        }),
        Some(Marker {
            kind: MarkerType::ClassExtension,
            at: Point::new(40.0, 30.0),
            dx: 0.0,
            dy: 1.0,
        }),
    ];
    let mut edge = Item::new(
        Kind::Edge(Edge {
            route: vec![],
            ends,
        }),
        "class.edge",
    );
    edge.parts = vec![Part::path(line, Painting::StrokeOnly)];
    edge.style = Style {
        stroke: Some(Paint::LINE),
        fill: Some(Paint::NONE),
        cap: Some(Cap::Round),
        ..Style::default()
    };
    let mut sc = Scene {
        width: 50.0,
        height: 40.0,
        font_size: 16.0,
        items: vec![edge],
        ..Scene::default()
    };
    let got = render(&sc, &Options::default());
    for want in [
        r##"<path d="M0,0L40,0L40,30" fill="none" stroke="var(--limu-line,#1f1f1f)" stroke-linecap="round"/>"##,
        r##"<path class="limu-marker" d="M7.5,3.5L0.5,-3.5M7.5,-3.5L0.5,3.5" fill="none" stroke="var(--limu-line,#1f1f1f)" stroke-linecap="round"/>"##,
        r#"stroke-linecap="round" stroke-linejoin="round"/>"#,
    ] {
        assert!(got.contains(want), "no {want} in\n{got}");
    }
    sc.items[0].author = Style {
        cap: Some(Cap::Square),
        join: Some(Join::Bevel),
        ..Style::default()
    };
    let got = render(&sc, &Options::default());
    assert!(
        got.contains(r##"L40,30" fill="none" stroke="var(--limu-line,#1f1f1f)" stroke-linecap="square" stroke-linejoin="bevel"/>"##)
            && got.contains(r#"stroke-linecap="square" stroke-linejoin="round"/>"#),
        "the author's cap and join, under the marker's join:\n{got}"
    );
}

#[test]
fn nums() {
    let big = |sign: &str| format!("{sign}1{}", "0".repeat(300));
    for (f, want) in [
        (0.0, "0".to_string()),
        (1.0, "1".into()),
        (-1.0, "-1".into()),
        (0.004, "0".into()),
        (-0.004, "0".into()),
        (12.5, "12.5".into()),
        (12.05, "12.05".into()),
        (12.345, "12.35".into()),
        (1.999, "2".into()),
        (-3.25, "-3.25".into()),
        (0.1, "0.1".into()),
        (123456789012.5, "123456789012.5".into()),
        // The product is rounded before Math.round takes it: fused into one multiply-add with
        // the half, -0.005 would be -0.01.
        (-0.005, "0".into()),
        (22.505, "22.51".into()),
        (f64::NAN, "0".into()),
        (f64::INFINITY, "0".into()),
        (f64::NEG_INFINITY, "0".into()),
        (1e300, big("")),
        (-1e300, big("-")),
    ] {
        let mut b = String::new();
        num(&mut b, f);
        assert_eq!(b, want, "num({f})");
    }
}

#[test]
fn escapes() {
    for (s, want) in [
        (
            "a<b>&\"c\"\u{1}\u{7f}\td",
            "a&lt;b&gt;&amp;&quot;c&quot;\td",
        ),
        // U+FFFE and U+FFFF are left out, and other characters kept.
        ("x\u{FFFE}y\u{FFFF}z", "xyz"),
        ("é\u{FFFD}😀\u{10FFFD}", "é\u{FFFD}😀\u{10FFFD}"),
    ] {
        let mut b = String::new();
        esc(&mut b, s);
        assert_eq!(b, want, "{s:?}");
    }
}

#[test]
fn safe_links() {
    let links = ["http", "https", "mailto", "tel"];
    for (s, want) in [
        ("https://a.b/c", Some("https://a.b/c")),
        (" HTTP://a.b ", Some("HTTP://a.b")),
        ("mailto:x@y.z", Some("mailto:x@y.z")),
        ("javascript:alert(1)", None),
        ("JaVaScRiPt:alert(1)", None),
        ("data:text/html,x", None),
        ("java\tscript:alert(1)", None),
        ("page.html", Some("page.html")),
        ("/notes/a:b", Some("/notes/a:b")),
        ("#section", Some("#section")),
        ("?q=a:b", Some("?q=a:b")),
        ("//evil.example/x", None),
        ("vbscript:x", None),
        ("", None),
    ] {
        assert_eq!(safe_url(s, None, &links), want, "{s:?}");
    }
}

// A line of text is anchored at the label's point itself, and its spans follow one another,
// so that where it starts, ends or is centered does not hang on how its text was measured.
// A line with an icon is placed span by span where layout measured them.
#[test]
fn align() {
    let write = |l: &Label, align: Align| {
        let mut it = Item::new(Kind::Text, "x.text");
        it.label = Some(Placed {
            align,
            ..Placed::new(l.clone(), Point::new(50.0, 50.0))
        });
        let sc = Scene {
            diagram: "t".into(),
            width: 100.0,
            height: 100.0,
            font_size: 10.0,
            items: vec![it],
            ..Scene::default()
        };
        read_label(&render(
            &sc,
            &Options {
                icon_markup: icons(),
                ..Options::default()
            },
        ))
    };
    let mut l = label(&[&["ab"], &["a ", "bold", " c"]], 10.0);
    l.lines[1][1].face = Face::BOLD;
    for (align, anchor) in [
        (Align::Middle, "middle"),
        (Align::Start, "start"),
        (Align::End, "end"),
    ] {
        let want = [
            format!("t {anchor} 50,46 ab"),
            format!("t {anchor} 50,61 a [b:bold] c"),
        ];
        assert_eq!(write(&l, align), want);
        // As a measurer that finds text a third wider would measure it.
        let mut wider = l.clone();
        wider.widths.iter_mut().for_each(|w| *w *= 4.0 / 3.0);
        wider
            .lines
            .iter_mut()
            .flatten()
            .for_each(|s| s.width *= 4.0 / 3.0);
        assert_eq!(write(&wider, align), want);
    }
    let l = icon_label("fa:fa-car", " go", 10.0);
    let (icon, w) = (l.lines[0][0].width, l.widths[0]);
    let n = |f: f64| {
        let mut b = String::new();
        num(&mut b, f);
        b
    };
    for (align, x) in [
        (Align::Middle, 50.0 - w / 2.0),
        (Align::Start, 50.0),
        (Align::End, 50.0 - w),
    ] {
        let want = [
            format!("t start {},53.5  go", n(x + icon)),
            format!("icon fa:fa-car {},44.5,11,11", n(x)),
        ];
        assert_eq!(write(&l, align), want, "{align:?}");
    }
}

// The fixture is what Pele's labelSvg and leftLabelSvg write of labels, read back as what a
// browser is given to place: a line for each run of text placed on its own, with its anchor,
// and a line for each icon. The head of pele-oracle's suites/svg.mjs says how the lines are
// read. The test writes each label as a Middle and a Start label about the same point and
// reads its own SVG back the same way.
#[test]
fn labels_match_pele() {
    #[derive(serde::Deserialize)]
    struct PeleSpan {
        text: String,
        style: u8,
        #[serde(default)]
        icon: Option<String>,
        width: f64,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PeleLabel {
        lines: Vec<Vec<PeleSpan>>,
        widths: Vec<f64>,
        width: f64,
        height: f64,
        size: f64,
        line_height: f64,
    }
    #[derive(serde::Deserialize)]
    struct Case {
        text: String,
        label: PeleLabel,
        middle: Vec<String>,
        start: Vec<String>,
    }
    #[derive(serde::Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }
    let f: Fixture = limu_fixtures::load("svg");
    assert!(f.cases.len() >= 300, "only {} cases", f.cases.len());
    let o = Options {
        icon_markup: Some(Box::new(|_| Some("<path/>".into()))),
        ..Options::default()
    };
    for c in f.cases {
        let p = c.label;
        let l = Label {
            lines: p
                .lines
                .into_iter()
                .map(|line| {
                    line.into_iter()
                        .map(|s| Span {
                            text: s.text,
                            face: Face::from_bits(s.style),
                            icon: s.icon.filter(|i| !i.is_empty()),
                            width: s.width,
                        })
                        .collect()
                })
                .collect(),
            widths: p.widths,
            width: p.width,
            height: p.height,
            size: p.size,
            line_height: p.line_height,
        };
        for (align, want) in [(Align::Middle, &c.middle), (Align::Start, &c.start)] {
            let mut it = Item::new(Kind::Text, "x.text");
            it.label = Some(Placed {
                align,
                ..Placed::new(l.clone(), Point::new(100.0, 50.0))
            });
            let sc = Scene {
                diagram: "t".into(),
                width: 200.0,
                height: 100.0,
                font_size: l.size,
                items: vec![it],
                ..Scene::default()
            };
            let got = read_label(&render(&sc, &o));
            assert_eq!(&got, want, "label {:?}, {align:?}", c.text);
        }
    }
}

/// Reads the SVG of a label as pele-oracle's suites/svg.mjs reads Pele's: a line for each run
/// of text placed on its own, "t anchor x,y spans", and then a line for each icon, "icon name
/// x,y,width,height".
fn read_label(svg: &str) -> Vec<String> {
    #[derive(Clone, Default)]
    struct El {
        bold: bool,
        italic: bool,
        mono: bool,
        under: bool,
        shift: &'static str,
        anchor: String,
    }
    struct RunOf {
        anchor: String,
        x: String,
        y: String,
        spans: Vec<(String, String)>,
    }
    #[derive(Default)]
    struct Reader {
        runs: Vec<RunOf>,
        icons: Vec<String>,
    }
    fn walk(n: roxmltree::Node<'_, '_>, up: Option<&El>, r: &mut Reader) {
        for c in n.children() {
            if c.is_text() {
                let (Some(top), Some(cur)) = (up, r.runs.last_mut()) else {
                    continue;
                };
                let mut face = String::new();
                for (on, f) in [
                    (top.bold, "b"),
                    (top.italic, "i"),
                    (top.mono, "m"),
                    (top.under, "u"),
                ] {
                    if on {
                        face.push_str(f);
                    }
                }
                face.push_str(top.shift);
                let t = c.text().unwrap_or_default();
                match cur.spans.last_mut() {
                    Some(last) if last.0 == face => last.1.push_str(t),
                    _ => cur.spans.push((face, t.to_string())),
                }
                continue;
            }
            if !c.is_element() {
                continue;
            }
            let name = c.tag_name().name();
            if name == "svg" {
                if let Some(icon) = c.attribute("data-icon") {
                    let a = |k| c.attribute(k).unwrap_or_default();
                    r.icons.push(format!(
                        "icon {icon} {},{},{},{}",
                        a("x"),
                        a("y"),
                        a("width"),
                        a("height")
                    ));
                } else {
                    walk(c, up, r);
                }
                continue;
            }
            if name != "text" && name != "tspan" {
                walk(c, None, r);
                continue;
            }
            let mut e = up.cloned().unwrap_or(El {
                anchor: "start".into(),
                ..El::default()
            });
            e.bold |= c.has_attribute("font-weight");
            e.italic |= c.attribute("font-style") == Some("italic");
            e.mono |= c.has_attribute("font-family");
            e.under |= c.attribute("text-decoration") == Some("underline");
            match c.attribute("baseline-shift") {
                Some("sub") => e.shift = "_",
                Some("super") => e.shift = "^",
                _ => {}
            }
            if name == "text" {
                e.anchor = c.attribute("text-anchor").unwrap_or("start").into();
            }
            if let Some(x) = c.attribute("x") {
                r.runs.push(RunOf {
                    anchor: c.attribute("text-anchor").unwrap_or(&e.anchor).into(),
                    x: x.into(),
                    y: c.attribute("y").unwrap_or_default().into(),
                    spans: Vec::new(),
                });
            }
            walk(c, Some(&e), r);
        }
    }
    let doc = roxmltree::Document::parse(svg).unwrap_or_else(|e| panic!("{e} in\n{svg}"));
    let mut r = Reader::default();
    walk(doc.root(), None, &mut r);
    let mut lines = Vec::new();
    for run in &r.runs {
        let all: String = run.spans.iter().map(|s| s.1.as_str()).collect();
        // A run of nothing but spaces draws nothing.
        if all.trim_matches(' ').is_empty() {
            continue;
        }
        let spans: String = run
            .spans
            .iter()
            .map(|(f, t)| {
                if f.is_empty() {
                    t.clone()
                } else {
                    format!("[{f}:{t}]")
                }
            })
            .collect();
        lines.push(format!("t {} {},{} {spans}", run.anchor, run.x, run.y));
    }
    lines.extend(r.icons);
    lines
}
