//! Labels laid out in lines of measured spans, as Pele's src/text/label.ts lays them out.
//!
//! Pele also sets TeX between `$$` as math where the host typesets it. limu-rs does not, as
//! limu does not, and such a label is laid out as text.

use super::Setting;
use super::entities::decode_entities;
use crate::js::chars::{is_space_char, is_word, trim, trim_end, trim_start};
use crate::js::math::round;
use crate::scene::text::{Face, Label, Span};

/// The height of a line, in ems.
pub(crate) const LINE_HEIGHT: f64 = 1.5;
/// The height of a line of a label of more than one line, in ems. Lines of one wrapped label
/// sit closer, so that it reads as one block.
const WRAPPED_LINE_HEIGHT: f64 = 1.3;

/// The faces set smaller and off the baseline.
const SHIFT: Face = Face::from_bits(Face::SUB.bits() | Face::SUP.bits());

/// The faces HTML tags set, as Pele's TAG_STYLES has them.
fn tag_style(tag: &str) -> Face {
    match tag {
        "b" | "strong" => Face::BOLD,
        "i" | "em" | "cite" | "dfn" | "var" => Face::ITALIC,
        "code" | "kbd" | "samp" | "tt" => Face::MONO,
        "sub" => Face::SUB,
        "sup" => Face::SUP,
        "u" | "ins" => Face::UNDERLINE,
        _ => Face::REGULAR,
    }
}

/// The tags of HTML, as Pele's HTML_TAGS has them, in order. A label drops them and keeps
/// their text, and anything else shaped like a tag, such as the type in List<T>, is text.
const HTML_TAGS: &[&str] = &[
    "a",
    "abbr",
    "address",
    "article",
    "aside",
    "audio",
    "b",
    "bdi",
    "bdo",
    "big",
    "blockquote",
    "body",
    "br",
    "button",
    "caption",
    "center",
    "cite",
    "code",
    "col",
    "colgroup",
    "data",
    "dd",
    "del",
    "details",
    "dfn",
    "dialog",
    "div",
    "dl",
    "dt",
    "em",
    "embed",
    "fieldset",
    "figcaption",
    "figure",
    "font",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hr",
    "html",
    "i",
    "iframe",
    "img",
    "input",
    "ins",
    "kbd",
    "label",
    "legend",
    "li",
    "link",
    "main",
    "map",
    "mark",
    "meta",
    "meter",
    "nav",
    "noscript",
    "object",
    "ol",
    "optgroup",
    "option",
    "output",
    "p",
    "picture",
    "pre",
    "progress",
    "q",
    "rp",
    "rt",
    "ruby",
    "s",
    "samp",
    "script",
    "section",
    "select",
    "small",
    "source",
    "span",
    "strike",
    "strong",
    "style",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "template",
    "textarea",
    "tfoot",
    "th",
    "thead",
    "time",
    "title",
    "tr",
    "track",
    "tt",
    "u",
    "ul",
    "var",
    "video",
    "wbr",
];

/// Whether c is one of the Chinese and Japanese characters a line may break between, as
/// Pele's CJK has them. Hangul is left out, as Korean puts spaces between words.
fn is_cjk(c: char) -> bool {
    matches!(c, '\u{2e80}'..='\u{9fff}' | '\u{f900}'..='\u{faff}' | '\u{ff00}'..='\u{ff60}')
}

/// Characters that may not begin a line, and those that may not end one, as Pele's CLOSE and
/// OPEN have them.
const CLOSE: &str = "、。，．：；！？）］｝」』】〕〉》〙〗ー…‥・ゝゞぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮ,.:;!?)]}";
const OPEN: &str = "（［｛「『【〔〈《〘〖([{";

fn is_close(c: char) -> bool {
    CLOSE.contains(c)
}

fn is_open(c: char) -> bool {
    OPEN.contains(c)
}

/// A soft hyphen, which shows as a hyphen where a line breaks at it, and not otherwise.
const SHY: char = '\u{ad}';

