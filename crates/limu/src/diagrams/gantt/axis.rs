//! Time axis support, as Pele's axis.ts has it, following d3-time and d3-time-format
//! (English locale, local time), which Mermaid uses: the axisFormat specifiers, tick
//! intervals with d3's every rule, and its choice of automatic ticks.

use super::dates::{DAY, MONTHS, WEEK, WEEKDAYS, num_str, or_zero};
use crate::js::date::{Zone, set_time, time_clip, utc_seconds};
use crate::js::math::{log10, max, pow10};

const SECOND: f64 = 1000.0;
const MINUTE: f64 = 60.0 * SECOND;
const HOUR: f64 = 60.0 * MINUTE;
const MONTH: f64 = 30.0 * DAY;
const YEAR: f64 = 365.0 * DAY;

/// A unit of ticks, Pele's TickUnit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TickUnit {
    Millisecond,
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

/// An interval of time, Pele's Calendar: a unit with the day weeks start on, 0 for Sunday,
/// taken every step, as d3's `unit.every(step)` gives it. The floor and the offset of a date
/// give the date they leave.
#[derive(Clone, Copy)]
struct Calendar {
    unit: TickUnit,
    /// The step, 1 for every unit.
    step: f64,
    week_start: f64,
    /// The start of the week the epoch is in, from which weeks are counted by the step.
    epoch: f64,
}

/// Pele's startOfDay.
fn start_of_day(z: Zone, d: f64) -> f64 {
    z.set_hours(d, 0.0, 0.0, 0.0, 0.0)
}

impl Calendar {
    /// A unit taken every time, Pele's CALENDARS and weekCalendar.
    fn base(unit: TickUnit, week_start: f64) -> Calendar {
        Calendar {
            unit,
            step: 1.0,
            week_start,
            epoch: 0.0,
        }
    }

    /// How many values the unit's field cycles through, which bounds how far apart filtered
    /// ticks can be.
    fn cycle(&self) -> f64 {
        match self.unit {
            TickUnit::Second | TickUnit::Minute => 60.0,
            TickUnit::Hour => 24.0,
            TickUnit::Day => 31.0,
            TickUnit::Month => 12.0,
            _ => f64::INFINITY,
        }
    }

    /// The unit's length in milliseconds, months and years as d3 reckons them.
    fn ms(&self) -> f64 {
        match self.unit {
            TickUnit::Millisecond => 1.0,
            TickUnit::Second => SECOND,
            TickUnit::Minute => MINUTE,
            TickUnit::Hour => HOUR,
            TickUnit::Day => DAY,
            TickUnit::Week => WEEK,
            TickUnit::Month => MONTH,
            TickUnit::Year => YEAR,
        }
    }

    /// The value d3 tests against a step: a tick falls where it is a multiple of the step.
    fn field(&self, z: Zone, d: f64) -> f64 {
        match self.unit {
            TickUnit::Second => utc_seconds(d),
            TickUnit::Minute => z.minutes(d),
            TickUnit::Hour => z.hours(d),
            TickUnit::Day => z.date(d) - 1.0,
            _ => z.month(d),
        }
    }

    /// The unit's floor, every time.
    fn base_floor(&self, z: Zone, d: f64) -> f64 {
        match self.unit {
            TickUnit::Millisecond => d,
            TickUnit::Second => set_time(d - z.milliseconds(d)),
            TickUnit::Minute => set_time(d - z.milliseconds(d) - z.seconds(d) * SECOND),
            TickUnit::Hour => {
                set_time(d - z.milliseconds(d) - z.seconds(d) * SECOND - z.minutes(d) * MINUTE)
            }
            TickUnit::Day => start_of_day(z, d),
            TickUnit::Week => {
                let back = (z.day(d) + 7.0 - self.week_start) % 7.0;
                start_of_day(z, z.set_date(d, z.date(d) - back))
            }
            TickUnit::Month => start_of_day(z, z.set_date(d, 1.0)),
            TickUnit::Year => start_of_day(z, z.set_month_date(d, 0.0, 1.0)),
        }
    }

    /// The unit's offset, every time.
    fn base_offset(&self, z: Zone, d: f64, step: f64) -> f64 {
        match self.unit {
            TickUnit::Millisecond => set_time(d + step),
            TickUnit::Second => set_time(d + step * SECOND),
            TickUnit::Minute => set_time(d + step * MINUTE),
            TickUnit::Hour => set_time(d + step * HOUR),
            TickUnit::Day => z.set_date(d, z.date(d) + step),
            TickUnit::Week => z.set_date(d, z.date(d) + step * 7.0),
            TickUnit::Month => z.set_month(d, z.month(d) + step),
            TickUnit::Year => z.set_full_year(d, z.full_year(d) + step),
        }
    }

