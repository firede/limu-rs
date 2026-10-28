//! A timeline's tokens read into its db, as Pele's parser.ts reads them, failing where
//! Mermaid's Jison parser fails, with its words.

use super::db::TimelineDb;
use super::lexer::{
    ACC_DESCR, ACC_DESCR_MULTILINE_VALUE, ACC_TITLE, END, EOF, EVENT, NEWLINE, PERIOD, SECTION,
    TIMELINE, TIMELINE_LR, TIMELINE_TD, TITLE, TOKEN_NAMES, tokenize,
};
use crate::detect::DiagramType;
use crate::errors::{Error, syntax_error};
use crate::js::chars::{substring, trim};

/// What a statement may start with, as Mermaid's errors list it.
const STATEMENT: &str = "'EOF', 'SPACE', 'NEWLINE', 'title', 'acc_title', 'acc_descr', \
                         'acc_descr_multiline_value', 'section', 'period', 'event'";

/// Parses a timeline's text into db, as Pele's parseTimeline does.
pub(crate) fn parse_timeline(src: &str, db: &mut TimelineDb) -> Result<(), Error> {
    let t = tokenize(src);
    let fail = |i: usize, expected: &str| {
        let got = if t.types[i] == END {
            "1"
        } else {
            TOKEN_NAMES[t.types[i]]
        };
        let detail = format!("Expecting {expected}, got '{got}'");
        syntax_error(DiagramType::Timeline, src, t.starts[i], &detail, false)
    };

    let head = t.types[0];
    if head != TIMELINE && head != TIMELINE_LR && head != TIMELINE_TD {
        return Err(fail(0, "'timeline', 'timeline_lr', 'timeline_td'"));
    }
    if head != TIMELINE {
        db.set_direction(if head == TIMELINE_LR { "LR" } else { "TD" });
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
            PERIOD => db.add_task(text, ""),
            EVENT => db.add_event(substring(text, 2))?,
            EOF => return Ok(()),
            _ => return Err(fail(i, STATEMENT)),
        }
        i += 1;
    }
}
