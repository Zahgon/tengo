//! Go's time zone database support (time/zoneinfo.go, zoneinfo_read.go, zoneinfo_unix.go): TZif
//! files, the POSIX TZ rule in their footer, `TZ` for the Local zone and time.LoadLocation.

use std::rc::Rc;

pub const ALPHA: i64 = i64::MIN;
pub const OMEGA: i64 = i64::MAX;

const SOURCES: [&str; 4] =
    ["/usr/share/zoneinfo/", "/usr/share/lib/zoneinfo/", "/usr/lib/locale/TZ/", "/etc/zoneinfo"];
const HOUR: i64 = 3600;
const DAY: i64 = 86400;

#[derive(Clone, Debug, PartialEq)]
pub struct ZoneInfo {
    pub name: Vec<u8>,
    pub offset: i64,
    pub dst: bool,
}

/// A loaded time.Location. With no zones it behaves as UTC, as Go's Local falls back to.
#[derive(Clone, Debug, PartialEq)]
pub struct Zone {
    pub name: Vec<u8>,
    pub zones: Vec<ZoneInfo>,
    pub tx: Vec<(i64, usize)>,
    pub extend: Vec<u8>,
}

/// What Location.lookup returns: the zone in effect at an instant and the span it holds for.
pub struct Lookup {
    pub name: Vec<u8>,
    pub offset: i64,
    pub start: i64,
    pub end: i64,
}

thread_local! {
    static LOCAL: Rc<Zone> = Rc::new(init_local());
}

/// time.Local.
pub fn local_zone() -> Rc<Zone> {
    LOCAL.with(Rc::clone)
}

fn utc_fallback() -> Zone {
    Zone { name: b"UTC".to_vec(), zones: Vec::new(), tx: Vec::new(), extend: Vec::new() }
}

/// initLocal: no TZ means /etc/localtime, an empty TZ or an unusable one means UTC.
fn init_local() -> Zone {
    let tz = match std::env::var("TZ") {
        Err(_) => {
            return match load_file("localtime", "/etc/localtime") {
                Ok(Some(z)) => Zone { name: b"Local".to_vec(), ..z },
                _ => utc_fallback(),
            };
        }
        Ok(tz) => tz,
    };
    let tz = tz.strip_prefix(':').unwrap_or(&tz);
    if tz.starts_with('/') {
        if let Ok(Some(z)) = load_file(tz, tz) {
            let name = if tz == "/etc/localtime" { "Local" } else { tz };
            return Zone { name: name.as_bytes().to_vec(), ..z };
        }
    } else if !tz.is_empty() && tz != "UTC" {
        if let Ok(z) = load_from(tz, &SOURCES) {
            return z;
        }
    }
    utc_fallback()
}

/// Reads and parses one file: Ok(None) when it does not exist, Err with Go's text otherwise.
fn load_file(name: &str, path: &str) -> Result<Option<Zone>, Vec<u8>> {
    match std::fs::read(path) {
        Ok(data) => from_tzdata(name, &data).map(Some).ok_or_else(|| b"malformed time zone information".to_vec()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) if std::path::Path::new(path).is_dir() => Err(b"is a directory".to_vec()),
        Err(e) => Err(crate::compiler::go_read_error(path, &e).into_bytes()),
    }
}

/// loadLocation: the first source holding a valid file wins; the first real error is reported.
fn load_from(name: &str, sources: &[&str]) -> Result<Zone, Vec<u8>> {
    let mut first_err = None;
    for source in sources {
        let path = format!("{}/{}", source.trim_end_matches('/'), name);
        match load_file(name, &path) {
            Ok(Some(z)) => return Ok(z),
            Ok(None) => {}
            Err(e) => first_err = first_err.or(Some(e)),
        }
    }
    Err(first_err.unwrap_or_else(|| format!("unknown time zone {}", name).into_bytes()))
}

