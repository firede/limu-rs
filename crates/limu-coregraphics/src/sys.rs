//! The calls into Core Foundation, Core Graphics and Core Text that Rust can't check, each
//! wrapped in a function that can't misuse it, with why it is sound.
#![allow(unsafe_code)]

use std::ptr::{self, NonNull};

use objc2_core_foundation::{
    CFArray, CFAttributedString, CFData, CFDictionary, CFRange, CFRetained, CFString, CFType,
    CGAffineTransform, CGFloat, CGPoint,
};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapInfo, CGColorSpace, CGContext, CGGlyph, CGImageAlphaInfo,
    CGImageByteOrderInfo, CGMutablePath, kCGColorSpaceSRGB,
};
use objc2_core_text::{CTFont, CTFontManagerCreateFontDescriptorFromData, CTLine, CTRun};

/// A font from the data of a TrueType or OpenType file, or none for data that is not one.
pub(crate) fn font_from_data(data: &[u8]) -> Option<CFRetained<CTFont>> {
    let data = CFData::from_bytes(data);
    // SAFETY: the data is a CFData, which the function only reads.
    let descriptor = unsafe { CTFontManagerCreateFontDescriptorFromData(&data) }?;
    // SAFETY: a null matrix stands for the identity.
    Some(unsafe { CTFont::with_font_descriptor(&descriptor, 12.0, ptr::null()) })
}

/// The font at a size.
pub(crate) fn sized(font: &CTFont, size: f64) -> CFRetained<CTFont> {
    // SAFETY: a null matrix keeps the font's, and no attributes keep its own.
    unsafe { font.copy_with_attributes(size, ptr::null(), None) }
}

/// The distance below the baseline of the top of the font's underline, and its thickness.
pub(crate) fn underline(font: &CTFont) -> (f64, f64) {
    // SAFETY: both only read the font.
    unsafe { (-font.underline_position(), font.underline_thickness()) }
}

/// A line of text set in a font, shaped by Core Text.
pub(crate) fn line(text: &str, font: &CTFont) -> CFRetained<CTLine> {
    let s = CFString::from_str(text);
    // SAFETY: the key is a constant Core Text defines.
    let key: &CFString = unsafe { objc2_core_text::kCTFontAttributeName };
    let attrs = CFDictionary::<CFString, CTFont>::from_slices(&[key], &[font]);
    // SAFETY: the string and the attributes are valid, and the attributes map the font's
    // key to a font, as Core Text expects.
    let a = unsafe { CFAttributedString::new(None, Some(&s), Some(attrs.as_opaque())) }
        .expect("an attributed string of a string");
    // SAFETY: the attributed string is valid.
    unsafe { CTLine::with_attributed_string(&a) }
}

/// How far a line advances.
pub(crate) fn advance(line: &CTLine) -> f64 {
    // SAFETY: null pointers ask for none of the other measures.
    unsafe { line.typographic_bounds(ptr::null_mut(), ptr::null_mut(), ptr::null_mut()) }
}

/// The glyphs of a line, each with the font it is set in and where it is drawn from the
/// line's start, y up.
pub(crate) fn glyphs(line: &CTLine) -> Vec<(CFRetained<CTFont>, CGGlyph, CGPoint)> {
    // SAFETY: a line's glyph runs are an array of runs.
    let runs: CFRetained<CFArray<CTRun>> = unsafe { CFRetained::cast_unchecked(line.glyph_runs()) };
    // SAFETY: the key is a constant Core Text defines.
    let key: &CFString = unsafe { objc2_core_text::kCTFontAttributeName };
    let mut out = Vec::new();
    for run in runs.iter() {
        // SAFETY: a run's attributes are a dictionary keyed by strings.
        let attrs = unsafe { run.attributes() };
        let attrs: &CFDictionary<CFString, CFType> = unsafe { attrs.cast_unchecked() };
        let Some(font) = attrs.get(key).and_then(|f| f.downcast::<CTFont>().ok()) else {
            continue;
        };
        // SAFETY: only reads the run.
        let n = usize::try_from(unsafe { run.glyph_count() }).unwrap_or(0);
        if n == 0 {
            continue;
        }
        let mut ids: Vec<CGGlyph> = vec![0; n];
        let mut at = vec![CGPoint::new(0.0, 0.0); n];
        let all = CFRange::new(0, 0);
        // SAFETY: a range of no length asks for every glyph of the run, and each buffer has
        // room for all of them.
        unsafe {
            run.glyphs(all, NonNull::new(ids.as_mut_ptr()).expect("a buffer"));
            run.positions(all, NonNull::new(at.as_mut_ptr()).expect("a buffer"));
        }
        out.extend(ids.into_iter().zip(at).map(|(g, p)| (font.clone(), g, p)));
    }
    out
}

/// Adds a glyph's outline to a path, placed by a transform.
pub(crate) fn add_glyph(
    path: &CGMutablePath,
    font: &CTFont,
    glyph: CGGlyph,
    t: &CGAffineTransform,
) {
    // SAFETY: the transform is a valid pointer for the call.
    if let Some(g) = unsafe { font.path_for_glyph(glyph, t) } {
        // SAFETY: a null matrix adds the glyph's path as it is.
        unsafe { CGMutablePath::add_path(Some(path), ptr::null(), Some(&g)) };
    }
}

