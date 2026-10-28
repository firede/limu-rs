//! V8's DateParser, as `new Date(string)` reads a string: the date time strings of the spec
//! first, and then what is left with a legacy parser compatible with Safari's.

/// What the parser reads of a date, V8's output slots: the year, the month from 0 and the
/// day, the time, and the offset from UTC in seconds, none where the string gives none.
pub(super) struct Parsed {
    pub(super) year: f64,
    pub(super) month: f64,
    pub(super) day: f64,
    pub(super) hour: f64,
    pub(super) minute: f64,
    pub(super) second: f64,
    pub(super) millisecond: f64,
    pub(super) utc_offset: Option<f64>,
}

/// V8's kNone, a slot not yet given.
const NONE: i64 = i32::MAX as i64;

/// The largest small integer of a V8 without pointer compression, as Node is built.
const SMI_MAX: i64 = i32::MAX as i64;

/// How many digits of a number the parser keeps.
const MAX_SIGNIFICANT_DIGITS: usize = 9;

fn between(x: i64, lo: i64, hi: i64) -> bool {
    lo <= x && x <= hi
}

/// V8's InputReader: it reads code units, and a NUL reads as the end.
struct Reader<'a> {
    s: &'a [u16],
    index: usize,
    ch: i64,
}

impl Reader<'_> {
    fn next(&mut self) {
        self.ch = self.s.get(self.index).map_or(0, |&c| i64::from(c));
        self.index += 1;
    }

    fn read_unsigned_numeral(&mut self) -> i64 {
        let (mut n, mut i) = (0, 0);
        while self.ch == i64::from(b'0') {
            self.next();
        }
        while self.is_digit() {
            if i < MAX_SIGNIFICANT_DIGITS {
                n = n * 10 + self.ch - i64::from(b'0');
            }
            i += 1;
            self.next();
        }
        n
    }

    fn read_word(&mut self, prefix: &mut [i64; 3]) -> usize {
        let mut n = 0;
        while self.is_alpha_or_above() && !is_white_space(self.ch) {
            if n < 3 {
                prefix[n] = self.ch | 0x20;
            }
            n += 1;
            self.next();
        }
        for p in prefix.iter_mut().skip(n) {
            *p = 0;
        }
        n
    }

    fn skip(&mut self, c: u8) -> bool {
        if self.ch == i64::from(c) {
            self.next();
            return true;
        }
        false
    }

    fn skip_white_space(&mut self) -> bool {
        if is_white_space(self.ch) || is_line_terminator(self.ch) {
            self.next();
            return true;
        }
        false
    }

    fn skip_parentheses(&mut self) -> bool {
        if self.ch != i64::from(b'(') {
            return false;
        }
        let mut balance = 0;
        loop {
            if self.ch == i64::from(b')') {
                balance -= 1;
            } else if self.ch == i64::from(b'(') {
                balance += 1;
            }
            self.next();
            if !(balance > 0 && self.ch != 0) {
                break;
            }
        }
        true
    }

    fn is_end(&self) -> bool {
        self.ch == 0
    }

    fn is_digit(&self) -> bool {
        (i64::from(b'0')..=i64::from(b'9')).contains(&self.ch)
    }

    fn is_alpha_or_above(&self) -> bool {
        self.ch >= i64::from(b'A')
    }
}

/// The spec's WhiteSpace, as V8 knows it.
fn is_white_space(c: i64) -> bool {
    matches!(
        c,
        9 | 11 | 12 | 32 | 0xa0 | 0x1680 | 0x202f | 0x205f | 0x3000 | 0xfeff | 0x2000..=0x200a
    )
}

fn is_line_terminator(c: i64) -> bool {
    matches!(c, 10 | 13 | 0x2028 | 0x2029)
}

// The kinds of keyword, V8's KeywordType.
const KW_INVALID: i64 = 0;
const KW_MONTH_NAME: i64 = 1;
const KW_TIME_ZONE_NAME: i64 = 2;
const KW_TIME_SEPARATOR: i64 = 3;
const KW_AM_PM: i64 = 4;