/// The file-loading half of time.LoadLocation; "", "UTC" and "Local" are the caller's.
pub fn load_named(name: &str) -> Result<Rc<Zone>, Vec<u8>> {
    if name.contains("..") || name.starts_with('/') || name.starts_with('\\') {
        return Err(b"time: invalid location name".to_vec());
    }
    let dir = std::env::var("ZONEINFO").unwrap_or_default();
    let mut first_err = None;
    if !dir.is_empty() {
        match load_from(name, &[dir.as_str()]) {
            Ok(z) => return Ok(Rc::new(z)),
            Err(e) if !e.starts_with(b"unknown time zone") => first_err = Some(e),
            Err(_) => {}
        }
    }
    match load_from(name, &SOURCES) {
        Ok(z) => Ok(Rc::new(z)),
        Err(e) => Err(first_err.unwrap_or(e)),
    }
}

/// LoadLocationFromTZData; None is errBadData.
fn from_tzdata(name: &str, data: &[u8]) -> Option<Zone> {
    let mut d = Reader { data, at: 0 };
    if d.take(4)? != b"TZif" {
        return None;
    }
    let version = match d.take(16)?[0] {
        0 => 1,
        b'2' => 2,
        b'3' => 3,
        _ => return None,
    };
    let mut n = d.counts()?;
    let mut size = 4;
    if version > 1 {
        let [ut, std, leap, time, zone, chars] = n;
        d.take(time * 4 + time + zone * 6 + chars + leap * 8 + std + ut + 4 + 16)?;
        n = d.counts()?;
        size = 8;
    }
    let [ut, std, leap, time, zone, chars] = n;
    let times = d.take(time * size)?;
    let indexes = d.take(time)?;
    let zone_data = d.take(zone * 6)?;
    let abbrev = d.take(chars)?;
    d.take(leap * (size + 4))?;
    d.take(std)?;
    d.take(ut)?;
    let rest = &d.data[d.at..];
    let extend = if rest.len() > 2 && rest[0] == b'\n' && rest[rest.len() - 1] == b'\n' {
        rest[1..rest.len() - 1].to_vec()
    } else {
        Vec::new()
    };
    if zone == 0 {
        return None;
    }
    let mut zones = Vec::with_capacity(zone);
    for z in zone_data.chunks(6) {
        let at = z[5] as usize;
        if at >= abbrev.len() {
            return None;
        }
        let tail = &abbrev[at..];
        let end = tail.iter().position(|c| *c == 0).unwrap_or(tail.len());
        zones.push(ZoneInfo {
            name: tail[..end].to_vec(),
            offset: i32::from_be_bytes([z[0], z[1], z[2], z[3]]) as i64,
            dst: z[4] != 0,
        });
    }
    let mut tx = Vec::with_capacity(time);
    for i in 0..time {
        let when = if size == 8 {
            let b = &times[i * 8..i * 8 + 8];
            i64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
        } else {
            let b = &times[i * 4..i * 4 + 4];
            i32::from_be_bytes([b[0], b[1], b[2], b[3]]) as i64
        };
        let index = indexes[i] as usize;
        if index >= zone {
            return None;
        }
        tx.push((when, index));
    }
    if tx.is_empty() {
        tx.push((ALPHA, 0));
    }
    Some(Zone { name: name.as_bytes().to_vec(), zones, tx, extend })
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(n)?;
        let s = self.data.get(self.at..end)?;
        self.at = end;
        Some(s)
    }

    fn counts(&mut self) -> Option<[usize; 6]> {
        let mut n = [0; 6];
        for v in n.iter_mut() {
            let b = self.take(4)?;
            *v = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize;
        }
        Some(n)
    }
}

impl Zone {
    /// Location.lookup.
    pub fn lookup(&self, sec: i64) -> Lookup {
        if self.zones.is_empty() {
            return Lookup { name: b"UTC".to_vec(), offset: 0, start: ALPHA, end: OMEGA };
        }
        if self.tx.is_empty() || sec < self.tx[0].0 {
            let z = &self.zones[self.first_zone()];
            let end = self.tx.first().map_or(OMEGA, |t| t.0);
            return Lookup { name: z.name.clone(), offset: z.offset, start: ALPHA, end };
        }
        let mut end = OMEGA;
        let (mut lo, mut hi) = (0, self.tx.len());
        while hi - lo > 1 {
            let m = (lo + hi) / 2;
            if sec < self.tx[m].0 {
                end = self.tx[m].0;
                hi = m;
            } else {
                lo = m;
            }
        }
        let z = &self.zones[self.tx[lo].1];
        let start = self.tx[lo].0;
        if lo == self.tx.len() - 1 && !self.extend.is_empty() {
            if let Some(found) = tzset(&self.extend, start, sec) {
                return found;
            }
        }
        Lookup { name: z.name.clone(), offset: z.offset, start, end }
    }

