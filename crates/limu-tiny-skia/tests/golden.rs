//! The samples every backend draws, drawn by tiny-skia in the Liberation fonts, compared
//! with golden images. A golden image is written anew where UPDATE_GOLDEN is set. What a test
//! draws is written into target/golden for a look when it differs.

use limu_scene::style::{Look, Theme};
use limu_scene::text::{self, Face};
use limu_tiny_skia::{Fonts, Options, render, tiny_skia};

fn fonts() -> Fonts {
    let mut f = Fonts::new(limu_fixtures::font("LiberationSans-Regular.ttf")).unwrap();
    for (face, name) in [
        (Face::BOLD, "LiberationSans-Bold.ttf"),
        (Face::ITALIC, "LiberationSans-Italic.ttf"),
        (Face::BOLD | Face::ITALIC, "LiberationSans-BoldItalic.ttf"),
        (Face::MONO, "LiberationMono-Regular.ttf"),
    ] {
        f.set(face, limu_fixtures::font(name)).unwrap();
    }
    f
}

fn options(fonts: &Fonts, scale: f32) -> Options<'_> {
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
            tiny_skia::Pixmap::from_vec(data, tiny_skia::IntSize::from_wh(img.width, img.height)?)
        })),
        ..Options::default()
    }
}

/// Compares a pixmap with a golden image. Channels may differ by a little, as tiny-skia's
/// arithmetic does from one processor to another, and a few pixels by more.
fn golden(name: &str, got: &tiny_skia::Pixmap) -> Result<(), String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let path = dir.join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(&dir).unwrap();
        got.save_png(&path).unwrap();
        return Ok(());
    }
    let off = match tiny_skia::Pixmap::load_png(&path) {
        Ok(want) if (want.width(), want.height()) == (got.width(), got.height()) => want
            .data()
            .chunks(4)
            .zip(got.data().chunks(4))
            .filter(|(a, b)| a.iter().zip(*b).any(|(x, y)| x.abs_diff(*y) > 8))
            .count(),
        _ => usize::MAX,
    };
    if off > got.data().len() / 4 / 1000 {
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/golden");
        std::fs::create_dir_all(&out).unwrap();
        got.save_png(out.join(name)).unwrap();
        let off = if off == usize::MAX {
            "no golden image of its size".to_string()
        } else {
            format!("{off} pixels differ")
        };
        return Err(format!(
            "{name}: {off}; what was drawn is in target/golden/{name}"
        ));
    }
    Ok(())
}

/// Draws samples at a scale in a look and compares each with its golden image, named by the
/// sample and a suffix.
fn compare(samples: Vec<limu_samples::Sample>, scale: f32, look: Look, suffix: &str) {
    let fonts = fonts();
    let o = Options {
        look,
        ..options(&fonts, scale)
    };
    let failed: Vec<String> = samples
        .iter()
        .filter_map(|s| {
            let got = render(&s.scene, &o).unwrap_or_else(|| panic!("{}: no pixmap", s.name));
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

// Liberation Sans is as wide as Arial, which Pele measures text in.
#[test]
fn measures_as_arial() {
    let fonts = fonts();
    let w = text::width(&fonts, "Start", 16.0, Face::REGULAR);
    assert!((w - 33.792).abs() < 0.01, "{w}");
    let w = text::width(&fonts, "Start", 16.0, Face::SUB);
    assert!((w - 33.792 / 1.2).abs() < 0.01, "{w}");
}
