//! The tokens of a gantt chart, as Pele's lexer.ts and tokens.ts make them: a lexer that
//! reads what Mermaid's gantt.jison lexes, its patterns written out by hand.

use crate::js::chars::{has_prefix_fold, is_space, is_word, space_run, unit, unit_at};

/// A kind of token, Pele's T, in the order of Mermaid's gantt.jison.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub(super) enum T {
    End,
    Error,
    OpenDirective,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    Nl,
    Href,
    Callbackname,
    Callbackargs,
    Click,
    Gantt,
    DateFormat,
    InclusiveEndDates,
    TopAxis,
    AxisFormat,
    TickInterval,
    Includes,
    Excludes,
    TodayMarker,
    WeekdayMonday,
    WeekdayTuesday,
    WeekdayWednesday,
    WeekdayThursday,
    WeekdayFriday,
    WeekdaySaturday,
    WeekdaySunday,
    WeekendFriday,
    WeekendSaturday,
    Date,
    Title,
    AccDescription,
    Section,
    TaskTxt,
    TaskData,
    Colon,
    Eof,
}

/// The weekday tokens in order, from Monday.
pub(super) const WEEKDAY_TOKENS: [T; 7] = [
    T::WeekdayMonday,
    T::WeekdayTuesday,
    T::WeekdayWednesday,
    T::WeekdayThursday,
    T::WeekdayFriday,
    T::WeekdaySaturday,
    T::WeekdaySunday,
];

/// The names Mermaid's parser gives the tokens in its errors, Pele's TOKEN_NAMES.
pub(super) const TOKEN_NAMES: [&str; 39] = [
    "$end",
    "INVALID",
    "open_directive",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "NL",
    "href",
    "callbackname",
    "callbackargs",
    "click",
    "gantt",
    "dateFormat",
    "inclusiveEndDates",
    "topAxis",
    "axisFormat",
    "tickInterval",
    "includes",
    "excludes",
    "todayMarker",
    "weekday_monday",
    "weekday_tuesday",
    "weekday_wednesday",
    "weekday_thursday",
    "weekday_friday",
    "weekday_saturday",
    "weekday_sunday",
    "weekend_friday",
    "weekend_saturday",
    "date",
    "title",
    "accDescription",
    "section",
    "taskTxt",
    "taskData",
    ":",
    "EOF",
];

/// A token: its kind, its text and the byte of the source it starts at.
#[derive(Clone, Copy, Debug)]
pub(super) struct Tok<'a> {
    pub(super) kind: T,
    pub(super) text: &'a str,
    pub(super) start: usize,
}

/// The states of the lexer, Pele's S.
#[derive(Clone, Copy, PartialEq, Eq)]
enum S {
    Initial,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    Href,
    Callbackname,
    Callbackargs,
    Click,
}

const DAYS: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

/// Whether the byte at i of s is in `\w`, as the `\b` of Pele's patterns tests what follows a
/// word.
fn word_at(s: &str, i: usize) -> bool {
    s.as_bytes().get(i).is_some_and(|&c| is_word(i32::from(c)))
}

/// Whether word, in ASCII letters, is at byte p of s but for case, as the i flag of Pele's
/// patterns matches it.
fn fold_at(s: &str, p: usize, word: &str) -> bool {
    has_prefix_fold(&s[p..], word)
}

// The patterns the lexer matches where it stands, as Pele's sticky regular expressions, each
// written out: the end of the match, or none.

/// `/word\s*sep\s*/iy`, for accTitle and accDescr with ':' or '{': Pele's RE_ACC_TITLE,
/// RE_ACC_DESCR and RE_ACC_DESCR_ML.
fn match_acc(s: &str, p: usize, word: &str, sep: u8) -> Option<usize> {
    if !fold_at(s, p, word) {
        return None;
    }
    let e = space_run(s, p + word.len());
    (s.as_bytes().get(e) == Some(&sep)).then(|| space_run(s, e + 1))
}

