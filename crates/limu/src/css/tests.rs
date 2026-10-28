//! Author styles read as Pele's resolveStyle keeps them and as a browser then reads them,
//! with the cases of limu's tests. The oracle has no fixture of styles.

use super::*;

fn decls(d: &[&str]) -> Vec<String> {
    d.iter().map(|s| s.to_string()).collect()
}

fn author(d: &[&str]) -> Author {
    declarations(&decls(d))
}

fn resolved(d: &[&str]) -> Resolved {
    resolve_style(&decls(d))
}

fn own(r: u8, g: u8, b: u8) -> Option<Paint> {
    Some(Paint::own(Color::rgb(r, g, b)))
}

#[test]
fn font_family_declared() {
    for (d, want) in [
        (&["font-family:Courier"][..], true),
        (&["color:red", "Font-Family: Arial !important"], true),
        (&["font-family: "], false),
        (&["font-family:url(x)"], false),
        (&["font-family:a\x01"], false),
        (&["fill:#eee", "stroke:#333", "font-size:12px"], false),
        // An entity Pele writes for a quote or an ampersand ends in the semicolon it tests for.
        (&["color:\"x\"font-family:y"], true),
        (&["color:a&font-family:b"], true),
        (&["color:font-family:b"], false),
    ] {
        assert_eq!(resolved(d).font_family_declared(), want, "{d:?}");
    }
}

#[test]
fn reads_declarations() {
    let a = author(&[
        "fill:#f9f",
        "stroke:#333",
        "stroke-width:4px",
        "color: red !IMPORTANT",
        "stroke-dasharray: 5 5",
        "font-weight:bold",
        "font-style:italic",
        "font-size:20px",
        "fill-opacity:0.5",
        "rx:10",
    ]);
    let s = &a.style;
    assert_eq!(s.fill, own(255, 153, 255));
    assert_eq!(s.fill_opacity, Some(0.5));
    assert_eq!(s.stroke, Some(Paint::own(Color::hex("#333"))));
    assert_eq!(s.text, own(255, 0, 0));
    assert_eq!(s.stroke_width, Some(4.0));
    assert_eq!(s.radius, Some(10.0));
    assert_eq!(s.dash, Some(vec![5.0, 5.0]));
    assert!(a.bold && a.italic && a.font_size == Some(20.0), "{a:?}");
}

// Caps and joins are read as the browser takes what Pele passes through: a keyword in either
// case. Pele keeps only the last value of each, so one the browser drops, or inherit, leaves
// them to what is under them.
#[test]
fn reads_caps_and_joins() {
    for (d, cap, join) in [
        (
            &["stroke-linecap:round", "stroke-linejoin:bevel"][..],
            Some(Cap::Round),
            Some(Join::Bevel),
        ),
        (
            &["stroke-linecap: SQUARE !important", "stroke-linejoin:Miter"],
            Some(Cap::Square),
            Some(Join::Miter),
        ),
        (
            &[
                "stroke-linecap:round",
                "stroke-linecap:initial",
                "stroke-linejoin:initial",
            ],
            Some(Cap::Butt),
            Some(Join::Miter),
        ),
        (
            &[
                "stroke-linecap:round",
                "stroke-linecap:inherit",
                "stroke-linejoin:round",
                "stroke-linejoin:unset",
            ],
            None,
            None,
        ),
        (
            &[
                "stroke-linecap:round",
                "stroke-linecap:flat",
                "stroke-linejoin:round",
                "stroke-linejoin:miter-clip",
            ],
            None,
            None,
        ),
        (
            &["stroke-linecap:\u{17f}quare", "stroke-linejoin:arcs"],
            None,
            None,
        ),
    ] {
        let s = author(d).style;
        assert_eq!((s.cap, s.join), (cap, join), "{d:?}");
    }
}

// Font sizes and weights are read as Pele's resolveStyle reads them, with JavaScript's
// /^[\d.]+(px)?$/, parseFloat and Number.
#[test]
fn reads_fonts() {
    for (size, want) in [
        ("12px", Some(12.0)),
        ("12", Some(12.0)),
        (".5", Some(0.5)),
        ("1.2.3", Some(1.2)),
        ("3.", Some(3.0)),
        ("1e2", None),
        ("+5", None),
        ("5pxpx", None),
        ("50%", None),
        (".", None),
        ("px", None),
        ("0", None),
        ("201", None),
        ("200px", Some(200.0)),
        ("12 px", None),
    ] {
        assert_eq!(
            author(&[&format!("font-size:{size}")]).font_size,
            want,
            "font-size:{size}"
        );
    }
    for (weight, want) in [
        ("bold", true),
        ("600", true),
        (" 700 ", true),
        ("0x258", true),
        ("599", false),
        ("6e2", true),
        ("x", false),
    ] {
        assert_eq!(
            author(&[&format!("font-weight:{weight}")]).bold,
            want,
            "font-weight:{weight}"
        );
    }
}

