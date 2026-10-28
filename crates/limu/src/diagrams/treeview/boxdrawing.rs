//! Mermaid's box-drawing preprocessor, as Pele's boxDrawing.ts has it. It turns `├──`, `└──`
//! and `│`, and their heavy variants, into the indentation the grammar reads. Columns are
//! counted in UTF-16 code units, as Pele counts them, and positions in the text are bytes.

use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode, locate};
use crate::js::chars::{
    has_prefix_fold, is_digit, is_space, is_space_char, is_word, len16, space_run, trim, trim_end,
    unit_at,
};
use crate::js::math::round;

/// The box-drawing characters, Pele's ALL_BOX_CHARS.
const ALL_BOX_CHARS: [char; 8] = ['─', '━', '│', '┃', '└', '┗', '├', '┣'];

/// The characters that start a branch, Pele's BRANCH_CHAR.
const BRANCH_CHAR: [char; 4] = ['└', '┗', '├', '┣'];

/// The characters of a branch's dashes, Pele's DASH_CHAR.
const DASH_CHAR: [char; 2] = ['─', '━'];

/// The indentation of a level of the tree, Pele's INDENT_UNIT.
const INDENT_UNIT: &str = "    ";

/// Where a code unit of a line the preprocessor rewrote came from in the line as written,
/// as the functions of Pele's columns find it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Column<'a> {
    /// The line as written.
    line: &'a str,
    /// The code unit of the line, its tabs expanded, that the line's content starts at.
    from: usize,
    /// How many code units of indentation the rewritten line starts with.
    indent: usize,
}

impl Column<'_> {
    /// The byte of the line as written that code unit at of the rewritten line came from.
    pub(crate) fn at(&self, at: usize) -> usize {
        before_tabs(self.line, self.from + at.saturating_sub(self.indent))
    }
}

/// What the preprocessor makes of a text, Pele's PreprocessResult: the text, and for each of
/// its lines, the line of the input it came from, from 1, and where it was rewritten, what
/// finds where its code units came from. Both are empty when the text is the input
/// unchanged.
#[derive(Debug)]
pub(crate) struct PreprocessResult<'a> {
    pub(crate) text: String,
    pub(crate) line_map: Vec<usize>,
    pub(crate) columns: Vec<Option<Column<'a>>>,
}

/// Whether s holds a box-drawing character, as Pele's ALL_BOX_CHARS tests it.
fn has_box_char(s: &str) -> bool {
    s.contains(ALL_BOX_CHARS)
}

/// Whether any line holds a box-drawing character, as Pele's isBoxDrawingFormat finds.
fn is_box_drawing_format(lines: &[String]) -> bool {
    lines.iter().any(|line| has_box_char(line))
}

/// Where BRANCH_CHAR first matches in a line: its byte, and its code unit.
fn branch_at(line: &str) -> Option<(usize, usize)> {
    let at = line.find(BRANCH_CHAR)?;
    Some((at, len16(&line[..at])))
}

/// How many columns a level of the tree takes, as Pele's inferSegmentWidth finds it: where
/// the first branch that is not at the start of its line is.
fn infer_segment_width(content_lines: &[String]) -> usize {
    content_lines
        .iter()
        .filter_map(|line| branch_at(line))
        .map(|(_, unit)| unit)
        .find(|&unit| unit > 0)
        .unwrap_or(4)
}

/// Whether every character of s is white space, as \s matches, or one of extra, and there
/// is at least one: /^[\s…]+$/.
fn spaces_or(s: &str, extra: &[char]) -> bool {
    !s.is_empty() && s.chars().all(|c| is_space_char(c) || extra.contains(&c))
}

/// Pele's DECORATION_ONLY, /^[\s│┃]+$/.
fn decoration_only(line: &str) -> bool {
    spaces_or(line, &['│', '┃'])
}

/// Pele's BOX_ONLY, /^[\s─━│┃└┗├┣]+$/.
fn box_only(line: &str) -> bool {
    spaces_or(line, &ALL_BOX_CHARS)
}

/// The end of the run of tabs and spaces from byte i, [\t ]*.
fn blanks(s: &str, mut i: usize) -> usize {
    let b = s.as_bytes();
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    i
}

/// Pele's METADATA_LINE, /^\s*(title[\t ]|accTitle[\t ]*:|accDescr[\t ]*[:{])/.
fn metadata_line(line: &str) -> bool {
    let rest = &line[space_run(line, 0)..];
    let b = rest.as_bytes();
    if rest.starts_with("title") {
        matches!(b.get(5), Some(b'\t' | b' '))
    } else if rest.starts_with("accTitle") {
        b.get(blanks(rest, "accTitle".len())) == Some(&b':')
    } else if rest.starts_with("accDescr") {
        matches!(b.get(blanks(rest, "accDescr".len())), Some(b':' | b'{'))
    } else {
        false
    }
}

/// Pele's COMMENT_LINE, /^\s*%%/.
fn comment_line(line: &str) -> bool {
    line[space_run(line, 0)..].starts_with("%%")
}

