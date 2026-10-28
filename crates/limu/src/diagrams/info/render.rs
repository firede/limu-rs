//! An info diagram laid out as a scene, as Pele's renderInfo in render.ts draws it.

use super::InfoModel;
use crate::common::setting;
use crate::layout::Options;
use crate::scene::style::{Paint, Style};
use crate::scene::text::Face;
use crate::scene::{Item, Kind, Placed, Point, Scene};
use crate::text::layout_label;
use crate::value::Map;

/// The roles of the items of an info diagram.
pub(crate) const ROLE_TITLE: &str = "info.title";
pub(crate) const ROLE_VERSION: &str = "info.version";

/// The renderer's name and version that an info diagram draws. Pele draws its own, "Pele
/// v1.12.0", and limu draws its own in its place.
pub(crate) const VERSION: &str = concat!("limu v", env!("CARGO_PKG_VERSION"));

/// Lays an info diagram out as a scene, as Pele's renderInfo draws it: the title, where
/// there is one, and under it limu's name and version.
pub(crate) fn layout(model: &InfoModel, _config: &Map, o: &Options) -> Scene {
    lay(model, o, VERSION)
}

/// Pele's renderInfo, drawing name as the renderer's name and version.
pub(crate) fn lay(model: &InfoModel, o: &Options, name: &str) -> Scene {
    let size = o.font_size;
    let set = setting(o);
    let pad = o.padding;

    let raw = model.title.as_deref().unwrap_or("");
    let title = layout_label(raw, false, &set, size, 4000.0, Face::BOLD);
    let title_height = if title.height > 0.0 {
        title.height + 8.0
    } else {
        0.0
    };
    let text = layout_label(name, false, &set, size, 4000.0, Face::default());

    let width = (crate::js::math::max(title.width, text.width) + 2.0 * pad).ceil();
    let mut sc = Scene {
        diagram: "info".into(),
        width,
        height: (title_height + text.height + 2.0 * pad).ceil(),
        font_size: size,
        title: model.acc_title.clone(),
        desc: model.acc_descr.clone(),
        items: Vec::new(),
    };
    if !title.is_empty() {
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        let at = Point::new(width / 2.0, pad + title.height / 2.0);
        it.label = Some(Placed::new(title.clone(), at));
        it.style = Style {
            text: Some(Paint::FG),
            ..Style::default()
        };
        sc.items.push(it);
    }
    if !text.is_empty() {
        let mut it = Item::new(Kind::Text, ROLE_VERSION);
        let at = Point::new(width / 2.0, pad + title_height + text.height / 2.0);
        it.label = Some(Placed::new(text, at));
        it.style = Style {
            text: Some(Paint::MUTED),
            ..Style::default()
        };
        sc.items.push(it);
    }
    sc
}
