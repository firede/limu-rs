//! What the diagrams share beyond layout and drawing, as Pele's src/diagrams/common does:
//! fitting a diagram to the width the host has, the curve the host's routing asks for, a
//! diagram's title, the names of classes, generic types written with tildes, text laid out
//! plainly or kept as written, words matched without regard to case, icons, the tokenizer of
//! the diagrams Mermaid defines with Langium grammars, the lexer rules of those it defines
//! with Jison grammars, and the indented outline that mindmaps and kanban boards are written
//! in.

mod caseless;
mod fit;
mod fit_text;
mod generics;
mod icon;
mod literal_label;
pub(crate) mod outline;
pub(crate) mod scan;
pub(crate) mod tokens;

#[cfg(test)]
mod tests;

pub(crate) use caseless::word_at;
pub(crate) use fit::{fit_label, plain_label};
pub(crate) use fit_text::{Fitted, fit_text};
pub(crate) use generics::parse_generic_types;
pub(crate) use icon::{has_icon, icon_svg};
pub(crate) use literal_label::{break_label, literal_label};

use crate::js::chars::is_space_char;
use crate::js::math::{max, min, round};
use crate::layout::{Options, Routing};
use crate::scene::style::{Paint, Style};
use crate::scene::text::{Face, Label, Span};
use crate::scene::{Item, Kind, Placed, Point, Scene};
use crate::text::{Setting, layout_label};

/// The width under which a diagram that runs across may be turned to run down, by default,
/// as Pele's BREAKPOINT.
const BREAKPOINT: f64 = 640.0;

/// The curve, by Mermaid's name, that the host's routing asks edges to be drawn with, or none
/// for Routing::Auto, which leaves the diagram's own. Pele has no such setting. It is limu's.
pub(crate) fn routing_curve(r: Routing) -> Option<&'static str> {
    match r {
        Routing::Auto => None,
        Routing::Orthogonal => Some("step"),
        Routing::Straight => Some("linear"),
    }
}

/// Whether a direction runs across, as Pele's runsAcross says.
pub(crate) fn runs_across(direction: Option<&str>) -> bool {
    matches!(direction, Some("LR" | "RL"))
}

/// Lays a diagram out with room to spare, and tightly instead when that does not fit the
/// width the host has and tight is narrower, as Pele's tighten does.
pub(crate) fn tighten(o: &Options, mut lay: impl FnMut(bool) -> Scene) -> Scene {
    let loose = lay(false);
    if !(o.max_width > 0.0) || loose.width <= o.max_width {
        return loose;
    }
    let tight = lay(true);
    if tight.width < loose.width {
        tight
    } else {
        loose
    }
}

/// Lays a diagram out across, and down instead when the host allows it, the width it has is
/// under the breakpoint, across does not fit, and down is narrower, as Pele's turnToFit does.
pub(crate) fn turn_to_fit(o: &Options, mut lay: impl FnMut(bool) -> Scene) -> Scene {
    let across = lay(false);
    if o.keep_direction || !(o.max_width > 0.0) || across.width <= o.max_width {
        return across;
    }
    let breakpoint = if o.direction_breakpoint > 0.0 {
        o.direction_breakpoint
    } else {
        BREAKPOINT
    };
    if o.max_width >= breakpoint {
        return across;
    }
    let down = lay(true);
    if down.width < across.width {
        down
    } else {
        across
    }
}

/// Lays out a chart whose plot can be any width so that the whole of it fits the width the
/// host has, as Pele's fitWidth does. What surrounds the plot is only known once it is laid
/// out, so the plot gives up the overflow and is laid out again, at most three times more.
/// The second argument of lay tells the chart that it did not fit at its own width.
pub(crate) fn fit_width(
    o: &Options,
    natural: f64,
    least: f64,
    mut lay: impl FnMut(f64, bool) -> Scene,
) -> Scene {
    let mut sc = lay(natural, false);
    let available = o.max_width;
    if !(available > 0.0) || sc.width <= available {
        return sc;
    }
    let floor = min(least, natural);
    let mut plot = natural;
    sc = lay(plot, true);
    let mut round = 0;
    while round < 3 && sc.width > available && plot > floor {
        plot = max(floor, plot - (sc.width - available));
        sc = lay(plot, true);
        round += 1;
    }
    sc
}

