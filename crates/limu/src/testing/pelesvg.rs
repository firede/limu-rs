//! A scene written as the render oracles read Pele's SVG, for the tests that compare what
//! limu lays out with what Pele draws: one line for each thing drawn, in the order drawn, with
//! numbers as Pele's SVG writes them.

use super::{pele_d, pele_num};
use crate::scene::style::{Look, Paint};
use crate::scene::{
    Item, Kind, MARKER_SIZE, Marker, MarkerType, Op, Outline, Part, PictureSource, Placed, Rect,
    Scene,
};
use crate::text::decode_entities;

/// The cap and the join an item is stroked with, its author's style laid over its own, as the
/// render oracles write them: "round miter".
pub(crate) fn cap_join(it: &Item) -> String {
    let r = Look::default().resolve(it.role.as_str(), &it.style, &it.author);
    format!("{} {}", r.cap, r.join)
}

/// An item's classes and then its tags, as the render oracles write the classes Pele gives
/// what it draws once [`tags_last`] has moved its tags last.
pub(crate) fn classes(it: &Item) -> String {
    it.class
        .iter()
        .chain(&it.tags)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reads a line with the words of tags moved to the end of its first list in brackets,
/// "[a clickable b]" to "[a b clickable]": where Pele writes what the scene holds as a tag
/// among an author's classes, in the order they were given, the scene holds them apart. A
/// line with none of the tags in that list is read as it is.
pub(crate) fn tags_last(tags: &'static [&'static str]) -> impl Fn(&str) -> String {
    move |line: &str| {
        let Some(open) = line.find(" [") else {
            return line.to_string();
        };
        let Some(end) = line[open..].find(']').map(|e| e + open) else {
            return line.to_string();
        };
        let (mut class, mut tagged) = (Vec::new(), Vec::new());
        for w in line[open + 2..end].split_ascii_whitespace() {
            if tags.contains(&w) {
                tagged.push(w);
            } else {
                class.push(w);
            }
        }
        if tagged.is_empty() {
            return line.to_string();
        }
        class.extend(tagged);
        format!("{}{}{}", &line[..open + 2], class.join(" "), &line[end..])
    }
}

/// The item's own fill at its own fill opacity, one paint, as the render oracles compare a
/// fill Pele writes and the fill-opacity it writes beside it.
pub(crate) fn own_fill(it: &Item) -> Option<Paint> {
    it.style
        .fill
        .map(|p| p.alpha(it.style.fill_opacity.unwrap_or(1.0)))
}

/// The color Pele's ink gives what lies on a fill a style sets, by the theme's paint that
/// stands for each: ON_LIGHT is the theme's foreground, and ON_DARK its background.
fn ink_of(p: Option<Paint>) -> &'static str {
    match p {
        Some(p) if p == Paint::FG => "var(--pele-fg-on-light,#1f1f1f)",
        Some(p) if p == Paint::BG => "var(--pele-fg-on-dark,#fff)",
        _ => "",
    }
}

/// The color Pele's ink writes for an item's text, where the item's style picks one that
/// stands out on a fill (on_fill, ground or text_ground), as [`Look::cascade`] settles it, or
/// "" where it picks none.
pub(crate) fn text_ink(it: &Item) -> &'static str {
    let mut own = it.style.clone();
    // A paint no item's text has, to tell a color picked from the one the item gives.
    own.text = Some(Paint::CRITICAL);
    ink_of(
        Look::default()
            .cascade(it.role.as_str(), &own, &it.author)
            .text,
    )
}

/// The color Pele's ink writes for an item's stroke, where the ground it lies on picks one, as
/// [`Look::cascade`] settles it, or "" where it picks none.
pub(crate) fn stroke_ink(it: &Item) -> &'static str {
    let mut own = it.style.clone();
    own.stroke = Some(Paint::CRITICAL);
    ink_of(
        Look::default()
            .cascade(it.role.as_str(), &own, &it.author)
            .stroke,
    )
}

/// The path of a part, which a part of an edge or a casing is.
fn path_of(part: &Part) -> &crate::scene::Path {
    match &part.outline {
        Outline::Path(p) => p,
        Outline::Box { .. } => panic!("a box where a path was looked for"),
    }
}

/// The box of a part that is one.
fn box_of(part: &Part) -> Rect {
    match &part.outline {
        Outline::Box { rect, .. } => *rect,
        Outline::Path(p) => p.bounds(),
    }
}

/// A casing as the render oracles read the casings Pele draws under edges that cross a fill
/// a style sets: its width, its path, and the markers it cases, p for an arrow's point, o for
/// a circle and x for a cross.
pub(crate) fn casing_line(it: &Item) -> String {
    let mut line = format!(
        "casing {} {}",
        pele_num(it.style.stroke_width.unwrap_or(1.0)),
        pele_d(path_of(&it.parts[0]))
    );
    let mut marks = String::new();
    for part in &it.parts[1..] {
        let ops = &path_of(part).ops;
        marks.push(if ops.contains(&Op::CubeTo) {
            'o'
        } else if ops.contains(&Op::Close) {
            'p'
        } else {
            'x'
        });
    }
    if !marks.is_empty() {
        line.push(' ');
        line.push_str(&marks);
    }
    line
}

