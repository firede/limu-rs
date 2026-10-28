//! JavaScript's Date as V8 works it out, for the code ported from Pele that computes with dates
//! in local time: the arithmetic of its constructor, getters and setters, Date.UTC, and the
//! parser of `new Date(string)`. Where JavaScript reads the machine's time zone, a [`Zone`]
//! holds the host's [`TimeZone`] instead, so that a host decides which zone a diagram is in.
//!
//! A date is a time value as JavaScript keeps one: an f64 of milliseconds since the epoch, a
//! whole number from -8.64e15 to 8.64e15, or NaN for an invalid date.
//!
//! Local time is converted to UTC as V8 has ICU do it: a local time that a change of offset
//! skips is read with the offset before the change, and one that a change repeats is the
//! earlier of the two instants it names.

mod parse;

use crate::layout::TimeZone;

const MS_PER_SECOND: i64 = 1000;
const MS_PER_MINUTE: i64 = 60000;
const MS_PER_HOUR: i64 = 3600000;
const MS_PER_DAY: i64 = 86400000;

/// The largest time value a date can hold, and its negation the smallest.
pub(crate) const MAX_TIME: f64 = 8.64e15;
/// A local time may be ten days past MAX_TIME before it is converted, as V8 allows.
const MAX_TIME_BEFORE_UTC: f64 = MAX_TIME + 864000000.0;

// The range of years and months V8's MakeDay works with.
const MIN_YEAR: f64 = -1000000.0;
const MAX_YEAR: f64 = 1000000.0;
const MIN_MONTH: f64 = -10000000.0;
const MAX_MONTH: f64 = 10000000.0;

/// Whether t is a date, not NaN: JavaScript's `!isNaN(date.getTime())`.
pub(crate) fn valid(t: f64) -> bool {
    !t.is_nan()
}

/// V8's DoubleToInteger, ToIntegerOrInfinity: NaN is 0, and the rest are cut toward zero,
/// with no negative zero.
fn to_integer(x: f64) -> f64 {
    if x.is_nan() || x == 0.0 {
        return 0.0;
    }
    if x.is_infinite() {
        return x;
    }
    x.trunc() + 0.0
}

/// The spec's TimeClip: a time value in range cut to a whole number, and NaN outside it.
pub(crate) fn time_clip(t: f64) -> f64 {
    if (-MAX_TIME..=MAX_TIME).contains(&t) {
        to_integer(t)
    } else {
        f64::NAN
    }
}

/// The spec's MakeTime, as V8 computes it.
pub(crate) fn make_time(h: f64, m: f64, s: f64, ms: f64) -> f64 {
    if h.is_finite() && m.is_finite() && s.is_finite() && ms.is_finite() {
        to_integer(h) * MS_PER_HOUR as f64
            + to_integer(m) * MS_PER_MINUTE as f64
            + to_integer(s) * MS_PER_SECOND as f64
            + to_integer(ms)
    } else {
        f64::NAN
    }
}

const DAYS_BEFORE_MONTH: [[i64; 12]; 2] = [
    [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334],
    [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335],
];

/// The spec's MakeDay, as V8 computes it: a year or a month past the range V8 works with is
/// NaN.
pub(crate) fn make_day(year: f64, month: f64, date: f64) -> f64 {
    if !((MIN_YEAR..=MAX_YEAR).contains(&year)
        && (MIN_MONTH..=MAX_MONTH).contains(&month)
        && date.is_finite())
    {
        return f64::NAN;
    }
    let mut y = year as i64;
    let mut m = month as i64;
    y += m / 12;
    m %= 12;
    if m < 0 {
        m += 12;
        y -= 1;
    }
    const YEAR_DELTA: i64 = 399999;
    const BASE_DAY: i64 = 365 * (1970 + YEAR_DELTA) + (1970 + YEAR_DELTA) / 4
        - (1970 + YEAR_DELTA) / 100
        + (1970 + YEAR_DELTA) / 400;
    let mut day = 365 * (y + YEAR_DELTA) + (y + YEAR_DELTA) / 4 - (y + YEAR_DELTA) / 100
        + (y + YEAR_DELTA) / 400
        - BASE_DAY;
    let leap = usize::from(y % 4 == 0 && (y % 100 != 0 || y % 400 == 0));
    day += DAYS_BEFORE_MONTH[leap][m as usize];
    (day - 1) as f64 + to_integer(date)
}

/// The spec's MakeDate, as V8 computes it.
pub(crate) fn make_date(day: f64, t: f64) -> f64 {
    if !day.is_finite() || !t.is_finite() {
        return f64::NAN;
    }
    if t == 0.0 && day != 0.0 {
        return day * MS_PER_DAY as f64;
    }
    t + day * MS_PER_DAY as f64
}

