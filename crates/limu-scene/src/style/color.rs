//! Colors, and reading them as CSS writes them.

use std::fmt;

/// A color in sRGB with straight alpha.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Color {
    #[allow(missing_docs)]
    pub r: u8,
    #[allow(missing_docs)]
    pub g: u8,
    #[allow(missing_docs)]
    pub b: u8,
    /// The alpha: 0 draws nothing, 255 is opaque.
    pub a: u8,
}

impl Color {
    /// The color with these channels.
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }

    /// An opaque color.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 255 }
    }

    /// The color #rgb, #rgba, #rrggbb or #rrggbbaa writes. It panics on anything else, so it
    /// is for colors written in code, and in a constant fails to compile. [`Color::parse`]
    /// reads colors from text.
    pub const fn hex(s: &str) -> Color {
        let b = s.as_bytes();
        let start = if !b.is_empty() && b[0] == b'#' { 1 } else { 0 };
        match parse_hex(b, start) {
            Some(c) => c,
            None => panic!("bad hex color"),
        }
    }

    /// Reads a CSS color: a hex color, rgb() or rgba(), hsl() or hsla(), a named color or
    /// transparent, with the white space of CSS about it, its names in either case of their
    /// ASCII letters. It gives none for anything else, such as currentColor or var().
    pub fn parse(s: &str) -> Option<Color> {
        let s = s
            .trim_matches([' ', '\t', '\n', '\r', '\x0c'])
            .to_ascii_lowercase();
        if let Some(h) = s.strip_prefix('#') {
            return parse_hex(h.as_bytes(), 0);
        }
        if let Some((name, args)) = function(&s) {
            return match name {
                "rgb" | "rgba" => parse_rgb(&args),
                "hsl" | "hsla" => parse_hsl(&args),
                _ => None,
            };
        }
        if s == "transparent" {
            return Some(Color::default());
        }
        NAMED
            .binary_search_by(|&(name, _)| name.cmp(&s))
            .ok()
            .map(|i| {
                let [r, g, b] = NAMED[i].1;
                Color::rgb(r, g, b)
            })
    }
}

/// Writes the color as #rrggbb, or #rrggbbaa when it is not opaque.
impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)?;
        if self.a != 255 {
            write!(f, "{:02x}", self.a)?;
        }
        Ok(())
    }
}

const fn parse_hex(b: &[u8], start: usize) -> Option<Color> {
    let n = b.len() - start;
    let mut v = [0u8; 8];
    let mut i = 0;
    while i < n {
        v[i % 8] = match b[start + i] {
            c @ b'0'..=b'9' => c - b'0',
            c @ b'a'..=b'f' => c - b'a' + 10,
            c @ b'A'..=b'F' => c - b'A' + 10,
            _ => return None,
        };
        i += 1;
    }
    match n {
        3 | 4 => Some(Color::new(
            v[0] * 17,
            v[1] * 17,
            v[2] * 17,
            if n == 4 { v[3] * 17 } else { 255 },
        )),
        6 | 8 => Some(Color::new(
            (v[0] << 4) | v[1],
            (v[2] << 4) | v[3],
            (v[4] << 4) | v[5],
            if n == 8 { (v[6] << 4) | v[7] } else { 255 },
        )),
        _ => None,
    }
}

/// Splits name(args) into the name and its arguments, by commas, a slash or the white space
/// of CSS. CSS has nothing between a function's name and its parenthesis.
fn function(s: &str) -> Option<(&str, Vec<&str>)> {
    let open = s.find('(')?;
    if open == 0 || !s.ends_with(')') {
        return None;
    }
    let args = s[open + 1..s.len() - 1]
        .split([',', '/', ' ', '\t', '\n', '\r', '\x0c'])
        .filter(|a| !a.is_empty())
        .collect();
    Some((&s[..open], args))
}

/// Reads a number, or a percentage of full.
fn number(s: &str, full: f64) -> Option<f64> {
    let (digits, pct) = match s.strip_suffix('%') {
        Some(d) => (d, true),
        None => (s, false),
    };
    let f = parse_float(digits)?;
    Some(if pct { f * full / 100.0 } else { f })
}

/// Reads a number as Go's ParseFloat reads one, which limu reads CSS's numbers with: Rust's
/// grammar less its names of infinity and NaN, and no number that is not finite.
fn parse_float(s: &str) -> Option<f64> {
    let f = s.parse::<f64>().ok()?;
    f.is_finite().then_some(f)
}

fn channel(f: f64) -> u8 {
    f.clamp(0.0, 255.0).round() as u8
}

fn alpha(args: &[&str]) -> Option<u8> {
    match args.get(3) {
        None => Some(255),
        Some(a) => Some(channel(number(a, 1.0)? * 255.0)),
    }
}

fn parse_rgb(args: &[&str]) -> Option<Color> {
    if args.len() != 3 && args.len() != 4 {
        return None;
    }
    let mut c = [0u8; 3];
    for (i, v) in c.iter_mut().enumerate() {
        *v = channel(number(args[i], 255.0)?);
    }
    Some(Color::new(c[0], c[1], c[2], alpha(args)?))
}

