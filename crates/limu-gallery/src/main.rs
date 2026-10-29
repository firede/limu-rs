//! Writes a page that sets the samples every backend draws side by side, into
//! target/gallery/index.html: the scene written as SVG, which is checked against Pele, and
//! each backend's golden file. Where a backend's test drew a sample otherwise than its golden
//! file, and wrote what it drew into target/golden, the page shows both and where they
//! differ, so a change to a backend is looked at before its golden files are written anew.
//!
//! The page holds everything it shows, so it opens from the disk and can be sent on.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use limu_scene::style::{Look, Theme, Token};
use limu_scene::svg;

/// A backend whose golden files the page shows: its name, its crate, and the extension of
/// its golden files, an image for "png" and text otherwise.
struct Backend {
    name: &'static str,
    krate: &'static str,
    ext: &'static str,
}

const BACKENDS: [Backend; 4] = [
    Backend {
        name: "tiny-skia",
        krate: "limu-tiny-skia",
        ext: "png",
    },
    Backend {
        name: "coregraphics",
        krate: "limu-coregraphics",
        ext: "png",
    },
    Backend {
        name: "gpui",
        krate: "limu-gpui",
        ext: "txt",
    },
    Backend {
        name: "ascii",
        krate: "limu-ascii",
        ext: "txt",
    },
];

/// What a backend drew of a sample: its golden file, and what its test drew instead where
/// that differs and was drawn since the golden file was written.
struct Drawn {
    golden: Option<Vec<u8>>,
    new: Option<Vec<u8>>,
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = root.join("target/gallery");
    fs::create_dir_all(&out).expect("making target/gallery");
    let page = page(&root);
    let path = out.join("index.html");
    fs::write(&path, page).expect("writing the page");
    println!("{}", path.canonicalize().unwrap_or(path).display());
}