// The kinds of token, V8's tags. A keyword's tag is its kind, from 0.
const TAG_INVALID: i64 = -6;
const TAG_UNKNOWN: i64 = -5;
const TAG_WHITE_SPACE: i64 = -4;
const TAG_NUMBER: i64 = -3;
const TAG_SYMBOL: i64 = -2;
const TAG_END: i64 = -1;

/// V8's DateToken.
#[derive(Clone, Copy)]
struct Token {
    tag: i64,
    length: usize,
    value: i64,
}

impl Token {
    fn of(tag: i64) -> Token {
        Token {
            tag,
            length: 0,
            value: 0,
        }
    }

    fn is_invalid(&self) -> bool {
        self.tag == TAG_INVALID
    }

    fn is_number(&self) -> bool {
        self.tag == TAG_NUMBER
    }

    fn is_symbol(&self, c: u8) -> bool {
        self.tag == TAG_SYMBOL && self.value == i64::from(c)
    }

    fn is_white_space(&self) -> bool {
        self.tag == TAG_WHITE_SPACE
    }

    fn is_end(&self) -> bool {
        self.tag == TAG_END
    }

    fn is_keyword(&self) -> bool {
        self.tag >= 0
    }

    fn is_fixed_length_number(&self, n: usize) -> bool {
        self.is_number() && self.length == n
    }

    fn is_ascii_sign(&self) -> bool {
        self.tag == TAG_SYMBOL && (self.value == i64::from(b'-') || self.value == i64::from(b'+'))
    }

    fn ascii_sign(&self) -> i64 {
        44 - self.value
    }

    fn is_keyword_type(&self, kind: i64) -> bool {
        self.tag == kind
    }

    fn is_keyword_z(&self) -> bool {
        self.tag == KW_TIME_ZONE_NAME && self.length == 1 && self.value == 0
    }
}

/// V8's KeywordTable: a word matches by its first three letters, and only a month's name may
/// be longer.
const KEYWORDS: [([u8; 3], i64, i64); 27] = [
    (*b"jan", KW_MONTH_NAME, 1),
    (*b"feb", KW_MONTH_NAME, 2),
    (*b"mar", KW_MONTH_NAME, 3),
    (*b"apr", KW_MONTH_NAME, 4),
    (*b"may", KW_MONTH_NAME, 5),
    (*b"jun", KW_MONTH_NAME, 6),
    (*b"jul", KW_MONTH_NAME, 7),
    (*b"aug", KW_MONTH_NAME, 8),
    (*b"sep", KW_MONTH_NAME, 9),
    (*b"oct", KW_MONTH_NAME, 10),
    (*b"nov", KW_MONTH_NAME, 11),
    (*b"dec", KW_MONTH_NAME, 12),
    (*b"am\0", KW_AM_PM, 0),
    (*b"pm\0", KW_AM_PM, 12),
    (*b"ut\0", KW_TIME_ZONE_NAME, 0),
    (*b"utc", KW_TIME_ZONE_NAME, 0),
    (*b"z\0\0", KW_TIME_ZONE_NAME, 0),
    (*b"gmt", KW_TIME_ZONE_NAME, 0),
    (*b"cdt", KW_TIME_ZONE_NAME, -5),
    (*b"cst", KW_TIME_ZONE_NAME, -6),
    (*b"edt", KW_TIME_ZONE_NAME, -4),
    (*b"est", KW_TIME_ZONE_NAME, -5),
    (*b"mdt", KW_TIME_ZONE_NAME, -6),
    (*b"mst", KW_TIME_ZONE_NAME, -7),
    (*b"pdt", KW_TIME_ZONE_NAME, -7),
    (*b"pst", KW_TIME_ZONE_NAME, -8),
    (*b"t\0\0", KW_TIME_SEPARATOR, 0),
];

