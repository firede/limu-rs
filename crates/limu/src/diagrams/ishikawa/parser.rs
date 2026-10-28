//! An Ishikawa diagram's tokens read into its db, as Pele's parser.ts reads them, failing
//! where Mermaid's Jison parser fails, with its words.

use super::db::IshikawaDb;
use super::lexer::{END, EOF, ISHIKAWA, NL, SPACELINE, SPACELIST, TEXT, TOKEN_NAMES, tokenize};
use crate::detect::DiagramType;
use crate::errors::{Error, syntax_error};
use crate::js::chars::{len16, trim};

/// What may end a statement, as Mermaid's errors list it.
const STOP: &str = "'SPACELINE', 'NL', 'EOF'";

/// Parses an Ishikawa diagram's text into db, as Pele's parseIshikawa does, following the
/// parser Jison builds from Mermaid's grammar: blank lines and comments may come first, then
/// the keyword and at most one line break, then statements. A statement is a line of text,
/// with or without indentation, or a blank line. Each is ended by a blank line, a line break
/// or the end of the text, and any number of line breaks after that.
pub(crate) fn parse_ishikawa(src: &str, db: &mut IshikawaDb) -> Result<(), Error> {
    let t = tokenize(src);
    let (types, starts, ends) = (&t.types, &t.starts, &t.ends);
    let fail = |i: usize, expected: &str| {
        let detail = format!("Expecting {expected}, got '{}'", TOKEN_NAMES[types[i]]);
        Err(syntax_error(
            DiagramType::Ishikawa,
            src,
            starts[i],
            &detail,
            false,
        ))
    };
    let stops = |kind: usize| kind == SPACELINE || kind == NL || kind == EOF;
    let mut i = 0;

    if types[i] == SPACELINE {
        i += 1;
        while types[i] == SPACELINE || types[i] == NL {
            i += 1;
        }
        if types[i] != ISHIKAWA {
            return fail(i, "'SPACELINE', 'NL', 'ISHIKAWA'");
        }
    } else if types[i] != ISHIKAWA {
        return fail(i, "'SPACELINE', 'ISHIKAWA'");
    }
    i += 1;
    if types[i] == NL {
        i += 1;
    }

    loop {
        let kind = types[i];
        if kind == TEXT || kind == SPACELIST && types[i + 1] == TEXT {
            let text = if kind == TEXT { i } else { i + 1 };
            i = text + 1;
            if !stops(types[i]) {
                return fail(i, STOP);
            }
            let level = if kind == TEXT {
                0
            } else {
                len16(&src[starts[text - 1]..ends[text - 1]])
            };
            db.add_node(level, trim(&src[starts[text]..ends[text]]));
        } else if kind == SPACELINE || kind == SPACELIST {
            i += 1;
            if !stops(types[i]) {
                if kind == SPACELIST {
                    return fail(i, &format!("{STOP}, 'TEXT'"));
                }
                return fail(i, STOP);
            }
        } else {
            return fail(i, "'SPACELINE', 'SPACELIST', 'TEXT'");
        }
        i += 1;
        while types[i] == NL || types[i] == EOF {
            i += 1;
        }
        if types[i] == END {
            return Ok(());
        }
    }
}