/// The page.
fn page(root: &Path) -> String {
    let samples = limu_samples::all();
    let order: Vec<&str> = samples.iter().map(|s| s.name).collect();
    // The SVG of each sample, and of the sample drawn in a dark look, as backends draw it,
    // with the background of its theme, which a page gives it.
    let light = Theme::default().color(Token::Bg).to_string();
    let mut svgs: BTreeMap<String, (String, String)> = samples
        .iter()
        .map(|s| {
            let svg = svg::render(&s.scene, &Default::default());
            (s.name.to_string(), (svg, light.clone()))
        })
        .collect();
    if let Some(s) = samples.iter().find(|s| s.name == "sample") {
        let dark = svg::Options {
            look: Look::new(Theme::KOHU_DARK),
            literal: true,
            ..Default::default()
        };
        let bg = Theme::KOHU_DARK.color(Token::Bg).to_string();
        svgs.insert("sample-dark".into(), (svg::render(&s.scene, &dark), bg));
    }
    // Each name drawn by any backend, after the sample it is drawn from, as "class-ascii"
    // after "class".
    let mut names: Vec<String> = svgs.keys().cloned().collect();
    for b in &BACKENDS {
        for name in stems(&golden_dir(root, b), b.ext)
            .into_iter()
            .chain(stems(&new_dir(root, b), b.ext))
        {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    let base = |name: &str| {
        order
            .iter()
            .position(|&s| name == s || name.strip_prefix(s).is_some_and(|r| r.starts_with('-')))
            .unwrap_or(order.len())
    };
    names.sort_by(|a, b| (base(a), a).cmp(&(base(b), b)));
    // Each sample is shown as wide as its scene in every column, though a backend may draw
    // it at a larger scale.
    let width = |name: &str| samples.get(base(name)).map(|s| s.scene.width);

    let mut rows = String::new();
    let mut changed = 0;
    for name in &names {
        let drawn: Vec<(&Backend, Drawn)> =
            BACKENDS.iter().map(|b| (b, drawn(root, b, name))).collect();
        let differ: Vec<&str> = drawn
            .iter()
            .filter(|(_, d)| d.new.is_some())
            .map(|(b, _)| b.name)
            .collect();
        changed += usize::from(!differ.is_empty());
        let style = width(name)
            .map(|w| format!(" style=\"--w: {w}px\""))
            .unwrap_or_default();
        let _ = write!(
            rows,
            "<section class=\"sample\" id=\"{n}\" data-changed=\"{c}\"{style}><h2><a href=\"#{n}\">{n}</a>",
            n = escape(name),
            c = !differ.is_empty(),
        );
        if !differ.is_empty() {
            let _ = write!(rows, " <span class=\"badge\">{}</span>", differ.join(", "));
        }
        rows.push_str("</h2><div class=\"cells\">");
        rows.push_str("<figure class=\"cell\" data-col=\"svg\"><figcaption>svg</figcaption>");
        match svgs.get(name) {
            Some((s, bg)) => {
                let _ = write!(
                    rows,
                    "<img alt=\"\" style=\"background: {bg}\" src=\"data:image/svg+xml;base64,{}\">",
                    base64(s.as_bytes())
                );
            }
            None => rows.push_str("<p class=\"none\">none</p>"),
        }
        rows.push_str("</figure>");
        for (b, d) in &drawn {
            cell(&mut rows, b, d);
        }
        rows.push_str("</div></section>");
    }
    let columns: String = std::iter::once("svg")
        .chain(BACKENDS.iter().map(|b| b.name))
        .map(|c| format!("<label><input type=\"checkbox\" data-col=\"{c}\" checked> {c}</label>"))
        .collect();
    TEMPLATE
        .replace("{{count}}", &names.len().to_string())
        .replace("{{changed}}", &changed.to_string())
        .replace("{{columns}}", &columns)
        .replace("{{rows}}", &rows)
}

/// A backend's cell of a sample: its golden file, or that and what its test drew instead.
fn cell(out: &mut String, b: &Backend, d: &Drawn) {
    let image = b.ext == "png";
    let _ = write!(
        out,
        "<figure class=\"cell{}\" data-col=\"{}\" data-kind=\"{}\"><figcaption>{}",
        if d.new.is_some() { " changed" } else { "" },
        b.name,
        if image { "image" } else { "text" },
        b.name,
    );
    let show = |bytes: &[u8]| {
        if image {
            format!(
                "<img alt=\"\" src=\"data:image/png;base64,{}\">",
                base64(bytes)
            )
        } else {
            let text = String::from_utf8_lossy(bytes);
            let pre = format!("<pre>{}</pre>", escape(&text));
            // A record of a canvas's calls is long, and is opened where it is wanted.
            if b.name == "gpui" {
                format!(
                    "<details><summary>{} calls</summary>{pre}</details>",
                    text.lines().count()
                )
            } else {
                pre
            }
        }
    };
    match (&d.golden, &d.new) {
        (golden, Some(new)) => {
            out.push_str(
                " <span class=\"tabs\"><button data-show=\"diff\">diff</button>\
                 <button data-show=\"golden\">golden</button>\
                 <button data-show=\"new\">new</button></span></figcaption>",
            );
            let golden = golden.as_deref().map(show);
            let _ = write!(
                out,
                "<div class=\"view\" data-view=\"diff\"></div>\
                 <div class=\"view\" data-view=\"golden\" hidden>{}</div>\
                 <div class=\"view\" data-view=\"new\" hidden>{}</div>",
                golden.unwrap_or_else(|| "<p class=\"none\">none</p>".into()),
                show(new),
            );
        }
        (Some(golden), None) => {
            out.push_str("</figcaption>");
            out.push_str(&show(golden));
        }
        (None, None) => out.push_str("</figcaption><p class=\"none\">none</p>"),
    }
    out.push_str("</figure>");
}

/// What a backend drew of a sample. What its test drew is left out where it is older than
/// the golden file, which was written anew since, or the same as it.
fn drawn(root: &Path, b: &Backend, name: &str) -> Drawn {
    let file = format!("{name}.{}", b.ext);
    let golden_path = golden_dir(root, b).join(&file);
    let new_path = new_dir(root, b).join(&file);
    let golden = fs::read(&golden_path).ok();
    let modified = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
    let new = fs::read(&new_path).ok().filter(|new| {
        Some(new) != golden.as_ref()
            && match (modified(&new_path), modified(&golden_path)) {
                (Some(n), Some(g)) => n >= g,
                _ => true,
            }
    });
    Drawn { golden, new }
}

/// The folder a backend's golden files are in.
fn golden_dir(root: &Path, b: &Backend) -> PathBuf {
    root.join("crates").join(b.krate).join("tests/golden")
}

/// The folder a backend's tests write what they drew into where it differs.
fn new_dir(root: &Path, b: &Backend) -> PathBuf {
    root.join("target/golden").join(b.name)
}

/// The names of the files of a folder with an extension, without it.
fn stems(dir: &Path, ext: &str) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension()? == ext).then(|| p.file_stem()?.to_str().map(String::from))?
        })
        .collect()
}

/// Text with the characters HTML gives a meaning escaped.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Bytes in Base64, as a data URL holds them.
fn base64(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(DIGITS[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

const TEMPLATE: &str = include_str!("page.html");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_pads() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