fn lookup_keyword(prefix: [i64; 3], length: usize) -> (i64, i64) {
    for (word, kind, value) in KEYWORDS {
        if word.map(i64::from) == prefix && (length <= 3 || kind == KW_MONTH_NAME) {
            return (kind, value);
        }
    }
    (KW_INVALID, 0)
}

/// V8's DateStringTokenizer.
struct Tokenizer<'a> {
    input: Reader<'a>,
    ahead: Token,
}

impl<'a> Tokenizer<'a> {
    fn new(input: Reader<'a>) -> Tokenizer<'a> {
        let mut t = Tokenizer {
            input,
            ahead: Token::of(TAG_END),
        };
        t.ahead = t.scan();
        t
    }

    fn scan(&mut self) -> Token {
        let input = &mut self.input;
        let pre = input.index;
        if input.is_end() {
            return Token::of(TAG_END);
        }
        if input.is_digit() {
            let n = input.read_unsigned_numeral();
            return Token {
                tag: TAG_NUMBER,
                length: input.index - pre,
                value: n,
            };
        }
        for &c in b":-+.)" {
            if input.skip(c) {
                return Token {
                    tag: TAG_SYMBOL,
                    length: 1,
                    value: i64::from(c),
                };
            }
        }
        if input.is_alpha_or_above() && !is_white_space(input.ch) {
            let mut prefix = [0; 3];
            let n = input.read_word(&mut prefix);
            let (kind, value) = lookup_keyword(prefix, n);
            return Token {
                tag: kind,
                length: n,
                value,
            };
        }
        if input.skip_white_space() {
            return Token {
                tag: TAG_WHITE_SPACE,
                length: input.index - pre,
                value: 0,
            };
        }
        if input.skip_parentheses() {
            return Token::of(TAG_UNKNOWN);
        }
        input.next();
        Token::of(TAG_UNKNOWN)
    }

    fn next(&mut self) -> Token {
        let result = self.ahead;
        self.ahead = self.scan();
        result
    }

    fn peek(&self) -> Token {
        self.ahead
    }

    fn skip_symbol(&mut self, c: u8) -> bool {
        if self.ahead.is_symbol(c) {
            self.ahead = self.scan();
            return true;
        }
        false
    }
}

/// V8's TimeZoneComposer.
struct TimeZone {
    sign: i64,
    hour: i64,
    minute: i64,
}

impl TimeZone {
    fn set(&mut self, hours: i64) {
        self.sign = if hours < 0 { -1 } else { 1 };
        self.hour = hours * self.sign;
        self.minute = 0;
    }

    fn set_sign(&mut self, sign: i64) {
        self.sign = if sign < 0 { -1 } else { 1 };
    }

    fn is_expecting(&self, n: i64) -> bool {
        self.hour != NONE && self.minute == NONE && is_minute(n)
    }

    fn is_utc(&self) -> bool {
        self.hour == 0 && self.minute == 0
    }

    fn is_empty(&self) -> bool {
        self.hour == NONE
    }

    fn write(&mut self, out: &mut Parsed) -> bool {
        if self.sign == NONE {
            out.utc_offset = None;
            return true;
        }
        if self.hour == NONE {
            self.hour = 0;
        }
        if self.minute == NONE {
            self.minute = 0;
        }
        // V8 adds them as unsigned 32-bit numbers, which wrap.
        let total = (self.hour as u32)
            .wrapping_mul(3600)
            .wrapping_add((self.minute as u32).wrapping_mul(60));
        if i64::from(total) > SMI_MAX {
            return false;
        }
        let mut seconds = i64::from(total);
        if self.sign < 0 {
            seconds = -seconds;
        }
        out.utc_offset = Some(seconds as f64);
        true
    }
}

fn is_minute(x: i64) -> bool {
    between(x, 0, 59)
}

fn is_hour(x: i64) -> bool {
    between(x, 0, 23)
}

fn is_second(x: i64) -> bool {
    between(x, 0, 59)
}

fn is_hour12(x: i64) -> bool {
    between(x, 0, 12)
}

fn is_millisecond(x: i64) -> bool {
    between(x, 0, 999)
}

fn is_month(x: i64) -> bool {
    between(x, 1, 12)
}

fn is_day(x: i64) -> bool {
    between(x, 1, 31)
}

/// V8's TimeComposer.
struct Clock {
    comp: [i64; 4],
    index: usize,
    hour_offset: i64,
}

impl Clock {
    fn is_empty(&self) -> bool {
        self.index == 0
    }

