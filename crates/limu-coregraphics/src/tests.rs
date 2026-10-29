//! The samples every backend draws, drawn by Core Graphics in the Liberation fonts, compared
//! with golden images of its own and, more loosely, with limu-tiny-skia's. A golden image is
//! written anew where UPDATE_GOLDEN is set. What a test draws is written into target/golden
//! for a look when it differs.

use std::path::{Path, PathBuf};

use limu_samples::Sample;
use limu_scene::style::Theme;
use limu_scene::text;
use tiny_skia::{IntSize, Pixmap};

use super::*;

fn fonts() -> Fonts {
    let mut f = Fonts::from_data(&limu_fixtures::font("LiberationSans-Regular.ttf")).unwrap();
    for (face, name) in [
        (Face::BOLD, "LiberationSans-Bold.ttf"),
        (Face::ITALIC, "LiberationSans-Italic.ttf"),
        (Face::BOLD | Face::ITALIC, "LiberationSans-BoldItalic.ttf"),
        (Face::MONO, "LiberationMono-Regular.ttf"),
    ] {
        f.set_data(face, &limu_fixtures::font(name)).unwrap();
    }
    f
}

fn options(fonts: &Fonts, scale: f64) -> Options<'_> {
    Options {
        fonts: Some(fonts),
        scale,
        icons: Some(Box::new(limu_samples::icon)),
        images: Some(Box::new(|src| {
            let img = limu_samples::image(src)?;
            let mut data = img.rgba;
            for px in data.chunks_mut(4) {
                let a = u16::from(px[3]);
                for c in &mut px[..3] {
                    *c = ((u16::from(*c) * a + 127) / 255) as u8;
                }
            }
            sys::image(img.width as usize, img.height as usize, &data)
        })),
        ..Options::default()
    }
}

/// A scene drawn by Core Graphics, as a pixmap.
fn draw_pixmap(scene: &Scene, o: &Options<'_>) -> Pixmap {
    let cx = bitmap(scene, o).expect("a bitmap");
    let (w, h, data) = sys::pixels(&cx);
    Pixmap::from_vec(data, IntSize::from_wh(w as u32, h as u32).unwrap()).unwrap()
}

/// How many pixels of two pixmaps of a size differ in a channel by more than by, or none for
/// pixmaps of different sizes.
fn differ(a: &Pixmap, b: &Pixmap, by: u8) -> Option<usize> {
    ((a.width(), a.height()) == (b.width(), b.height())).then(|| {
        a.data()
            .chunks(4)
            .zip(b.data().chunks(4))
            .filter(|(p, q)| p.iter().zip(*q).any(|(x, y)| x.abs_diff(*y) > by))
            .count()
    })
}

fn here() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Writes what was drawn into target/golden, and says where.
fn keep(name: &str, got: &Pixmap) -> PathBuf {
    let out = here().join("../../target/golden/coregraphics");
    std::fs::create_dir_all(&out).unwrap();
    let path = out.join(name);
    got.save_png(&path).unwrap();
    path
}

/// Compares a pixmap with a golden image of this crate's, which it may differ from by a
/// little in a few pixels, as Core Graphics smooths edges a little differently from one
/// release of macOS to the next. Then compares it with limu-tiny-skia's, which it may differ
/// from by more in more pixels, as two rasterizers do, but not as a drawing turned over, out
/// of place or missing a stroke does. Core Graphics and tiny-skia draw the samples within a
/// channel of 32 of each other in all but some tenths of a percent of their pixels.
fn golden(name: &str, got: &Pixmap) -> Result<(), String> {
    let path = here().join("tests/golden").join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        got.save_png(&path).unwrap();
    }
    let pixels = got.data().len() / 4;
    let own = Pixmap::load_png(&path)
        .ok()
        .and_then(|want| differ(&want, got, 8));
    if own.is_none_or(|n| n > pixels / 200) {
        let kept = keep(name, got);
        return Err(format!(
            "{name}: {} pixels differ from its golden image; what was drawn is in {}",
            own.map_or("all".into(), |n| n.to_string()),
            kept.display()
        ));
    }
    let skia = here().join("../limu-tiny-skia/tests/golden").join(name);
    let skia = Pixmap::load_png(&skia)
        .ok()
        .and_then(|want| differ(&want, got, 32));
    if skia.is_none_or(|n| n > pixels / 100) {
        let kept = keep(name, got);
        return Err(format!(
            "{name}: {} pixels differ much from limu-tiny-skia's; what was drawn is in {}",
            skia.map_or("all".into(), |n| n.to_string()),
            kept.display()
        ));
    }
    Ok(())
}

/// Draws samples at a scale in a look and compares each with its golden images, named by the
/// sample and a suffix.
fn compare(samples: Vec<Sample>, scale: f64, look: Look, suffix: &str) {
    let fonts = fonts();
    let o = Options {
        look,
        ..options(&fonts, scale)
    };
    let failed: Vec<String> = samples
        .iter()
        .filter_map(|s| {
            let got = draw_pixmap(&s.scene, &o);
            golden(&format!("{}{suffix}.png", s.name), &got).err()
        })
        .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn primitives() {
    compare(limu_samples::primitives(), 2.0, Look::default(), "");
}

#[test]
fn diagrams() {
    compare(limu_samples::diagrams(), 1.0, Look::default(), "");
}

#[test]
fn dark() {
    let sample = limu_samples::primitives().into_iter().take(1).collect();
    compare(sample, 2.0, Look::new(Theme::KOHU_DARK), "-dark");
}

#[test]
fn renders_an_image() {
    let fonts = fonts();
    let sample = limu_samples::primitives().remove(0);
    let img = render(&sample.scene, &options(&fonts, 2.0)).unwrap();
    assert_eq!(
        (CGImage::width(Some(&img)), CGImage::height(Some(&img))),
        (480, 440)
    );
}

// Core Text sets Liberation Sans as wide as Arial, which Pele measures text in.
#[test]
fn measures_as_arial() {
    let fonts = fonts();
    let w = text::width(&fonts, "Start", 16.0, Face::REGULAR);
    assert!((w - 33.792).abs() < 0.01, "{w}");
    let w = text::width(&fonts, "Start", 16.0, Face::SUB);
    assert!((w - 33.792 / 1.2).abs() < 0.01, "{w}");
}

#[test]
fn dashes_as_svg() {
    assert_eq!(dash_of(&[3.0]), vec![3.0, 3.0]);
    assert_eq!(dash_of(&[4.0, 2.0]), vec![4.0, 2.0]);
    assert!(dash_of(&[0.0, 0.0]).is_empty());
    assert!(dash_of(&[4.0, -1.0]).is_empty());
    assert!(dash_of(&[]).is_empty());
}
