//! A gantt chart's statements read into its db, as Pele's parser.ts reads them: what
//! Mermaid's gantt.jison accepts, with its actions run in the order it runs them.

use super::db::GanttDb;
use super::lexer::{T, TOKEN_NAMES, WEEKDAY_TOKENS, tokenize};
use crate::detect::DiagramType;
use crate::errors::{Error, syntax_error};
use crate::js::chars::trim;

const WEEKDAYS: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

/// The tokens Mermaid's parser names as expected at the start of a line, Pele's LINE.
const LINE: &str = "'EOF', 'SPACE', 'NL', 'weekday_monday', 'weekday_tuesday', \
                    'weekday_wednesday', 'weekday_thursday', 'weekday_friday', \
                    'weekday_saturday', 'weekday_sunday', 'weekend_friday', 'weekend_saturday', \
                    'dateFormat', 'inclusiveEndDates', 'topAxis', 'axisFormat', 'tickInterval', \
                    'excludes', 'includes', 'todayMarker', 'title', 'acc_title', 'acc_descr', \
                    'acc_descr_multiline_value', 'section', 'taskTxt', 'click'";

/// Whether a token can start a line, which the tokens that can follow a complete statement
/// are too, as Pele's STARTS_LINE has them.
fn starts_line(t: T) -> bool {
    matches!(
        t,
        T::Eof
            | T::Nl
            | T::DateFormat
            | T::InclusiveEndDates
            | T::TopAxis
            | T::AxisFormat
            | T::TickInterval
            | T::Excludes
            | T::Includes
            | T::TodayMarker
            | T::Title
            | T::AccTitle
            | T::AccDescr
            | T::AccDescrMultilineValue
            | T::Section
            | T::TaskTxt
            | T::Click
    ) || (T::WeekdayMonday..=T::WeekendSaturday).contains(&t)
}

/// s after its first n code units, as JavaScript's slice(n) gives it.
fn after_units(s: &str, n: usize) -> &str {
    let mut units = 0;
    for (i, c) in s.char_indices() {
        if units >= n {
            return &s[i..];
        }
        units += c.len_utf16();
    }
    ""
}

/// Reads the statements of src into db, as Pele's parseGantt does.
pub(super) fn parse_gantt(src: &str, db: &mut GanttDb) -> Result<(), Error> {
    let toks = tokenize(src);
    let mut i = 0;

    // Pele's fail: the syntax error of the token at i, where the parser expected others.
    let fail = |i: usize, expected: &str| -> Error {
        let t = toks[i].kind;
        let got = if t == T::End {
            "1"
        } else {
            TOKEN_NAMES[t as usize]
        };
        let detail = format!("Expecting {expected}, got '{got}'");
        syntax_error(DiagramType::Gantt, src, toks[i].start, &detail, false)
    };
    // Pele's end. Mermaid's parser runs a statement's action only after seeing that the next
    // token can follow it.
    let end = |i: usize, also: &str| -> Result<(), Error> {
        if starts_line(toks[i].kind) {
            Ok(())
        } else {
            Err(fail(i, &format!("{LINE}{also}")))
        }
    };

    if toks[i].kind != T::Gantt {
        return Err(fail(i, "'gantt'"));
    }
    i += 1;

    loop {
        let t = toks[i].kind;
        let text = toks[i].text;
        if !starts_line(t) {
            return Err(fail(i, LINE));
        }
        i += 1;
        match t {
            T::Eof => {
                if toks[i].kind == T::End {
                    return Ok(());
                }
                end(i, "")?;
            }
            T::Nl => {}
            T::DateFormat => {
                end(i, "")?;
                db.set_date_format(after_units(text, 11));
            }
            T::InclusiveEndDates => {
                end(i, "")?;
                db.enable_inclusive_end_dates();
            }
            T::TopAxis => {
                end(i, "")?;
                db.set_top_axis();
            }
            T::AxisFormat => {
                end(i, "")?;
                db.set_axis_format(after_units(text, 11));
            }
            T::TickInterval => {
                end(i, "")?;
                db.set_tick_interval(after_units(text, 13));
            }
            T::Excludes => {
                end(i, "")?;
                db.set_excludes(after_units(text, 9));
            }
            T::Includes => {
                end(i, "")?;
                db.set_includes(after_units(text, 9));
            }
            T::TodayMarker => {
                end(i, "")?;
                db.set_today_marker(after_units(text, 12));
            }
            T::WeekendFriday => {
                end(i, "")?;
                db.set_weekend("friday");
            }
            T::WeekendSaturday => {
                end(i, "")?;
                db.set_weekend("saturday");
            }
            T::Title => {
                end(i, "")?;
                db.set_diagram_title(after_units(text, 6));
            }
            T::AccTitle | T::AccDescr => {
                let want = if t == T::AccTitle {
                    T::AccTitleValue
                } else {
                    T::AccDescrValue
                };
                if toks[i].kind != want {
                    return Err(fail(i, &format!("'{}'", TOKEN_NAMES[want as usize])));
                }
                let value = toks[i].text;
                i += 1;
                end(i, "")?;
                if t == T::AccTitle {
                    db.set_acc_title(trim(value));
                } else {
                    db.set_acc_description(trim(value));
                }
            }
            T::AccDescrMultilineValue => {
                end(i, "")?;
                db.set_acc_description(trim(text));
            }
            T::Section => {
                end(i, "")?;
                db.add_section(after_units(text, 8));
            }
            T::TaskTxt => {
                if toks[i].kind != T::TaskData {
                    return Err(fail(i, "'taskData'"));
                }
                let data = toks[i].text;
                i += 1;
                end(i, "")?;
                db.add_task(text, data)?;
            }
            T::Click => {
                let (mut name, mut href) = (false, None);
                if toks[i].kind == T::Callbackname {
                    name = true;
                    i += 1;
                    let args = toks[i].kind == T::Callbackargs;
                    if args {
                        i += 1;
                    }
                    if toks[i].kind == T::Href {
                        href = Some(toks[i].text);
                        i += 1;
                        end(i, "")?;
                    } else if args {
                        end(i, ", 'href'")?;
                    } else {
                        end(i, ", 'callbackargs', 'href'")?;
                    }
                } else if toks[i].kind == T::Href {
                    href = Some(toks[i].text);
                    i += 1;
                    if toks[i].kind == T::Callbackname {
                        name = true;
                        i += 1;
                        if toks[i].kind == T::Callbackargs {
                            i += 1;
                            end(i, "")?;
                        } else {
                            end(i, ", 'callbackargs'")?;
                        }
                    } else {
                        end(i, ", 'callbackname'")?;
                    }
                } else {
                    return Err(fail(i, "'callbackname', 'href'"));
                }
                if name {
                    db.set_click_event(text);
                }
                if let Some(href) = href {
                    db.set_link(text, href);
                }
            }
            _ => {
                end(i, "")?;
                let k = WEEKDAY_TOKENS.iter().position(|&w| w == t).unwrap_or(0);
                db.set_weekday(WEEKDAYS[k]);
            }
        }
    }
}
