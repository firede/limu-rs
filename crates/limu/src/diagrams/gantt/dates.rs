//! The parts of dayjs 1.11 that Mermaid's gantt code relies on, with the plugins it loads
//! (custom parse format, advanced format, ISO week) and the English locale, as Pele's
//! dates.ts has them: strict parsing against a format, formatting, and calendar arithmetic,
//! all in local time. Quirks are kept, because a date is accepted only when formatting the
//! parsed result gives back the input.
//!
//! A date is a JavaScript time value (see [`crate::js::date`]), and local time is the zone's.

use crate::js::chars::{is_space, is_space_char, len16};
use crate::js::date::{Zone, time_clip, utc, valid};
use crate::js::math::round;
use crate::js::number::{number, to_string};

pub(super) const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

pub(super) const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

pub(super) const DAY: f64 = 86400000.0;
pub(super) const WEEK: f64 = 7.0 * DAY;

/// A unit of a duration, Pele's Unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Unit {
    Ms,
    S,
    M,
    H,
    D,
    W,
    Month,
    Y,
}

/// String() of a number, as Pele's dates.ts and axis.ts call it.
pub(super) fn num_str(v: f64) -> String {
    to_string(v)
}

/// Pele's pad: String(value), with zeros before it to length.
fn pad(value: f64, length: usize) -> String {
    let s = num_str(value);
    let n = len16(&s);
    if n >= length {
        s
    } else {
        "0".repeat(length - n) + &s
    }
}

/// Pele's ordinal: n with its English ordinal suffix, as Do writes a day.
fn ordinal(n: f64) -> String {
    const S: [&str; 4] = ["th", "st", "nd", "rd"];
    let v = n % 100.0;
    // An index of the suffixes is one only where it is a whole number from 0 to 3.
    let at = |k: f64| (k.fract() == 0.0 && (0.0..4.0).contains(&k)).then(|| S[k as usize]);
    let suffix = at((v - 20.0) % 10.0).or_else(|| at(v)).unwrap_or(S[0]);
    num_str(n) + suffix
}

/// Pele's utcOffset: minutes ahead of UTC, rounded to a quarter of an hour.
fn utc_offset(z: Zone, date: f64) -> f64 {
    let offset = -round(z.timezone_offset(date) / 15.0) * 15.0;
    if offset == 0.0 { 0.0 } else { offset }
}

/// Pele's zone: the UTC offset as +hh:mm.
fn zone(z: Zone, date: f64) -> String {
    let offset = utc_offset(z, date);
    let minutes = offset.abs();
    let sign = if offset >= 0.0 { "+" } else { "-" };
    format!(
        "{sign}{}:{}",
        pad((minutes / 60.0).floor(), 2),
        pad(minutes % 60.0, 2)
    )
}

/// `x || or`: or in place of 0 or NaN, as Pele writes `|| 12`, `|| 7` and `|| 24`.
pub(super) fn or_zero(x: f64, or: f64) -> f64 {
    if x == 0.0 || x.is_nan() { or } else { x }
}

/// Pele's isoWeekday: `getDay() || 7`.
pub(super) fn iso_weekday(z: Zone, date: f64) -> f64 {
    or_zero(z.day(date), 7.0)
}

/// Pele's addDays: the date days later on the calendar, at the same time of day.
pub(super) fn add_days(z: Zone, date: f64, days: f64) -> f64 {
    z.set_date(date, z.date(date) + days)
}

/// Pele's daysInMonth: the number of days in the month of date.
fn days_in_month(z: Zone, date: f64) -> f64 {
    let last = z.local(
        z.full_year(date),
        z.month(date) + 1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
    );
    let last = z.set_hours(last, 23.0, 59.0, 59.0, 999.0);
    z.date(last)
}

/// Pele's setCalendar: it sets the month or the year with apply, keeping the day of the
/// month unless the new month is shorter.
fn set_calendar(z: Zone, date: f64, apply: impl FnOnce(f64) -> f64) -> f64 {
    let next = apply(z.set_date(date, 1.0));
    z.set_date(next, z.date(date).min(days_in_month(z, next)))
}

