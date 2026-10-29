//! The samples every backend draws, recorded as the calls the canvas would make of a window
//! and compared with golden records. A record is written anew where UPDATE_GOLDEN is set, and
//! into target/golden when it differs. The viewer example shows the samples in a window, for
//! what a record can't show.

use std::path::{Path, PathBuf};

use gpui::RenderImage;
use image::{Frame, RgbaImage};
use limu_samples::Sample;
use limu_scene::style::Theme;

use super::*;

/// The sample image, as GPUI holds images: its colors in the order blue, green, red, alpha.
fn sample_image(src: &str) -> Option<Arc<RenderImage>> {
    let img = limu_samples::image(src)?;
    let mut data = img.rgba;
    for px in data.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    let buf = RgbaImage::from_raw(img.width, img.height, data)?;
    Some(Arc::new(RenderImage::new(vec![Frame::new(buf)])))
}

fn options() -> Options<'static> {
    Options {
        icons: Some(Box::new(limu_samples::icon)),
        images: Some(Box::new(sample_image)),
        ..Options::default()
    }
}

/// What the canvas paints of a scene at a scale, a line for each call.
fn record(scene: &Scene, o: &Options<'_>, scale: f64) -> String {
    let mut out = Vec::new();
    let mut c = GpuiCanvas {
        out: Out::Record(&mut out),
        o,
        transforms: vec![Affine::place(0.0, 0.0, scale)],
    };
    limu_scene::draw(scene, &o.look, &mut c);
    let mut s = out.join("\n");
    s.push('\n');
    s
}

fn here() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Writes what was recorded into target/golden, and says where.
fn keep(name: &str, got: &str) -> PathBuf {
    let out = here().join("../../target/golden/gpui");
    std::fs::create_dir_all(&out).unwrap();
    let path = out.join(name);
    std::fs::write(&path, got).unwrap();
    path
}

fn golden(name: &str, got: &str) -> Result<(), String> {
    let path = here().join("tests/golden").join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, got).unwrap();
    }
    match std::fs::read_to_string(&path) {
        Ok(want) if want == got => Ok(()),
        Ok(want) => {
            let line = want
                .lines()
                .zip(got.lines())
                .position(|(a, b)| a != b)
                .unwrap_or_else(|| want.lines().count().min(got.lines().count()));
            Err(format!(
                "{name}: differs from its golden record at line {}; what was recorded is in {}",
                line + 1,
                keep(name, got).display()
            ))
        }
        Err(_) => Err(format!(
            "{name}: has no golden record; what was recorded is in {}",
            keep(name, got).display()
        )),
    }
}

/// Records samples at a scale in a look and compares each with its golden record, named by
/// the sample and a suffix.
fn compare(samples: Vec<Sample>, scale: f64, look: Look, suffix: &str) {
    let o = Options { look, ..options() };
    let failed: Vec<String> = samples
        .iter()
        .filter_map(|s| {
            let got = record(&s.scene, &o, scale);
            golden(&format!("{}{suffix}.txt", s.name), &got).err()
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

// A turn is clockwise on the screen, as SVG's rotate and GPUI's are.
#[test]
fn turns_clockwise() {
    let t = Affine::place(10.0, 20.0, 2.0).then(Affine::rotate(90.0, Point::new(5.0, 5.0)));
    let (x, y) = t.apply(Point::new(6.0, 5.0));
    assert!(
        (x - 20.0).abs() < 1e-9 && (y - 32.0).abs() < 1e-9,
        "{x} {y}"
    );
    assert!((t.angle() - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    assert!((t.scale() - 2.0).abs() < 1e-9);
}

// A corner whose miter is 4.35 times the line's width is bevelled, as SVG bevels a miter
// longer than 4 times it, and one of 2.24 times is mitred.
#[test]
fn limits_miters_as_svg() {
    use lyon::tessellation::{BuffersBuilder, StrokeTessellator, StrokeVertex, VertexBuffers};
    let extent = |join| {
        let mut b = lyon::path::Path::builder();
        b.begin(lyon::math::point(0.0, 150.0));
        b.line_to(lyon::math::point(25.0, 100.0));
        b.line_to(lyon::math::point(50.0, 150.0));
        b.line_to(lyon::math::point(50.0, 100.0));
        b.end(false);
        let mut out: VertexBuffers<lyon::math::Point, u16> = VertexBuffers::new();
        StrokeTessellator::new()
            .tessellate_path(
                &b.build(),
                &stroke_options(8.0, Cap::Butt, join),
                &mut BuffersBuilder::new(&mut out, |v: StrokeVertex| v.position()),
            )
            .unwrap();
        let ys = out.vertices.iter().map(|p| p.y);
        (
            ys.clone().fold(f32::MAX, f32::min),
            ys.fold(f32::MIN, f32::max),
        )
    };
    let (top, bottom) = extent(Join::Miter);
    // The top corner is mitred 4 times 2.24 above it. The bottom one would be 4 times 4.35 below.
    assert!((top - (100.0 - 4.0 * 5f32.sqrt())).abs() < 0.01, "{top}");
    assert!(bottom < 150.0 + 4.0, "{bottom}");
}

#[test]
fn dashes_as_svg() {
    assert_eq!(dash_of(&[3.0]), &[3.0]);
    assert_eq!(dash_of(&[4.0, 2.0]), &[4.0, 2.0]);
    assert!(dash_of(&[0.0, 0.0]).is_empty());
    assert!(dash_of(&[4.0, -1.0]).is_empty());
    assert!(dash_of(&[]).is_empty());
}

#[test]
fn writes_text_as_svg() {
    let spans = [
        limu_scene::text::Span {
            text: "a<b".into(),
            face: Face::BOLD,
            icon: None,
            width: 20.0,
        },
        limu_scene::text::Span {
            text: "2".into(),
            face: Face::SUP | Face::MONO,
            icon: None,
            width: 5.0,
        },
    ];
    let ink = Ink::NONE;
    let run = Run {
        spans: &spans,
        at: Point::new(0.0, 0.0),
        anchor: Align::Middle,
        size: 10.0,
        paint: TextPaint::Halo { ink, width: 4.0 },
    };
    let t = svg::text_svg(&Fonts::default(), &run, 2.0, Some(4.0));
    assert_eq!(t.half, 98.0);
    assert_eq!(
        t.svg,
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="196" height="196" viewBox="0 0 196 196">"#,
            r#"<text xml:space="preserve" x="98" y="98" text-anchor="middle" font-family="Arial" font-size="20" "#,
            r##"fill="none" stroke="#000" stroke-width="8" stroke-linejoin="round">"##,
            r#"<tspan font-weight="bold">a&lt;b</tspan>"#,
            r#"<tspan font-family="Courier New" font-size="16.67" baseline-shift="6.67">2</tspan>"#,
            "</text></svg>"
        )
    );
}
