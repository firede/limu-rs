//! Time zones of the tz database, read from the machine's TZif files, for the tests that
//! check dates in the zones the oracle wrote them in. Offsets are looked up as Go's time
//! package looks them up, with the rule a file ends with for the times past its last change,
//! as limu's tests had them.

use std::fs;

use crate::layout::TimeZone;

/// A zone of the tz database.
pub(crate) struct Tzif {
    /// The instants its offset changes at, in seconds since the epoch, and the type of each.
    changes: Vec<(i64, usize)>,
    /// Its types: an offset east of UTC, in seconds, and whether it is daylight saving time.
    types: Vec<(i32, bool)>,
    /// The rule of the times past the last change.
    rule: Option<Rule>,
}

/// When a rule changes the offset: the day, as a POSIX TZ string writes it, and the time of
/// day, in seconds, in the offset before the change.
#[derive(Clone, Copy)]
enum Day {
    /// Jn, from 1 to 365, February 29 never counted.
    Julian(i64),
    /// n, from 0 to 365.
    Year(i64),
    /// Mm.w.d: the d-th day of the week of the w-th week of month m, 5 for the last.
    Month(i64, i64, i64),
}

/// A day of a rule, with the second of that day the change is made at, in the local time in
/// force before it.
type Change = (Day, i64);

/// A POSIX TZ string's rule.
struct Rule {
    std: i32,
    /// Daylight saving time's offset, with when it starts and ends.
    dst: Option<(i32, Change, Change)>,
}

/// The zone of a name, as "America/New_York", from /usr/share/zoneinfo.
pub(crate) fn zone(name: &str) -> Tzif {
    let path = format!("/usr/share/zoneinfo/{name}");
    let data = fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    read(&data).unwrap_or_else(|| panic!("{path}: not a TZif file this reads"))
}

fn be32(b: &[u8], at: usize) -> Option<i64> {
    Some(i64::from(i32::from_be_bytes(
        b.get(at..at + 4)?.try_into().ok()?,
    )))
}

