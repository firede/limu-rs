//! A venn diagram laid out as a scene, as Pele's renderVenn in render.ts draws it.

use std::collections::HashMap;

use super::VennDb;
use super::db::{VennSubset, normalize_text, set_declaration};
use super::layout::{Circle, Spot, find_spot, layout_circles, region_path};
use crate::common::setting;
use crate::css::{self, Resolved, declarations, resolve_style};
use crate::js::math::{hypot, max, min, round};
use crate::layout::Options;
use crate::scene::style::{Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{Item, Kind, Painting, Part, Path, Placed, Point, Rect, Scene, Shape};
use crate::text::layout_label;
use crate::value::Map;

/// The roles of the items of a venn diagram: its title above it, the circle of each set, its
/// id the set's, the outline of an overlap a style gives a look of its own, its id its sets
/// joined by |, the label of a set or an overlap, its id its sets joined by |, and each text
/// written in a region under the region's label, its id the text's.
pub(crate) const ROLE_TITLE: &str = "venn.title";
pub(crate) const ROLE_SET: &str = "venn.set";
pub(crate) const ROLE_OVERLAP: &str = "venn.overlap";
pub(crate) const ROLE_LABEL: &str = "venn.label";
pub(crate) const ROLE_TEXT: &str = "venn.text";

// The measures of Pele's render.ts: the radius the largest circle is drawn at, the most the
// scale grows for what is written in the regions, and the opacity sets are filled at.
const RADIUS: f64 = 100.0;
const MAX_GROWTH: f64 = 1.6;
pub(crate) const FILL_OPACITY: f64 = 0.2;

/// A line of what is written in a region, its label first, then its texts. The id is that of
/// a text.
struct Line {
    id: Option<String>,
    raw: String,
    size: f64,
    base: Face,
    label: Label,
}

/// A set or a stated overlap, Pele's Region.
struct Region {
    sets: Vec<String>,
    inside: Vec<usize>,
    spot: Spot,
    lines: Vec<Line>,
    decls: Vec<String>,
    style: Resolved,
    /// Whether its text is written across other circles.
    across: bool,
}

/// What of an author's declarations a set's circle or an overlap takes: all but the color of
/// text.
fn shape_style(decls: &[String]) -> Style {
    let mut s = declarations(decls).style;
    s.text = None;
    s
}

/// What of an author's declarations a text takes: its color and its opacity.
fn text_style(decls: &[String]) -> Style {
    let s = declarations(decls).style;
    Style {
        text: s.text,
        opacity: s.opacity,
        ..Style::default()
    }
}

/// The region whose fill is under a region's text, as Pele's under finds it: the last
/// overlap drawn over all of the region, or its own circle when its style sets an opacity,
/// and none for text written across other circles. painted are the overlaps drawn.
fn under<'a>(region: &'a Region, painted: &[&'a Region]) -> Option<&'a Region> {
    if region.across {
        return None;
    }
    let mut fill = None;
    if region.sets.len() == 1 && css::declares(&region.style.shape, "fill-opacity") {
        fill = Some(region);
    }
    for &other in painted {
        if other.sets.iter().all(|id| region.sets.contains(id)) {
            fill = Some(other);
        }
    }
    fill
}

/// Lays a venn diagram out as a scene, as Pele's renderVenn draws it: a circle for each set,
/// filled in its series' color, scaled so that the largest has a radius of 100 or, within
/// limits, more for what is written in the regions, each region's label and texts where the
/// region has most room, and the overlaps a style gives a look of their own outlined. Pele
/// reads nothing of the configuration for a venn diagram, and draws it at its own size
/// whatever the width of the host.
pub(crate) fn layout(db: &VennDb, _config: &Map, o: &Options) -> Scene {
    let size = o.font_size;
    let small = round(size * 0.875);
    let tiny = round(size * 0.8125);
    let set = setting(o);
    let pad = o.pad();

    let circles = layout_circles(&db.subsets);
    let index: HashMap<&str, usize> = circles
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();

    // Styles for the same targets add up, later declarations winning. A list of one id is
    // the key of that id, as Pele's keyOf makes them.
    let mut styles: HashMap<&[String], Vec<(String, String)>> = HashMap::new();
    for s in &db.styles {
        let merged = styles.entry(&s.targets).or_default();
        for (property, value) in &s.styles {
            set_declaration(merged, property, value.clone());
        }
    }
    let decls_of = |key: &[String]| -> Vec<String> {
        styles.get(key).map_or(Vec::new(), |declared| {
            declared.iter().map(|(p, v)| format!("{p}:{v}")).collect()
        })
    };

    // Circles that touch each circle. Only those can cut into a region of it.
    let mut neighbors: Vec<Vec<usize>> = vec![Vec::new(); circles.len()];
    for a in 0..circles.len() {
        for b in a + 1..circles.len() {
            let (ca, cb) = (&circles[a], &circles[b]);
            if hypot(ca.x - cb.x, ca.y - cb.y) < ca.r + cb.r {
                neighbors[a].push(b);
                neighbors[b].push(a);
            }
        }
    }

    // One region for each set and each stated overlap. A later statement about the same sets
    // replaces an earlier one.
    let mut order: Vec<&[String]> = Vec::new();
    let mut stated: HashMap<&[String], &VennSubset> = HashMap::new();
    for subset in &db.subsets {
        if stated.insert(&subset.sets, subset).is_none() {
            order.push(&subset.sets);
        }
    }
    let mut regions: Vec<Region> = Vec::new();
    let mut by_key: HashMap<&[String], usize> = HashMap::new();
    for key in order {
        let subset = stated[key];
        let mut sets: Vec<String> = Vec::new();
        for id in &subset.sets {
            if !sets.contains(id) {
                sets.push(id.clone());
            }
        }
        let inside: Vec<usize> = sets.iter().map(|id| index[id.as_str()]).collect();
        let mut smallest = inside[0];
        for &i in &inside {
            if circles[i].r < circles[smallest].r {
                smallest = i;
            }
        }
        let decls = decls_of(key);
        let style = resolve_style(&decls);
        let text = subset.label.as_deref().or(if subset.sets.len() == 1 {
            Some(subset.sets[0].as_str())
        } else {
            None
        });
        let mut base = Face::REGULAR;
        if style.bold {
            base = Face::from_bits(base.bits() | Face::BOLD.bits());
        }
        if style.italic {
            base = Face::from_bits(base.bits() | Face::ITALIC.bits());
        }
        let others: Vec<usize> = neighbors[smallest]
            .iter()
            .copied()
            .filter(|i| !inside.contains(i))
            .collect();
        let mut spot = find_spot(&circles, &inside, &others);
        // A region that other circles cover, or all but cover, is written across them
        // instead.
        let across = !others.is_empty() && spot.margin < circles[smallest].r * 0.2;
        if across {
            spot = find_spot(&circles, &inside, &[]);
        }
        let mut lines = Vec::new();
        if let Some(text) = text.filter(|t| !t.is_empty()) {
            let font_size = if sets.len() == 1 { size } else { small };
            lines.push(Line {
                id: None,
                raw: text.to_string(),
                size: font_size,
                base,
                label: layout_label(text, false, &set, font_size, 4000.0, base),
            });
        }
        by_key.insert(key, regions.len());
        regions.push(Region {
            sets,
            inside,
            spot,
            lines,
            decls,
            style,
            across,
        });
    }
    for node in &db.text_nodes {
        let raw = node.label.as_deref().unwrap_or(&node.id);
        if let Some(&r) = by_key.get(node.sets.as_slice()) {
            regions[r].lines.push(Line {
                id: Some(node.id.clone()),
                raw: raw.to_string(),
                size: tiny,
                base: Face::REGULAR,
                label: layout_label(raw, false, &set, tiny, 4000.0, Face::REGULAR),
            });
        }
    }

    // Lengths so far are in units of the layout. The scale gives the largest circle a set
    // radius, and grows, within limits, until every region holds what is written in it.
    let mut largest = 0.0;
    for c in &circles {
        if c.r > largest {
            largest = c.r;
        }
    }
    let mut scale = if largest > 0.0 { RADIUS / largest } else { 1.0 };
    let mut growth = 1.0;
    for region in &regions {
        if region.spot.margin <= 0.0 {
            continue;
        }
        let (mut width, mut height) = (0.0, 0.0);
        for line in &region.lines {
            width = max(width, line.label.width);
            height += line.label.height;
        }
        let need = hypot(width, height) / 2.0 + 4.0;
        growth = max(growth, min(need / (region.spot.margin * scale), MAX_GROWTH));
    }
    scale *= growth;

    // What is still too wide for its region is wrapped to it.
    for region in &mut regions {
        let room = max(region.spot.margin * scale * 1.8, 72.0);
        for line in &mut region.lines {
            if line.label.width > room {
                line.label = layout_label(&line.raw, false, &set, line.size, room, line.base);
            }
        }
    }

    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for c in &circles {
        min_x = min(min_x, (c.x - c.r) * scale);
        min_y = min(min_y, (c.y - c.r) * scale);
        max_x = max(max_x, (c.x + c.r) * scale);
        max_y = max(max_y, (c.y + c.r) * scale);
    }
    for region in &regions {
        let (mut wide, mut tall) = (0.0, 0.0);
        for line in &region.lines {
            wide = max(wide, line.label.width);
            tall += line.label.height;
        }
        min_x = min(min_x, region.spot.x * scale - wide / 2.0);
        max_x = max(max_x, region.spot.x * scale + wide / 2.0);
        min_y = min(min_y, region.spot.y * scale - tall / 2.0);
        max_y = max(max_y, region.spot.y * scale + tall / 2.0);
    }
    if circles.is_empty() {
        (min_x, min_y, max_x, max_y) = (0.0, 0.0, 0.0, 0.0);
    }

    let title_text = db.title.as_deref().map(normalize_text).unwrap_or_default();
    let title = layout_label(&title_text, false, &set, size, 4000.0, Face::BOLD);
    let title_height = if title.height > 0.0 {
        title.height + 12.0
    } else {
        0.0
    };
    let inner = max(max_x - min_x, title.width);
    let ox = pad + (inner - (max_x - min_x)) / 2.0 - min_x;
    let oy = pad + title_height - min_y;
    let width = (inner + 2.0 * pad).ceil();
    let height = (max_y - min_y + title_height + 2.0 * pad).ceil();
    let placed: Vec<Circle> = circles
        .iter()
        .map(|c| Circle {
            id: c.id.clone(),
            x: c.x * scale + ox,
            y: c.y * scale + oy,
            r: c.r * scale,
        })
        .collect();

    let mut items = Vec::new();
    if !title.is_empty() {
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        let at = Point::new(width / 2.0, pad + title.height / 2.0);
        it.label = Some(Placed::new(title, at));
        it.style.text = Some(Paint::FG);
        items.push(it);
    }

    let single = |id: &str| vec![id.to_string()];
    for (i, c) in placed.iter().enumerate() {
        let decls = by_key
            .get(single(&c.id).as_slice())
            .map_or(&[][..], |&r| &regions[r].decls[..]);
        let mut p = Path::new();
        p.ellipse(c.x, c.y, c.r, c.r);
        let mut it = Item::new(Kind::Mark, ROLE_SET);
        it.shape = Some(Shape::new("circle"));
        it.id = Some(c.id.clone());
        it.bounds = Rect::new(c.x - c.r, c.y - c.r, 2.0 * c.r, 2.0 * c.r);
        it.parts = vec![Part::path(p, Painting::FillAndStroke)];
        // Pele fills a circle in its series' color at an opacity of 0.2, and strokes it in it.
        it.style = Style {
            fill: Some(Paint::series(i)),
            fill_opacity: Some(FILL_OPACITY),
            stroke: Some(Paint::series(i)),
            ..Style::default()
        };
        it.author = shape_style(decls);
        items.push(it);
    }

    for region in &regions {
        // An overlap is only drawn when a style gives it a look of its own. Otherwise the
        // circles show it.
        if region.sets.len() < 2 || region.style.shape.is_empty() {
            continue;
        }
        let shared: Vec<Circle> = region.inside.iter().map(|&i| placed[i].clone()).collect();
        let Some(path) = region_path(&shared) else {
            continue;
        };
        let mut it = Item::new(Kind::Mark, ROLE_OVERLAP);
        it.id = Some(region.sets.join("|"));
        it.bounds = path.bounds();
        it.parts = vec![Part::path(path, Painting::FillAndStroke)];
        it.style = Style {
            fill: Some(Paint::NONE),
            stroke: Some(Paint::NONE),
            ..Style::default()
        };
        it.author = shape_style(&region.decls);
        items.push(it);
    }

    let painted: Vec<&Region> = regions
        .iter()
        .filter(|r| r.sets.len() > 1 && !r.style.shape.is_empty())
        .collect();

    for region in &regions {
        let total: f64 = region.lines.iter().map(|l| l.label.height).sum();
        if total == 0.0 {
            continue;
        }
        let id = region.sets.join("|");
        // The text takes the color that stands out on the fill under it, as Pele's ink picks
        // it, unless its own style sets a color.
        let fill = under(region, &painted);
        let ground = |text: &Resolved| -> Option<Paint> {
            let fill = fill?;
            if !fill.style.inks_text(text) {
                return None;
            }
            shape_style(&fill.decls).fill
        };
        let x = region.spot.x * scale + ox;
        let mut y = region.spot.y * scale + oy - total / 2.0;
        for line in &region.lines {
            if !line.label.is_empty() {
                let at = Point::new(x, y + line.label.height / 2.0);
                let mut it = match &line.id {
                    None => {
                        let mut it = Item::new(Kind::Text, ROLE_LABEL);
                        it.id = Some(id.clone());
                        it.style.text = Some(Paint::FG);
                        it.style.text_ground = ground(&region.style);
                        it.author = text_style(&region.decls);
                        it
                    }
                    Some(text_id) => {
                        let decls = decls_of(&single(text_id));
                        let mut it = Item::new(Kind::Text, ROLE_TEXT);
                        it.id = Some(text_id.clone());
                        it.style.text = Some(Paint::MUTED);
                        it.style.text_ground = ground(&resolve_style(&decls));
                        it.author = text_style(&decls);
                        it
                    }
                };
                it.label = Some(Placed::new(line.label.clone(), at));
                it.bounds = it.label_bounds();
                items.push(it);
            }
            y += line.label.height;
        }
    }

    Scene {
        diagram: "venn".into(),
        width,
        height,
        font_size: size,
        title: None,
        desc: None,
        items,
    }
}
