//! A user journey's tokens read into its db, as Pele's parser.ts reads them, failing where
//! Mermaid's Jison parser fails, with its words.

use super::db::JourneyDb;
use super::lexer::{
    ACC_DESCR, ACC_DESCR_MULTILINE_VALUE, ACC_TITLE, END, EOF, JOURNEY, NEWLINE, SECTION,
    TASK_DATA, TASK_NAME, TITLE, TOKEN_NAMES, tokenize,
};
use crate::detect::DiagramType;
use crate::errors::{Error, syntax_error};
use crate::js::chars::{substring, trim};

/// What a statement may start with, as Mermaid's errors list it.
const STATEMENT: &str = "'EOF', 'SPACE', 'NEWLINE', 'title', 'acc_title', 'acc_descr', \
                         'acc_descr_multiline_value', 'section', 'taskName'";

/// Parses a user journey's text into db, as Pele's parseJourney does.
pub(crate) fn parse_journey(src: &str, db: &mut JourneyDb) -> Result<(), Error> {
    let t = tokenize(src);
    let fail = |i: usize, expected: &str| {
        let got = if t.types[i] == END {
            "1"
        } else {
            TOKEN_NAMES[t.types[i]]
        };
        let detail = format!("Expecting {expected}, got '{got}'");
        syntax_error(DiagramType::Journey, src, t.starts[i], &detail, false)
    };

    if t.types[0] != JOURNEY {
        return Err(fail(0, "'journey'"));
    }
    let mut i = 1;
    loop {
        let text = &t.texts[i];
        match t.types[i] {
            NEWLINE => {}
            TITLE => db.set_diagram_title(substring(text, 6)),
            ACC_TITLE => {
                i += 1;
                db.set_acc_title(trim(&t.texts[i]));
            }
            ACC_DESCR => {
                i += 1;
                db.set_acc_description(trim(&t.texts[i]));
            }
            ACC_DESCR_MULTILINE_VALUE => db.set_acc_description(trim(text)),
            SECTION => db.add_section(substring(text, 8)),
            TASK_NAME => {
                i += 1;
                if t.types[i] != TASK_DATA {
                    return Err(fail(i, "'taskData'"));
                }
                db.add_task(text, &t.texts[i]);
            }
            EOF => return Ok(()),
            _ => return Err(fail(i, STATEMENT)),
        }
        i += 1;
    }
}