    fn test(&self, z: Zone, d: f64) -> bool {
        self.field(z, d) % self.step == 0.0
    }

    fn floor(&self, z: Zone, d: f64) -> f64 {
        let step = self.step;
        if step <= 1.0 {
            return self.base_floor(z, d);
        }
        match self.unit {
            TickUnit::Millisecond => set_time((d / step).floor() * step),
            TickUnit::Year => {
                let d = z.set_full_year(d, (z.full_year(d) / step).floor() * step);
                self.base_floor(z, d)
            }
            TickUnit::Week => {
                let d = self.base_floor(z, d);
                self.base_offset(z, d, -modulo(span(z, self.epoch, d, WEEK), step))
            }
            _ => {
                // Date overflow makes the field NaN, which can never match the interval.
                let mut d = self.base_floor(z, d);
                while d.is_finite() && !self.test(z, d) {
                    d = self.base_floor(z, set_time(d - 1.0));
                }
                d
            }
        }
    }

    fn offset(&self, z: Zone, mut d: f64, n: f64) -> f64 {
        let step = self.step;
        if step <= 1.0 {
            return self.base_offset(z, d, n);
        }
        match self.unit {
            TickUnit::Millisecond => set_time(d + n * step),
            TickUnit::Year => z.set_full_year(d, z.full_year(d) + n * step),
            TickUnit::Week => self.base_offset(z, d, n * step),
            _ => {
                let mut i = 0.0;
                while i < n && d.is_finite() {
                    loop {
                        d = self.base_offset(z, d, 1.0);
                        if !(d.is_finite() && !self.test(z, d)) {
                            break;
                        }
                    }
                    i += 1.0;
                }
                d
            }
        }
    }
}

/// Pele's span: whole units between two floored dates, discounting a change of UTC offset
/// between them.
fn span(z: Zone, start: f64, end: f64, unit: f64) -> f64 {
    ((end - start - (z.timezone_offset(end) - z.timezone_offset(start)) * MINUTE) / unit).floor()
}

/// Pele's mod: a modulo b, taken to be at least 0.
fn modulo(a: f64, b: f64) -> f64 {
    (a % b + b) % b
}

/// The interval d3's `unit.every(step)` gives, as Pele's every gives it.
fn every(z: Zone, unit: TickUnit, step: f64, week_start: f64) -> Calendar {
    let mut c = Calendar::base(unit, week_start);
    if step <= 1.0 {
        return c;
    }
    c.step = step;
    if unit == TickUnit::Week {
        c.epoch = Calendar::base(unit, week_start).base_floor(z, 0.0);
    }
    c
}

/// Pele's range: ticks from start to stop inclusive, or none where there would be more than
/// limit.
fn range(z: Zone, c: &Calendar, start: f64, stop: f64, limit: f64) -> Option<Vec<f64>> {
    let mut d = time_clip(start - 1.0);
    d = c.floor(z, d);
    d = c.offset(z, d, 1.0);
    d = c.floor(z, d);
    let mut out = Vec::new();
    let mut previous = f64::NEG_INFINITY;
    while d <= stop && d > previous {
        if out.len() as f64 >= limit {
            return None;
        }
        previous = d;
        out.push(previous);
        d = c.offset(z, d, 1.0);
        d = c.floor(z, d);
    }
    Some(out)
}

/// Pele's tickStep.
fn tick_step(start: f64, stop: f64, count: f64) -> f64 {
    let step = (stop - start) / max(0.0, count);
    let power = log10(step).floor();
    let error = step / pow10(power);
    let factor = if error >= 50f64.sqrt() {
        10.0
    } else if error >= 10f64.sqrt() {
        5.0
    } else if error >= std::f64::consts::SQRT_2 {
        2.0
    } else {
        1.0
    };
    pow10(power) * factor
}