/// The label of an item, where it has lines.
fn label_of(it: &Item) -> Option<&Placed> {
    it.label.as_ref().filter(|p| !p.label.lines.is_empty())
}

/// Writes the scene of a flowchart or a swimlane diagram as the render oracle reads Pele's
/// SVG of one: a line for each thing drawn, in the order drawn, with numbers as Pele's SVG
/// writes them.
pub(crate) fn flow_lines(sc: &Scene) -> Vec<String> {
    let num = pele_num;
    let pt = |x: f64, y: f64| format!("{},{}", num(x), num(y));
    let boxed =
        |r: Rect, cx: f64, cy: f64| format!("{},{},{}", pt(r.x - cx, r.y - cy), num(r.w), num(r.h));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    // Where Pele's text puts the first line of a label centered on cx, cy.
    let text_at = |l: &crate::scene::text::Label, cx: f64, cy: f64| {
        let shift = l.size * 0.35;
        format!(
            "t {}",
            pt(cx, cy - l.height / 2.0 + l.line_height / 2.0 + shift)
        )
    };
    for it in &sc.items {
        let id = id(it.id.as_deref().unwrap_or(""));
        let classes = classes(it);
        let label = |lines: &mut Vec<String>, cx: f64, cy: f64| {
            if let Some(p) = label_of(it) {
                lines.push(text_at(&p.label, p.at.x - cx, p.at.y - cy));
            }
        };
        // The color the item's label takes on a fill a style sets, if it takes one.
        let ink = |lines: &mut Vec<String>| {
            if label_of(it).is_some() {
                let s = text_ink(it);
                if !s.is_empty() {
                    lines.push(format!("ink {s}"));
                }
            }
        };
        match it.role.as_str() {
            "flowchart.title" => {
                if let Some(p) = label_of(it) {
                    lines.push(format!("title {}", text_at(&p.label, p.at.x, p.at.y)));
                }
            }
            "flowchart.cluster" => {
                lines.push(format!(
                    "cluster {id} [{classes}] {}",
                    boxed(it.bounds, 0.0, 0.0)
                ));
                if it.style.fill_opacity.is_none() {
                    lines.push("opaque".into());
                }
                label(&mut lines, 0.0, 0.0);
                ink(&mut lines);
            }
            "swimlane.lane" => {
                lines.push(format!(
                    "lane {id} [{classes}] {} band {}",
                    boxed(box_of(&it.parts[0]), 0.0, 0.0),
                    boxed(box_of(&it.parts[1]), 0.0, 0.0)
                ));
                label(&mut lines, 0.0, 0.0);
                if let Some(p) = label_of(it)
                    && p.rotate != 0.0
                {
                    lines.push(format!(
                        "rotate {} {} {}",
                        num(p.rotate),
                        num(p.at.x),
                        num(p.at.y)
                    ));
                }
                ink(&mut lines);
            }
            "flowchart.casing" => lines.push(casing_line(it)),
            "flowchart.edge" => {
                let mut flags = String::new();
                if it.style.stroke_width == Some(2.5) {
                    flags.push_str(" thick");
                }
                if it.style.dash.is_some() {
                    flags.push_str(" dotted");
                }
                lines.push(format!(
                    "edge {id} [{classes}]{flags} {}",
                    pele_d(path_of(&it.parts[0]))
                ));
                lines.push(format!("stroke {}", cap_join(it)));
                let s = stroke_ink(it);
                if !s.is_empty() {
                    lines.push(format!("ink {s}"));
                }
                if let Kind::Edge(e) = &it.kind {
                    for m in e.ends.iter().flatten() {
                        if let Some(s) = marker(m) {
                            lines.push(format!("m {s}"));
                        }
                    }
                }
            }
            "flowchart.cluster-title" => {
                lines.push(format!("struck {id}"));
                label(&mut lines, 0.0, 0.0);
            }
            "flowchart.edge-label" => {
                lines.push(format!("label {id} {}", boxed(it.bounds, 0.0, 0.0)));
                label(&mut lines, 0.0, 0.0);
            }
            "flowchart.node" => {
                let b = it.bounds;
                let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
                let shape = it.shape.as_ref().map_or("", |s| s.as_str());
                lines.push(format!(
                    "node {id} {shape} [{classes}] translate({})",
                    pt(cx, cy)
                ));
                if let Some(pic) = &it.picture {
                    match &pic.source {
                        PictureSource::Image { src, .. } => {
                            lines.push(format!("image {src} {}", boxed(pic.bounds, cx, cy)));
                        }
                        PictureSource::Icon(name) => {
                            lines.push(format!(
                                "icon {} {}",
                                self::id(name),
                                boxed(pic.bounds, cx, cy)
                            ));
                        }
                    }
                }
                if let Some(link) = &it.link {
                    lines.push(format!("link {link}"));
                }
                label(&mut lines, cx, cy);
                ink(&mut lines);
            }
            role => lines.push(format!("unknown role {role}")),
        }
    }
    lines
}