#[test]
fn drops_what_is_unsafe() {
    let a = author(&[
        "fill:url(#x)",
        "stroke:  url (x)",
        "color:red;x",
        "fill-opacity",
        ":red",
        "font-weight:500",
    ]);
    assert!(
        a.style.fill.is_none() && a.style.stroke.is_none() && a.style.text.is_none() && !a.bold,
        "{a:?}"
    );
    let b = author(&["font-weight:700", "fill:none"]);
    assert!(b.bold && b.style.fill == Some(Paint::NONE), "{b:?}");
}

// Declarations are trimmed and lowered as Pele's resolveStyle does, with JavaScript's trim
// and toLowerCase, and their values then read as a browser reads CSS, with its white space
// only and its keywords in either case of their ASCII letters.
#[test]
fn reads_as_javascript() {
    let red = own(255, 0, 0);
    for (decl, want) in [
        ("fill:\u{feff}red", red), // trim takes off a byte order mark
        ("\u{a0}FILL\u{3000}:red", red),
        ("fill:red\u{85}", None), // which trim leaves, and a browser does not read
        ("fill:RED", red),
        ("fill:blac\u{212a}", None), // a Kelvin sign is no k to CSS
        ("fill:rgb (255,0,0)", None),
        ("fill:rgb(255\n0\r0)", red),
        ("fill:rgb(255\n0\x0c0)", None), // a form feed is no character XML allows
        ("fill:NONE", Some(Paint::NONE)),
    ] {
        assert_eq!(author(&[decl]).style.fill, want, "{decl:?}");
    }
    assert_eq!(
        author(&["stroke-dasharray:4\t2"]).style.dash,
        Some(vec![4.0, 2.0])
    );
    // \s in Pele's RE_UNSAFE is JavaScript's, and its i flag folds ASCII letters only.
    assert_eq!(author(&["fill:url\u{3000}(x)"]).style.fill, None);
    assert!(!resolved(&["rx:javascr\u{130}pt:x"]).shape.is_empty());
    assert!(resolved(&["rx:JavaScript:x"]).shape.is_empty());
    for (value, want) in [("0.5", 0.5), ("50%", 0.5), ("150%", 1.0), ("-1", 0.0)] {
        assert_eq!(
            author(&[&format!("opacity:{value}")]).style.opacity,
            Some(want),
            "opacity:{value}"
        );
    }
    assert_eq!(author(&["opacity:0.5px"]).style.opacity, None);
}

// A later value of a property that cannot be read leaves it unset, as Pele writes only the
// last and a browser drops it.
#[test]
fn last_value_wins() {
    let s = author(&[
        "stroke-width:3px",
        "stroke-width:0x",
        "opacity:0.5",
        "opacity:half",
        "fill-opacity:0.5",
        "fill:red",
        "fill-opacity:x",
    ])
    .style;
    assert!(
        s.stroke_width.is_none() && s.opacity.is_none() && s.fill_opacity.is_none(),
        "{s:?}"
    );
    assert_eq!(s.fill, own(255, 0, 0));
    // So does a radius, which takes the box's ry where its rx cannot be read.
    assert_eq!(author(&["rx:5", "rx:x"]).style.radius, None);
    assert_eq!(author(&["rx:x", "ry:4"]).style.radius, Some(4.0));
    assert_eq!(author(&["ry:4", "rx:6"]).style.radius, Some(6.0));
}

#[test]
fn reads_opacities() {
    let s = author(&["fill-opacity:0.5", "stroke-opacity:150%"]).style;
    assert_eq!((s.fill_opacity, s.stroke_opacity), (Some(0.5), Some(1.0)));
    let s = author(&["fill-opacity:0.5", "fill-opacity:x", "stroke-opacity:20%"]).style;
    assert_eq!((s.fill_opacity, s.stroke_opacity), (None, Some(0.2)));
    // An opacity alone is kept, with no paint for it to fold into.
    let s = author(&["fill-opacity:0.5"]).style;
    assert_eq!((s.fill, s.fill_opacity), (None, Some(0.5)));
}

#[test]
fn writes_styles_as_pele() {
    for (d, shape) in [
        (&[][..], ""),
        (&["color:red", "font-size:20px"], ""),
        (
            &[
                " FILL :red !important",
                "stroke:blue",
                "fill:green",
                "x:y",
                ":z",
                "rx",
            ],
            "fill:green;stroke:blue;",
        ),
        (
            &["stroke:a\"b&c", "fill:url(x)", "stroke-width:\\1"],
            "stroke:a&quot;b&amp;c;",
        ),
        // A value with a character XML does not allow is dropped.
        (
            &[
                "opacity:0.5",
                "stroke-linecap:round",
                "stroke-dasharray:1\x7f2",
                "rx:\x01",
                "ry:\u{fffe}",
            ],
            "opacity:0.5;stroke-linecap:round;",
        ),
    ] {
        assert_eq!(resolved(d).shape, shape, "{d:?}");
    }
    let r = resolved(&[
        "fill:red",
        "Color:blue",
        "stroke:#333",
        "fill-opacity:0.5",
        "opacity:0.8",
        "font-weight:Bold",
    ]);
    assert_eq!(
        r.shape,
        "fill:red;stroke:#333;fill-opacity:0.5;opacity:0.8;"
    );
    assert_eq!(r.line, "stroke:#333;opacity:0.8;");
    assert_eq!(r.text, "fill:blue;opacity:0.8;font-weight:Bold;");
    assert_eq!(r.stroke.as_deref(), Some("#333"));
    // Pele compares the weight as it was written, but for Number's reading of it.
    assert!(!r.bold);
    assert!(r.line_declared() && !resolved(&["fill:red", "fill-opacity:1"]).line_declared());
    assert!(resolved(&["fill:x"]).fill_declared() && !resolved(&["stroke:x"]).fill_declared());
}