const AUTO: [(TickUnit, f64, f64); 18] = [
    (TickUnit::Second, 1.0, SECOND),
    (TickUnit::Second, 5.0, 5.0 * SECOND),
    (TickUnit::Second, 15.0, 15.0 * SECOND),
    (TickUnit::Second, 30.0, 30.0 * SECOND),
    (TickUnit::Minute, 1.0, MINUTE),
    (TickUnit::Minute, 5.0, 5.0 * MINUTE),
    (TickUnit::Minute, 15.0, 15.0 * MINUTE),
    (TickUnit::Minute, 30.0, 30.0 * MINUTE),
    (TickUnit::Hour, 1.0, HOUR),
    (TickUnit::Hour, 3.0, 3.0 * HOUR),
    (TickUnit::Hour, 6.0, 6.0 * HOUR),
    (TickUnit::Hour, 12.0, 12.0 * HOUR),
    (TickUnit::Day, 1.0, DAY),
    (TickUnit::Day, 2.0, 2.0 * DAY),
    (TickUnit::Week, 1.0, WEEK),
    (TickUnit::Month, 1.0, MONTH),
    (TickUnit::Month, 3.0, 3.0 * MONTH),
    (TickUnit::Year, 1.0, YEAR),
];

/// About count ticks at round times, chosen the way d3's time scale chooses them, as Pele's
/// autoTicks does.
pub(super) fn auto_ticks(z: Zone, start: f64, stop: f64, count: f64) -> Vec<f64> {
    let target = (stop - start) / count;
    let mut i = 0;
    while i < AUTO.len() && AUTO[i].2 <= target {
        i += 1;
    }
    let c = if i == AUTO.len() {
        let step = max(1.0, tick_step(start / YEAR, stop / YEAR, count));
        every(z, TickUnit::Year, step, 0.0)
    } else if i == 0 {
        every(
            z,
            TickUnit::Millisecond,
            max(tick_step(start, stop, count), 1.0),
            0.0,
        )
    } else {
        let k = if target / AUTO[i - 1].2 < AUTO[i].2 / target {
            i - 1
        } else {
            i
        };
        every(z, AUTO[k].0, AUTO[k].1, 0.0)
    };
    range(z, &c, start, stop, 4.0 * count + 8.0).unwrap_or_default()
}

const WEEK_STARTS: [&str; 7] = [
    "sunday",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
];

/// A tick interval, as `/^([1-9]\d*)(millisecond|second|minute|hour|day|week|month)$/`, Pele's
/// RE_INTERVAL, reads it: the step and the unit.
fn read_interval(s: &str) -> Option<(f64, TickUnit)> {
    let b = s.as_bytes();
    if !b.first().is_some_and(|c| (b'1'..=b'9').contains(c)) {
        return None;
    }
    let i = 1 + b[1..].iter().take_while(|c| c.is_ascii_digit()).count();
    let unit = match &s[i..] {
        "millisecond" => TickUnit::Millisecond,
        "second" => TickUnit::Second,
        "minute" => TickUnit::Minute,
        "hour" => TickUnit::Hour,
        "day" => TickUnit::Day,
        "week" => TickUnit::Week,
        "month" => TickUnit::Month,
        _ => return None,
    };
    Some((crate::js::number::parse_int(&s[..i], 10), unit))
}

/// Ticks for a tickInterval such as `1week`, or none where it is not one or asks for more
/// than limit, as Pele's intervalTicks gives them.
pub(super) fn interval_ticks(
    z: Zone,
    interval: &str,
    weekday: &str,
    start: f64,
    stop: f64,
    limit: f64,
) -> Option<Vec<f64>> {
    let (step, unit) = read_interval(interval)?;
    let week_start = WEEK_STARTS
        .iter()
        .position(|&w| w == weekday)
        .map_or(0.0, |k| k as f64);
    let c = every(z, unit, step, week_start);
    if (stop - start) / (step.min(c.cycle()) * c.ms()) > limit {
        return None;
    }
    range(z, &c, start, stop, limit)
}

/// Pele's pad of axis.ts: the number's digits filled to width, after its sign.
fn pad(value: f64, fill: &str, width: usize) -> String {
    let (sign, v) = if value < 0.0 {
        ("-", -value)
    } else {
        ("", value)
    };
    let digits = num_str(v);
    if digits.len() < width {
        format!("{sign}{}{digits}", fill.repeat(width - digits.len()))
    } else {
        format!("{sign}{digits}")
    }
}

/// Weeks since the last week_start day before the year began, which is d3's %U and %W, as
/// Pele's weekOfYear counts them.
fn week_of_year(z: Zone, date: f64, week_start: f64) -> f64 {
    let week = Calendar::base(TickUnit::Week, week_start);
    let year_start = Calendar::base(TickUnit::Year, 0.0).floor(z, date);
    span(
        z,
        week.floor(z, time_clip(year_start - 1.0)),
        week.floor(z, date),
        WEEK,
    )
}

