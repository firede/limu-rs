//! What Pele's parseSource in src/core.ts does before it hands a diagram's text to the parser
//! of its type: it holds the text to its limit, reads its front matter, directives and type,
//! and hides its entity codes. The parser's errors then report where they are in the text as
//! its author wrote it.

use std::cell::OnceCell;
use std::rc::Rc;

use crate::detect::{Detected, detect_diagram};
use crate::errors::{Edit, Error, ErrorCode, Origin, input_offset, locating_in};
use crate::js::chars::len16;
use crate::preprocess::{cleanup_comments, encode_entities, normalize_line_breaks, preprocess};
use crate::value::Map;

/// The most UTF-16 code units a diagram's text may have by default, Mermaid's maxTextSize.
pub const DEFAULT_LIMIT: usize = 50_000;

/// The most edges a flowchart or an agent flow may have by default. Mermaid stops at 500.
/// Pele draws 5,000 in tens of milliseconds, and `A & B --> C & D` multiplies, so that
/// without a limit a few kilobytes of text ask for millions of edges.
pub const DEFAULT_MAX_EDGES: usize = 5_000;

/// What a diagram's text is read with, as Pele's ParseOptions has it.
#[derive(Clone, Debug, PartialEq)]
pub struct ParseOptions {
    /// The most UTF-16 code units the text may have, [`DEFAULT_LIMIT`] by default, or no
    /// limit where it is none.
    pub limit: Option<usize>,
    /// The most edges a flowchart or an agent flow may have, [`DEFAULT_MAX_EDGES`] by
    /// default, or no limit where it is none.
    pub max_edges: Option<usize>,
    /// Configuration under the diagram's own, whose keys the diagram's take the place of.
    pub config: Map,
}

impl Default for ParseOptions {
    fn default() -> Self {
        ParseOptions {
            limit: Some(DEFAULT_LIMIT),
            max_edges: Some(DEFAULT_MAX_EDGES),
            config: Map::new(),
        }
    }
}

/// A diagram's text made ready for the parser of its type.
#[derive(Clone, Debug)]
pub(crate) struct Prepared {
    /// The diagram's type, as Pele and as Mermaid name it.
    pub(crate) found: Detected,
    /// The text after its front matter, with directives and comment lines taken out: what
    /// detect read.
    pub(crate) text: String,
    /// The text with its comment lines, for a diagram that keeps them.
    pub(crate) with_comments: String,
    /// The diagram's configuration, with the options' under its own.
    pub(crate) config: Map,
    /// The title its front matter gives.
    pub(crate) title: Option<String>,
    /// The number of lines of front matter before the text.
    pub(crate) line_offset: usize,
    /// The most edges a flowchart or an agent flow may have, or none for no limit.
    pub(crate) max_edges: Option<usize>,
    /// The text as its author wrote it.
    source: String,
}

/// Reads a diagram's front matter, directives and type, as Pele's parseSource does before it
/// calls the parser of the type. It does not know which types have a parser.
pub(crate) fn prepare(source: &str, options: &ParseOptions) -> Result<Prepared, Error> {
    if let Some(limit) = options.limit
        && len16(source) > limit
    {
        return Err(Error::new(
            format!("Diagram source is longer than the limit of {limit} characters."),
            ErrorCode::Limit,
        ));
    }
    let pre = preprocess(source, None)?;
    let Some(found) = detect_diagram(&pre.text) else {
        return Err(Error::new(
            "No diagram type detected for the given text.",
            ErrorCode::UnsupportedDiagram,
        ));
    };
    // { ...extra, ...pre.config }: the options' keys first, each the diagram's where it has
    // one, and then the diagram's own.
    let mut config = options.config.clone();
    for (k, v) in &pre.config {
        config.insert(k, v.clone());
    }
    Ok(Prepared {
        found,
        text: pre.text,
        with_comments: pre.with_comments,
        config,
        title: pre.title,
        line_offset: pre.line_offset,
        max_edges: options.max_edges,
        source: source.to_string(),
    })
}

impl Prepared {
    /// The text a diagram's parser reads, as Pele's parseSource hands it over: the text, or
    /// the text with its comments for a diagram that keeps them, with its entity codes hidden
    /// and a line feed after it.
    pub(crate) fn source(&self, keep_comments: bool) -> String {
        let text = if keep_comments {
            &self.with_comments
        } else {
            &self.text
        };
        encode_entities(text, None) + "\n"
    }

    /// Parses the diagram with parse, given the text source gives, as Pele's parseSource
    /// does: the errors parse locates in that text report where they are in the text its
    /// author wrote, as locatingIn has them, and they are tagged as tagErrors tags them.
    pub(crate) fn parse_with<T>(
        &self,
        keep_comments: bool,
        parse: impl FnOnce(&str) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let parsed: Rc<str> = self.source(keep_comments).into();
        let found = InSource {
            source: self.source.clone(),
            keep_comments,
            read: OnceCell::new(),
        };
        let origin = Origin {
            parsed: parsed.clone(),
            find: Box::new(move |at| found.find(at)),
        };
        locating_in(origin, || parse(&parsed)).map_err(|e| e.tagged(self.found))
    }
}

/// What Pele's inSource makes: finds a byte of the text the parser reads in the text as its
/// author wrote it. It reads the text again, recording how it was rewritten, only when it is
/// first asked, since that is only to report an error.
struct InSource {
    /// The text as its author wrote it.
    source: String,
    keep_comments: bool,
    /// The text as written, with its line breaks normalized and the line feed the parser's
    /// text has after it, and the edits that made the parser's text of it.
    read: OnceCell<(String, Vec<Vec<Edit>>)>,
}

impl InSource {
    fn find(&self, at: usize) -> (String, usize) {
        let (source, steps) = self.read.get_or_init(|| {
            let mut steps = Vec::new();
            // The text was read once without error, so it is read again without one.
            let pre = preprocess(&self.source, Some(&mut steps)).expect("read once already");
            let text = if self.keep_comments {
                pre.with_comments
            } else {
                cleanup_comments(&pre.with_comments, Some(&mut steps))
            };
            encode_entities(&text, Some(&mut steps));
            // Match the line feed the parser's text has after it.
            (normalize_line_breaks(&self.source) + "\n", steps)
        });
        (source.clone(), input_offset(steps, at))
    }
}
