//! Shows the samples every backend draws in a GPUI window, for looking at what the golden
//! records can't show: where glyphs fall, how turned text and halos look, how strokes join.
//!
//! The right and left arrows step through the samples, d switches between the light and the
//! dark theme, and q quits. Text is set in the system's Arial and Courier New, which the
//! samples were measured as.
//!
//! cargo run --example viewer [sample]
//!
//! It starts at the sample named, or at the first.

use std::sync::Arc;

use gpui::{
    App, Bounds, Context, FocusHandle, KeyDownEvent, Pixels, RenderImage, Rgba, Window,
    WindowBounds, WindowOptions, canvas, div, prelude::*, px, size,
};
use limu_gpui::{Options, paint};
use limu_samples::Sample;
use limu_scene::style::{Look, Theme};

struct Viewer {
    samples: Vec<Arc<Sample>>,
    at: usize,
    dark: bool,
    image: Option<Arc<RenderImage>>,
    focus: FocusHandle,
}

impl Viewer {
    fn look(&self) -> Look {
        if self.dark {
            Look::new(Theme::KOHU_DARK)
        } else {
            Look::default()
        }
    }

    fn key(&mut self, e: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let n = self.samples.len();
        match e.keystroke.key.as_str() {
            "right" => self.at = (self.at + 1) % n,
            "left" => self.at = (self.at + n - 1) % n,
            "d" => self.dark = !self.dark,
            "q" => cx.quit(),
            _ => return,
        }
        cx.notify();
    }
}

impl Render for Viewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sample = self.samples[self.at].clone();
        let look = self.look();
        window.set_window_title(&format!(
            "{} ({} of {}) - limu-gpui",
            sample.name,
            self.at + 1,
            self.samples.len()
        ));
        let bg = rgba(look.theme.bg);
        let image = self.image.clone();
        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .size_full()
            .bg(bg)
            .p(px(16.0))
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds: Bounds<Pixels>, _, window, cx| {
                        let scene = &sample.scene;
                        let fit = (f32::from(bounds.size.width) / scene.width as f32)
                            .min(f32::from(bounds.size.height) / scene.height as f32)
                            .min(2.0);
                        let o = Options {
                            look,
                            icons: Some(Box::new(limu_samples::icon)),
                            images: Some(Box::new(move |src| {
                                (src == limu_samples::IMAGE)
                                    .then(|| image.clone())
                                    .flatten()
                            })),
                            ..Options::default()
                        };
                        paint(scene, &o, bounds.origin, fit, window, cx);
                    },
                )
                .size_full(),
            )
    }
}

fn rgba(c: limu_scene::style::Color) -> Rgba {
    let k = |v: u8| f32::from(v) / 255.0;
    Rgba {
        r: k(c.r),
        g: k(c.g),
        b: k(c.b),
        a: k(c.a),
    }
}

/// The sample image, as GPUI holds images: its colors in the order blue, green, red, alpha.
fn sample_image() -> Option<Arc<RenderImage>> {
    let img = limu_samples::image(limu_samples::IMAGE)?;
    let mut data = img.rgba;
    for px in data.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    let buf = image::RgbaImage::from_raw(img.width, img.height, data)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buf)])))
}

fn main() {
    let samples: Vec<Arc<Sample>> = limu_samples::all().into_iter().map(Arc::new).collect();
    let at = std::env::args()
        .nth(1)
        .and_then(|name| samples.iter().position(|s| s.name == name))
        .unwrap_or(0);
    gpui_platform::application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1100.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..WindowOptions::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    let focus = cx.focus_handle();
                    window.focus(&focus, cx);
                    Viewer {
                        samples,
                        at,
                        dark: false,
                        image: sample_image(),
                        focus,
                    }
                })
            },
        )
        .expect("a window");
        cx.activate(true);
    });
}