    /// Location.lookupFirstZone.
    fn first_zone(&self) -> usize {
        if !self.tx.iter().any(|t| t.1 == 0) {
            return 0;
        }
        if let Some(t) = self.tx.first() {
            if self.zones[t.1].dst {
                for zi in (0..t.1).rev() {
                    if !self.zones[zi].dst {
                        return zi;
                    }
                }
            }
        }
        self.zones.iter().position(|z| !z.dst).unwrap_or(0)
    }

    /// Location.lookupName: prefers a zone of that name in effect at the given pseudo-Unix time.
    pub fn lookup_name(&self, name: &[u8], unix: i64) -> Option<i64> {
        for z in self.zones.iter().filter(|z| z.name == name) {
            let found = self.lookup(unix - z.offset);
            if found.name == z.name {
                return Some(found.offset);
            }
        }
        self.zones.iter().find(|z| z.name == name).map(|z| z.offset)
    }
}

/// tzset: the zone a POSIX TZ rule gives at `sec`.
fn tzset(s: &[u8], last_tx: i64, sec: i64) -> Option<Lookup> {
    let (mut std_name, s) = tzset_name(s)?;
    let (off, s) = tzset_offset(s)?;
    // TZ offsets are added to local time to get UTC; ours are added to UTC to get local time.
    let mut std_offset = -off;
    if s.is_empty() || s[0] == b',' {
        return Some(Lookup { name: std_name, offset: std_offset, start: last_tx, end: OMEGA });
    }
    let (mut dst_name, mut s) = tzset_name(s)?;
    let mut dst_offset = std_offset + HOUR;
    if !s.is_empty() && s[0] != b',' {
        let (off, rest) = tzset_offset(s)?;
        dst_offset = -off;
        s = rest;
    }
    let default_rules: &[u8] = b",M3.2.0,M11.1.0";
    let s = if s.is_empty() { default_rules } else { s };
    if s[0] != b',' && s[0] != b';' {
        return None;
    }
    let (start_rule, s) = tzset_rule(&s[1..])?;
    if s.first() != Some(&b',') {
        return None;
    }
    let (end_rule, s) = tzset_rule(&s[1..])?;
    if !s.is_empty() {
        return None;
    }

    let days = sec.div_euclid(DAY);
    let (year, _, _) = crate::gotime::civil_from_days(days);
    let yday = days - crate::gotime::days_from_civil(year, 1, 1);
    let ysec = yday * DAY + sec % DAY;
    let ystart = sec - ysec;
    let mut start_sec = rule_time(year, &start_rule, std_offset);
    let mut end_sec = rule_time(year, &end_rule, dst_offset);
    // In the southern hemisphere the year starts in daylight time: swap the labels.
    if end_sec < start_sec {
        std::mem::swap(&mut start_sec, &mut end_sec);
        std::mem::swap(&mut std_name, &mut dst_name);
        std::mem::swap(&mut std_offset, &mut dst_offset);
    }
    Some(if ysec < start_sec {
        Lookup { name: std_name, offset: std_offset, start: ystart, end: start_sec + ystart }
    } else if ysec >= end_sec {
        Lookup { name: std_name, offset: std_offset, start: end_sec + ystart, end: ystart + 365 * DAY }
    } else {
        Lookup { name: dst_name, offset: dst_offset, start: start_sec + ystart, end: end_sec + ystart }
    })
}

fn tzset_name(s: &[u8]) -> Option<(Vec<u8>, &[u8])> {
    if s.is_empty() {
        return None;
    }
    if s[0] == b'<' {
        let i = s.iter().position(|c| *c == b'>')?;
        return Some((s[1..i].to_vec(), &s[i + 1..]));
    }
    match s.iter().position(|c| c.is_ascii_digit() || matches!(c, b',' | b'-' | b'+')) {
        Some(i) if i < 3 => None,
        Some(i) => Some((s[..i].to_vec(), &s[i..])),
        None if s.len() < 3 => None,
        None => Some((s.to_vec(), &s[s.len()..])),
    }
}