#[test]
fn inks_and_paints() {
    for (d, inks, paints) in [
        (&[][..], false, false),
        (&["fill:#000080"], true, true),
        (&["fill:none"], false, false),
        (&["fill:transparent"], false, false),
        (&["fill:var(--x)"], false, true),
        (&["fill:#000", "fill-opacity:0.4"], false, true),
        (&["fill:#000", "fill-opacity:abc"], false, true),
        (&["fill:#000", "fill-opacity:50%"], true, true),
        (
            &["fill:#000", "opacity:0.7", "fill-opacity:0.7"],
            false,
            true,
        ),
        (&["fill:#000", "opacity:0"], false, false),
        (&["fill:#000", "opacity:1e999"], true, true),
        (&["fill:#000", "opacity:1deg"], false, true),
        (&["fill:rgba(0,0,0,0.4)"], false, true),
        // Colors as Pele's color reads them: an empty argument, a mix of numbers and
        // percentages, or none in the comma syntax is no color it reads, and an alpha is
        // taken as it is written, not as a byte.
        (&["fill:rgb(10,20,,30)"], false, true),
        (&["fill:rgb(10%,20,30)"], false, true),
        (&["fill:rgb(none,0,0)"], false, true),
        (&["fill:rgb(none 0 0)"], true, true),
        (&["fill:rgb(0 0 0 / 0.499)"], false, true),
        (&["fill:rgb(0 0 0 / 0.5 / 1)"], false, true),
        (&["fill:hsl(200,80,30)"], false, true),
        (&["fill:hsl(200deg 80% 30%)"], true, true),
        (&["fill:hsl(1turn,80%,30%)"], true, true),
        (&["fill:hsl(1e999,80%,30%)"], false, true),
        (&["fill:#0000"], false, false),
        (&["fill:#00000080"], true, true),
        (&["fill:DarkSlateGrey"], true, true),
        (&["fill:red#"], false, true),
        // Pele finds the fill and the opacities in the style it writes, where an entity for a
        // quote in another value ends in a semicolon.
        (&["stroke:x\"fill:none", "fill:navy"], false, false),
        (&["fill:navy", "stroke:x\"opacity:0.2"], false, true),
    ] {
        let r = resolved(d);
        assert_eq!((r.inks(), r.paints()), (inks, paints), "{d:?}");
    }
}

#[test]
fn inks_text() {
    let navy = resolved(&["fill:#000080"]);
    for (text, want) in [
        (&[][..], true),
        (&["color:#fff"], false),
        // A color Pele can't read still keeps ink from picking one.
        (&["color:#f"], false),
        // One it drops does not.
        (&["color:url(x)"], true),
    ] {
        assert_eq!(navy.inks_text(&resolved(text)), want, "{text:?}");
    }
    let none = resolved(&["fill:none"]);
    assert!(!none.inks_text(&none));
}

#[test]
fn finds_values_as_pele() {
    let r = resolved(&["stroke:none", "stroke-width:2px", "opacity:0.5"]);
    assert!(
        declares(&r.line, "stroke") && declares(&r.line, "opacity") && !declares(&r.line, "fill")
    );
    assert_eq!(declared(&r.line, "stroke"), Some("none"));
    assert_eq!(declared(&r.line, "stroke-width"), Some("2px"));
    assert_eq!(declared(&r.line, "width"), None);
    // A value is cut short at an entity, as /[";]prop:([^;"]+);/ takes it.
    assert_eq!(
        declared(&resolved(&["stroke:a\"b"]).line, "stroke"),
        Some("a&quot")
    );
}

// Widths and opacities are numbers as CSS writes them.
#[test]
fn reads_css_numbers() {
    for (value, want) in [
        ("1e1", Some(10.0)),
        ("+3", Some(3.0)),
        ("3PX", Some(3.0)),
        (" 2px ", Some(2.0)),
        ("inf", None),
        ("NaN", None),
        ("0x10", None),
        ("1.", None),
        ("1e400", None),
        ("2em", None),
    ] {
        assert_eq!(
            author(&[&format!("stroke-width:{value}")])
                .style
                .stroke_width,
            want,
            "stroke-width:{value}"
        );
    }
    for (value, want) in [
        ("0.5", Some(0.5)),
        ("50%", Some(0.5)),
        ("2", Some(1.0)),
        ("0.", None),
        ("inf", None),
        ("5deg", None),
    ] {
        assert_eq!(
            author(&[&format!("opacity:{value}")]).style.opacity,
            want,
            "opacity:{value}"
        );
    }
}