/// Pele's add: the date amount units later, months and years by the calendar, days and weeks
/// rounded to whole days, and smaller units by the clock.
pub(super) fn add(z: Zone, date: f64, amount: f64, unit: Unit) -> f64 {
    match unit {
        Unit::Month => set_calendar(z, date, |d| z.set_month(d, z.month(date) + amount)),
        Unit::Y => set_calendar(z, date, |d| z.set_full_year(d, z.full_year(date) + amount)),
        Unit::D => add_days(z, date, round(amount)),
        Unit::W => add_days(z, date, round(7.0 * amount)),
        Unit::H => time_clip(date + amount * 3600000.0),
        Unit::M => time_clip(date + amount * 60000.0),
        Unit::S => time_clip(date + amount * 1000.0),
        Unit::Ms => time_clip(date + amount),
    }
}

/// Pele's weekThursday: the Thursday of the ISO week date is in.
fn week_thursday(z: Zone, date: f64) -> f64 {
    add_days(z, date, 4.0 - iso_weekday(z, date))
}

/// Pele's isoWeek: the number of the ISO week date is in.
fn iso_week(z: Zone, date: f64) -> f64 {
    let thursday = week_thursday(z, date);
    let jan1 = z.local(z.full_year(thursday), 0.0, 1.0, 0.0, 0.0, 0.0, 0.0);
    let weekday = iso_weekday(z, jan1);
    let extra = if weekday > 4.0 { 7.0 } else { 0.0 };
    let first = add_days(z, jan1, 4.0 - weekday + extra);
    let zone_delta = (utc_offset(z, first) - utc_offset(z, thursday)) * 60000.0;
    let weeks = (thursday - first - zone_delta) / WEEK;
    if weeks < 0.0 {
        or_zero(weeks.ceil(), 0.0) + 1.0
    } else {
        weeks.floor() + 1.0
    }
}

const DEFAULT_FORMAT: &str = "YYYY-MM-DDTHH:mm:ssZ";

/// The length of `\[([^\]]+)]` at byte i of s, or 0: the first alternative of Pele's
/// RE_ADVANCED and RE_FORMAT, and its RE_LITERAL.
fn bracket_at(s: &str, i: usize) -> usize {
    if s.as_bytes()[i] != b'[' {
        return 0;
    }
    match s[i + 1..].find(']') {
        Some(j) if j > 0 => j + 2,
        _ => 0,
    }
}

/// The number of bytes c at byte i of s, up to most.
fn run(s: &str, i: usize, c: u8, most: usize) -> usize {
    s.as_bytes()[i..]
        .iter()
        .take(most)
        .take_while(|&&b| b == c)
        .count()
}

/// The length of a match of
/// `/\[([^\]]+)]|Q|wo|ww|w|WW|W|zzz|z|gggg|GGGG|Do|X|x|k{1,2}|S/` at byte i of s, or 0. It is
/// Pele's RE_ADVANCED, written out by hand.
fn advanced_at(s: &str, i: usize) -> usize {
    let n = bracket_at(s, i);
    if n > 0 {
        return n;
    }
    let rest = &s[i..];
    for alt in [
        "Q", "wo", "ww", "w", "WW", "W", "zzz", "z", "gggg", "GGGG", "Do", "X", "x",
    ] {
        if rest.starts_with(alt) {
            return alt.len();
        }
    }
    match rest.as_bytes()[0] {
        b'k' => run(s, i, b'k', 2),
        b'S' => 1,
        _ => 0,
    }
}