fn be64(b: &[u8], at: usize) -> Option<i64> {
    Some(i64::from_be_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// The counts of a TZif header at an offset: isutcnt, isstdcnt, leapcnt, timecnt, typecnt
/// and charcnt.
fn counts(b: &[u8], at: usize) -> Option<[usize; 6]> {
    if b.get(at..at + 4)? != b"TZif" {
        return None;
    }
    let mut c = [0; 6];
    for (k, n) in c.iter_mut().enumerate() {
        *n = be32(b, at + 20 + 4 * k)? as usize;
    }
    Some(c)
}

/// The length of a block of data of the counts, with times of a size in bytes.
fn block(c: [usize; 6], time: usize) -> usize {
    let [isut, isstd, leap, times, types, chars] = c;
    times * time + times + types * 6 + chars + leap * (time + 4) + isstd + isut
}

fn read(b: &[u8]) -> Option<Tzif> {
    let first = counts(b, 0)?;
    if b[4] < b'2' {
        return None;
    }
    // The second header, of 64-bit times, follows the data of the first.
    let at = 44 + block(first, 4);
    let c = counts(b, at)?;
    let [_, _, _, times, types, _] = c;
    let data = at + 44;
    let mut changes = Vec::with_capacity(times);
    for k in 0..times {
        let when = be64(b, data + 8 * k)?;
        let ty = *b.get(data + 8 * times + k)? as usize;
        changes.push((when, ty));
    }
    let info = data + 9 * times;
    let mut zone_types = Vec::with_capacity(types);
    for k in 0..types {
        let off = be32(b, info + 6 * k)? as i32;
        let dst = *b.get(info + 6 * k + 4)? != 0;
        zone_types.push((off, dst));
    }
    let footer = &b[at + 44 + block(c, 8)..];
    let rule = std::str::from_utf8(footer)
        .ok()
        .map(|s| s.trim_matches('\n'))
        .filter(|s| !s.is_empty())
        .and_then(parse_rule);
    Some(Tzif {
        changes,
        types: zone_types,
        rule,
    })
}

/// A POSIX TZ string, as "EST5EDT,M3.2.0,M11.1.0", as Go's tzset reads one.
fn parse_rule(s: &str) -> Option<Rule> {
    let mut p = s.as_bytes();
    name(&mut p)?;
    let std = -offset(&mut p)?;
    if p.is_empty() {
        return Some(Rule { std, dst: None });
    }
    name(&mut p)?;
    let dst = if p.first().is_some_and(|&c| c != b',') {
        -offset(&mut p)?
    } else {
        std + 3600
    };
    let (start, end) = if p.is_empty() {
        // The rule the United States had in 2007 is the default.
        ((Day::Month(3, 2, 0), 7200), (Day::Month(11, 1, 0), 7200))
    } else {
        p = p.strip_prefix(b",")?;
        let start = change(&mut p)?;
        p = p.strip_prefix(b",")?;
        let end = change(&mut p)?;
        (start, end)
    };
    Some(Rule {
        std,
        dst: Some((dst, start, end)),
    })
}

fn name(p: &mut &[u8]) -> Option<()> {
    if let Some(rest) = p.strip_prefix(b"<") {
        let end = rest.iter().position(|&c| c == b'>')?;
        *p = &rest[end + 1..];
    } else {
        let end = p
            .iter()
            .position(|c| !c.is_ascii_alphabetic())
            .unwrap_or(p.len());
        if end < 3 {
            return None;
        }
        *p = &p[end..];
    }
    Some(())
}

fn number(p: &mut &[u8]) -> Option<i64> {
    let end = p
        .iter()
        .position(|c| !c.is_ascii_digit())
        .unwrap_or(p.len());
    if end == 0 {
        return None;
    }
    let n = std::str::from_utf8(&p[..end]).ok()?.parse().ok()?;
    *p = &p[end..];
    Some(n)
}

/// [+-]hh[:mm[:ss]], in seconds.
fn time(p: &mut &[u8]) -> Option<i64> {
    let sign = match p.first() {
        Some(b'-') => -1,
        Some(b'+') => 1,
        _ => 0,
    };
    if sign != 0 {
        *p = &p[1..];
    }
    let mut t = number(p)? * 3600;
    for unit in [60, 1] {
        let Some(rest) = p.strip_prefix(b":") else {
            break;
        };
        *p = rest;
        t += number(p)? * unit;
    }
    Some(if sign < 0 { -t } else { t })
}

fn offset(p: &mut &[u8]) -> Option<i32> {
    time(p).map(|t| t as i32)
}

/// A day and the time of day of a change, 2:00 where it gives none.
fn change(p: &mut &[u8]) -> Option<(Day, i64)> {
    let day = match p.first()? {
        b'J' => {
            *p = &p[1..];
            Day::Julian(number(p)?)
        }
        b'M' => {
            *p = &p[1..];
            let m = number(p)?;
            *p = p.strip_prefix(b".")?;
            let w = number(p)?;
            *p = p.strip_prefix(b".")?;
            Day::Month(m, w, number(p)?)
        }
        _ => Day::Year(number(p)?),
    };
    let at = match p.strip_prefix(b"/") {
        Some(rest) => {
            *p = rest;
            time(p)?
        }
        None => 7200,
    };
    Some((day, at))
}

fn is_leap(y: i64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

const DAYS_BEFORE: [i64; 13] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334, 365];

/// The days from 1970-01-01 to January 1 of a year.
fn days_to_year(y: i64) -> i64 {
    let y1 = y - 1;
    365 * (y - 1970) + (y1.div_euclid(4) - 1969 / 4) - (y1.div_euclid(100) - 1969 / 100)
        + (y1.div_euclid(400) - 1969 / 400)
}

/// The year of a day since 1970-01-01, and the day of the year from 0.
fn year_of(days: i64) -> (i64, i64) {
    let mut y = 1970 + days.div_euclid(366);
    while days_to_year(y + 1) <= days {
        y += 1;
    }
    while days_to_year(y) > days {
        y -= 1;
    }
    (y, days - days_to_year(y))
}

/// When a change falls, in seconds since the start of the year in UTC, as Go's tzruleTime
/// gives it: off is the offset before the change.
fn rule_time(year: i64, (day, at): (Day, i64), off: i32) -> i64 {
    let s = match day {
        Day::Julian(d) => (d - 1) * 86400 + if is_leap(year) && d >= 60 { 86400 } else { 0 },
        Day::Year(d) => d * 86400,
        Day::Month(mon, week, wday) => {
            // Zeller's congruence, for the day of the week of the first of the month.
            let m1 = (mon + 9) % 12 + 1;
            let yy0 = if mon <= 2 { year - 1 } else { year };
            let (yy1, yy2) = (yy0 / 100, yy0 % 100);
            let mut dow = ((26 * m1 - 2) / 10 + 1 + yy2 + yy2 / 4 + yy1 / 4 - 2 * yy1) % 7;
            if dow < 0 {
                dow += 7;
            }
            let mut d = wday - dow;
            if d < 0 {
                d += 7;
            }
            let days_in = DAYS_BEFORE[mon as usize] - DAYS_BEFORE[mon as usize - 1]
                + i64::from(mon == 2 && is_leap(year));
            for _ in 1..week {
                if d + 7 >= days_in {
                    break;
                }
                d += 7;
            }
            d += DAYS_BEFORE[mon as usize - 1];
            if is_leap(year) && mon > 2 {
                d += 1;
            }
            d * 86400
        }
    };
    s + at - i64::from(off)
}

impl Rule {
    fn offset(&self, at: i64) -> i32 {
        let Some((dst, start, end)) = self.dst else {
            return self.std;
        };
        let (year, yday) = year_of(at.div_euclid(86400));
        let ysec = yday * 86400 + at.rem_euclid(86400);
        let start = rule_time(year, start, self.std);
        let end = rule_time(year, end, dst);
        if end < start {
            // Daylight saving time spans the new year, as in the southern hemisphere.
            if ysec < end || ysec >= start {
                dst
            } else {
                self.std
            }
        } else if ysec < start || ysec >= end {
            self.std
        } else {
            dst
        }
    }
}

impl Tzif {
    /// The type of the times before the first change, as Go's lookupFirstZone picks it.
    fn first(&self) -> usize {
        let used = self.changes.iter().any(|&(_, t)| t == 0);
        if !used {
            return 0;
        }
        if let Some(&(_, t)) = self.changes.first()
            && self.types[t].1
        {
            for k in (0..t).rev() {
                if !self.types[k].1 {
                    return k;
                }
            }
        }
        for (k, &(_, dst)) in self.types.iter().enumerate() {
            if !dst {
                return k;
            }
        }
        0
    }
}

impl TimeZone for Tzif {
    fn offset(&self, at: i64) -> i32 {
        match self.changes.last() {
            Some(&(last, _)) if at >= last && self.rule.is_some() => {
                return self.rule.as_ref().map_or(0, |r| r.offset(at));
            }
            None => {
                if let Some(r) = &self.rule {
                    return r.offset(at);
                }
            }
            _ => {}
        }
        let n = self.changes.partition_point(|&(when, _)| when <= at);
        let ty = if n == 0 {
            self.first()
        } else {
            self.changes[n - 1].1
        };
        self.types[ty].0
    }
}