/// The width a title may take before it wraps, as Pele's titleRoom gives it.
pub(crate) fn title_room(o: &Options) -> f64 {
    if o.max_width > 0.0 {
        max(80.0, o.max_width - 2.0 * o.pad())
    } else {
        4000.0
    }
}

/// The width of a chart drawn to a set width: its own, or the host's when that is smaller,
/// with around what the chart adds outside it, as Pele's capWidth gives it.
pub(crate) fn cap_width(o: &Options, natural: f64, least: f64, around: f64) -> f64 {
    if !(o.max_width > 0.0) {
        return natural;
    }
    max(min(least, natural), min(natural, o.max_width - around))
}

/// What a diagram's text is set with, of the options.
pub(crate) fn setting<'a>(o: &'a Options<'a>) -> Setting<'a> {
    Setting {
        measurer: o.measurer,
        icons: match &o.icons {
            Some(icons) => *icons,
            None => &|_| false,
        },
    }
}

/// A diagram's title laid out, as Pele's titleLabel lays it out: as text, whatever it holds,
/// in bold.
pub(crate) fn title_label(title: Option<&str>, set: &Setting, size: f64, max_width: f64) -> Label {
    layout_label(title.unwrap_or(""), false, set, size, max_width, Face::BOLD)
}

/// Puts a title above a scene that was laid out without one, as Pele's withTitle does: the
/// scene grows to hold it, what was in it moves down below it, and across to stay centered
/// where the title is the wider, and the title is pad under the top. The title is an item of
/// the role given.
pub(crate) fn with_title(
    sc: &mut Scene,
    title: Option<&str>,
    role: &'static str,
    set: &Setting,
    size: f64,
    pad: f64,
) {
    let label = title_label(title, set, size, 4000.0);
    if label.height == 0.0 {
        return;
    }
    let shift = label.height + 12.0;
    let full = max(sc.width, (label.width + 2.0 * pad).ceil());
    let dx = (full - sc.width) / 2.0;
    for it in &mut sc.items {
        it.move_by(dx, shift);
    }
    sc.width = full;
    sc.height = (sc.height + shift).ceil();
    let at = Point::new(full / 2.0, pad + label.height / 2.0);
    let mut it = Item::new(Kind::Text, role);
    it.label = Some(Placed::new(label, at));
    it.style = Style {
        text: Some(Paint::FG),
        ..Style::default()
    };
    sc.items.insert(0, it);
}

/// A label of one line of plain text, as wide as it was measured, in a size and face of type,
/// a line of Pele's height for the size. It holds text Pele writes as it is, with no markup
/// read, as a percentage, a value or a name. Pele has no such label. It is limu's.
pub(crate) fn line(s: &str, width: f64, size: f64, face: Face) -> Label {
    line_of(s, width, size, round(size * crate::text::LINE_HEIGHT), face)
}

/// A label of one line of text the diagram measured itself, width wide, set at size in a
/// face, a line height high. Pele writes such a text as it is, with no label laid out, where
/// the scene holds a label for every text.
pub(crate) fn line_of(text: &str, width: f64, size: f64, line_height: f64, face: Face) -> Label {
    Label {
        lines: vec![vec![Span {
            text: text.to_string(),
            face,
            icon: None,
            width,
        }]],
        widths: vec![width],
        width,
        height: line_height,
        size,
        line_height,
    }
}

/// The names of a class attribute, as Pele's classNames in src/svg/theme.ts cleans them:
/// what is not an ASCII letter, a digit, an underscore or a hyphen, as /[^\w-]+/ takes it,
/// separates them.
pub(crate) fn class_names(raw: &str) -> Vec<String> {
    raw.split(|c: char| !(c == '_' || c == '-' || c.is_ascii_alphanumeric()))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// The classes of a node without /^default\s?/, as Pele takes it off before it writes them:
/// the class Mermaid gives every node, and the space after it.
pub(crate) fn trim_default(classes: &str) -> &str {
    let Some(rest) = classes.strip_prefix("default") else {
        return classes;
    };
    match rest.chars().next() {
        Some(c) if is_space_char(c) => &rest[c.len_utf8()..],
        _ => rest,
    }
}

/// The classes Pele gives a node, split into those its author gave it and its tags:
/// "clickable", which Mermaid adds to the classes of a node with a click, is a tag, and the
/// rest are the author's, in their order.
pub(crate) fn clickable(classes: Vec<String>) -> (Vec<String>, Vec<String>) {
    classes.into_iter().partition(|c| c != "clickable")
}