/// `/word\s+/iy`, and with quote set `/word\s+"/iy`: Pele's RE_CALL, RE_CLICK and RE_HREF.
fn match_spaced(s: &str, p: usize, word: &str, quote: bool) -> Option<usize> {
    if !fold_at(s, p, word) {
        return None;
    }
    let mut e = space_run(s, p + word.len());
    if e == p + word.len() {
        return None;
    }
    if quote {
        if s.as_bytes().get(e) != Some(&b'"') {
            return None;
        }
        e += 1;
    }
    Some(e)
}

/// `/word\b/iy`, as the keywords of Pele's KEYWORDS that take nothing after them.
fn match_word(s: &str, p: usize, word: &str) -> Option<usize> {
    (fold_at(s, p, word) && !word_at(s, p + word.len())).then_some(p + word.len())
}

/// `/word\s[^stop]+/iy`: the keyword, one white space character, and then all up to the
/// first of the stop bytes, of which there must be one or more. It is the keywords of Pele's
/// KEYWORDS that take a value.
fn match_rest(s: &str, p: usize, word: &str, stop: &[u8]) -> Option<usize> {
    if !fold_at(s, p, word) {
        return None;
    }
    let mut q = p + word.len();
    let (c, size) = unit_at(s, q);
    if !is_space(c) {
        return None;
    }
    q += size;
    let b = s.as_bytes();
    let mut e = q;
    while e < b.len() && !stop.contains(&b[e]) {
        e += 1;
    }
    (e > q).then_some(e)
}

/// `/title[^\S\r\n]*(?=[\r\n]|$)/iy`, Pele's RE_BARE_TITLE: the word alone on the rest of its
/// line, but for white space.
fn match_bare_title(s: &str, p: usize) -> Option<usize> {
    if !fold_at(s, p, "title") {
        return None;
    }
    let mut e = p + "title".len();
    loop {
        let (c, size) = unit_at(s, e);
        if c == 10 || c == 13 || !is_space(c) {
            break;
        }
        e += size;
    }
    match s.as_bytes().get(e) {
        None | Some(b'\n' | b'\r') => Some(e),
        _ => None,
    }
}

/// `/title[^\S\r\n][^\n]+/iy`: the word, a white space character that breaks no line, and
/// the rest of the line, of which there must be some.
fn match_title(s: &str, p: usize) -> Option<usize> {
    let c = unit(s, p + "title".len());
    if c == 10 || c == 13 {
        return None;
    }
    match_rest(s, p, "title", b"\n")
}

/// `/prefix\s+(name|…)\b/iy`: the end and the index of the name. It is Pele's RE_WEEKDAY and
/// RE_WEEKEND.
fn match_days(s: &str, p: usize, prefix: &str, names: &[&str]) -> Option<(usize, usize)> {
    let e = match_spaced(s, p, prefix, false)?;
    names
        .iter()
        .position(|name| fold_at(s, e, name) && !word_at(s, e + name.len()))
        .map(|k| (e + names[k].len(), k))
}

/// `/\d\d\d\d-\d\d-\d\d\b/y`, Pele's RE_DATE.
fn match_date(s: &str, p: usize) -> bool {
    let Some(b) = s.as_bytes().get(p..p + 10) else {
        return false;
    };
    b.iter().enumerate().all(|(i, &c)| {
        if i == 4 || i == 7 {
            c == b'-'
        } else {
            c.is_ascii_digit()
        }
    }) && !word_at(s, p + 10)
}

/// A statement that starts with a keyword: its pattern and its token.
type Rule = (fn(&str, usize) -> Option<usize>, T);