    fn is_expecting(&self, n: i64) -> bool {
        self.index == 1 && is_minute(n)
            || self.index == 2 && is_second(n)
            || self.index == 3 && is_millisecond(n)
    }

    fn add(&mut self, n: i64) -> bool {
        if self.index < self.comp.len() {
            self.comp[self.index] = n;
            self.index += 1;
            return true;
        }
        false
    }

    fn add_final(&mut self, n: i64) -> bool {
        if !self.add(n) {
            return false;
        }
        while self.index < self.comp.len() {
            self.comp[self.index] = 0;
            self.index += 1;
        }
        true
    }

    fn write(&mut self, out: &mut Parsed) -> bool {
        while self.index < self.comp.len() {
            self.comp[self.index] = 0;
            self.index += 1;
        }
        let [mut hour, minute, second, ms] = self.comp;
        if self.hour_offset != NONE {
            if !is_hour12(hour) {
                return false;
            }
            hour %= 12;
            hour += self.hour_offset;
        }
        if (!is_hour(hour) || !is_minute(minute) || !is_second(second) || !is_millisecond(ms))
            && (hour != 24 || minute != 0 || second != 0 || ms != 0)
        {
            return false;
        }
        out.hour = hour as f64;
        out.minute = minute as f64;
        out.second = second as f64;
        out.millisecond = ms as f64;
        true
    }
}

/// V8's DayComposer.
struct Calendar {
    comp: [i64; 3],
    index: usize,
    named_month: i64,
    iso_date: bool,
}

impl Calendar {
    fn is_empty(&self) -> bool {
        self.index == 0
    }

    fn add(&mut self, n: i64) -> bool {
        if self.index < self.comp.len() {
            self.comp[self.index] = n;
            self.index += 1;
            return true;
        }
        false
    }