/// The length of a match of
/// `/\[([^\]]+)]|YYYY|YY|M{1,4}|D{1,2}|d{1,4}|H{1,2}|h{1,2}|a|A|m{1,2}|s{1,2}|Z{1,2}|SSS/`
/// at byte i of s, or 0. It is Pele's RE_FORMAT, written out by hand.
fn format_at(s: &str, i: usize) -> usize {
    let n = bracket_at(s, i);
    if n > 0 {
        return n;
    }
    match s.as_bytes()[i] {
        b'Y' => match run(s, i, b'Y', 4) {
            4 => 4,
            2 | 3 => 2,
            _ => 0,
        },
        c @ (b'M' | b'd') => run(s, i, c, 4),
        c @ (b'D' | b'H' | b'h' | b'm' | b's' | b'Z') => run(s, i, c, 2),
        b'a' | b'A' => 1,
        b'S' if run(s, i, b'S', 3) == 3 => 3,
        _ => 0,
    }
}

/// The length in bytes of the character at byte i of s, where a search that matches nothing
/// there moves on.
fn next_char(s: &str, i: usize) -> usize {
    s[i..].chars().next().map_or(1, char::len_utf8)
}

/// A global replace with a regular expression whose matches at finds: each match, leftmost
/// first, is replaced with what repl gives for it. It is String.prototype.replace with a
/// global regular expression and a function, as Pele's format and compile call it.
fn replace_all(
    s: &str,
    at: fn(&str, usize) -> usize,
    mut repl: impl FnMut(&str) -> String,
) -> String {
    let mut out = String::with_capacity(s.len());
    let mut from = 0;
    // A replacement depends on the match alone, so a run of one token, as a long format may
    // hold, is replaced once.
    let (mut last, mut last_out) = ("", String::new());
    let mut i = 0;
    while i < s.len() {
        let n = at(s, i);
        if n == 0 {
            i += next_char(s, i);
            continue;
        }
        out.push_str(&s[from..i]);
        let m = &s[i..i + n];
        if m != last {
            last = m;
            last_out = repl(m);
        }
        out.push_str(&last_out);
        i += n;
        from = i;
    }
    out.push_str(&s[from..]);
    out
}

/// Pele's format. It gives none for the week-of-year and time zone name tokens, which need
/// dayjs plugins Mermaid does not load and so fail there.
pub(super) fn format(z: Zone, date: f64, template: &str) -> Option<String> {
    if !valid(date) {
        return Some("Invalid Date".into());
    }
    let s = if template.is_empty() {
        DEFAULT_FORMAT
    } else {
        template
    };
    if s == "YYYY-MM-DD" {
        return Some(iso_date(z, date));
    }
    let mut unsupported = false;
    let advanced = replace_all(s, advanced_at, |m| match m {
        "Q" => num_str(((z.month(date) + 1.0) / 3.0).ceil()),
        "Do" => format!("[{}]", ordinal(z.date(date))),
        "GGGG" => num_str(z.full_year(week_thursday(z, date))),
        "W" => num_str(iso_week(z, date)),
        "WW" => pad(iso_week(z, date), 2),
        "k" => num_str(or_zero(z.hours(date), 24.0)),
        "kk" => pad(or_zero(z.hours(date), 24.0), 2),
        "X" => num_str((date / 1000.0).floor()),
        "x" => num_str(date),
        "gggg" | "wo" | "w" | "ww" | "z" | "zzz" => {
            unsupported = true;
            m.to_string()
        }
        _ => m.to_string(),
    });
    if unsupported {
        return None;
    }
    let hours = z.hours(date);
    let month = z.month(date) as usize;
    let weekday = z.day(date) as usize;
    Some(replace_all(&advanced, format_at, |m| {
        if let Some(literal) = m.strip_prefix('[') {
            return literal[..literal.len() - 1].to_string();
        }
        match m {
            "YY" => {
                let s = num_str(z.full_year(date));
                s[s.len().saturating_sub(2)..].to_string()
            }
            "YYYY" => pad(z.full_year(date), 4),
            "M" => num_str(z.month(date) + 1.0),
            "MM" => pad(z.month(date) + 1.0, 2),
            "MMM" => MONTHS[month][..3].to_string(),
            "MMMM" => MONTHS[month].to_string(),
            "D" => num_str(z.date(date)),
            "DD" => pad(z.date(date), 2),
            "d" => num_str(z.day(date)),
            "dd" => WEEKDAYS[weekday][..2].to_string(),
            "ddd" => WEEKDAYS[weekday][..3].to_string(),
            "dddd" => WEEKDAYS[weekday].to_string(),
            "H" => num_str(hours),
            "HH" => pad(hours, 2),
            "h" => num_str(or_zero(hours % 12.0, 12.0)),
            "hh" => pad(or_zero(hours % 12.0, 12.0), 2),
            "a" => (if hours < 12.0 { "am" } else { "pm" }).into(),
            "A" => (if hours < 12.0 { "AM" } else { "PM" }).into(),
            "m" => num_str(z.minutes(date)),
            "mm" => pad(z.minutes(date), 2),
            "s" => num_str(z.seconds(date)),
            "ss" => pad(z.seconds(date), 2),
            "SSS" => pad(z.milliseconds(date), 3),
            "Z" => zone(z, date),
            _ => zone(z, date).replacen(':', "", 1),
        }
    }))
}