/// A time zone dates are read and written in.
#[derive(Clone, Copy)]
pub(crate) struct Zone<'a>(pub(crate) &'a dyn TimeZone);

/// A date broken down in local time.
struct Fields {
    year: i64,
    month: i64,
    date: i64,
    weekday: i64,
    days: i64,
    time_in_day: i64,
}

/// The year, the month from 0 and the day of the month of a number of days since
/// 1970-01-01.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mut m = mp + 3;
    if m > 12 {
        m -= 12;
    }
    if m <= 2 {
        y += 1;
    }
    (y, m - 1, d)
}

impl Zone<'_> {
    /// The zone's offset from UTC, in milliseconds, at the instant t.
    fn offset(self, t: i64) -> i64 {
        i64::from(self.0.offset(t.div_euclid(MS_PER_SECOND))) * MS_PER_SECOND
    }

    /// V8's DateCache::ToLocal.
    fn to_local(self, t: i64) -> i64 {
        t + self.offset(t)
    }

    /// V8's DateCache::ToUTC: the instant of a local time, as ICU's getOffsetFromLocal gives
    /// it with the former offset for a time that is skipped and for one that is repeated. It
    /// looks at the offsets a day either side, as a change of offset is never closer than
    /// that to another.
    pub(crate) fn to_utc(self, local: i64) -> i64 {
        let before = self.offset(local - MS_PER_DAY);
        let after = self.offset(local + MS_PER_DAY);
        if before == after {
            return local - before;
        }
        let (early, late) = (local - before, local - after);
        let early_fits = self.offset(early) == before;
        let late_fits = self.offset(late) == after;
        match (early_fits, late_fits) {
            (true, true) => early.min(late),
            (true, false) => early,
            (false, true) => late,
            // Skipped: read with the offset before the change.
            (false, false) => early,
        }
    }

    /// V8's SetLocalDateValue: a local time converted to a date.
    fn set_local(self, local: f64) -> f64 {
        if (-MAX_TIME_BEFORE_UTC..=MAX_TIME_BEFORE_UTC).contains(&local) {
            time_clip(self.to_utc(local as i64) as f64)
        } else {
            f64::NAN
        }
    }

    fn break_down(self, t: f64) -> Fields {
        let local = self.to_local(t as i64);
        let days = local.div_euclid(MS_PER_DAY);
        let (year, month, date) = civil(days);
        Fields {
            year,
            month,
            date,
            weekday: (days + 4).rem_euclid(7),
            days,
            time_in_day: local - days * MS_PER_DAY,
        }
    }

    /// A field of a date in local time, NaN for an invalid date.
    fn field(self, t: f64, of: impl FnOnce(Fields) -> i64) -> f64 {
        if t.is_nan() {
            return t;
        }
        of(self.break_down(t)) as f64
    }

    /// getFullYear.
    pub(crate) fn full_year(self, t: f64) -> f64 {
        self.field(t, |f| f.year)
    }

    /// getMonth, from 0.
    pub(crate) fn month(self, t: f64) -> f64 {
        self.field(t, |f| f.month)
    }

    /// getDate, the day of the month.
    pub(crate) fn date(self, t: f64) -> f64 {
        self.field(t, |f| f.date)
    }

    /// getDay, the day of the week from Sunday, 0.
    pub(crate) fn day(self, t: f64) -> f64 {
        self.field(t, |f| f.weekday)
    }

    /// getHours.
    pub(crate) fn hours(self, t: f64) -> f64 {
        self.field(t, |f| f.time_in_day / MS_PER_HOUR)
    }

    /// getMinutes.
    pub(crate) fn minutes(self, t: f64) -> f64 {
        self.field(t, |f| f.time_in_day / MS_PER_MINUTE % 60)
    }

    /// getSeconds.
    pub(crate) fn seconds(self, t: f64) -> f64 {
        self.field(t, |f| f.time_in_day / MS_PER_SECOND % 60)
    }

    /// getMilliseconds.
    pub(crate) fn milliseconds(self, t: f64) -> f64 {
        self.field(t, |f| f.time_in_day % MS_PER_SECOND)
    }

    /// getTimezoneOffset: minutes from local time to UTC, cut to a whole number as V8 cuts
    /// them, so that an offset of local mean time in seconds loses them.
    pub(crate) fn timezone_offset(self, t: f64) -> f64 {
        if t.is_nan() {
            return t;
        }
        let ms = t as i64;
        ((ms - self.to_local(ms)) / MS_PER_MINUTE) as f64
    }

    // The setters, in local time: each gives the date that the setter leaves in its object.

    /// setDate.
    pub(crate) fn set_date(self, t: f64, date: f64) -> f64 {
        if t.is_nan() {
            return t;
        }
        let f = self.break_down(t);
        let day = make_day(f.year as f64, f.month as f64, date);
        self.set_local(make_date(day, f.time_in_day as f64))
    }

    /// setMonth with one argument.
    pub(crate) fn set_month(self, t: f64, month: f64) -> f64 {
        if t.is_nan() {
            return t;
        }
        let f = self.break_down(t);
        let day = make_day(f.year as f64, month, f.date as f64);
        self.set_local(make_date(day, f.time_in_day as f64))
    }

    /// setMonth with two arguments, the month and the day of the month.
    pub(crate) fn set_month_date(self, t: f64, month: f64, date: f64) -> f64 {
        if t.is_nan() {
            return t;
        }
        let f = self.break_down(t);
        let day = make_day(f.year as f64, month, date);
        self.set_local(make_date(day, f.time_in_day as f64))
    }

    /// setFullYear with one argument. An invalid date is taken as +0, as the spec has it.
    pub(crate) fn set_full_year(self, t: f64, year: f64) -> f64 {
        let (m, d, tid) = if t.is_nan() {
            (0.0, 1.0, 0.0)
        } else {
            let f = self.break_down(t);
            (f.month as f64, f.date as f64, f.time_in_day as f64)
        };
        self.set_local(make_date(make_day(year, m, d), tid))
    }

    /// setHours with all four arguments.
    pub(crate) fn set_hours(self, t: f64, h: f64, m: f64, s: f64, ms: f64) -> f64 {
        if t.is_nan() {
            return t;
        }
        let f = self.break_down(t);
        self.set_local(make_date(f.days as f64, make_time(h, m, s, ms)))
    }

    /// `new Date(year, month, date, hours, minutes, seconds, ms)` in local time. The
    /// arguments JavaScript leaves out are 1 for date and 0 for the rest.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn local(
        self,
        year: f64,
        month: f64,
        date: f64,
        h: f64,
        m: f64,
        s: f64,
        ms: f64,
    ) -> f64 {
        let day = make_day(full_year(year), month, date);
        self.set_local(make_date(day, make_time(h, m, s, ms)))
    }

    /// `new Date(s)` for a string: V8's DateParser, which reads the date time strings of the
    /// spec first, and what it leaves goes to a legacy parser compatible with Safari's that
    /// takes words, numbers and signs in many orders. It gives NaN for what neither reads. A
    /// date with no offset is in local time, but one of the spec's that has no time is in
    /// UTC.
    pub(crate) fn parse(self, s: &str) -> f64 {
        let units: Vec<u16> = s.encode_utf16().collect();
        let Some(out) = parse::parse_date(&units) else {
            return f64::NAN;
        };
        let day = make_day(out.year, out.month, out.day);
        let t = make_time(out.hour, out.minute, out.second, out.millisecond);
        let mut date = make_date(day, t);
        match out.utc_offset {
            None => {
                if !(-MAX_TIME_BEFORE_UTC..=MAX_TIME_BEFORE_UTC).contains(&date) {
                    return f64::NAN;
                }
                date = self.to_utc(date as i64) as f64;
            }
            Some(offset) => {
                date -= offset * 1000.0;
                if !(-MAX_TIME..=MAX_TIME).contains(&date) {
                    return f64::NAN;
                }
            }
        }
        time_clip(date)
    }
}

/// getUTCSeconds.
pub(crate) fn utc_seconds(t: f64) -> f64 {
    if t.is_nan() {
        return t;
    }
    let ms = t as i64;
    ((ms - ms.div_euclid(MS_PER_DAY) * MS_PER_DAY) / MS_PER_SECOND % 60) as f64
}

/// setTime.
pub(crate) fn set_time(v: f64) -> f64 {
    time_clip(v)
}

/// The year the constructor and Date.UTC take: one from 0 to 99 is in the 1900s.
fn full_year(year: f64) -> f64 {
    if !year.is_nan() {
        let y = to_integer(year);
        if (0.0..=99.0).contains(&y) {
            return 1900.0 + y;
        }
    }
    year
}

/// Date.UTC.
pub(crate) fn utc(year: f64, month: f64, date: f64, h: f64, m: f64, s: f64, ms: f64) -> f64 {
    time_clip(make_date(
        make_day(full_year(year), month, date),
        make_time(h, m, s, ms),
    ))
}

#[cfg(test)]
mod tests;