/// Pele's KEYWORDS: statements that start with a keyword, by the keyword's first letter and in
/// the grammar's rule order.
fn keyword_rules(c: u8) -> &'static [Rule] {
    match c {
        b'a' => &[
            (
                |s, p| match_rest(s, p, "axisFormat", b"#\n;"),
                T::AxisFormat,
            ),
            (
                |s, p| match_rest(s, p, "accDescription", b"#\n;"),
                T::AccDescription,
            ),
        ],
        b'g' => &[(|s, p| match_word(s, p, "gantt"), T::Gantt)],
        b'd' => &[(
            |s, p| match_rest(s, p, "dateFormat", b"#\n;"),
            T::DateFormat,
        )],
        b'i' => &[
            (
                |s, p| match_word(s, p, "inclusiveEndDates"),
                T::InclusiveEndDates,
            ),
            (|s, p| match_rest(s, p, "includes", b"#\n;"), T::Includes),
        ],
        b't' => &[
            (|s, p| match_word(s, p, "topAxis"), T::TopAxis),
            (
                |s, p| match_rest(s, p, "tickInterval", b"#\n;"),
                T::TickInterval,
            ),
            (
                |s, p| match_rest(s, p, "todayMarker", b"\n;"),
                T::TodayMarker,
            ),
            (match_title, T::Title),
        ],
        b'e' => &[(|s, p| match_rest(s, p, "excludes", b"#\n;"), T::Excludes)],
        b's' => &[(|s, p| match_rest(s, p, "section", b"\n"), T::Section)],
        _ => &[],
    }
}