/// Pele's isoDate: the date as YYYY-MM-DD.
pub(super) fn iso_date(z: Zone, date: f64) -> String {
    let (year, month, day) = (z.full_year(date), z.month(date) + 1.0, z.date(date));
    let year = if (1000.0..=9999.0).contains(&year) {
        num_str(year)
    } else {
        pad(year, 4)
    };
    let sep = |v: f64| if v < 10.0 { "-0" } else { "-" };
    format!(
        "{year}{}{}{}{}",
        sep(month),
        num_str(month),
        sep(day),
        num_str(day)
    )
}

/// A number a format may leave undefined, as a field of Pele's Fields.
type Opt = Option<f64>;

/// Whether a field is true in JavaScript: set, and neither 0 nor NaN, as Pele's loose tests
/// one.
fn truthy(o: Opt) -> bool {
    o.is_some_and(|v| v != 0.0 && !v.is_nan())
}

/// The field `|| d`, as Pele's loose defaults one.
fn or(o: Opt, d: f64) -> f64 {
    if truthy(o) { o.unwrap_or(d) } else { d }
}

/// What a format reads of a date, Pele's Fields.
#[derive(Default)]
struct Fields {
    year: Opt,
    month: Opt,
    day: Opt,
    day_given: bool,
    hours: Opt,
    minutes: Opt,
    seconds: Opt,
    milliseconds: Opt,
    week: Opt,
    offset: Opt,
    afternoon: Option<bool>,
}

fn is_digit16(c: u16) -> bool {
    (u16::from(b'0')..=u16::from(b'9')).contains(&c)
}

/// The first match of a pattern, which matches at i with the length at gives, as exec finds
/// one with a pattern without the y or g flag, as Pele's loose executes its MATCH_ patterns.
fn search(s: &[u16], at: impl Fn(&[u16], usize) -> Option<usize>) -> Option<&[u16]> {
    (0..s.len()).find_map(|i| at(s, i).map(|n| &s[i..i + n]))
}

/// Pele's MATCH_ patterns, each written out.
#[derive(Clone, Copy)]
enum Match {
    /// `\d{n}`: MATCH_1, MATCH_2, MATCH_3 and MATCH_4.
    Digits(usize),
    /// `\d\d?`, MATCH_1_TO_2.
    OneOrTwo,
    /// `[+-]?\d+`, MATCH_SIGNED.
    Signed,
    /// `[+-]\d\d:?(\d\d)?|Z`, MATCH_OFFSET.
    Offset,
    /// `\d*[^-_:/,()\s\d]+`, MATCH_WORD.
    Word,
}

/// Whether c is in `[^-_:/,()\s\d]`, the class of Pele's MATCH_WORD.
fn is_word_unit(c: u16) -> bool {
    !matches!(c, 45 | 95 | 58 | 47 | 44 | 40 | 41) && !is_digit16(c) && !is_space(i32::from(c))
}

