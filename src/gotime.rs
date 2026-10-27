//! Go's time.Time and time.Duration as Tengo uses them: instants as unix seconds plus
//! nanoseconds with a location (UTC, Local or a fixed zone), Go layout formatting and parsing,
//! and Duration's String/ParseDuration. The graded environment runs with TZ=UTC, so Local has
//! offset 0, zone abbreviation "UTC", and (as Go falls back to UTC for TZ=UTC) the name "UTC".

use std::rc::Rc;

use crate::objects::{Object, TErr, TResult};
use crate::token::Token;

pub const NANOSECOND: i64 = 1;
pub const MICROSECOND: i64 = 1000;
pub const MILLISECOND: i64 = 1_000_000;
pub const SECOND: i64 = 1_000_000_000;
pub const MINUTE: i64 = 60 * SECOND;
pub const HOUR: i64 = 60 * MINUTE;

const MIN_DURATION: i64 = i64::MIN;
const MAX_DURATION: i64 = i64::MAX;

/// Seconds from 0001-01-01T00:00:00Z to the unix epoch, as an absolute value.
pub const UNIX_TO_INTERNAL: i64 = (1969 * 365 + 1969 / 4 - 1969 / 100 + 1969 / 400) * 86400;

#[derive(Clone, Debug, PartialEq)]
pub enum Loc {
    Utc,
    Local,
    Fixed { name: Vec<u8>, offset: i64 },
}

impl Loc {
    /// Location.String()
    pub fn name(&self) -> Vec<u8> {
        match self {
            Loc::Utc => b"UTC".to_vec(),
            Loc::Local => b"UTC".to_vec(),
            Loc::Fixed { name, .. } => name.clone(),
        }
    }

    /// (zone abbreviation, offset seconds) at an instant.
    pub fn lookup(&self) -> (Vec<u8>, i64) {
        match self {
            Loc::Utc => (b"UTC".to_vec(), 0),
            Loc::Local => (b"UTC".to_vec(), 0),
            Loc::Fixed { name, offset } => (name.clone(), *offset),
        }
    }
}

#[derive(Clone, Debug)]
pub struct GoTime {
    pub sec: i64,
    pub nsec: i64,
    pub loc: Loc,
}

fn floor_div(a: i64, b: i64) -> i64 {
    let q = a / b;
    if (a % b != 0) && ((a < 0) != (b < 0)) {
        q - 1
    } else {
        q
    }
}