/// Whether c is a break hint, a zero-width space or a soft hyphen, where a line may break.
fn is_hint(c: char) -> bool {
    c == '\u{200b}' || c == SHY
}

/// Matches `/[<\\\n*_]|fa[bklrs]?:fa-/`, Pele's RE_MARKUP.
fn has_markup(s: &str) -> bool {
    if s.contains(['<', '\\', '\n', '*', '_']) {
        return true;
    }
    s.match_indices("fa").any(|(i, _)| {
        let rest = &s[i + 2..];
        rest.starts_with(":fa-")
            || rest.starts_with(['b', 'k', 'l', 'r', 's']) && rest[1..].starts_with(":fa-")
    })
}

/// The end of the run of characters that keep, from byte i of s.
fn run_at(s: &str, i: usize, keep: impl Fn(char) -> bool) -> usize {
    i + s[i..]
        .chars()
        .take_while(|&c| keep(c))
        .map(char::len_utf8)
        .sum::<usize>()
}

/// Splits text where a line may break, as `text.match(RE_PIECE)` does in Pele's wrap:
///
/// ```text
/// \s*(?:[OPEN]*[CJK][CLOSE]*|[^\sCJKHINT]+[HINT]*[CLOSE]*|[HINT]+)\s*|\s+
/// ```
///
/// Each piece keeps the space around it.
fn pieces(s: &str) -> Vec<&str> {
    // What the group between the spaces matches from byte j, and where it ends.
    let word = |j: usize| -> Option<usize> {
        // [OPEN]*[CJK][CLOSE]*: the opening run is taken greedily, then given back a
        // character at a time until a CJK character follows it.
        let opens: Vec<usize> = s[j..]
            .char_indices()
            .take_while(|&(_, c)| is_open(c))
            .map(|(k, c)| j + k + c.len_utf8())
            .collect();
        for at in opens.iter().rev().copied().chain([j]) {
            if let Some(c) = s[at..].chars().next().filter(|&c| is_cjk(c)) {
                return Some(run_at(s, at + c.len_utf8(), is_close));
            }
        }
        // [^\sCJKHINT]+[HINT]*[CLOSE]*
        let k = run_at(s, j, |c| !is_space_char(c) && !is_cjk(c) && !is_hint(c));
        if k > j {
            return Some(run_at(s, run_at(s, k, is_hint), is_close));
        }
        // [HINT]+
        let k = run_at(s, j, is_hint);
        (k > j).then_some(k)
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let lead = run_at(s, i, is_space_char);
        // Where only white space is left, the expression would give the spaces back one at a
        // time, and might take U+3000, a CJK character too, as a word, but the spaces after it
        // would take the rest. The piece is the same.
        let end = match word(lead) {
            Some(end) => run_at(s, end, is_space_char),
            None => lead,
        };
        out.push(&s[i..end]);
        i = end;
    }
    out
}