/// The Thursday that decides which ISO week and ISO year a date belongs to, Pele's
/// isoThursday.
fn iso_thursday(z: Zone, date: f64) -> f64 {
    let thursday = Calendar::base(TickUnit::Week, 4.0);
    let day = z.day(date);
    let late = day >= 4.0 || day == 0.0;
    let from = if late { date } else { time_clip(date - 1.0) };
    let d = thursday.floor(z, from);
    if late {
        d
    } else {
        thursday.floor(z, thursday.offset(z, d, 1.0))
    }
}

/// Formats a date with a d3 time format specifier such as `%Y-%m-%d`, as Pele's timeFormat
/// does.
pub(super) fn time_format(z: Zone, specifier: &str, date: f64) -> String {
    let mut out = String::new();
    let mut chars = specifier.char_indices().peekable();
    let mut from = 0;
    while let Some((i, c)) = chars.next() {
        if c != '%' {
            continue;
        }
        out.push_str(&specifier[from..i]);
        let mut next = chars.next();
        let fill = match next.map(|(_, c)| c) {
            Some('-') => Some(""),
            Some('_') => Some(" "),
            Some('0') => Some("0"),
            _ => None,
        };
        let fill = match fill {
            Some(fill) => {
                next = chars.next();
                fill
            }
            None if next.is_some_and(|(_, c)| c == 'e') => " ",
            None => "0",
        };
        match next {
            Some((j, c)) => {
                out.push_str(&directive(z, c, date, fill));
                from = j + c.len_utf8();
            }
            None => from = specifier.len(),
        }
    }
    out.push_str(&specifier[from..]);
    out
}

/// What one directive c of a d3 time format writes of date d, p being the fill its numbers
/// are padded with, as Pele's directive writes it.
fn directive(z: Zone, c: char, d: f64, p: &str) -> String {
    let year = || Calendar::base(TickUnit::Year, 0.0);
    match c {
        'a' => WEEKDAYS[z.day(d) as usize][..3].into(),
        'A' => WEEKDAYS[z.day(d) as usize].into(),
        'b' => MONTHS[z.month(d) as usize][..3].into(),
        'B' => MONTHS[z.month(d) as usize].into(),
        'c' => time_format(z, "%x, %X", d),
        'd' | 'e' => pad(z.date(d), p, 2),
        'f' => pad(z.milliseconds(d), p, 3) + "000",
        'g' => pad(z.full_year(iso_thursday(z, d)) % 100.0, p, 2),
        'G' => pad(z.full_year(iso_thursday(z, d)) % 10000.0, p, 4),
        'H' => pad(z.hours(d), p, 2),
        'I' => pad(or_zero(z.hours(d) % 12.0, 12.0), p, 2),
        'j' => {
            let day = Calendar::base(TickUnit::Day, 0.0);
            pad(
                1.0 + span(z, year().floor(z, d), day.floor(z, d), DAY),
                p,
                3,
            )
        }
        'L' => pad(z.milliseconds(d), p, 3),
        'm' => pad(z.month(d) + 1.0, p, 2),
        'M' => pad(z.minutes(d), p, 2),
        'p' => (if z.hours(d) >= 12.0 { "PM" } else { "AM" }).into(),
        'q' => num_str(1.0 + (z.month(d) / 3.0).floor()),
        'Q' => num_str(d),
        's' => num_str((d / 1000.0).floor()),
        'S' => pad(z.seconds(d), p, 2),
        'u' => num_str(or_zero(z.day(d), 7.0)),
        'U' => pad(week_of_year(z, d, 0.0), p, 2),
        'V' => {
            let thursday = iso_thursday(z, d);
            let year_start = year().floor(z, thursday);
            let first = Calendar::base(TickUnit::Week, 4.0).floor(z, year_start);
            let extra = if z.day(year_start) == 4.0 { 1.0 } else { 0.0 };
            pad(span(z, first, thursday, WEEK) + extra, p, 2)
        }
        'w' => num_str(z.day(d)),
        'W' => pad(week_of_year(z, d, 1.0), p, 2),
        'x' => time_format(z, "%-m/%-d/%Y", d),
        'X' => time_format(z, "%-I:%M:%S %p", d),
        'y' => pad(z.full_year(d) % 100.0, p, 2),
        'Y' => pad(z.full_year(d) % 10000.0, p, 4),
        'Z' => {
            let off = z.timezone_offset(d);
            let abs = off.abs();
            let sign = if off > 0.0 { "-" } else { "+" };
            format!(
                "{sign}{}{}",
                pad((abs / 60.0).floor(), "0", 2),
                pad(abs % 60.0, "0", 2)
            )
        }
        '%' => "%".into(),
        _ => c.to_string(),
    }
}