pub(crate) fn move_to(path: &CGMutablePath, x: f64, y: f64) {
    // SAFETY: a null matrix adds the point as it is.
    unsafe { CGMutablePath::move_to_point(Some(path), ptr::null(), x, y) }
}

pub(crate) fn line_to(path: &CGMutablePath, x: f64, y: f64) {
    // SAFETY: as move_to.
    unsafe { CGMutablePath::add_line_to_point(Some(path), ptr::null(), x, y) }
}

pub(crate) fn quad_to(path: &CGMutablePath, cx: f64, cy: f64, x: f64, y: f64) {
    // SAFETY: as move_to.
    unsafe { CGMutablePath::add_quad_curve_to_point(Some(path), ptr::null(), cx, cy, x, y) }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn cube_to(
    path: &CGMutablePath,
    c1x: f64,
    c1y: f64,
    c2x: f64,
    c2y: f64,
    x: f64,
    y: f64,
) {
    // SAFETY: as move_to.
    unsafe { CGMutablePath::add_curve_to_point(Some(path), ptr::null(), c1x, c1y, c2x, c2y, x, y) }
}

/// Sets the dash lines are stroked in, none for solid.
pub(crate) fn set_dash(cx: &CGContext, dash: &[CGFloat]) {
    // SAFETY: the pointer and the count are the slice's.
    unsafe { CGContext::set_line_dash(Some(cx), 0.0, dash.as_ptr(), dash.len()) }
}

/// Begins a layer drawn apart and laid down at the context's alpha when it ends.
pub(crate) fn begin_layer(cx: &CGContext) {
    // SAFETY: no auxiliary information is allowed.
    unsafe { CGContext::begin_transparency_layer(Some(cx), None) }
}

/// The sRGB color space.
pub(crate) fn srgb() -> CFRetained<CGColorSpace> {
    // SAFETY: the name is a constant Core Graphics defines.
    CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB })).expect("sRGB")
}

/// The layout of the pixels of the bitmaps drawn and the images made here: red, green, blue
/// and alpha, in that order of bytes, premultiplied.
fn rgba() -> CGBitmapInfo {
    CGBitmapInfo(CGImageByteOrderInfo::Order32Big.0 | CGImageAlphaInfo::PremultipliedLast.0)
}

/// A bitmap context of a size in pixels, cleared, in sRGB, its pixels laid out as [`rgba`]
/// says.
pub(crate) fn bitmap(width: usize, height: usize) -> Option<CFRetained<CGContext>> {
    // SAFETY: a null data pointer has Core Graphics keep the pixels itself, and a row of
    // no bytes has it work out their number.
    unsafe {
        CGBitmapContextCreate(
            ptr::null_mut(),
            width,
            height,
            8,
            0,
            Some(&srgb()),
            rgba().0,
        )
    }
}

/// The width and height of a bitmap context made by [`bitmap`], and its pixels, row by row
/// from the top, four bytes each, premultiplied.
#[cfg(test)]
pub(crate) fn pixels(cx: &CGContext) -> (usize, usize, Vec<u8>) {
    use objc2_core_graphics::{
        CGBitmapContextGetBytesPerRow, CGBitmapContextGetData, CGBitmapContextGetHeight,
        CGBitmapContextGetWidth,
    };
    let (w, h) = (
        CGBitmapContextGetWidth(Some(cx)),
        CGBitmapContextGetHeight(Some(cx)),
    );
    let stride = CGBitmapContextGetBytesPerRow(Some(cx));
    let data = CGBitmapContextGetData(Some(cx)).cast::<u8>();
    if data.is_null() {
        return (0, 0, Vec::new());
    }
    // SAFETY: a bitmap context keeps stride bytes for each of its rows, from its data.
    let all = unsafe { std::slice::from_raw_parts(data, stride * h) };
    let rows = all
        .chunks(stride)
        .flat_map(|row| &row[..w * 4])
        .copied()
        .collect();
    (w, h, rows)
}

/// An image of a size in pixels from premultiplied pixels, as [`pixels`] gives them.
#[cfg(test)]
pub(crate) fn image(
    width: usize,
    height: usize,
    rgba_pixels: &[u8],
) -> Option<CFRetained<objc2_core_graphics::CGImage>> {
    use objc2_core_graphics::{CGColorRenderingIntent, CGDataProvider, CGImage};
    if rgba_pixels.len() != width * height * 4 {
        return None;
    }
    let data = CFData::from_bytes(rgba_pixels);
    let provider = CGDataProvider::with_cf_data(Some(&data))?;
    // SAFETY: the provider holds width * height pixels of four bytes, laid out as the
    // information says, and a null decode array keeps them as they are.
    unsafe {
        CGImage::new(
            width,
            height,
            8,
            32,
            width * 4,
            Some(&srgb()),
            rgba(),
            Some(&provider),
            ptr::null(),
            true,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
}
