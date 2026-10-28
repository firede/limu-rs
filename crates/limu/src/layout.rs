//! What a layout takes besides the diagram: what changes where things are. A change of these
//! lays a diagram out again, where a change of style does not.

use crate::scene::Grid;
use crate::scene::text::Measurer;
use crate::text::Arial;

static ARIAL: Arial = Arial;

/// How edges run between the things they join, in the diagrams laid out in ranks:
/// flowcharts, swimlane, agent flow, state, class, ER, requirement and use case diagrams. The
/// other diagrams route their edges as Pele does and pay it no heed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Routing {
    /// As the diagram's text asks with its curve setting, or as Pele draws them where it
    /// asks nothing.
    #[default]
    Auto,
    /// In straight runs across and down, turning at right angles, in place of the step
    /// curves.
    Orthogonal,
    /// In straight lines from corner to corner, in place of the linear curve.
    Straight,
}

/// The parts of diagrams that are set in the monospace face, as Pele's monospace option lists
/// them. A part whose author gives it a font of its own keeps it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Monospace(u8);

impl Monospace {
    /// None of them.
    pub const NONE: Monospace = Monospace(0);
    /// The attributes and methods of a class diagram's classes, Pele's "members".
    pub const MEMBERS: Monospace = Monospace(1);
    /// The types, names and keys of the attributes of an entity relationship diagram's
    /// entities, Pele's "attributes". Their comments keep the regular face.
    pub const ATTRIBUTES: Monospace = Monospace(1 << 1);
    /// The names of a tree view's entries, Pele's "files".
    pub const FILES: Monospace = Monospace(1 << 2);
    /// The text of a sequence diagram's messages, Pele's "messages".
    pub const MESSAGES: Monospace = Monospace(1 << 3);

    /// Whether all of other's parts are among these.
    pub const fn contains(self, other: Monospace) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for Monospace {
    type Output = Monospace;

    fn bitor(self, other: Monospace) -> Monospace {
        Monospace(self.0 | other.0)
    }
}

/// What a layout takes besides the diagram. `Options::default()` is what Pele does with no
/// options of the host's, with text measured by [`Arial`].
#[derive(Clone, Copy)]
pub struct Options<'a> {
    /// What text is measured with. Layout depends on it, so a diagram is drawn in the faces
    /// it was measured in.
    pub measurer: &'a dyn Measurer,
    /// The size of text with no size of its own, 16 by default.
    pub font_size: f64,
    /// Whether the host can draw an icon of a name, as "fa:fa-car". A label leaves room only
    /// for the icons it can draw. With none, it draws none.
    pub icons: Option<&'a dyn Fn(&str) -> bool>,
    /// The width the host has for the diagram. A diagram that can draw itself narrower does,
    /// so that its text is not shrunk with it. It is infinite by default, which is no limit.
    pub max_width: f64,
    /// Whether a diagram that runs across keeps to it when it does not fit `max_width`. By
    /// default it turns to run down where `max_width` is under `direction_breakpoint` and
    /// running down is narrower.
    pub keep_direction: bool,
    /// The width under which a diagram may turn to run down, 640 by default.
    pub direction_breakpoint: f64,
    /// Whether the links a diagram's text asks for are kept, as they are by default.
    pub links: bool,
    /// Whether the images a diagram's text asks for are kept, as they are by default.
    pub images: bool,
    /// The schemes a link may use besides a relative address: http, https, mailto and tel
    /// by default.
    pub link_schemes: &'a [&'a str],
    /// The schemes an image may use besides a relative address: http and https by default.
    pub image_schemes: &'a [&'a str],
    /// The room about a diagram, 8 by default, as Pele's padding option.
    pub padding: f64,
    /// A grid to snap where things are to, for a backend that draws on a grid of pixels or
    /// of cells, or none to leave them as they fall.
    pub snap: Option<Grid>,
    /// How edges run, in the diagrams laid out in ranks.
    pub routing: Routing,
    /// The parts of diagrams set in the monospace face. They are measured in it, so it
    /// changes where things are.
    pub monospace: Monospace,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Options {
            measurer: &ARIAL,
            font_size: 16.0,
            icons: None,
            max_width: f64::INFINITY,
            keep_direction: false,
            direction_breakpoint: 640.0,
            links: true,
            images: true,
            link_schemes: &["http", "https", "mailto", "tel"],
            image_schemes: &["http", "https"],
            padding: 8.0,
            snap: None,
            routing: Routing::Auto,
            monospace: Monospace::NONE,
        }
    }
}

impl Options<'_> {
    /// The links and images the host lets a diagram have.
    pub(crate) fn schemes(&self) -> crate::url::Schemes<'_> {
        crate::url::Schemes {
            links: self.links,
            images: self.images,
            link_schemes: self.link_schemes,
            image_schemes: self.image_schemes,
        }
    }

    /// Whether the host can draw an icon of a name.
    pub(crate) fn has_icon(&self, name: &str) -> bool {
        self.icons.is_some_and(|icons| icons(name))
    }
}

impl std::fmt::Debug for Options<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Options")
            .field("font_size", &self.font_size)
            .field("icons", &self.icons.is_some())
            .field("max_width", &self.max_width)
            .field("keep_direction", &self.keep_direction)
            .field("direction_breakpoint", &self.direction_breakpoint)
            .field("links", &self.links)
            .field("images", &self.images)
            .field("link_schemes", &self.link_schemes)
            .field("image_schemes", &self.image_schemes)
            .field("padding", &self.padding)
            .field("snap", &self.snap)
            .field("routing", &self.routing)
            .field("monospace", &self.monospace)
            .finish_non_exhaustive()
    }
}