impl Match {
    fn exec(self, s: &[u16]) -> Option<&[u16]> {
        let digit = |s: &[u16], i: usize| s.get(i).is_some_and(|&c| is_digit16(c));
        match self {
            Match::Digits(n) => search(s, |s, i| (0..n).all(|k| digit(s, i + k)).then_some(n)),
            Match::OneOrTwo => search(s, |s, i| {
                digit(s, i).then(|| if digit(s, i + 1) { 2 } else { 1 })
            }),
            Match::Signed => search(s, |s, i| {
                let j = i + usize::from(s[i] == u16::from(b'+') || s[i] == u16::from(b'-'));
                let k = j + s[j..].iter().take_while(|&&c| is_digit16(c)).count();
                (k > j).then_some(k - i)
            }),
            Match::Offset => search(s, |s, i| {
                if (s[i] == u16::from(b'+') || s[i] == u16::from(b'-'))
                    && digit(s, i + 1)
                    && digit(s, i + 2)
                {
                    let mut j = i + 3;
                    if s.get(j) == Some(&u16::from(b':')) {
                        j += 1;
                    }
                    if digit(s, j) && digit(s, j + 1) {
                        j += 2;
                    }
                    return Some(j - i);
                }
                (s[i] == u16::from(b'Z')).then_some(1)
            }),
            Match::Word => search(s, |s, i| {
                let j = i + s[i..].iter().take_while(|&&c| is_digit16(c)).count();
                let k = j + s[j..].iter().take_while(|&&c| is_word_unit(c)).count();
                (k > j).then_some(k - i)
            }),
        }
    }
}

/// What a token of a format reads, the readers of Pele's READERS.
#[derive(Clone, Copy)]
enum Read {
    Afternoon(&'static str),
    Quarter,
    Fraction(f64),
    Seconds,
    Minutes,
    Hours,
    Day,
    Ordinal,
    Week,
    Month,
    MonthName(bool),
    Year,
    ShortYear,
    Offset,
}

/// A step of a compiled format: a literal, or a token to read.
#[derive(Clone)]
enum Step {
    Literal(String),
    Token(Match, Read),
}

/// The reader of a token, as Pele's READERS has it.
fn reader(token: &str) -> Option<(Match, Read)> {
    use Match::*;
    Some(match token {
        "A" => (Word, Read::Afternoon("PM")),
        "a" => (Word, Read::Afternoon("pm")),
        "Q" => (Digits(1), Read::Quarter),
        "S" => (Digits(1), Read::Fraction(100.0)),
        "SS" => (Digits(2), Read::Fraction(10.0)),
        "SSS" => (Digits(3), Read::Fraction(1.0)),
        "s" | "ss" => (OneOrTwo, Read::Seconds),
        "m" | "mm" => (OneOrTwo, Read::Minutes),
        "H" | "h" | "HH" | "hh" => (OneOrTwo, Read::Hours),
        "D" => (OneOrTwo, Read::Day),
        "DD" => (Digits(2), Read::Day),
        "Do" => (Word, Read::Ordinal),
        "w" => (OneOrTwo, Read::Week),
        "ww" => (Digits(2), Read::Week),
        "M" => (OneOrTwo, Read::Month),
        "MM" => (Digits(2), Read::Month),
        "MMM" => (Word, Read::MonthName(true)),
        "MMMM" => (Word, Read::MonthName(false)),
        "Y" => (Signed, Read::Year),
        "YY" => (Digits(2), Read::ShortYear),
        "YYYY" => (Digits(4), Read::Year),
        "Z" | "ZZ" => (Offset, Read::Offset),
        _ => return None,
    })
}

/// Pele's readOffset: it reads a UTC offset, Z or [+-]hh[:mm], as the minutes to add to the
/// time to reach UTC.
fn read_offset(f: &mut Fields, input: &str) {
    if input == "Z" {
        f.offset = Some(0.0);
        return;
    }
    // input.match(/([+-]|\d\d)/g)
    let b = input.as_bytes();
    let mut parts = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'+' || b[i] == b'-' {
            parts.push(&input[i..i + 1]);
            i += 1;
        } else if i + 1 < b.len() && b[i].is_ascii_digit() && b[i + 1].is_ascii_digit() {
            parts.push(&input[i..i + 2]);
            i += 2;
        } else {
            i += 1;
        }
    }
    let second = parts.get(2).map_or(0.0, |p| or_zero(number(p), 0.0));
    let minutes = number(parts[1]) * 60.0 + second;
    f.offset = Some(if minutes == 0.0 {
        0.0
    } else if parts[0] == "+" {
        -minutes
    } else {
        minutes
    });
}