/// Takes off the indent every line has, as Pele's dedent does. Indents are counted in
/// characters, as JavaScript counts them in code units, since white space is all in the
/// Basic Multilingual Plane.
fn dedent(text: &str) -> String {
    let indent = |line: &str| line.chars().take_while(|&c| is_space_char(c)).count();
    let least = text
        .split('\n')
        .filter(|l| !trim(l).is_empty())
        .map(indent)
        .min();
    let Some(least) = least.filter(|&n| n > 0) else {
        return text.to_string();
    };
    text.split('\n')
        .map(|line| {
            let skip = least.min(indent(line));
            let at = line.char_indices().nth(skip).map_or(line.len(), |(i, _)| i);
            &line[at..]
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Splits text at `/<br\s*\/?>|\\n|\n/gi`, Pele's RE_BREAK: a tag, a backslash and an n, of
/// either case, or a new line.
fn split_breaks(s: &str) -> Vec<&str> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let (mut last, mut i) = (0, 0);
    while i < b.len() {
        let end = match b[i] {
            b'\n' => Some(i + 1),
            b'\\' if matches!(b.get(i + 1), Some(b'n' | b'N')) => Some(i + 2),
            b'<' if b.len() >= i + 3 && b[i + 1..i + 3].eq_ignore_ascii_case(b"br") => {
                let mut j = run_at(s, i + 3, is_space_char);
                if b.get(j) == Some(&b'/') {
                    j += 1;
                }
                (b.get(j) == Some(&b'>')).then_some(j + 1)
            }
            _ => None,
        };
        match end {
            Some(end) => {
                out.push(&s[last..i]);
                (last, i) = (end, end);
            }
            None => i += 1,
        }
    }
    out.push(&s[last..]);
    out
}

/// Adds text to a line, as Pele's pushText does where there is no math.
fn push_text(line: &mut Vec<Span>, text: &str, style: Face) {
    if !text.is_empty() {
        push_plain(line, text, style);
    }
}

fn push_plain(line: &mut Vec<Span>, text: &str, style: Face) {
    let decoded = decode_entities(text);
    match line.last_mut() {
        Some(last) if last.icon.is_none() && last.face == style => last.text.push_str(&decoded),
        _ => line.push(Span {
            text: decoded,
            face: style,
            ..Span::default()
        }),
    }
}

/// Matches `/(fa[bklrs]?):fa-([\w-]+)/`, Pele's RE_ICON, at byte i of s, and gives where the
/// match ends.
fn icon_at(s: &str, i: usize) -> Option<usize> {
    let rest = s[i..].strip_prefix("fa")?;
    let rest = match rest.strip_prefix(['b', 'k', 'l', 'r', 's']) {
        Some(r) if r.starts_with(":fa-") => r,
        _ => rest,
    };
    let name = rest.strip_prefix(":fa-")?;
    let n = name
        .bytes()
        .take_while(|&c| is_word(i32::from(c)) || c == b'-')
        .count();
    (n > 0).then(|| s.len() - name.len() + n)
}

/// Adds text to a line with its icons, as Pele's pushIcons does.
fn push_icons(line: &mut Vec<Span>, text: &str, style: Face, set: &Setting) {
    if !text.contains(":fa-") {
        push_text(line, text, style);
        return;
    }
    let mut last = 0;
    let mut i = 0;
    while let Some(n) = text[i..].find("fa") {
        let at = i + n;
        let Some(end) = icon_at(text, at) else {
            i = at + 1;
            continue;
        };
        let icon = &text[at..end];
        push_text(line, &text[last..at], style);
        last = end;
        if (set.icons)(icon) {
            line.push(Span {
                face: style,
                icon: Some(icon.to_string()),
                ..Span::default()
            });
        } else if text.as_bytes().get(last) == Some(&b' ') {
            // An icon the host does not have leaves no gap, nor the space that set it off
            // from the text.
            last += 1;
        } else if last == text.len()
            && let Some(before) = line.last_mut().filter(|s| s.icon.is_none())
        {
            before.text = trim_end(&before.text).to_string();
        }
        i = end;
    }
    push_text(line, &text[last..], style);
}

/// Matches `/<\/?([a-zA-Z][\w-]*)(?:\s[^<>]*)?\/?>/y`, Pele's RE_TAG, at byte i of s, and
/// gives the tag's name and where the match ends.
fn tag_at(s: &str, i: usize) -> Option<(&str, usize)> {
    let b = s.as_bytes();
    let mut j = i + 1;
    if b.get(j) == Some(&b'/') {
        j += 1;
    }
    if !b.get(j)?.is_ascii_alphabetic() {
        return None;
    }
    let k = j
        + 1
        + b[j + 1..]
            .iter()
            .take_while(|&&c| is_word(i32::from(c)) || c == b'-')
            .count();
    let name = &s[j..k];
    if s[k..].chars().next().is_some_and(is_space_char) {
        let m = k + b[k..]
            .iter()
            .take_while(|&&c| c != b'<' && c != b'>')
            .count();
        return (b.get(m) == Some(&b'>')).then_some((name, m + 1));
    }
    let k = if b.get(k) == Some(&b'/') { k + 1 } else { k };
    (b.get(k) == Some(&b'>')).then_some((name, k + 1))
}

/// What the HTML tags of a label leave open from one line to the next, as Pele's Open holds
/// it: the faces they set, and the `<sub>` and `<sup>` tags open, innermost last, of which
/// only the innermost shifts text.
struct Open {
    style: Face,
    shifts: Vec<Face>,
}

/// Splits inline HTML and, for markdown, emphasis markers into spans, as Pele's inline does.
/// A line starts with what HTML tags left open on the lines before it, in open, and leaves
/// there what they leave open at its end.
fn inline(
    line: &mut Vec<Span>,
    source: &str,
    markdown: bool,
    base: Face,
    open: &mut Open,
    set: &Setting,
) {
    let mut style = open.style;
    let mut marked = Face::REGULAR;
    let mut text = String::new();
    let flush = |line: &mut Vec<Span>, text: &mut String, style: Face, open: &Open| {
        let shift = open.shifts.last().copied().unwrap_or(Face::REGULAR);
        push_icons(line, text, style | shift, set);
        text.clear();
    };
    let b = source.as_bytes();
    let n = b.len();
    let mut i = 0;
    while i < n {
        let ch = b[i];
        if ch == b'<' {
            if let Some((name, end)) = tag_at(source, i) {
                let tag = name.to_ascii_lowercase();
                let closing = b[i + 1] == b'/';
                let bit = tag_style(&tag);
                if bit.intersects(SHIFT) {
                    flush(line, &mut text, style, open);
                    let at = open.shifts.iter().rposition(|&s| s == bit);
                    if !closing {
                        open.shifts.push(bit);
                    } else if let Some(at) = at {
                        open.shifts.remove(at);
                    }
                } else if bit != Face::REGULAR {
                    flush(line, &mut text, style, open);
                    if closing {
                        open.style = open.style.without(bit) | (base & bit);
                        style = style.without(bit) | (base & bit);
                    } else {
                        open.style |= bit;
                        style |= bit;
                    }
                }
                // Other HTML tags are dropped and their text kept. Anything else shaped like
                // a tag, such as the type in List<T>, is text.
                if bit != Face::REGULAR || HTML_TAGS.contains(&tag.as_str()) {
                    i = end;
                    continue;
                }
            }
        } else if markdown && (ch == b'*' || ch == b'_') {
            let double = b.get(i + 1) == Some(&ch);
            let (bit, len) = if double {
                (Face::BOLD, 2)
            } else {
                (Face::ITALIC, 1)
            };
            let opening = !marked.intersects(bit);
            // JavaScript reads a code unit on either side. A character outside ASCII is
            // neither white space nor a word's to the tests here, whichever unit is read.
            let prev = if i > 0 {
                source[..i].chars().next_back()
            } else {
                None
            }
            .unwrap_or(' ');
            let next = if i + len < n {
                source[i + len..].chars().next()
            } else {
                None
            }
            .unwrap_or(' ');
            let marker = &source[i..i + len];
            let can_open = opening && !is_space_char(next) && source[i + len..].contains(marker);
            let can_close = !opening && !is_space_char(prev);
            let wordy = ch == b'_' && is_word(prev as i32) && is_word(next as i32);
            if (can_open || can_close) && !wordy {
                flush(line, &mut text, style, open);
                if can_open {
                    marked |= bit;
                    style |= bit;
                } else {
                    marked = marked.without(bit);
                    style = style.without(bit) | (base & bit);
                }
                i += len;
                continue;
            }
        }
        let c = source[i..].chars().next().unwrap_or_default();
        text.push(c);
        i += c.len_utf8();
    }
    flush(line, &mut text, style, open);
}

/// Measures each span of a line, and gives the line's width, as Pele's measureSpans does.
fn measure_spans(line: &mut [Span], set: &Setting, size: f64) -> f64 {
    let mut total = 0.0;
    for span in line {
        span.width = if span.icon.is_some() {
            size * 1.25
        } else {
            set.width(&span.text, size, span.face)
        };
        total += span.width;
    }
    total
}

/// Breaks a line into lines no wider than max where it can, as Pele's wrap does. A line
/// breaks after a piece, and the spaces it ends with are dropped.
fn wrap(
    line: Vec<Span>,
    set: &Setting,
    size: f64,
    max: f64,
    out: &mut Vec<Vec<Span>>,
    widths: &mut Vec<f64>,
) {
    let mut current: Vec<Span> = Vec::new();
    let mut width = 0.0;
    let commit = |current: &mut Vec<Span>,
                  width: &mut f64,
                  hyphen: bool,
                  out: &mut Vec<Vec<Span>>,
                  widths: &mut Vec<f64>| {
        if let Some(last) = current.last_mut().filter(|s| s.icon.is_none()) {
            let mut trimmed = trim_end(&last.text).to_string();
            if hyphen && last.text.ends_with(SHY) {
                trimmed.pop();
                trimmed.push('-');
            }
            if trimmed != last.text {
                let w = set.width(&trimmed, size, last.face);
                last.text = trimmed;
                *width += w - last.width;
                last.width = w;
            }
        }
        out.push(std::mem::take(current));
        widths.push(*width);
        *width = 0.0;
    };
    let count = line.len();
    for (at, span) in line.into_iter().enumerate() {
        if span.icon.is_some() {
            let w = size * 1.25;
            if width > 0.0 && width + w > max {
                commit(&mut current, &mut width, false, out, widths);
            }
            current.push(Span { width: w, ..span });
            width += w;
            continue;
        }
        let pieces = pieces(&span.text);
        // A piece ending in a soft hyphen fits if its hyphen does, or if the rest of the word
        // does, which hides the hyphen. The line only grows along a word, so no later hyphen
        // in it fits either, and the width left of each word makes this one check.
        let soft = span.text.contains(SHY);
        let n = if soft { pieces.len() } else { 0 };
        let mut own = vec![0.0; n];
        let mut rest = vec![0.0; n + 1];
        for q in (0..n).rev() {
            // A soft hyphen before a space is no break of its own: the space is.
            let shy = pieces[q].ends_with(SHY);
            let end = trim_end(pieces[q]);
            own[q] = set.width(
                if shy {
                    &end[..end.len() - SHY.len_utf8()]
                } else {
                    end
                },
                size,
                span.face,
            );
            // The width of a word past this span is not known.
            rest[q] = if !shy {
                own[q]
            } else if q == n - 1 {
                if at == count - 1 {
                    own[q]
                } else {
                    f64::INFINITY
                }
            } else {
                own[q] + rest[q + 1]
            };
        }
        for (p, &piece) in pieces.iter().enumerate() {
            let mut part = piece;
            let part_width = if soft {
                own[p]
            } else {
                set.width(trim_end(part), size, span.face)
            };
            let fits = if !soft || !part.ends_with(SHY) {
                width + part_width <= max
            } else {
                let hyphened = format!("{}-", &part[..part.len() - SHY.len_utf8()]);
                width + set.width(&hyphened, size, span.face) <= max || width + rest[p] <= max
            };
            if width > 0.0 && !fits {
                // A break at a space leaves a soft hyphen before it hidden.
                commit(
                    &mut current,
                    &mut width,
                    trim_start(part) == part,
                    out,
                    widths,
                );
                part = trim_start(part);
            }
            match current.last_mut() {
                Some(last) if last.icon.is_none() && last.face == span.face => {
                    last.text.push_str(part);
                    // Measuring again for each piece of no width would take quadratic time.
                    if part_width > 0.0 || trim_end(part) != part {
                        let before = last.width;
                        last.width = set.width(&last.text, size, last.face);
                        width += last.width - before;
                    }
                }
                _ => {
                    let w = set.width(part, size, span.face);
                    current.push(Span {
                        text: part.to_string(),
                        face: span.face,
                        icon: None,
                        width: w,
                    });
                    width += w;
                }
            }
        }
    }
    if !current.is_empty() || out.is_empty() {
        commit(&mut current, &mut width, false, out, widths);
    }
}

/// Lays out a label as Pele's layoutLabel does. `<br>` and new lines break it, `<b>`,
/// `<strong>`, `<i>`, `<em>`, `<code>`, `<sub>`, `<sup>`, `<u>` and their kin set its faces,
/// as do `**` and `_` in markdown, other HTML tags are dropped, `fa:fa-name` is an icon where
/// the host has it, and entities are decoded. Lines wider than max_width wrap between words,
/// between any two Chinese or Japanese characters, and at a zero-width space or a soft
/// hyphen. The lines of a label of more than one line sit closer.
pub(crate) fn layout_label(
    raw: &str,
    markdown: bool,
    set: &Setting,
    size: f64,
    max_width: f64,
    base: Face,
) -> Label {
    if raw.is_empty() {
        return Label::default();
    }
    let line_height = round(size * LINE_HEIGHT);

    if !has_markup(raw) && !raw.contains('&') && !raw.contains('ﬂ') {
        let text = trim(raw);
        let width = set.width(text, size, base);
        if width <= max_width
            || !(text.contains(' ') || text.chars().any(is_cjk) || text.chars().any(is_hint))
        {
            return Label {
                lines: vec![vec![Span {
                    text: text.to_string(),
                    face: base,
                    icon: None,
                    width,
                }]],
                widths: vec![width],
                width,
                height: line_height,
                size,
                line_height,
            };
        }
        let mut lines = Vec::new();
        let mut widths = Vec::new();
        let span = Span {
            text: text.to_string(),
            face: base,
            icon: None,
            width,
        };
        wrap(vec![span], set, size, max_width, &mut lines, &mut widths);
        let widest = widths
            .iter()
            .fold(0.0, |w: f64, &x| if x > w { x } else { w });
        let snug = if lines.len() > 1 {
            round(size * WRAPPED_LINE_HEIGHT)
        } else {
            line_height
        };
        let height = lines.len() as f64 * snug;
        return Label {
            lines,
            widths,
            width: widest,
            height,
            size,
            line_height: snug,
        };
    }
    layout_lines(raw, markdown, set, size, max_width, base, line_height)
}

/// Lays out a label with markup, as Pele's layoutLines does.
fn layout_lines(
    raw: &str,
    markdown: bool,
    set: &Setting,
    size: f64,
    max_width: f64,
    base: Face,
    line_height: f64,
) -> Label {
    let source = if markdown {
        trim(&dedent(&raw.replace("\r\n", "\n").replace('\r', "\n"))).to_string()
    } else {
        trim(raw).to_string()
    };
    let mut lines = Vec::new();
    let mut widths = Vec::new();
    // HTML tags can span the breaks, while markdown's emphasis stays within its line.
    let mut open = Open {
        style: base,
        shifts: Vec::new(),
    };
    for part in split_breaks(&source) {
        let mut line = Vec::new();
        inline(&mut line, trim(part), markdown, base, &mut open, set);
        let w = measure_spans(&mut line, set, size);
        if w > max_width {
            wrap(line, set, size, max_width, &mut lines, &mut widths);
        } else {
            lines.push(line);
            widths.push(w);
        }
    }
    let width = widths
        .iter()
        .fold(0.0, |w: f64, &x| if x > w { x } else { w });
    let snug = if lines.len() > 1 {
        round(size * WRAPPED_LINE_HEIGHT)
    } else {
        line_height
    };
    let height = lines.len() as f64 * snug;
    Label {
        lines,
        widths,
        width,
        height,
        size,
        line_height: snug,
    }
}