    fn write(&mut self, out: &mut Parsed) -> bool {
        if self.index < 1 {
            return false;
        }
        while self.index < self.comp.len() {
            self.comp[self.index] = 1;
            self.index += 1;
        }
        let c = self.comp;
        let (mut year, month, day) = if self.named_month == NONE {
            if self.iso_date || self.index == 3 && !is_day(c[0]) {
                (c[0], c[1], c[2])
            } else {
                (if self.index == 3 { c[2] } else { 0 }, c[0], c[1])
            }
        } else if self.index == 1 {
            (0, self.named_month, c[0])
        } else if !is_day(c[0]) {
            (c[0], self.named_month, c[1])
        } else {
            (c[1], self.named_month, c[0])
        };
        if !self.iso_date {
            if between(year, 0, 49) {
                year += 2000;
            } else if between(year, 50, 99) {
                year += 1900;
            }
        }
        if year < i64::from(i32::MIN) || year > SMI_MAX || !is_month(month) || !is_day(day) {
            return false;
        }
        out.year = year as f64;
        out.month = (month - 1) as f64;
        out.day = day as f64;
        true
    }
}

/// V8's ReadMilliseconds: the first three significant digits of a number, as its length
/// shows where they are.
fn read_milliseconds(t: Token) -> i64 {
    let (mut number, mut length) = (t.value, t.length);
    if length < 3 {
        if length == 1 {
            number *= 100;
        } else if length == 2 {
            number *= 10;
        }
    } else if length > 3 {
        length = length.min(MAX_SIGNIFICANT_DIGITS);
        let mut factor = 1;
        loop {
            factor *= 10;
            length -= 1;
            if length <= 3 {
                break;
            }
        }
        number /= factor;
    }
    number
}

/// V8's ParseES5DateTime: the date time string format of the spec,
/// `[('-'|'+')yy]yyyy[-MM[-DD]][THH:mm[:ss[.sss]][Z|(+|-)hh:mm]]`. It gives the first token
/// it did not read, for the legacy parser to go on with, or an invalid one.
fn parse_es5(
    scanner: &mut Tokenizer,
    day: &mut Calendar,
    clock: &mut Clock,
    tz: &mut TimeZone,
) -> Token {
    if scanner.peek().is_ascii_sign() {
        let sign = scanner.next();
        if !scanner.peek().is_fixed_length_number(6) {
            return sign;
        }
        let year = scanner.next().value;
        if sign.ascii_sign() < 0 && year == 0 {
            return sign;
        }
        day.add(sign.ascii_sign() * year);
    } else if scanner.peek().is_fixed_length_number(4) {
        day.add(scanner.next().value);
    } else {
        return scanner.next();
    }
    if scanner.skip_symbol(b'-') {
        if !scanner.peek().is_fixed_length_number(2) || !is_month(scanner.peek().value) {
            return scanner.next();
        }
        day.add(scanner.next().value);
        if scanner.skip_symbol(b'-') {
            if !scanner.peek().is_fixed_length_number(2) || !is_day(scanner.peek().value) {
                return scanner.next();
            }
            day.add(scanner.next().value);
        }
    }
    if !scanner.peek().is_keyword_type(KW_TIME_SEPARATOR) {
        if !scanner.peek().is_end() {
            return scanner.next();
        }
    } else {
        scanner.next();
        if !scanner.peek().is_fixed_length_number(2) || !between(scanner.peek().value, 0, 24) {
            return Token::of(TAG_INVALID);
        }
        let hour_is_24 = scanner.peek().value == 24;
        clock.add(scanner.next().value);
        if !scanner.skip_symbol(b':') {
            return Token::of(TAG_INVALID);
        }
        if !scanner.peek().is_fixed_length_number(2)
            || !is_minute(scanner.peek().value)
            || hour_is_24 && scanner.peek().value > 0
        {
            return Token::of(TAG_INVALID);
        }
        clock.add(scanner.next().value);
        if scanner.skip_symbol(b':') {
            if !scanner.peek().is_fixed_length_number(2)
                || !is_second(scanner.peek().value)
                || hour_is_24 && scanner.peek().value > 0
            {
                return Token::of(TAG_INVALID);
            }
            clock.add(scanner.next().value);
            if scanner.skip_symbol(b'.') {
                if !scanner.peek().is_number() || hour_is_24 && scanner.peek().value > 0 {
                    return Token::of(TAG_INVALID);
                }
                clock.add(read_milliseconds(scanner.next()));
            }
        }
        if scanner.peek().is_keyword_z() {
            scanner.next();
            tz.set(0);
        } else if scanner.peek().is_symbol(b'+') || scanner.peek().is_symbol(b'-') {
            let sign = if scanner.next().value == i64::from(b'+') {
                1
            } else {
                -1
            };
            tz.set_sign(sign);
            if scanner.peek().is_fixed_length_number(4) {
                let hourmin = scanner.next().value;
                let (hour, minute) = (hourmin / 100, hourmin % 100);
                if !is_hour(hour) || !is_minute(minute) {
                    return Token::of(TAG_INVALID);
                }
                (tz.hour, tz.minute) = (hour, minute);
            } else {
                if !scanner.peek().is_fixed_length_number(2) || !is_hour(scanner.peek().value) {
                    return Token::of(TAG_INVALID);
                }
                tz.hour = scanner.next().value;
                if !scanner.skip_symbol(b':') {
                    return Token::of(TAG_INVALID);
                }
                if !scanner.peek().is_fixed_length_number(2) || !is_minute(scanner.peek().value) {
                    return Token::of(TAG_INVALID);
                }
                tz.minute = scanner.next().value;
            }
        }
        if !scanner.peek().is_end() {
            return Token::of(TAG_INVALID);
        }
    }
    // A date with no time and no offset is in UTC. A date and time with no offset is local.
    if tz.is_empty() && clock.is_empty() {
        tz.set(0);
    }
    day.iso_date = true;
    Token::of(TAG_END)
}

/// V8's DateParser::Parse.
pub(super) fn parse_date(s: &[u16]) -> Option<Parsed> {
    let mut input = Reader { s, index: 0, ch: 0 };
    input.next();
    let mut scanner = Tokenizer::new(input);
    let mut tz = TimeZone {
        sign: NONE,
        hour: NONE,
        minute: NONE,
    };
    let mut clock = Clock {
        comp: [0; 4],
        index: 0,
        hour_offset: NONE,
    };
    let mut day = Calendar {
        comp: [0; 3],
        index: 0,
        named_month: NONE,
        iso_date: false,
    };

    let unhandled = parse_es5(&mut scanner, &mut day, &mut clock, &mut tz);
    if unhandled.is_invalid() {
        return None;
    }
    let mut has_read_number = !day.is_empty();
    let mut tok = unhandled;
    while !tok.is_end() {
        if tok.is_number() {
            has_read_number = true;
            let n = tok.value;
            if scanner.skip_symbol(b':') {
                if scanner.skip_symbol(b':') {
                    if !clock.is_empty() {
                        return None;
                    }
                    clock.add(n);
                    clock.add(0);
                } else {
                    if !clock.add(n) {
                        return None;
                    }
                    if scanner.peek().is_symbol(b'.') {
                        scanner.next();
                    }
                }
            } else if scanner.skip_symbol(b'.') && clock.is_expecting(n) {
                clock.add(n);
                if !scanner.peek().is_number() {
                    return None;
                }
                let ms = read_milliseconds(scanner.next());
                if ms < 0 {
                    return None;
                }
                clock.add_final(ms);
            } else if tz.is_expecting(n) {
                tz.minute = n;
            } else if clock.is_expecting(n) {
                clock.add_final(n);
                let p = scanner.peek();
                if !p.is_end() && !p.is_white_space() && !p.is_keyword_z() && !p.is_ascii_sign() {
                    return None;
                }
            } else {
                if !day.add(n) {
                    return None;
                }
                scanner.skip_symbol(b'-');
            }
        } else if tok.is_keyword() {
            if tok.tag == KW_AM_PM && !clock.is_empty() {
                clock.hour_offset = tok.value;
            } else if tok.tag == KW_MONTH_NAME {
                day.named_month = tok.value;
                scanner.skip_symbol(b'-');
            } else if tok.tag == KW_TIME_ZONE_NAME && has_read_number {
                tz.set(tok.value);
            } else {
                // Words are garbage, allowed only before a number and apart from it.
                if has_read_number || scanner.peek().is_number() {
                    return None;
                }
            }
        } else if tok.is_ascii_sign() && (tz.is_utc() || !clock.is_empty()) {
            // An offset from UTC, only after UTC or a time.
            tz.set_sign(tok.ascii_sign());
            let (mut n, mut length) = (0, 0);
            if scanner.peek().is_number() {
                let t = scanner.next();
                (length, n) = (t.length, t.value);
            }
            has_read_number = true;
            if scanner.peek().is_symbol(b':') {
                tz.hour = n;
                tz.minute = NONE;
            } else if length == 2 || length == 1 {
                (tz.hour, tz.minute) = (n, 0);
            } else if length == 4 || length == 3 {
                (tz.hour, tz.minute) = (n / 100, n % 100);
            } else {
                return None;
            }
        } else if (tok.is_ascii_sign() || tok.is_symbol(b')')) && has_read_number {
            return None;
        }
        tok = scanner.next();
    }
    let mut out = Parsed {
        year: 0.0,
        month: 0.0,
        day: 0.0,
        hour: 0.0,
        minute: 0.0,
        second: 0.0,
        millisecond: 0.0,
        utc_offset: None,
    };
    (day.write(&mut out) && clock.write(&mut out) && tz.write(&mut out)).then_some(out)
}