fn parse_hsl(args: &[&str]) -> Option<Color> {
    if args.len() != 3 && args.len() != 4 {
        return None;
    }
    let h = number(args[0].strip_suffix("deg").unwrap_or(args[0]), 360.0)?;
    let mut s = number(args[1], 1.0)?;
    let mut l = number(args[2], 1.0)?;
    if !args[1].ends_with('%') {
        s /= 100.0;
    }
    if !args[2].ends_with('%') {
        l /= 100.0;
    }
    let (s, l) = (s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
    let h = ((h % 360.0) + 360.0) % 360.0 / 360.0;
    let f = |n: f64| {
        let k = (n + h * 12.0) % 12.0;
        l - s * l.min(1.0 - l) * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)
    };
    Some(Color::new(
        channel(f(0.0) * 255.0),
        channel(f(8.0) * 255.0),
        channel(f(4.0) * 255.0),
        alpha(args)?,
    ))
}

/// The named colors of CSS, in the order of their names.
const NAMED: &[(&str, [u8; 3])] = &[
    ("aliceblue", [240, 248, 255]),
    ("antiquewhite", [250, 235, 215]),
    ("aqua", [0, 255, 255]),
    ("aquamarine", [127, 255, 212]),
    ("azure", [240, 255, 255]),
    ("beige", [245, 245, 220]),
    ("bisque", [255, 228, 196]),
    ("black", [0, 0, 0]),
    ("blanchedalmond", [255, 235, 205]),
    ("blue", [0, 0, 255]),
    ("blueviolet", [138, 43, 226]),
    ("brown", [165, 42, 42]),
    ("burlywood", [222, 184, 135]),
    ("cadetblue", [95, 158, 160]),
    ("chartreuse", [127, 255, 0]),
    ("chocolate", [210, 105, 30]),
    ("coral", [255, 127, 80]),
    ("cornflowerblue", [100, 149, 237]),
    ("cornsilk", [255, 248, 220]),
    ("crimson", [220, 20, 60]),
    ("cyan", [0, 255, 255]),
    ("darkblue", [0, 0, 139]),
    ("darkcyan", [0, 139, 139]),
    ("darkgoldenrod", [184, 134, 11]),
    ("darkgray", [169, 169, 169]),
    ("darkgreen", [0, 100, 0]),
    ("darkgrey", [169, 169, 169]),
    ("darkkhaki", [189, 183, 107]),
    ("darkmagenta", [139, 0, 139]),
    ("darkolivegreen", [85, 107, 47]),
    ("darkorange", [255, 140, 0]),
    ("darkorchid", [153, 50, 204]),
    ("darkred", [139, 0, 0]),
    ("darksalmon", [233, 150, 122]),
    ("darkseagreen", [143, 188, 143]),
    ("darkslateblue", [72, 61, 139]),
    ("darkslategray", [47, 79, 79]),
    ("darkslategrey", [47, 79, 79]),
    ("darkturquoise", [0, 206, 209]),
    ("darkviolet", [148, 0, 211]),
    ("deeppink", [255, 20, 147]),
    ("deepskyblue", [0, 191, 255]),
    ("dimgray", [105, 105, 105]),
    ("dimgrey", [105, 105, 105]),
    ("dodgerblue", [30, 144, 255]),
    ("firebrick", [178, 34, 34]),
    ("floralwhite", [255, 250, 240]),
    ("forestgreen", [34, 139, 34]),
    ("fuchsia", [255, 0, 255]),
    ("gainsboro", [220, 220, 220]),
    ("ghostwhite", [248, 248, 255]),
    ("gold", [255, 215, 0]),
    ("goldenrod", [218, 165, 32]),
    ("gray", [128, 128, 128]),
    ("green", [0, 128, 0]),
    ("greenyellow", [173, 255, 47]),
    ("grey", [128, 128, 128]),
    ("honeydew", [240, 255, 240]),
    ("hotpink", [255, 105, 180]),
    ("indianred", [205, 92, 92]),
    ("indigo", [75, 0, 130]),
    ("ivory", [255, 255, 240]),
    ("khaki", [240, 230, 140]),
    ("lavender", [230, 230, 250]),
    ("lavenderblush", [255, 240, 245]),
    ("lawngreen", [124, 252, 0]),
    ("lemonchiffon", [255, 250, 205]),
    ("lightblue", [173, 216, 230]),
    ("lightcoral", [240, 128, 128]),
    ("lightcyan", [224, 255, 255]),
    ("lightgoldenrodyellow", [250, 250, 210]),
    ("lightgray", [211, 211, 211]),
    ("lightgreen", [144, 238, 144]),
    ("lightgrey", [211, 211, 211]),
    ("lightpink", [255, 182, 193]),
    ("lightsalmon", [255, 160, 122]),
    ("lightseagreen", [32, 178, 170]),
    ("lightskyblue", [135, 206, 250]),
    ("lightslategray", [119, 136, 153]),
    ("lightslategrey", [119, 136, 153]),
    ("lightsteelblue", [176, 196, 222]),
    ("lightyellow", [255, 255, 224]),
    ("lime", [0, 255, 0]),
    ("limegreen", [50, 205, 50]),
    ("linen", [250, 240, 230]),
    ("magenta", [255, 0, 255]),
    ("maroon", [128, 0, 0]),
    ("mediumaquamarine", [102, 205, 170]),
    ("mediumblue", [0, 0, 205]),
    ("mediumorchid", [186, 85, 211]),
    ("mediumpurple", [147, 112, 219]),
    ("mediumseagreen", [60, 179, 113]),
    ("mediumslateblue", [123, 104, 238]),
    ("mediumspringgreen", [0, 250, 154]),
    ("mediumturquoise", [72, 209, 204]),
    ("mediumvioletred", [199, 21, 133]),
    ("midnightblue", [25, 25, 112]),
    ("mintcream", [245, 255, 250]),
    ("mistyrose", [255, 228, 225]),
    ("moccasin", [255, 228, 181]),
    ("navajowhite", [255, 222, 173]),
    ("navy", [0, 0, 128]),
    ("oldlace", [253, 245, 230]),
    ("olive", [128, 128, 0]),
    ("olivedrab", [107, 142, 35]),
    ("orange", [255, 165, 0]),
    ("orangered", [255, 69, 0]),
    ("orchid", [218, 112, 214]),
    ("palegoldenrod", [238, 232, 170]),
    ("palegreen", [152, 251, 152]),
    ("paleturquoise", [175, 238, 238]),
    ("palevioletred", [219, 112, 147]),
    ("papayawhip", [255, 239, 213]),
    ("peachpuff", [255, 218, 185]),
    ("peru", [205, 133, 63]),
    ("pink", [255, 192, 203]),
    ("plum", [221, 160, 221]),
    ("powderblue", [176, 224, 230]),
    ("purple", [128, 0, 128]),
    ("rebeccapurple", [102, 51, 153]),
    ("red", [255, 0, 0]),
    ("rosybrown", [188, 143, 143]),
    ("royalblue", [65, 105, 225]),
    ("saddlebrown", [139, 69, 19]),
    ("salmon", [250, 128, 114]),
    ("sandybrown", [244, 164, 96]),
    ("seagreen", [46, 139, 87]),
    ("seashell", [255, 245, 238]),
    ("sienna", [160, 82, 45]),
    ("silver", [192, 192, 192]),
    ("skyblue", [135, 206, 235]),
    ("slateblue", [106, 90, 205]),
    ("slategray", [112, 128, 144]),
    ("slategrey", [112, 128, 144]),
    ("snow", [255, 250, 250]),
    ("springgreen", [0, 255, 127]),
    ("steelblue", [70, 130, 180]),
    ("tan", [210, 180, 140]),
    ("teal", [0, 128, 128]),
    ("thistle", [216, 191, 216]),
    ("tomato", [255, 99, 71]),
    ("turquoise", [64, 224, 208]),
    ("violet", [238, 130, 238]),
    ("wheat", [245, 222, 179]),
    ("white", [255, 255, 255]),
    ("whitesmoke", [245, 245, 245]),
    ("yellow", [255, 255, 0]),
    ("yellowgreen", [154, 205, 50]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse() {
        for (s, want) in [
            ("#f9f", Some(Color::new(255, 153, 255, 255))),
            ("#F9F8", Some(Color::new(255, 153, 255, 136))),
            ("#123456", Some(Color::new(0x12, 0x34, 0x56, 255))),
            ("#12345678", Some(Color::new(0x12, 0x34, 0x56, 0x78))),
            ("#12345", None),
            ("#ggg", None),
            ("red", Some(Color::new(255, 0, 0, 255))),
            (" RebeccaPurple ", Some(Color::new(102, 51, 153, 255))),
            ("transparent", Some(Color::default())),
            ("rgb(255, 0, 128)", Some(Color::new(255, 0, 128, 255))),
            ("rgba(255,0,128,0.5)", Some(Color::new(255, 0, 128, 128))),
            ("rgb(100% 0% 50% / 50%)", Some(Color::new(255, 0, 128, 128))),
            ("hsl(120, 100%, 50%)", Some(Color::new(0, 255, 0, 255))),
            ("hsl(0deg 100% 50%)", Some(Color::new(255, 0, 0, 255))),
            (
                "hsla(240, 100%, 50%, 0.25)",
                Some(Color::new(0, 0, 255, 64)),
            ),
            ("var(--x)", None),
            ("currentColor", None),
            ("rgb(1,2)", None),
        ] {
            assert_eq!(Color::parse(s), want, "{s:?}");
        }
    }

    #[test]
    fn display() {
        assert_eq!(Color::hex("#0a0b0c").to_string(), "#0a0b0c");
        assert_eq!(Color::new(1, 2, 3, 4).to_string(), "#01020304");
    }

    #[test]
    fn names_are_sorted() {
        assert!(NAMED.is_sorted_by(|a, b| a.0 < b.0));
    }
}