/// An id as Pele writes a data-id: its entities decoded, and the characters XML does not
/// allow dropped.
pub(crate) fn id(id: &str) -> String {
    decode_entities(id)
        .chars()
        .filter(|&c| {
            !(c < ' ' && !matches!(c, '\t' | '\n' | '\r')
                || matches!(c, '\x7f' | '\u{fffe}' | '\u{ffff}'))
        })
        .collect()
}

/// The outline Pele's marker draws, as the render oracle reads it, or none for a marker it
/// draws no outline of.
pub(crate) fn marker(m: &Marker) -> Option<String> {
    let num = pele_num;
    let (x, y, dx, dy) = (m.at.x, m.at.y, m.dx, m.dy);
    let (nx, ny) = (-dy, dx);
    match m.kind {
        MarkerType::ArrowPoint => {
            let (bx, by) = (x - dx * MARKER_SIZE, y - dy * MARKER_SIZE);
            let half = MARKER_SIZE * 0.42;
            Some(format!(
                "M{},{}L{},{}L{},{}Z",
                num(x),
                num(y),
                num(bx + nx * half),
                num(by + ny * half),
                num(bx - nx * half),
                num(by - ny * half)
            ))
        }
        MarkerType::ArrowCircle => Some(format!("o {},{}", num(x - dx * 4.0), num(y - dy * 4.0))),
        MarkerType::ArrowCross => {
            let (cx, cy) = (x - dx * 4.0, y - dy * 4.0);
            const A: f64 = 3.5;
            let (ux, uy) = ((dx + nx) * A, (dy + ny) * A);
            let (vx, vy) = ((dx - nx) * A, (dy - ny) * A);
            Some(format!(
                "M{},{}L{},{}M{},{}L{},{}",
                num(cx - ux),
                num(cy - uy),
                num(cx + ux),
                num(cy + uy),
                num(cx - vx),
                num(cy - vy),
                num(cx + vx),
                num(cy + vy)
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::style::Style;
    use crate::scene::{Path, Point};

    /// What Pele's num writes of these numbers, as Node 24 runs it.
    #[test]
    fn nums() {
        for (v, want) in [
            (0.0, "0"),
            (-0.0, "0"),
            (-0.004, "0"),
            (-0.005, "0"),
            (0.005, "0.01"),
            (22.505, "22.51"),
            (-22.505, "-22.5"),
            (1.5, "1.5"),
            (-1.25, "-1.25"),
            (1e9 - 0.01, "999999999.99"),
            (1e9, "1000000000"),
            (-1e9, "-1000000000"),
            (123456789.125, "123456789.13"),
            (1e21, "999999999999999900000"),
            (-1e21, "-999999999999999900000"),
            (1.5e22, "1.5e+22"),
            (1e300, "1e+300"),
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
            (f64::NAN, "NaN"),
        ] {
            assert_eq!(pele_num(v), want, "{v}");
        }
    }

    #[test]
    fn tags_go_last() {
        let read = tags_last(&["clickable"]);
        assert_eq!(
            read("node a rect [clickable x y] translate(1,2)"),
            "node a rect [x y clickable] translate(1,2)"
        );
        assert_eq!(
            read("node a rect [x y] translate(1,2)"),
            "node a rect [x y] translate(1,2)"
        );
        assert_eq!(read("size 1,2"), "size 1,2");
        assert_eq!(read("node a [clickable"), "node a [clickable");
    }

    #[test]
    fn ids() {
        assert_eq!(id("a&quot;b&#65;\u{1}\tc\u{7f}"), "a\"bA\tc");
    }

    #[test]
    fn lines_of_a_scene() {
        let mut path = Path::new();
        path.move_to(0.0, 0.0).line_to(10.0, 0.0);
        let mut edge = Item::new(
            Kind::Edge(crate::scene::Edge {
                route: vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0)],
                ends: [
                    None,
                    Some(Marker {
                        kind: MarkerType::ArrowPoint,
                        at: Point::new(10.0, 0.0),
                        dx: 1.0,
                        dy: 0.0,
                    }),
                ],
            }),
            "flowchart.edge",
        );
        edge.id = Some("L_A_B_0".into());
        edge.parts = vec![Part::path(path, crate::scene::Painting::StrokeOnly)];
        edge.style = Style {
            stroke_width: Some(2.5),
            cap: Some(crate::scene::style::Cap::Round),
            ..Style::default()
        };
        let mut node = Item::new(Kind::Node, "flowchart.node");
        node.id = Some("A".into());
        node.shape = Some("rect".into());
        node.class = vec!["cls".into()];
        node.tags = vec!["clickable".into()];
        node.bounds = Rect::new(0.0, 0.0, 20.0, 10.0);
        let sc = Scene {
            width: 30.0,
            height: 12.5,
            items: vec![edge, node],
            ..Scene::default()
        };
        assert_eq!(
            flow_lines(&sc),
            [
                "size 30,12.5",
                "edge L_A_B_0 [] thick M0,0L10,0",
                "stroke round miter",
                "m M10,0L2,3.36L2,-3.36Z",
                "node A rect [cls clickable] translate(10,5)",
            ]
        );
    }
}