/// The line numbers an error's message names, written as those of the original text, as
/// Pele's remapErrorLines writes them with /\bline\s+(\d+)\b/gi. line_map holds pairs of a
/// line of the text read and the line of the original it came from.
pub(crate) fn remap_error_lines(message: &str, line_map: &[(usize, usize)]) -> String {
    let b = message.as_bytes();
    let word = |i: usize| b.get(i).is_some_and(|&c| is_word(i32::from(c)));
    let mut out = String::new();
    let mut last = 0;
    let mut i = 0;
    while i + 4 <= b.len() {
        if !message.is_char_boundary(i)
            || !has_prefix_fold(&message[i..], "line")
            || (i > 0 && word(i - 1))
        {
            i += 1;
            continue;
        }
        let j = space_run(message, i + 4);
        let mut k = j;
        while k < b.len() && is_digit(i32::from(b[k])) {
            k += 1;
        }
        if j == i + 4 || k == j || word(k) {
            i += 1;
            continue;
        }
        let line = message[j..k].parse::<usize>().ok();
        out.push_str(&message[last..i]);
        match line_map.iter().find(|&&(from, _)| Some(from) == line) {
            Some(&(_, original)) if original > 0 => out.push_str(&format!("line {original}")),
            _ => out.push_str(&message[i..k]),
        }
        last = k;
        i = k;
    }
    out.push_str(&message[last..]);
    out
}

/// The byte at which line, from 1, of text starts, as Pele's lineStart finds it. Past the
/// last line it finds the start of the text, as Pele's does.
pub(crate) fn line_start(text: &str, line: usize) -> usize {
    let mut at = 0;
    for _ in 1..line {
        at = text[at..].find('\n').map_or(0, |i| at + i + 1);
    }
    at
}

/// The error of the line at index, from 0, of the input, as Pele's fail makes it.
fn fail(input: &str, index: usize, message: &str) -> Error {
    let line = locate(input, line_start(input, index + 1)).line;
    let mut e = Error::new(format!("Line {line}: {message}"), ErrorCode::Syntax);
    e.diagram = Some(DiagramType::TreeView);
    e.line = line;
    e
}

/// The byte of a line where the character is that the line, its tabs expanded to four
/// spaces, has at code unit at, as Pele's beforeTabs undoes the expansion.
fn before_tabs(line: &str, at: usize) -> usize {
    let mut i = 0;
    let mut n = 0;
    while i < line.len() && n < at {
        // A character above the Basic Multilingual Plane is two code units, and its four
        // bytes are stepped over at once.
        let (c, size) = unit_at(line, i);
        n += match (c, size) {
            (9, _) => 4,
            (_, 4) => 2,
            _ => 1,
        };
        i += size;
    }
    i
}

/// Turns a tree drawn with box-drawing characters into one indented as the grammar reads it,
/// as Pele's preprocessBoxDrawing does.
pub(crate) fn preprocess_box_drawing<'a>(input: &'a str) -> Result<PreprocessResult<'a>, Error> {
    let unchanged = || PreprocessResult {
        text: input.to_string(),
        line_map: Vec::new(),
        columns: Vec::new(),
    };
    if !has_box_char(input) {
        return Ok(unchanged());
    }
    let lines: Vec<&str> = input.split('\n').collect();
    let Some(keyword_idx) = lines.iter().position(|l| trim(l) == "treeView-beta") else {
        return Ok(unchanged());
    };

    let passes = |line: &str| trim(line).is_empty() || comment_line(line) || metadata_line(line);

    let content_lines: Vec<String> = lines[keyword_idx + 1..]
        .iter()
        .filter(|line| !passes(line) && !decoration_only(line))
        .map(|line| line.replace('\t', "    "))
        .collect();
    if !is_box_drawing_format(&content_lines) {
        return Ok(unchanged());
    }

    let segment_width = infer_segment_width(&content_lines);
    let mut out: Vec<String> = Vec::new();
    let mut line_map = Vec::new();
    let mut columns: Vec<Option<Column<'a>>> = Vec::new();
    let mut keep = |text: String, index: usize, column: Option<Column<'a>>| {
        out.push(text);
        line_map.push(index + 1);
        columns.push(column);
    };

    for (i, line) in lines.iter().enumerate().take(keyword_idx + 1) {
        keep(line.to_string(), i, None);
    }
    for (i, &line) in lines.iter().enumerate().skip(keyword_idx + 1) {
        if passes(line) {
            keep(line.to_string(), i, None);
            continue;
        }
        if decoration_only(line) {
            continue;
        }

        let normalized = line.replace('\t', "    ");
        if let Some((at, unit)) = branch_at(&normalized) {
            let depth = round(unit as f64 / segment_width as f64) as usize + 1;
            let mut pos = at + '├'.len_utf8();
            while let Some(c) = normalized[pos..]
                .chars()
                .next()
                .filter(|c| DASH_CHAR.contains(c))
            {
                pos += c.len_utf8();
            }
            while normalized.as_bytes().get(pos) == Some(&b' ') {
                pos += 1;
            }
            let content = trim_end(&normalized[pos..]);
            if content.is_empty() {
                return Err(fail(
                    input,
                    i,
                    "Empty node — expected a filename or directory name after the box-drawing prefix",
                ));
            }
            let indent = INDENT_UNIT.repeat(depth);
            let column = Column {
                line,
                from: len16(&normalized[..pos]),
                indent: indent.len(),
            };
            keep(indent + content, i, Some(column));
        } else if box_only(&normalized) {
            continue;
        } else if has_box_char(&normalized) {
            // A name that happens to contain a box character, such as "Section ─ A.txt", is a
            // root item.
            keep(line.to_string(), i, None);
        } else if is_space(unit_at(&normalized, 0).0) {
            return Err(fail(
                input,
                i,
                "Unexpected indentation without box-drawing characters. In box-drawing format, use ├── or └── prefixes for indented nodes.",
            ));
        } else {
            keep(line.to_string(), i, None);
        }
    }
    Ok(PreprocessResult {
        text: out.join("\n"),
        line_map,
        columns,
    })
}