/// The tokens of a gantt chart, as Pele's tokenize makes them, ending with [`T::End`].
pub(super) fn tokenize<'a>(src: &'a str) -> Vec<Tok<'a>> {
    let n = src.len();
    let b = src.as_bytes();
    let mut toks = Vec::new();
    let mut state = S::Initial;
    let mut p = 0;

    // Pele's emit: a token from s to e, and where the lexer goes on from.
    let emit = |toks: &mut Vec<Tok<'a>>, kind: T, s: usize, e: usize| -> usize {
        toks.push(Tok {
            kind,
            text: &src[s..e],
            start: s,
        });
        e
    };
    let until = |ch: u8, from: usize| -> usize {
        if from >= n {
            return n;
        }
        b[from..]
            .iter()
            .position(|&c| c == ch)
            .map_or(n, |e| from + e)
    };

    // Pele's keyword: a statement that starts with a keyword at p, if there is one.
    let keyword = |toks: &mut Vec<Tok<'a>>, p: &mut usize, state: &mut S, c: i32| -> bool {
        let lower = (c | 32) as u8;
        match lower {
            b'a' => {
                if let Some(e) = match_acc(src, *p, "accTitle", b':') {
                    *state = S::AccTitle;
                    *p = emit(toks, T::AccTitle, *p, e);
                    return true;
                }
                if let Some(e) = match_acc(src, *p, "accDescr", b':') {
                    *state = S::AccDescr;
                    *p = emit(toks, T::AccDescr, *p, e);
                    return true;
                }
                if let Some(e) = match_acc(src, *p, "accDescr", b'{') {
                    *state = S::AccDescrMultiline;
                    *p = e;
                    return true;
                }
            }
            b'h' => {
                if let Some(e) = match_spaced(src, *p, "href", true) {
                    *state = S::Href;
                    *p = e;
                    return true;
                }
            }
            b'c' => {
                if let Some(e) = match_spaced(src, *p, "call", false) {
                    *state = S::Callbackname;
                    *p = e;
                    return true;
                }
                if let Some(e) = match_spaced(src, *p, "click", false) {
                    *state = S::Click;
                    *p = e;
                    return true;
                }
            }
            b't' => {
                // A bare title is skipped, as no title.
                if let Some(e) = match_bare_title(src, *p) {
                    *p = e;
                    return true;
                }
            }
            b'w' => {
                if let Some((e, k)) = match_days(src, *p, "weekday", &DAYS) {
                    *p = emit(toks, WEEKDAY_TOKENS[k], *p, e);
                    return true;
                }
                if let Some((e, k)) = match_days(src, *p, "weekend", &["friday", "saturday"]) {
                    let kind = if k == 0 {
                        T::WeekendFriday
                    } else {
                        T::WeekendSaturday
                    };
                    *p = emit(toks, kind, *p, e);
                    return true;
                }
            }
            _ => {}
        }
        for &(pattern, kind) in keyword_rules(lower) {
            if let Some(e) = pattern(src, *p) {
                *p = emit(toks, kind, *p, e);
                return true;
            }
        }
        false
    };

    loop {
        if p >= n {
            let kind = match state {
                S::Initial => T::Eof,
                S::AccTitle => T::AccTitleValue,
                S::AccDescr => T::AccDescrValue,
                S::AccDescrMultiline => T::AccDescrMultilineValue,
                S::Href => T::Href,
                S::Callbackname => T::Callbackname,
                S::Callbackargs => T::Callbackargs,
                S::Click => T::Click,
            };
            emit(&mut toks, kind, n, n);
            break;
        }
        let (c, size) = unit_at(src, p);

        match state {
            S::Initial => {
                // The code unit after c. A character above the Basic Multilingual Plane is
                // followed by its low surrogate, which is no character looked for here.
                let d = if size < 4 { unit(src, p + size) } else { -1 };
                if c == 37 {
                    if d != 37 {
                        let mut e = until(b'\n', p + 1);
                        while e < n && b[e] == b'\n' {
                            e += 1;
                        }
                        p = e;
                    } else if unit(src, p + 2) == 123 {
                        // The grammar enters a lexer state it never defines, so nothing can
                        // follow.
                        p = emit(&mut toks, T::OpenDirective, p, p + 3);
                        emit(&mut toks, T::Error, p, p);
                        toks.push(Tok {
                            kind: T::End,
                            text: "",
                            start: n,
                        });
                        return toks;
                    } else {
                        p = until(b'\n', p + 2);
                    }
                } else if c == 10 {
                    let mut e = p + 1;
                    while e < n && b[e] == b'\n' {
                        e += 1;
                    }
                    p = emit(&mut toks, T::Nl, p, e);
                } else if is_space(c) {
                    p = space_run(src, p);
                } else if keyword(&mut toks, &mut p, &mut state, c) {
                } else if (48..=57).contains(&c) && match_date(src, p) {
                    p = emit(&mut toks, T::Date, p, p + 10);
                } else if c != 58 {
                    let mut e = p + 1;
                    while e < n && b[e] != b':' && b[e] != b'\n' {
                        e += 1;
                    }
                    p = emit(&mut toks, T::TaskTxt, p, e);
                } else {
                    let mut e = p + 1;
                    while e < n && b[e] != b'#' && b[e] != b'\n' && b[e] != b';' {
                        e += 1;
                    }
                    let kind = if e > p + 1 { T::TaskData } else { T::Colon };
                    p = emit(&mut toks, kind, p, e);
                }
            }
            S::AccTitle | S::AccDescr => {
                let kind = if state == S::AccTitle {
                    T::AccTitleValue
                } else {
                    T::AccDescrValue
                };
                state = S::Initial;
                p = emit(&mut toks, kind, p, until(b'\n', p));
            }
            S::AccDescrMultiline => {
                if c == 125 {
                    state = S::Initial;
                    p += 1;
                } else {
                    p = emit(&mut toks, T::AccDescrMultilineValue, p, until(b'}', p));
                }
            }
            S::Href => {
                if c == 34 {
                    state = S::Initial;
                    p += 1;
                } else {
                    p = emit(&mut toks, T::Href, p, until(b'"', p));
                }
            }
            S::Callbackname => {
                if c == 40 {
                    let e = space_run(src, p + 1);
                    if b.get(e) == Some(&b')') {
                        state = S::Initial;
                        p = e + 1;
                    } else {
                        state = S::Callbackargs;
                        p += 1;
                    }
                } else {
                    p = emit(&mut toks, T::Callbackname, p, until(b'(', p));
                }
            }
            S::Callbackargs => {
                if c == 41 {
                    state = S::Initial;
                    p += 1;
                } else {
                    p = emit(&mut toks, T::Callbackargs, p, until(b')', p));
                }
            }
            S::Click => {
                if is_space(c) {
                    state = S::Initial;
                    p += size;
                } else {
                    let mut e = p + size;
                    loop {
                        let (c, size) = unit_at(src, e);
                        if size == 0 || is_space(c) {
                            break;
                        }
                        e += size;
                    }
                    p = emit(&mut toks, T::Click, p, e);
                }
            }
        }
    }
    toks.push(Tok {
        kind: T::End,
        text: "",
        start: n,
    });
    toks
}
