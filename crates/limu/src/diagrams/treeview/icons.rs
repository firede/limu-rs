//! The icons of a tree view's entries, as Pele's icons.ts finds them, and the two it draws
//! itself.

use super::model::{NodeType, TreeNode};
use crate::js::chars::to_lower;
use crate::scene::Path;
use crate::value::{Map, Value};

/// Mermaid's name for its pack of the tree view's own icons, Pele's BUILTIN_PACK. Only `file`
/// and `folder` are in it, and every other icon name is offered to the host.
pub(crate) const BUILTIN_PACK: &str = "mermaid-treeview";

/// Pele's GLYPHS: the outlines of the icons in the built-in pack on a 24 by 24 grid, as SVG
/// path data.
pub(crate) const GLYPHS: [(&str, &str); 2] = [
    (
        "folder",
        "M3 6.5A1.5 1.5 0 0 1 4.5 5h4.6l2.4 2.5h8A1.5 1.5 0 0 1 21 9v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18z",
    ),
    (
        "file",
        "M6.5 3h7L19 8.5v11a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 5 19.5v-15A1.5 1.5 0 0 1 6.5 3zM13.5 3v5.5H19",
    ),
];

/// How wide Pele strokes a glyph, in units of its grid.
pub(crate) const GLYPH_STROKE: f64 = 1.5;

/// The name and the outline of a glyph of the built-in pack on its grid, or none for a name
/// not in it.
pub(crate) fn glyph(name: &str) -> Option<(&'static str, Path)> {
    let &(name, d) = GLYPHS.iter().find(|(n, _)| *n == name)?;
    Some((name, Path::parse(d).expect("a glyph's path data reads")))
}

/// How a tree view shows icons, as its config sets it, Pele's IconConfig: whether entries
/// show one without an icon() of their own, the pack a name without one is looked for in,
/// and the icons that file names and extensions are shown with.
pub(crate) struct IconConfig<'a> {
    pub(crate) show_icons: bool,
    pub(crate) default_icon_pack: &'a str,
    pub(crate) filename_icons: Option<&'a Map>,
    pub(crate) extension_icons: Option<&'a Map>,
}

/// The string a map holds under a key, as Pele's own reads it.
fn own<'a>(map: Option<&'a Map>, key: &str) -> Option<&'a str> {
    map?.get(key)?.as_str()
}

/// A file's icon from the maps of the config, as Pele's detectIcon finds it. A file name wins
/// over an extension, and extensions match in lower case with or without the leading dot.
fn detect_icon<'a>(name: &str, config: &IconConfig<'a>) -> Option<&'a str> {
    if let Some(by_name) = own(config.filename_icons, name).filter(|s| !s.is_empty()) {
        return Some(by_name);
    }
    let dot = name.rfind('.').filter(|&dot| dot > 0)?;
    let ext = to_lower(&name[dot..]);
    own(config.extension_icons, &ext).or_else(|| own(config.extension_icons, &ext[1..]))
}

/// An icon's name with its pack, as Pele's qualifyIcon gives it.
fn qualify_icon(icon: &str, default_icon_pack: &str) -> String {
    if icon.contains(':') {
        icon.to_string()
    } else if glyph_named(icon) || default_icon_pack.is_empty() {
        format!("{BUILTIN_PACK}:{icon}")
    } else {
        format!("{default_icon_pack}:{icon}")
    }
}

/// Whether the built-in pack has a glyph of a name.
fn glyph_named(name: &str) -> bool {
    GLYPHS.iter().any(|(n, _)| *n == name)
}

/// The qualified name of an entry's icon, or none when it shows none, as Pele's getNodeIcon
/// gives it. An icon() annotation always wins, and otherwise icons appear only with
/// showIcons.
pub(crate) fn get_node_icon(node: &TreeNode, config: &IconConfig) -> Option<String> {
    match node.icon.as_deref() {
        Some("none") => return None,
        Some(icon) if !icon.is_empty() => {
            return Some(qualify_icon(icon, config.default_icon_pack));
        }
        _ => {}
    }
    if !config.show_icons {
        return None;
    }
    if node.node_type == NodeType::File {
        match detect_icon(&node.name, config) {
            Some("none") => return None,
            Some(detected) if !detected.is_empty() => {
                return Some(qualify_icon(detected, config.default_icon_pack));
            }
            _ => {}
        }
    }
    let name = match node.node_type {
        NodeType::Directory => "folder",
        NodeType::File => "file",
    };
    Some(format!("{BUILTIN_PACK}:{name}"))
}

/// The icon settings of a diagram's config, as Pele's iconConfig reads them.
pub(crate) fn icon_config(config: &Map) -> IconConfig<'_> {
    let section = config.get("treeView").and_then(Value::as_object);
    let get = |key: &str| section.and_then(|c| c.get(key));
    IconConfig {
        show_icons: get("showIcons").is_some_and(Value::truthy),
        default_icon_pack: get("defaultIconPack").and_then(Value::as_str).unwrap_or(""),
        filename_icons: get("filenameIcons").and_then(Value::as_object),
        extension_icons: get("extensionIcons").and_then(Value::as_object),
    }
}
