//! The errors of a diagram's text, and the edits that map a place in the text a parser reads
//! back to where its author wrote it, as Pele's src/errors.ts has them.

use std::fmt;

use crate::detect::{Detected, DiagramType};
use crate::js::chars::{is_word, len16};

/// What kind of error an [`Error`] is, as Pele's PeleErrorCode says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// Text of no diagram type known, or of a type not supported.
    UnsupportedDiagram,
    /// Text that does not parse.
    Syntax,
    /// Text that parses but asks for what cannot be, as a shape of no such name.
    Semantic,
    /// Text past a limit, as of its length or of a flowchart's edges.
    Limit,
}

impl ErrorCode {
    /// The code as Pele writes it, as "unsupported-diagram".
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::UnsupportedDiagram => "unsupported-diagram",
            ErrorCode::Syntax => "syntax",
            ErrorCode::Semantic => "semantic",
            ErrorCode::Limit => "limit",
        }
    }
}

/// An error in a diagram's text, as Pele's PeleError is.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Error {
    /// What kind of error it is.
    pub code: ErrorCode,
    /// What is wrong, as Pele words it.
    pub message: String,
    /// The type of the diagram the error is in, where it is known.
    pub diagram: Option<DiagramType>,
    /// That type as Mermaid names it, as "flowchart-v2", where it is known.
    pub mermaid_type: Option<&'static str>,
    /// The line of the text the error is on, from 1, or 0 where it has no place.
    pub line: usize,
    /// The column of the line, from 1, counted in UTF-16 code units as JavaScript counts
    /// them, or 0 where the error has no place.
    pub column: usize,
    /// The line, and a caret under the column, or nothing where the error has no place.
    pub snippet: String,
}

impl Error {
    /// An error with no place.
    pub(crate) fn new(message: impl Into<String>, code: ErrorCode) -> Error {
        Error {
            code,
            message: message.into(),
            diagram: None,
            mermaid_type: None,
            line: 0,
            column: 0,
            snippet: String::new(),
        }
    }

    /// The error, of a diagram of a type found.
    pub(crate) fn of(mut self, found: Detected) -> Error {
        self.diagram = Some(found.diagram);
        self.mermaid_type = Some(found.mermaid_type);
        self
    }

    /// Gives the error the diagram's type where it has none, and writes the entity codes in
    /// its message and snippet as the text has them, not as the parser read them, as Pele's
    /// tagErrors does for the errors of a diagram's parser and of its layout.
    pub(crate) fn tagged(mut self, found: Detected) -> Error {
        self.diagram.get_or_insert(found.diagram);
        self.mermaid_type.get_or_insert(found.mermaid_type);
        self.message = written_codes(&self.message);
        self.snippet = written_codes(&self.snippet);
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

/// Pele's Edit: the bytes from..to of a text replaced by text. Pele records the length of
/// the replacement in code units. An edit keeps the replacement itself, so that a place
/// inside it is counted in code units as Pele counts it.
#[derive(Clone, Debug)]
pub(crate) struct Edit {
    pub(crate) from: usize,
    pub(crate) to: usize,
    pub(crate) text: String,
}

impl Edit {
    /// An edit that takes the bytes from..to out.
    pub(crate) fn cut(from: usize, to: usize) -> Edit {
        Edit {
            from,
            to,
            text: String::new(),
        }
    }
}

/// The edits of the passes over a text, each pass's in the order they were made, or none
/// where nothing is recorded.
pub(crate) type Steps<'a> = Option<&'a mut Vec<Vec<Edit>>>;

/// Adds the edits of a pass to steps, if they are recorded. A pass adds its edits once it is
/// done, where Pele's recording adds them as it begins. No pass adds while another is under
/// way, so the steps are in the same order.
pub(crate) fn push(steps: &mut Steps, edits: Vec<Edit>) {
    if let Some(steps) = steps {
        steps.push(edits);
    }
}

/// Maps byte at of the text edits made back to the text they were made in, as Pele's
/// stepInput does. A place inside a replacement maps to as many code units into what it
/// replaced, or to the last of those.
fn step_input(edits: &[Edit], at: usize) -> usize {
    let mut shift: isize = 0;
    for e in edits {
        let start = (e.from as isize - shift) as usize;
        if at < start {
            break;
        }
        if at < start + e.text.len() {
            return e.from + len16(&e.text[..at - start]).min(e.to - e.from - 1);
        }
        shift += e.to as isize - e.from as isize - e.text.len() as isize;
    }
    (at as isize + shift) as usize
}

/// Maps byte at of the text the last of steps made back to the text the first was made in,
/// as Pele's inputOffset does.
pub(crate) fn input_offset(steps: &[Vec<Edit>], at: usize) -> usize {
    steps
        .iter()
        .rev()
        .fold(at, |at, edits| step_input(edits, at))
}

/// Writes each entity code that Pele's encodeEntities hid behind placeholders, as `ﬂ°°35¶ß`
/// or `ﬂ°quot¶ß`, back as its author wrote it, `#35;` or `#quot;`, as Pele's writtenCodes
/// does with `text.replace(/ﬂ°°?(\w+)¶ß/g, '#$1;')`.
pub(crate) fn written_codes(text: &str) -> String {
    const OPEN: &str = "ﬂ°";
    const CLOSE: &str = "¶ß";
    if !text.contains(CLOSE) {
        return text.to_string();
    }
    let mut out = String::new();
    let (mut last, mut i) = (0, 0);
    while let Some(n) = text[i..].find(OPEN) {
        let at = i + n;
        let mut j = at + OPEN.len();
        if text[j..].starts_with('°') {
            j += '°'.len_utf8();
        }
        let k = j + text[j..]
            .bytes()
            .take_while(|&c| is_word(i32::from(c)))
            .count();
        if k > j && text[k..].starts_with(CLOSE) {
            out.push_str(&text[last..at]);
            out.push('#');
            out.push_str(&text[j..k]);
            out.push(';');
            last = k + CLOSE.len();
            i = last;
            continue;
        }
        // With no word after `ﬂ°°`, the expression backtracks to take the second `°` as no
        // part of it, and no word starts there either.
        i = at + OPEN.len();
    }
    out.push_str(&text[last..]);
    out
}