fn tzset_num(s: &[u8], min: i64, max: i64) -> Option<(i64, &[u8])> {
    let digits = s.iter().position(|c| !c.is_ascii_digit()).unwrap_or(s.len());
    if digits == 0 {
        return None;
    }
    let mut num = 0;
    for c in &s[..digits] {
        num = num * 10 + (*c - b'0') as i64;
        if num > max {
            return None;
        }
    }
    if num < min {
        return None;
    }
    Some((num, &s[digits..]))
}

fn tzset_offset(s: &[u8]) -> Option<(i64, &[u8])> {
    let (neg, s) = match s.first()? {
        b'+' => (false, &s[1..]),
        b'-' => (true, &s[1..]),
        _ => (false, s),
    };
    let (hours, mut s) = tzset_num(s, 0, 24 * 7)?;
    let mut off = hours * HOUR;
    if s.first() == Some(&b':') {
        let (mins, rest) = tzset_num(&s[1..], 0, 59)?;
        off += mins * 60;
        s = rest;
        if s.first() == Some(&b':') {
            let (secs, rest) = tzset_num(&s[1..], 0, 59)?;
            off += secs;
            s = rest;
        }
    }
    Some((if neg { -off } else { off }, s))
}

enum RuleKind {
    Julian,
    Doy,
    MonthWeekDay,
}

struct Rule {
    kind: RuleKind,
    day: i64,
    week: i64,
    mon: i64,
    time: i64,
}

fn tzset_rule(s: &[u8]) -> Option<(Rule, &[u8])> {
    let mut r = Rule { kind: RuleKind::Doy, day: 0, week: 0, mon: 0, time: 2 * HOUR };
    let mut s = match s.first()? {
        b'J' => {
            let (day, rest) = tzset_num(&s[1..], 1, 365)?;
            r.kind = RuleKind::Julian;
            r.day = day;
            rest
        }
        b'M' => {
            let (mon, rest) = tzset_num(&s[1..], 1, 12)?;
            if rest.first() != Some(&b'.') {
                return None;
            }
            let (week, rest) = tzset_num(&rest[1..], 1, 5)?;
            if rest.first() != Some(&b'.') {
                return None;
            }
            let (day, rest) = tzset_num(&rest[1..], 0, 6)?;
            r.kind = RuleKind::MonthWeekDay;
            r.mon = mon;
            r.week = week;
            r.day = day;
            rest
        }
        _ => {
            let (day, rest) = tzset_num(s, 0, 365)?;
            r.day = day;
            rest
        }
    };
    if s.first() == Some(&b'/') {
        let (time, rest) = tzset_offset(&s[1..])?;
        r.time = time;
        s = rest;
    }
    Some((r, s))
}

fn is_leap(y: i64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

const DAYS_BEFORE: [i64; 13] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334, 365];

fn days_in(mon: i64, year: i64) -> i64 {
    if mon == 2 && is_leap(year) {
        return 29;
    }
    DAYS_BEFORE[mon as usize] - DAYS_BEFORE[mon as usize - 1]
}

/// tzruleTime: seconds from the start of the year to where the rule takes effect.
fn rule_time(year: i64, r: &Rule, off: i64) -> i64 {
    let s = match r.kind {
        RuleKind::Julian => (r.day - 1) * DAY + if is_leap(year) && r.day >= 60 { DAY } else { 0 },
        RuleKind::Doy => r.day * DAY,
        RuleKind::MonthWeekDay => {
            // Zeller's congruence: the weekday of the month's first day.
            let m1 = (r.mon + 9) % 12 + 1;
            let yy0 = if r.mon <= 2 { year - 1 } else { year };
            let (yy1, yy2) = (yy0 / 100, yy0 % 100);
            let mut dow = ((26 * m1 - 2) / 10 + 1 + yy2 + yy2 / 4 + yy1 / 4 - 2 * yy1) % 7;
            if dow < 0 {
                dow += 7;
            }
            let mut d = r.day - dow;
            if d < 0 {
                d += 7;
            }
            for _ in 1..r.week {
                if d + 7 >= days_in(r.mon, year) {
                    break;
                }
                d += 7;
            }
            d += DAYS_BEFORE[r.mon as usize - 1];
            if is_leap(year) && r.mon > 2 {
                d += 1;
            }
            d * DAY
        }
    };
    s + r.time - off
}