impl Read {
    /// Stores what the token matched in the fields, or says it cannot.
    fn read(self, f: &mut Fields, input: &str) -> bool {
        match self {
            Read::Afternoon(pm) => f.afternoon = Some(input == pm),
            Read::Quarter => f.month = Some((number(input) - 1.0) * 3.0 + 1.0),
            Read::Fraction(scale) => f.milliseconds = Some(number(input) * scale),
            Read::Seconds => f.seconds = Some(number(input)),
            Read::Minutes => f.minutes = Some(number(input)),
            Read::Hours => f.hours = Some(number(input)),
            Read::Day => f.day = Some(number(input)),
            Read::Week => f.week = Some(number(input)),
            Read::Month => f.month = Some(number(input)),
            Read::Year => f.year = Some(number(input)),
            Read::ShortYear => {
                let v = number(input);
                f.year = Some(v + if v > 68.0 { 1900.0 } else { 2000.0 });
            }
            Read::Offset => read_offset(f, input),
            Read::Ordinal => {
                // dayjs keeps the digits as a string here, so even "0" counts as a day having
                // been given.
                let Some(i) = input.find(|c: char| c.is_ascii_digit()) else {
                    return false;
                };
                let j = i + input[i..].bytes().take_while(u8::is_ascii_digit).count();
                f.day = Some(number(&input[i..j]));
                f.day_given = true;
                for k in 1..=31 {
                    if ordinal(f64::from(k)) == input {
                        f.day = Some(f64::from(k));
                    }
                }
            }
            Read::MonthName(short) => {
                let index = MONTHS
                    .iter()
                    .position(|m| if short { &m[..3] == input } else { *m == input });
                match index {
                    Some(k) => f.month = Some((k + 1) as f64),
                    None => return false,
                }
            }
        }
        true
    }
}

/// The length of a match of Pele's RE_TOKENS,
/// `/(\[[^[]*\])|([-_:/.,()\s]+)|(A|a|Q|YYYY|YY?|ww?|MM?M?M?|Do|DD?|hh?|HH?|mm?|ss?|S{1,3}|z|ZZ?)/`,
/// at byte i of s, or 0.
fn token_at(s: &str, i: usize) -> usize {
    let b = s.as_bytes();
    if b[i] == b'[' {
        let end = s[i + 1..].find('[').map_or(s.len(), |e| i + 1 + e);
        if let Some(k) = s[i + 1..end].rfind(']') {
            return k + 2;
        }
    }
    let seps = s[i..]
        .chars()
        .take_while(|&c| "-_:/.,()".contains(c) || is_space_char(c))
        .map(char::len_utf8)
        .sum::<usize>();
    if seps > 0 {
        return seps;
    }
    match b[i] {
        b'A' | b'a' | b'Q' | b'z' => 1,
        b'Y' => {
            if run(s, i, b'Y', 4) == 4 {
                4
            } else {
                run(s, i, b'Y', 2)
            }
        }
        c @ (b'w' | b'h' | b'H' | b'm' | b's' | b'Z') => run(s, i, c, 2),
        b'M' => run(s, i, b'M', 4),
        b'D' => {
            if b.get(i + 1) == Some(&b'o') {
                2
            } else {
                run(s, i, b'D', 2)
            }
        }
        b'S' => run(s, i, b'S', 3),
        _ => 0,
    }
}

