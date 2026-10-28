//! Whether the first word of a box's title is a color, as Pele's colors.ts tells. Mermaid
//! asks the browser. Pele has no browser, so it knows the color keywords and the rgb() and
//! hsl() notations. Other functions, such as calc() and var(), are not recognized.

use crate::js::chars::{is_digit, is_space_char, len16, to_lower, trim};

const KEYWORDS: &str = "aliceblue antiquewhite aqua aquamarine azure beige bisque black blanchedalmond blue blueviolet brown burlywood \
    cadetblue chartreuse chocolate coral cornflowerblue cornsilk crimson cyan darkblue darkcyan darkgoldenrod darkgray \
    darkgreen darkgrey darkkhaki darkmagenta darkolivegreen darkorange darkorchid darkred darksalmon darkseagreen \
    darkslateblue darkslategray darkslategrey darkturquoise darkviolet deeppink deepskyblue dimgray dimgrey dodgerblue \
    firebrick floralwhite forestgreen fuchsia gainsboro ghostwhite gold goldenrod gray green greenyellow grey honeydew \
    hotpink indianred indigo ivory khaki lavender lavenderblush lawngreen lemonchiffon lightblue lightcoral lightcyan \
    lightgoldenrodyellow lightgray lightgreen lightgrey lightpink lightsalmon lightseagreen lightskyblue lightslategray \
    lightslategrey lightsteelblue lightyellow lime limegreen linen magenta maroon mediumaquamarine mediumblue \
    mediumorchid mediumpurple mediumseagreen mediumslateblue mediumspringgreen mediumturquoise mediumvioletred \
    midnightblue mintcream mistyrose moccasin navajowhite navy oldlace olive olivedrab orange orangered orchid \
    palegoldenrod palegreen paleturquoise palevioletred papayawhip peachpuff peru pink plum powderblue purple \
    rebeccapurple red rosybrown royalblue saddlebrown salmon sandybrown seagreen seashell sienna silver skyblue \
    slateblue slategray slategrey snow springgreen steelblue tan teal thistle tomato turquoise violet wheat white \
    whitesmoke yellow yellowgreen transparent currentcolor inherit initial unset revert \
    accentcolor accentcolortext activetext buttonborder buttonface buttontext canvas canvastext field fieldtext \
    graytext highlight highlighttext linktext mark marktext selecteditem selecteditemtext visitedtext \
    activeborder activecaption appworkspace background buttonhighlight buttonshadow captiontext inactiveborder \
    inactivecaption inactivecaptiontext infobackground infotext menu menutext scrollbar threeddarkshadow threedface \
    threedhighlight threedlightshadow threedshadow window windowframe windowtext";

/// The kinds of term of a color function, Pele's K.
#[derive(Clone, Copy, PartialEq, Eq)]
enum K {
    Bad,
    Number,
    Percent,
    Angle,
    None,
}

/// The kind of a term, as Pele's kind reads it with
/// /^[+-]?(?:\d+(?:\.\d+)?|\.\d+)(?:e[+-]?\d+)?(%|deg|grad|rad|turn)?$/i. No part of the
/// pattern can give back what it took and leave the rest to match, so it is read straight
/// through.
fn kind(term: &str) -> K {
    if to_lower(term) == "none" {
        return K::None;
    }
    let b = term.as_bytes();
    let digit = |i: usize| b.get(i).is_some_and(|&c| is_digit(i32::from(c)));
    let digits = |mut i: usize| {
        while digit(i) {
            i += 1;
        }
        i
    };
    let mut i = 0;
    if matches!(b.first(), Some(b'+' | b'-')) {
        i += 1;
    }
    if digit(i) {
        i = digits(i);
        if b.get(i) == Some(&b'.') && digit(i + 1) {
            i = digits(i + 1);
        }
    } else if b.get(i) == Some(&b'.') && digit(i + 1) {
        i = digits(i + 1);
    } else {
        return K::Bad;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(b.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        if digit(j) {
            i = digits(j);
        }
    }
    let unit = &term[i..];
    if unit.is_empty() {
        return K::Number;
    }
    if unit == "%" {
        return K::Percent;
    }
    if ["deg", "grad", "rad", "turn"]
        .iter()
        .any(|u| unit.eq_ignore_ascii_case(u))
    {
        return K::Angle;
    }
    K::Bad
}

fn is_alpha(term: &str, modern: bool) -> bool {
    let k = kind(term);
    k == K::Number || k == K::Percent || (modern && k == K::None)
}

fn is_function(color: &str) -> bool {
    let hsl = color.starts_with("hsl");
    let Some(open) = color.find('(') else {
        return false;
    };
    let name = &color[..open];
    if !matches!(name, "rgb" | "rgba" | "hsl" | "hsla") {
        return false;
    }
    if len16(color) > 120 || color.find(')') != Some(color.len() - 1) {
        return false;
    }
    let inner = trim(&color[open + 1..color.len() - 1]);
    let legacy = inner.contains(',');
    let mut parts: Vec<&str>;
    let mut alpha = None;
    if legacy {
        parts = inner.split(',').map(trim).collect();
        if parts.len() == 4 {
            alpha = parts.pop();
        }
    } else {
        let slash: Vec<&str> = inner.split('/').collect();
        if slash.len() > 2 {
            return false;
        }
        // What /\s+/ splits a trimmed text into: its runs of other characters, or one empty
        // part for empty text, which is not three parts either way.
        parts = trim(slash[0])
            .split(is_space_char)
            .filter(|p| !p.is_empty())
            .collect();
        if slash.len() == 2 {
            alpha = Some(trim(slash[1]));
        }
    }
    if parts.len() != 3 || alpha.is_some_and(|a| !is_alpha(a, !legacy)) {
        return false;
    }
    let kinds: Vec<K> = parts.iter().map(|p| kind(p)).collect();
    if legacy {
        if hsl {
            return (kinds[0] == K::Number || kinds[0] == K::Angle)
                && kinds[1] == K::Percent
                && kinds[2] == K::Percent;
        }
        return (kinds[0] == K::Number || kinds[0] == K::Percent)
            && kinds[1] == kinds[0]
            && kinds[2] == kinds[0];
    }
    kinds.iter().enumerate().all(|(i, &k)| {
        k == K::Number
            || k == K::None
            || if hsl && i == 0 {
                k == K::Angle
            } else {
                k == K::Percent
            }
    })
}

/// Whether a box's first word is a CSS color, as Pele's isCssColor tells.
pub(crate) fn is_css_color(color: &str) -> bool {
    if color.contains('(') {
        return is_function(color);
    }
    let lower = to_lower(color);
    KEYWORDS.split(' ').any(|k| k == lower)
}