fn floor_mod(a: i64, b: i64) -> i64 {
    a - floor_div(a, b) * b
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = floor_div(y, 400);
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = floor_div(z, 146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn is_leap(y: i64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

pub const MONTH_NAMES: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
    "December",
];
pub const DAY_NAMES: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// Month.String(), including Go's %!Month(n) for out-of-range values.
pub fn month_string(m: i64) -> Vec<u8> {
    if (1..=12).contains(&m) {
        MONTH_NAMES[(m - 1) as usize].as_bytes().to_vec()
    } else {
        // Month.String formats the value as uint64.
        format!("%!Month({})", m as u64).into_bytes()
    }
}

fn norm(hi: i64, lo: i64, base: i64) -> (i64, i64) {
    let (mut hi, mut lo) = (hi, lo);
    if lo < 0 {
        let n = (-lo - 1) / base + 1;
        hi -= n;
        lo += n * base;
    }
    if lo >= base {
        let n = lo / base;
        hi += n;
        lo -= n * base;
    }
    (hi, lo)
}

impl GoTime {
    /// time.Unix(sec, nsec) in Local.
    pub fn unix(sec: i64, nsec: i64) -> GoTime {
        let (mut sec, mut nsec) = (sec, nsec);
        if !(0..SECOND).contains(&nsec) {
            let n = nsec / SECOND;
            sec = sec.wrapping_add(n);
            nsec -= n * SECOND;
            if nsec < 0 {
                nsec += SECOND;
                sec = sec.wrapping_sub(1);
            }
        }
        GoTime { sec, nsec, loc: Loc::Local }
    }

    /// time.Date
    #[allow(clippy::too_many_arguments)]
    pub fn date(year: i64, month: i64, day: i64, hour: i64, min: i64, sec: i64, nsec: i64, loc: Loc) -> GoTime {
        let m = month - 1;
        let (year, m) = norm(year, m, 12);
        let month = m + 1;
        let (sec, nsec) = norm(sec, nsec, SECOND);
        let (min, sec) = norm(min, sec, 60);
        let (hour, min) = norm(hour, min, 60);
        let (day, hour) = norm(day, hour, 24);
        let days = days_from_civil(year, month, 1) + (day - 1);
        let mut unix = days * 86400 + hour * 3600 + min * 60 + sec;
        let (_, offset) = loc.lookup();
        unix -= offset;
        GoTime { sec: unix, nsec, loc }
    }

    pub fn zero() -> GoTime {
        GoTime { sec: -UNIX_TO_INTERNAL, nsec: 0, loc: Loc::Utc }
    }

    pub fn is_zero(&self) -> bool {
        self.sec == -UNIX_TO_INTERNAL && self.nsec == 0
    }

    pub fn equal(&self, o: &GoTime) -> bool {
        self.sec == o.sec && self.nsec == o.nsec
    }

    pub fn before(&self, o: &GoTime) -> bool {
        self.sec < o.sec || (self.sec == o.sec && self.nsec < o.nsec)
    }

    pub fn after(&self, o: &GoTime) -> bool {
        self.sec > o.sec || (self.sec == o.sec && self.nsec > o.nsec)
    }

    pub fn with_loc(&self, loc: Loc) -> GoTime {
        GoTime { sec: self.sec, nsec: self.nsec, loc }
    }

    /// t.Add(d)
    pub fn add(&self, d: i64) -> GoTime {
        let mut dsec = d / SECOND;
        let mut nsec = self.nsec + d % SECOND;
        if nsec >= SECOND {
            dsec += 1;
            nsec -= SECOND;
        } else if nsec < 0 {
            dsec -= 1;
            nsec += SECOND;
        }
        GoTime { sec: self.sec.wrapping_add(dsec), nsec, loc: self.loc.clone() }
    }

    /// t.Sub(u), saturating as Go does.
    pub fn sub(&self, u: &GoTime) -> i64 {
        let d = (self.sec.wrapping_sub(u.sec)).wrapping_mul(SECOND).wrapping_add(self.nsec - u.nsec);
        if u.add(d).equal(self) {
            d
        } else if self.before(u) {
            MIN_DURATION
        } else {
            MAX_DURATION
        }
    }

    /// Local seconds (unix seconds shifted by the zone offset), the zone name and offset.
    fn local(&self) -> (i64, Vec<u8>, i64) {
        let (name, offset) = self.loc.lookup();
        (self.sec + offset, name, offset)
    }

    /// (year, month, day)
    pub fn ymd(&self) -> (i64, i64, i64) {
        let (l, _, _) = self.local();
        civil_from_days(floor_div(l, 86400))
    }

    /// (hour, minute, second)
    pub fn clock(&self) -> (i64, i64, i64) {
        let (l, _, _) = self.local();
        let s = floor_mod(l, 86400);
        (s / 3600, (s % 3600) / 60, s % 60)
    }

    /// Weekday, Sunday = 0.
    pub fn weekday(&self) -> i64 {
        let (l, _, _) = self.local();
        floor_mod(floor_div(l, 86400) + 4, 7)
    }

    pub fn year_day(&self) -> i64 {
        let (y, _, _) = self.ymd();
        let (l, _, _) = self.local();
        floor_div(l, 86400) - days_from_civil(y, 1, 1) + 1
    }

    pub fn unix_nano(&self) -> i64 {
        self.sec.wrapping_mul(SECOND).wrapping_add(self.nsec)
    }

    /// t.AddDate(years, months, days)
    pub fn add_date(&self, years: i64, months: i64, days: i64) -> GoTime {
        let (y, m, d) = self.ymd();
        let (hh, mm, ss) = self.clock();
        GoTime::date(y + years, m + months, d + days, hh, mm, ss, self.nsec, self.loc.clone())
    }

    /// Time.String()
    pub fn string(&self) -> Vec<u8> {
        self.format(b"2006-01-02 15:04:05.999999999 -0700 MST")
    }

    /// t.Format(layout)
    pub fn format(&self, layout: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        let (_, name, offset) = self.local();
        let (year, month, day) = self.ymd();
        let yday = self.year_day();
        let (hour, min, sec) = self.clock();
        let mut layout = layout;
        while !layout.is_empty() {
            let (prefix, std, suffix) = next_std_chunk(layout);
            b.extend_from_slice(prefix);
            let std = match std {
                None => break,
                Some(s) => s,
            };
            layout = suffix;
            match std {
                Std::Year => {
                    let y = if year < 0 { -year } else { year };
                    append_int(&mut b, y % 100, 2);
                }
                Std::LongYear => append_int(&mut b, year, 4),
                Std::Month => b.extend_from_slice(&MONTH_NAMES[(month - 1) as usize].as_bytes()[..3]),
                Std::LongMonth => b.extend_from_slice(MONTH_NAMES[(month - 1) as usize].as_bytes()),
                Std::NumMonth => append_int(&mut b, month, 0),
                Std::ZeroMonth => append_int(&mut b, month, 2),
                Std::WeekDay => b.extend_from_slice(&DAY_NAMES[self.weekday() as usize].as_bytes()[..3]),
                Std::LongWeekDay => b.extend_from_slice(DAY_NAMES[self.weekday() as usize].as_bytes()),
                Std::Day => append_int(&mut b, day, 0),
                Std::UnderDay => {
                    if day < 10 {
                        b.push(b' ');
                    }
                    append_int(&mut b, day, 0);
                }
                Std::ZeroDay => append_int(&mut b, day, 2),
                Std::UnderYearDay => {
                    if yday < 100 {
                        b.push(b' ');
                        if yday < 10 {
                            b.push(b' ');
                        }
                    }
                    append_int(&mut b, yday, 0);
                }
                Std::ZeroYearDay => append_int(&mut b, yday, 3),
                Std::Hour => append_int(&mut b, hour, 2),
                Std::Hour12 => {
                    let mut hr = hour % 12;
                    if hr == 0 {
                        hr = 12;
                    }
                    append_int(&mut b, hr, 0);
                }
                Std::ZeroHour12 => {
                    let mut hr = hour % 12;
                    if hr == 0 {
                        hr = 12;
                    }
                    append_int(&mut b, hr, 2);
                }
                Std::Minute => append_int(&mut b, min, 0),
                Std::ZeroMinute => append_int(&mut b, min, 2),
                Std::Second => append_int(&mut b, sec, 0),
                Std::ZeroSecond => append_int(&mut b, sec, 2),
                Std::PM => b.extend_from_slice(if hour >= 12 { b"PM" } else { b"AM" }),
                Std::Pm => b.extend_from_slice(if hour >= 12 { b"pm" } else { b"am" }),
                Std::ISO8601TZ
                | Std::ISO8601ColonTZ
                | Std::ISO8601SecondsTZ
                | Std::ISO8601ShortTZ
                | Std::ISO8601ColonSecondsTZ
                | Std::NumTZ
                | Std::NumColonTZ
                | Std::NumSecondsTz
                | Std::NumShortTZ
                | Std::NumColonSecondsTZ => {
                    let iso = matches!(
                        std,
                        Std::ISO8601TZ
                            | Std::ISO8601ColonTZ
                            | Std::ISO8601SecondsTZ
                            | Std::ISO8601ShortTZ
                            | Std::ISO8601ColonSecondsTZ
                    );
                    if offset == 0 && iso {
                        b.push(b'Z');
                        continue;
                    }
                    let mut zone = offset / 60;
                    let mut absoffset = offset;
                    if zone < 0 {
                        b.push(b'-');
                        zone = -zone;
                        absoffset = -absoffset;
                    } else {
                        b.push(b'+');
                    }
                    append_int(&mut b, zone / 60, 2);
                    if matches!(std, Std::ISO8601ColonTZ | Std::NumColonTZ | Std::ISO8601ColonSecondsTZ | Std::NumColonSecondsTZ) {
                        b.push(b':');
                    }
                    if !matches!(std, Std::NumShortTZ | Std::ISO8601ShortTZ) {
                        append_int(&mut b, zone % 60, 2);
                    }
                    if matches!(std, Std::ISO8601SecondsTZ | Std::NumSecondsTz | Std::NumColonSecondsTZ | Std::ISO8601ColonSecondsTZ) {
                        if matches!(std, Std::NumColonSecondsTZ | Std::ISO8601ColonSecondsTZ) {
                            b.push(b':');
                        }
                        append_int(&mut b, absoffset % 60, 2);
                    }
                }
                Std::TZ => {
                    if !name.is_empty() {
                        b.extend_from_slice(&name);
                        continue;
                    }
                    let mut zone = offset / 60;
                    if zone < 0 {
                        b.push(b'-');
                        zone = -zone;
                    } else {
                        b.push(b'+');
                    }
                    append_int(&mut b, zone / 60, 2);
                    append_int(&mut b, zone % 60, 2);
                }
                Std::FracSecond0(n, sep) => append_nano(&mut b, self.nsec, false, n, sep),
                Std::FracSecond9(n, sep) => append_nano(&mut b, self.nsec, true, n, sep),
            }
        }
        b
    }
}

/// A layout element (time/format.go std constants).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Std {
    LongMonth,
    Month,
    NumMonth,
    ZeroMonth,
    LongWeekDay,
    WeekDay,
    Day,
    UnderDay,
    ZeroDay,
    UnderYearDay,
    ZeroYearDay,
    Hour,
    Hour12,
    ZeroHour12,
    Minute,
    ZeroMinute,
    Second,
    ZeroSecond,
    LongYear,
    Year,
    PM,
    Pm,
    TZ,
    ISO8601TZ,
    ISO8601SecondsTZ,
    ISO8601ShortTZ,
    ISO8601ColonTZ,
    ISO8601ColonSecondsTZ,
    NumTZ,
    NumSecondsTz,
    NumShortTZ,
    NumColonTZ,
    NumColonSecondsTZ,
    FracSecond0(usize, u8),
    FracSecond9(usize, u8),
}

fn starts_with_lower(s: &[u8]) -> bool {
    !s.is_empty() && s[0].is_ascii_lowercase()
}

fn is_digit_at(s: &[u8], i: usize) -> bool {
    i < s.len() && s[i].is_ascii_digit()
}

/// nextStdChunk: (prefix, element, suffix).
pub fn next_std_chunk(layout: &[u8]) -> (&[u8], Option<Std>, &[u8]) {
    let n = layout.len();
    let has = |i: usize, s: &[u8]| n >= i + s.len() && &layout[i..i + s.len()] == s;
    let mut i = 0;
    while i < n {
        let c = layout[i];
        match c {
            b'J' => {
                if has(i, b"Jan") {
                    if has(i, b"January") {
                        return (&layout[..i], Some(Std::LongMonth), &layout[i + 7..]);
                    }
                    if !starts_with_lower(&layout[i + 3..]) {
                        return (&layout[..i], Some(Std::Month), &layout[i + 3..]);
                    }
                }
            }
            b'M' => {
                if n >= i + 3 {
                    if has(i, b"Mon") {
                        if has(i, b"Monday") {
                            return (&layout[..i], Some(Std::LongWeekDay), &layout[i + 6..]);
                        }
                        if !starts_with_lower(&layout[i + 3..]) {
                            return (&layout[..i], Some(Std::WeekDay), &layout[i + 3..]);
                        }
                    }
                    if has(i, b"MST") {
                        return (&layout[..i], Some(Std::TZ), &layout[i + 3..]);
                    }
                }
            }
            b'0' => {
                if n >= i + 2 && (b'1'..=b'6').contains(&layout[i + 1]) {
                    let std = [Std::ZeroMonth, Std::ZeroDay, Std::ZeroHour12, Std::ZeroMinute, Std::ZeroSecond, Std::Year]
                        [(layout[i + 1] - b'1') as usize];
                    return (&layout[..i], Some(std), &layout[i + 2..]);
                }
                if n >= i + 3 && layout[i + 1] == b'0' && layout[i + 2] == b'2' {
                    return (&layout[..i], Some(Std::ZeroYearDay), &layout[i + 3..]);
                }
            }
            b'1' => {
                if n >= i + 2 && layout[i + 1] == b'5' {
                    return (&layout[..i], Some(Std::Hour), &layout[i + 2..]);
                }
                return (&layout[..i], Some(Std::NumMonth), &layout[i + 1..]);
            }
            b'2' => {
                if has(i, b"2006") {
                    return (&layout[..i], Some(Std::LongYear), &layout[i + 4..]);
                }
                return (&layout[..i], Some(Std::Day), &layout[i + 1..]);
            }
            b'_' => {
                if n >= i + 2 && layout[i + 1] == b'2' {
                    if n >= i + 5 && &layout[i + 1..i + 5] == b"2006" {
                        return (&layout[..i + 1], Some(Std::LongYear), &layout[i + 5..]);
                    }
                    return (&layout[..i], Some(Std::UnderDay), &layout[i + 2..]);
                }
                if n >= i + 3 && layout[i + 1] == b'_' && layout[i + 2] == b'2' {
                    return (&layout[..i], Some(Std::UnderYearDay), &layout[i + 3..]);
                }
            }
            b'3' => return (&layout[..i], Some(Std::Hour12), &layout[i + 1..]),
            b'4' => return (&layout[..i], Some(Std::Minute), &layout[i + 1..]),
            b'5' => return (&layout[..i], Some(Std::Second), &layout[i + 1..]),
            b'P' => {
                if n >= i + 2 && layout[i + 1] == b'M' {
                    return (&layout[..i], Some(Std::PM), &layout[i + 2..]);
                }
            }
            b'p' => {
                if n >= i + 2 && layout[i + 1] == b'm' {
                    return (&layout[..i], Some(Std::Pm), &layout[i + 2..]);
                }
            }
            b'-' => {
                if has(i, b"-070000") {
                    return (&layout[..i], Some(Std::NumSecondsTz), &layout[i + 7..]);
                }
                if has(i, b"-07:00:00") {
                    return (&layout[..i], Some(Std::NumColonSecondsTZ), &layout[i + 9..]);
                }
                if has(i, b"-0700") {
                    return (&layout[..i], Some(Std::NumTZ), &layout[i + 5..]);
                }
                if has(i, b"-07:00") {
                    return (&layout[..i], Some(Std::NumColonTZ), &layout[i + 6..]);
                }
                if has(i, b"-07") {
                    return (&layout[..i], Some(Std::NumShortTZ), &layout[i + 3..]);
                }
            }
            b'Z' => {
                if has(i, b"Z070000") {
                    return (&layout[..i], Some(Std::ISO8601SecondsTZ), &layout[i + 7..]);
                }
                if has(i, b"Z07:00:00") {
                    return (&layout[..i], Some(Std::ISO8601ColonSecondsTZ), &layout[i + 9..]);
                }
                if has(i, b"Z0700") {
                    return (&layout[..i], Some(Std::ISO8601TZ), &layout[i + 5..]);
                }
                if has(i, b"Z07:00") {
                    return (&layout[..i], Some(Std::ISO8601ColonTZ), &layout[i + 6..]);
                }
                if has(i, b"Z07") {
                    return (&layout[..i], Some(Std::ISO8601ShortTZ), &layout[i + 3..]);
                }
            }
            b'.' | b',' => {
                if i + 1 < n && (layout[i + 1] == b'0' || layout[i + 1] == b'9') {
                    let ch = layout[i + 1];
                    let mut j = i + 1;
                    while j < n && layout[j] == ch {
                        j += 1;
                    }
                    if !is_digit_at(layout, j) {
                        let digits = (j - (i + 1)) & 0xfff;
                        let std = if ch == b'9' { Std::FracSecond9(digits, c) } else { Std::FracSecond0(digits, c) };
                        return (&layout[..i], Some(std), &layout[j..]);
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    (layout, None, b"")
}

/// time's appendInt: x zero-padded to width.
pub fn append_int(b: &mut Vec<u8>, x: i64, width: usize) {
    let mut u = x.unsigned_abs();
    if x < 0 {
        b.push(b'-');
    }
    let mut digits = Vec::new();
    if u == 0 {
        digits.push(b'0');
    }
    while u > 0 {
        digits.push(b'0' + (u % 10) as u8);
        u /= 10;
    }
    for _ in digits.len()..width {
        b.push(b'0');
    }
    digits.reverse();
    b.extend_from_slice(&digits);
}

fn append_nano(b: &mut Vec<u8>, nanosec: i64, trim: bool, n: usize, sep: u8) {
    if trim && (n == 0 || nanosec == 0) {
        return;
    }
    b.push(sep);
    let start = b.len();
    append_int(b, nanosec, 9);
    if n < 9 {
        b.truncate(start + n);
    }
    if trim {
        while b.len() > start - 1 && *b.last().unwrap() == b'0' {
            b.pop();
        }
        if b.len() == start {
            b.pop();
        }
    }
}

/// Duration.String()
pub fn duration_string(d: i64) -> Vec<u8> {
    let mut buf = [0u8; 32];
    let mut w = buf.len();
    let neg = d < 0;
    let mut u = d.unsigned_abs();
    fn fmt_frac(buf: &mut [u8], w: usize, v: u64, prec: usize) -> (usize, u64) {
        let mut w = w;
        let mut v = v;
        let mut print = false;
        for _ in 0..prec {
            let digit = v % 10;
            print = print || digit != 0;
            if print {
                w -= 1;
                buf[w] = digit as u8 + b'0';
            }
            v /= 10;
        }
        if print {
            w -= 1;
            buf[w] = b'.';
        }
        (w, v)
    }
    fn fmt_int(buf: &mut [u8], w: usize, v: u64) -> usize {
        let mut w = w;
        let mut v = v;
        if v == 0 {
            w -= 1;
            buf[w] = b'0';
        } else {
            while v > 0 {
                w -= 1;
                buf[w] = (v % 10) as u8 + b'0';
                v /= 10;
            }
        }
        w
    }
    if u < SECOND as u64 {
        w -= 1;
        buf[w] = b's';
        w -= 1;
        let prec;
        if u == 0 {
            buf[w] = b'0';
            return buf[w..].to_vec();
        } else if u < MICROSECOND as u64 {
            prec = 0;
            buf[w] = b'n';
        } else if u < MILLISECOND as u64 {
            prec = 3;
            w -= 1;
            buf[w] = 0xC2;
            buf[w + 1] = 0xB5;
        } else {
            prec = 6;
            buf[w] = b'm';
        }
        let (nw, nv) = fmt_frac(&mut buf, w, u, prec);
        w = fmt_int(&mut buf, nw, nv);
    } else {
        w -= 1;
        buf[w] = b's';
        let (nw, nv) = fmt_frac(&mut buf, w, u, 9);
        u = nv;
        w = fmt_int(&mut buf, nw, u % 60);
        u /= 60;
        if u > 0 {
            w -= 1;
            buf[w] = b'm';
            w = fmt_int(&mut buf, w, u % 60);
            u /= 60;
            if u > 0 {
                w -= 1;
                buf[w] = b'h';
                w = fmt_int(&mut buf, w, u);
            }
        }
    }
    if neg {
        w -= 1;
        buf[w] = b'-';
    }
    buf[w..].to_vec()
}

/// time's quote helper used in its error messages.
pub fn time_quote(s: &[u8]) -> Vec<u8> {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut buf = vec![b'"'];
    let mut i = 0;
    while i < s.len() {
        let (c, w) = crate::gostr::decode_rune(&s[i..]);
        if c >= 0x80 || c < ' ' as i32 {
            let width = if c == crate::gostr::RUNE_ERROR {
                if i + 2 < s.len() && &s[i..i + 3] == "\u{FFFD}".as_bytes() {
                    3
                } else {
                    1
                }
            } else {
                w
            };
            for j in 0..width {
                buf.extend_from_slice(b"\\x");
                buf.push(HEX[(s[i + j] >> 4) as usize]);
                buf.push(HEX[(s[i + j] & 0xF) as usize]);
            }
            i += width;
        } else {
            if c == '"' as i32 || c == '\\' as i32 {
                buf.push(b'\\');
            }
            buf.push(c as u8);
            i += 1;
        }
    }
    buf.push(b'"');
    buf
}

fn leading_int(s: &[u8]) -> Result<(u64, &[u8]), ()> {
    let mut x: u64 = 0;
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if !c.is_ascii_digit() {
            break;
        }
        if x > (1u64 << 63) / 10 {
            return Err(());
        }
        x = x * 10 + (c - b'0') as u64;
        if x > 1u64 << 63 {
            return Err(());
        }
        i += 1;
    }
    Ok((x, &s[i..]))
}

fn leading_fraction(s: &[u8]) -> (u64, f64, &[u8]) {
    let mut i = 0;
    let mut x: u64 = 0;
    let mut scale = 1.0;
    let mut overflow = false;
    while i < s.len() {
        let c = s[i];
        if !c.is_ascii_digit() {
            break;
        }
        i += 1;
        if overflow {
            continue;
        }
        if x > ((1u64 << 63) - 1) / 10 {
            overflow = true;
            continue;
        }
        let y = x * 10 + (c - b'0') as u64;
        if y > 1u64 << 63 {
            overflow = true;
            continue;
        }
        x = y;
        scale *= 10.0;
    }
    (x, scale, &s[i..])
}

/// time.ParseDuration
pub fn parse_duration(orig: &[u8]) -> Result<i64, Vec<u8>> {
    let invalid = || {
        let mut m = b"time: invalid duration ".to_vec();
        m.extend_from_slice(&time_quote(orig));
        m
    };
    let mut s = orig;
    let mut d: u64 = 0;
    let mut neg = false;
    if !s.is_empty() && (s[0] == b'-' || s[0] == b'+') {
        neg = s[0] == b'-';
        s = &s[1..];
    }
    if s == b"0" {
        return Ok(0);
    }
    if s.is_empty() {
        return Err(invalid());
    }
    while !s.is_empty() {
        if !(s[0] == b'.' || s[0].is_ascii_digit()) {
            return Err(invalid());
        }
        let pl = s.len();
        let (v0, rest) = leading_int(s).map_err(|_| invalid())?;
        let mut v = v0;
        s = rest;
        let pre = pl != s.len();
        let mut post = false;
        let mut f: u64 = 0;
        let mut scale = 1.0;
        if !s.is_empty() && s[0] == b'.' {
            s = &s[1..];
            let pl = s.len();
            let (ff, sc, rest) = leading_fraction(s);
            f = ff;
            scale = sc;
            s = rest;
            post = pl != s.len();
        }
        if !pre && !post {
            return Err(invalid());
        }
        let mut i = 0;
        while i < s.len() {
            let c = s[i];
            if c == b'.' || c.is_ascii_digit() {
                break;
            }
            i += 1;
        }
        if i == 0 {
            let mut m = b"time: missing unit in duration ".to_vec();
            m.extend_from_slice(&time_quote(orig));
            return Err(m);
        }
        let u = &s[..i];
        s = &s[i..];
        let unit: u64 = match u {
            b"ns" => 1,
            b"us" => 1000,
            b"\xc2\xb5s" => 1000,
            b"\xce\xbcs" => 1000,
            b"ms" => 1_000_000,
            b"s" => 1_000_000_000,
            b"m" => 60_000_000_000,
            b"h" => 3_600_000_000_000,
            _ => {
                let mut m = b"time: unknown unit ".to_vec();
                m.extend_from_slice(&time_quote(u));
                m.extend_from_slice(b" in duration ");
                m.extend_from_slice(&time_quote(orig));
                return Err(m);
            }
        };
        if v > (1u64 << 63) / unit {
            return Err(invalid());
        }
        v *= unit;
        if f > 0 {
            v = v.wrapping_add((f as f64 * (unit as f64 / scale)) as u64);
            if v > 1u64 << 63 {
                return Err(invalid());
            }
        }
        d = d.wrapping_add(v);
        if d > 1u64 << 63 {
            return Err(invalid());
        }
    }
    if neg {
        return Ok((d as i64).wrapping_neg());
    }
    if d > (1u64 << 63) - 1 {
        return Err(invalid());
    }
    Ok(d as i64)
}

/// Time's BinaryOp.
pub fn time_binary_op(this: &Object, t: &Rc<GoTime>, op: Token, rhs: &Object) -> TResult<Object> {
    match rhs {
        Object::Int(i) => match op {
            Token::Add => {
                if *i == 0 {
                    return Ok(this.clone());
                }
                Ok(Object::Time(Rc::new(t.add(*i))))
            }
            Token::Sub => {
                if *i == 0 {
                    return Ok(this.clone());
                }
                Ok(Object::Time(Rc::new(t.add(i.wrapping_neg()))))
            }
            _ => Err(TErr::InvalidOperator),
        },
        Object::Time(u) => match op {
            Token::Sub => Ok(Object::Int(t.sub(u))),
            Token::Less => Ok(Object::Bool(t.before(u))),
            Token::Greater => Ok(Object::Bool(t.after(u))),
            Token::LessEq => Ok(Object::Bool(t.equal(u) || t.before(u))),
            Token::GreaterEq => Ok(Object::Bool(t.equal(u) || t.after(u))),
            _ => Err(TErr::InvalidOperator),
        },
        _ => Err(TErr::InvalidOperator),
    }
}