/// Pele's compile: the steps of a format, or none where dayjs cannot parse with it.
fn compile(template: &str) -> Option<Vec<Step>> {
    // dayjs expands localized tokens through a locale table the English locale does not have,
    // and fails.
    let stripped = replace_all(template, bracket_at, |_| String::new());
    if stripped.contains(['L', 'l']) {
        return None;
    }
    let mut steps = Vec::new();
    let mut i = 0;
    while i < template.len() {
        let n = token_at(template, i);
        if n == 0 {
            i += next_char(template, i);
            continue;
        }
        let token = &template[i..i + n];
        i += n;
        steps.push(match reader(token) {
            Some((m, r)) => Step::Token(m, r),
            None => {
                let t = token.strip_prefix('[').unwrap_or(token);
                Step::Literal(t.strip_suffix(']').unwrap_or(t).to_string())
            }
        });
    }
    (!steps.is_empty()).then_some(steps)
}

/// Pele's loose: the date input reads as in template, unchecked.
fn loose(z: Zone, input: &str, template: &str, now: f64) -> Option<f64> {
    if template == "x" || template == "X" {
        let scale = if template == "X" { 1000.0 } else { 1.0 };
        return Some(time_clip(scale * number(input)));
    }
    let steps = compile(template)?;
    let mut f = Fields::default();
    let mut rest: Vec<u16> = input.encode_utf16().collect();
    let mut start = 0;
    for step in &steps {
        let (m, r) = match step {
            Step::Literal(l) => {
                start += len16(l);
                continue;
            }
            Step::Token(m, r) => (*m, *r),
        };
        let from = start.min(rest.len());
        let matched = m.exec(&rest[from..])?.to_vec();
        if !r.read(&mut f, &String::from_utf16_lossy(&matched)) {
            return None;
        }
        // dayjs drops the first occurrence of the matched text, wherever it is, and does not
        // move start.
        if let Some(k) = rest.windows(matched.len()).position(|w| w == matched) {
            rest.drain(k..k + matched.len());
        }
    }
    if let (Some(pm), Some(h)) = (f.afternoon, f.hours) {
        if pm {
            if h < 12.0 {
                f.hours = Some(h + 12.0);
            }
        } else if h == 12.0 {
            f.hours = Some(0.0);
        }
    }
    let day = if f.day_given || truthy(f.day) {
        f.day.unwrap_or(f64::NAN)
    } else if !truthy(f.year) && !truthy(f.month) {
        z.date(now)
    } else {
        1.0
    };
    let year = or(f.year, z.full_year(now));
    let mut month = 0.0;
    if !(truthy(f.year) && !truthy(f.month)) {
        month = match f.month {
            Some(m) if m > 0.0 => m - 1.0,
            _ => z.month(now),
        };
    }
    let hours = or(f.hours, 0.0);
    let minutes = or(f.minutes, 0.0);
    let seconds = or(f.seconds, 0.0);
    let ms = or(f.milliseconds, 0.0);
    if let Some(offset) = f.offset {
        let ms = ms + offset * 60000.0;
        return Some(time_clip(utc(
            year, month, day, hours, minutes, seconds, ms,
        )));
    }
    // Applying a week number needs a dayjs plugin Mermaid does not load.
    if truthy(f.week) {
        return None;
    }
    Some(z.local(year, month, day, hours, minutes, seconds, ms))
}

/// Parses the way `dayjs(input, format, true)` does, as Pele's parseStrict does. now supplies
/// the fields the format leaves out.
pub(super) fn parse_strict(z: Zone, input: &str, template: &str, now: f64) -> Option<f64> {
    let date = loose(z, input, template, now)?;
    (valid(date) && format(z, date, template).as_deref() == Some(input)).then_some(date)
}
