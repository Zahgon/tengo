//! The times module (stdlib/times.go) and Go's time.Parse. The clock functions (now, since,
//! until, sleep) read the real clock; named locations other than UTC and Local are unknown.

use std::rc::Rc;

use crate::gotime::*;
use crate::objects::*;
use crate::stdlib::ORD;

const ORDINALS: [&str; 8] = ["first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth"];

fn arg_time(args: &[Object], i: usize) -> TResult<GoTime> {
    match &args[i] {
        Object::Time(t) => Ok((**t).clone()),
        Object::Int(s) => Ok(GoTime::unix(*s, 0)),
        o => Err(TErr::arg(ORDINALS[i], "time(compatible)", o)),
    }
}

fn arg_i(args: &[Object], i: usize) -> TResult<i64> {
    to_int(&args[i]).ok_or_else(|| TErr::arg(ORDINALS[i], "int(compatible)", &args[i]))
}

fn arg_s(args: &[Object], i: usize) -> TResult<Vec<u8>> {
    let (s, ok) = to_string(&args[i])?;
    if !ok {
        return Err(TErr::arg(ORDINALS[i], "string(compatible)", &args[i]));
    }
    Ok(s)
}

fn nargs(args: &[Object], n: usize) -> TResult<()> {
    if args.len() != n {
        return Err(TErr::WrongNumArguments);
    }
    Ok(())
}

fn time_obj(t: GoTime) -> Object {
    Object::Time(Rc::new(t))
}

fn now() -> GoTime {
    let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    GoTime::unix(d.as_secs() as i64, d.subsec_nanos() as i64)
}

/// time.LoadLocation for the zones this build knows.
fn load_location(name: &[u8]) -> Result<Loc, Vec<u8>> {
    match name {
        b"" | b"UTC" => Ok(Loc::Utc),
        b"Local" => Ok(Loc::Local),
        _ => {
            if name.windows(2).any(|w| w == b"..") || name.first() == Some(&b'/') || name.contains(&b'\\') {
                return Err(b"time: invalid location name".to_vec());
            }
            let mut m = b"unknown time zone ".to_vec();
            m.extend_from_slice(name);
            Err(m)
        }
    }
}

fn duration_hours(d: i64) -> f64 {
    let hour = d / HOUR;
    let nsec = d % HOUR;
    hour as f64 + nsec as f64 / (60.0 * 60.0 * 1e9)
}

fn duration_minutes(d: i64) -> f64 {
    let min = d / MINUTE;
    let nsec = d % MINUTE;
    min as f64 + nsec as f64 / (60.0 * 1e9)
}

fn duration_seconds(d: i64) -> f64 {
    let sec = d / SECOND;
    let nsec = d % SECOND;
    sec as f64 + nsec as f64 / 1e9
}

/// Time.MarshalJSON
pub fn marshal_json(t: &GoTime) -> Result<Vec<u8>, Vec<u8>> {
    let mut b = b"\"".to_vec();
    let s = t.format(b"2006-01-02T15:04:05.999999999Z07:00");
    let (_, _, offset_ok) = (0, 0, true);
    let _ = offset_ok;
    let year_ok = s.len() > 4 && s[4] == b'-';
    b.extend_from_slice(&s);
    b.push(b'"');
    if !year_ok {
        return Err(b"Time.MarshalJSON: year outside of range [0,9999]".to_vec());
    }
    let n = s.len();
    if s[n - 1] != b'Z' {
        let c = s[n - 6];
        let hh = (s[n - 5] - b'0') as i32 * 10 + (s[n - 4] - b'0') as i32;
        if c.is_ascii_digit() || hh >= 24 {
            return Err(b"Time.MarshalJSON: timezone hour outside of range [0,23]".to_vec());
        }
    }
    Ok(b)
}

pub fn times_module() -> Vec<(&'static str, Object)> {
    let mut v: Vec<(&'static str, Object)> = vec![
        ("format_ansic", new_str("Mon Jan _2 15:04:05 2006")),
        ("format_unix_date", new_str("Mon Jan _2 15:04:05 MST 2006")),
        ("format_ruby_date", new_str("Mon Jan 02 15:04:05 -0700 2006")),
        ("format_rfc822", new_str("02 Jan 06 15:04 MST")),
        ("format_rfc822z", new_str("02 Jan 06 15:04 -0700")),
        ("format_rfc850", new_str("Monday, 02-Jan-06 15:04:05 MST")),
        ("format_rfc1123", new_str("Mon, 02 Jan 2006 15:04:05 MST")),
        ("format_rfc1123z", new_str("Mon, 02 Jan 2006 15:04:05 -0700")),
        ("format_rfc3339", new_str("2006-01-02T15:04:05Z07:00")),
        ("format_rfc3339_nano", new_str("2006-01-02T15:04:05.999999999Z07:00")),
        ("format_kitchen", new_str("3:04PM")),
        ("format_stamp", new_str("Jan _2 15:04:05")),
        ("format_stamp_milli", new_str("Jan _2 15:04:05.000")),
        ("format_stamp_micro", new_str("Jan _2 15:04:05.000000")),
        ("format_stamp_nano", new_str("Jan _2 15:04:05.000000000")),
        ("nanosecond", Object::Int(NANOSECOND)),
        ("microsecond", Object::Int(MICROSECOND)),
        ("millisecond", Object::Int(MILLISECOND)),
        ("second", Object::Int(SECOND)),
        ("minute", Object::Int(MINUTE)),
        ("hour", Object::Int(HOUR)),
    ];
    let months = [
        "january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november",
        "december",
    ];
    for (i, m) in months.iter().enumerate() {
        v.push((m, Object::Int(i as i64 + 1)));
    }
    v.extend(vec![
        (
            "sleep",
            user_fn("sleep", |args| {
                nargs(args, 1)?;
                let d = arg_i(args, 0)?;
                if d > 0 {
                    std::thread::sleep(std::time::Duration::from_nanos(d as u64));
                }
                Ok(Object::Undefined)
            }),
        ),
        (
            "parse_duration",
            user_fn("parse_duration", |args| {
                nargs(args, 1)?;
                let s = arg_s(args, 0)?;
                // Go assigns the error to the named err result: a runtime error.
                match parse_duration(&s) {
                    Ok(d) => Ok(Object::Int(d)),
                    Err(e) => Err(TErr::Msg(e)),
                }
            }),
        ),
        (
            "since",
            user_fn("since", |args| {
                nargs(args, 1)?;
                let t = arg_time(args, 0)?;
                Ok(Object::Int(now().sub(&t)))
            }),
        ),
        (
            "until",
            user_fn("until", |args| {
                nargs(args, 1)?;
                let t = arg_time(args, 0)?;
                Ok(Object::Int(t.sub(&now())))
            }),
        ),
        (
            "duration_hours",
            user_fn("duration_hours", |args| {
                nargs(args, 1)?;
                Ok(Object::Float(duration_hours(arg_i(args, 0)?)))
            }),
        ),
        (
            "duration_minutes",
            user_fn("duration_minutes", |args| {
                nargs(args, 1)?;
                Ok(Object::Float(duration_minutes(arg_i(args, 0)?)))
            }),
        ),
        (
            "duration_nanoseconds",
            user_fn("duration_nanoseconds", |args| {
                nargs(args, 1)?;
                Ok(Object::Int(arg_i(args, 0)?))
            }),
        ),
        (
            "duration_seconds",
            user_fn("duration_seconds", |args| {
                nargs(args, 1)?;
                Ok(Object::Float(duration_seconds(arg_i(args, 0)?)))
            }),
        ),
        (
            "duration_string",
            user_fn("duration_string", |args| {
                nargs(args, 1)?;
                Ok(new_str(duration_string(arg_i(args, 0)?)))
            }),
        ),
        (
            "month_string",
            user_fn("month_string", |args| {
                nargs(args, 1)?;
                Ok(new_str(month_string(arg_i(args, 0)?)))
            }),
        ),
        (
            "date",
            user_fn("date", |args| {
                if args.len() < 7 || args.len() > 8 {
                    return Err(TErr::WrongNumArguments);
                }
                let mut v = [0i64; 7];
                for (i, slot) in v.iter_mut().enumerate() {
                    *slot = arg_i(args, i)?;
                }
                let loc = if args.len() == 8 {
                    let name = arg_s(args, 7)?;
                    match load_location(&name) {
                        Ok(l) => l,
                        Err(e) => return Err(TErr::Msg(e)),
                    }
                } else {
                    Loc::Local
                };
                Ok(time_obj(GoTime::date(v[0], v[1], v[2], v[3], v[4], v[5], v[6], loc)))
            }),
        ),
        (
            "now",
            user_fn("now", |args| {
                nargs(args, 0)?;
                Ok(time_obj(now()))
            }),
        ),
        (
            "parse",
            user_fn("parse", |args| {
                nargs(args, 2)?;
                let layout = arg_s(args, 0)?;
                let value = arg_s(args, 1)?;
                match parse(&layout, &value) {
                    Ok(t) => Ok(time_obj(t)),
                    Err(e) => Err(TErr::Msg(e)),
                }
            }),
        ),
        (
            "unix",
            user_fn("unix", |args| {
                nargs(args, 2)?;
                let s = arg_i(args, 0)?;
                let n = arg_i(args, 1)?;
                Ok(time_obj(GoTime::unix(s, n)))
            }),
        ),
        (
            "add",
            user_fn("add", |args| {
                nargs(args, 2)?;
                let t = arg_time(args, 0)?;
                let d = arg_i(args, 1)?;
                Ok(time_obj(t.add(d)))
            }),
        ),
        (
            "sub",
            user_fn("sub", |args| {
                nargs(args, 2)?;
                let t = arg_time(args, 0)?;
                let u = arg_time(args, 1)?;
                Ok(Object::Int(t.sub(&u)))
            }),
        ),
        (
            "add_date",
            user_fn("add_date", |args| {
                nargs(args, 4)?;
                let t = arg_time(args, 0)?;
                let y = arg_i(args, 1)?;
                let m = arg_i(args, 2)?;
                let d = arg_i(args, 3)?;
                Ok(time_obj(t.add_date(y, m, d)))
            }),
        ),
        (
            "after",
            user_fn("after", |args| {
                nargs(args, 2)?;
                let t = arg_time(args, 0)?;
                let u = arg_time(args, 1)?;
                Ok(Object::Bool(t.after(&u)))
            }),
        ),
        (
            "before",
            user_fn("before", |args| {
                nargs(args, 2)?;
                let t = arg_time(args, 0)?;
                let u = arg_time(args, 1)?;
                Ok(Object::Bool(t.before(&u)))
            }),
        ),
        ("time_year", time_int("time_year", |t| t.ymd().0)),
        ("time_month", time_int("time_month", |t| t.ymd().1)),
        ("time_day", time_int("time_day", |t| t.ymd().2)),
        ("time_weekday", time_int("time_weekday", |t| t.weekday())),
        ("time_hour", time_int("time_hour", |t| t.clock().0)),
        ("time_minute", time_int("time_minute", |t| t.clock().1)),
        ("time_second", time_int("time_second", |t| t.clock().2)),
        ("time_nanosecond", time_int("time_nanosecond", |t| t.nsec)),
        ("time_unix", time_int("time_unix", |t| t.sec)),
        ("time_unix_nano", time_int("time_unix_nano", |t| t.unix_nano())),
        (
            "time_format",
            user_fn("time_format", |args| {
                nargs(args, 2)?;
                let t = arg_time(args, 0)?;
                let f = arg_s(args, 1)?;
                Ok(new_str(t.format(&f)))
            }),
        ),
        (
            "time_location",
            user_fn("time_location", |args| {
                nargs(args, 1)?;
                let t = arg_time(args, 0)?;
                Ok(new_str(t.loc.name()))
            }),
        ),
        (
            "time_string",
            user_fn("time_string", |args| {
                nargs(args, 1)?;
                let t = arg_time(args, 0)?;
                Ok(new_str(t.string()))
            }),
        ),
        (
            "is_zero",
            user_fn("is_zero", |args| {
                nargs(args, 1)?;
                let t = arg_time(args, 0)?;
                Ok(Object::Bool(t.is_zero()))
            }),
        ),
        (
            "to_local",
            user_fn("to_local", |args| {
                nargs(args, 1)?;
                let t = arg_time(args, 0)?;
                Ok(time_obj(t.with_loc(Loc::Local)))
            }),
        ),
        (
            "to_utc",
            user_fn("to_utc", |args| {
                nargs(args, 1)?;
                let t = arg_time(args, 0)?;
                Ok(time_obj(t.with_loc(Loc::Utc)))
            }),
        ),
        (
            "in_location",
            user_fn("in_location", |args| {
                nargs(args, 2)?;
                let t = arg_time(args, 0)?;
                let name = arg_s(args, 1)?;
                match load_location(&name) {
                    Ok(l) => Ok(time_obj(t.with_loc(l))),
                    Err(e) => Err(TErr::Msg(e)),
                }
            }),
        ),
    ]);
    let _ = ORD;
    v
}

fn time_int(name: &str, f: fn(&GoTime) -> i64) -> Object {
    user_fn(name, move |args| {
        nargs(args, 1)?;
        let t = arg_time(args, 0)?;
        Ok(Object::Int(f(&t)))
    })
}

// ---------------------------------------------------------------------------------------------
// time.Parse

const SHORT_DAY: [&[u8]; 7] = [b"Sun", b"Mon", b"Tue", b"Wed", b"Thu", b"Fri", b"Sat"];
const LONG_DAY: [&[u8]; 7] = [b"Sunday", b"Monday", b"Tuesday", b"Wednesday", b"Thursday", b"Friday", b"Saturday"];
const SHORT_MONTH: [&[u8]; 12] =
    [b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun", b"Jul", b"Aug", b"Sep", b"Oct", b"Nov", b"Dec"];
const LONG_MONTH: [&[u8]; 12] = [
    b"January", b"February", b"March", b"April", b"May", b"June", b"July", b"August", b"September", b"October",
    b"November", b"December",
];

fn match_fold(s1: &[u8], s2: &[u8]) -> bool {
    for i in 0..s1.len() {
        let (mut c1, mut c2) = (s1[i], s2[i]);
        if c1 != c2 {
            c1 |= b'a' - b'A';
            c2 |= b'a' - b'A';
            if c1 != c2 || !c1.is_ascii_lowercase() {
                return false;
            }
        }
    }
    true
}

fn lookup<'a>(tab: &[&[u8]], val: &'a [u8]) -> Result<(i64, &'a [u8]), ()> {
    for (i, v) in tab.iter().enumerate() {
        if val.len() >= v.len() && match_fold(&val[..v.len()], v) {
            return Ok((i as i64, &val[v.len()..]));
        }
    }
    Err(())
}

fn is_digit(s: &[u8], i: usize) -> bool {
    i < s.len() && s[i].is_ascii_digit()
}

fn getnum(s: &[u8], fixed: bool) -> Result<(i64, &[u8]), ()> {
    if !is_digit(s, 0) {
        return Err(());
    }
    if !is_digit(s, 1) {
        if fixed {
            return Err(());
        }
        return Ok(((s[0] - b'0') as i64, &s[1..]));
    }
    Ok(((s[0] - b'0') as i64 * 10 + (s[1] - b'0') as i64, &s[2..]))
}

fn getnum3(s: &[u8], fixed: bool) -> Result<(i64, &[u8]), ()> {
    let mut n = 0i64;
    let mut i = 0;
    while i < 3 && is_digit(s, i) {
        n = n * 10 + (s[i] - b'0') as i64;
        i += 1;
    }
    if i == 0 || fixed && i != 3 {
        return Err(());
    }
    Ok((n, &s[i..]))
}

fn cutspace(s: &[u8]) -> &[u8] {
    let mut s = s;
    while !s.is_empty() && s[0] == b' ' {
        s = &s[1..];
    }
    s
}

fn skip<'a>(value: &'a [u8], prefix: &[u8]) -> Result<&'a [u8], &'a [u8]> {
    let mut value = value;
    let mut prefix = prefix;
    while !prefix.is_empty() {
        if prefix[0] == b' ' {
            if !value.is_empty() && value[0] != b' ' {
                return Err(value);
            }
            prefix = cutspace(prefix);
            value = cutspace(value);
            continue;
        }
        if value.is_empty() || value[0] != prefix[0] {
            return Err(value);
        }
        prefix = &prefix[1..];
        value = &value[1..];
    }
    Ok(value)
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

fn atoi(s: &[u8]) -> Result<i64, ()> {
    let mut s = s;
    let mut neg = false;
    if !s.is_empty() && (s[0] == b'-' || s[0] == b'+') {
        neg = s[0] == b'-';
        s = &s[1..];
    }
    let (q, rem) = leading_int(s)?;
    if !rem.is_empty() {
        return Err(());
    }
    let x = q as i64;
    Ok(if neg { x.wrapping_neg() } else { x })
}

fn parse_nanoseconds(value: &[u8], nbytes: usize) -> Result<(i64, &'static str), ()> {
    if !(value[0] == b'.' || value[0] == b',') {
        return Err(());
    }
    let (value, nbytes) = if nbytes > 10 { (&value[..10], 10) } else { (value, nbytes) };
    let mut ns = atoi(&value[1..nbytes])?;
    if ns < 0 {
        return Ok((0, "fractional second"));
    }
    for _ in 0..10 - nbytes {
        ns *= 10;
    }
    Ok((ns, ""))
}

fn parse_signed_offset(value: &[u8]) -> usize {
    let sign = value[0];
    if sign != b'-' && sign != b'+' {
        return 0;
    }
    match leading_int(&value[1..]) {
        Ok((x, rem)) => {
            if rem.len() == value.len() - 1 || x > 23 {
                return 0;
            }
            value.len() - rem.len()
        }
        Err(()) => 0,
    }
}

fn parse_time_zone(value: &[u8]) -> Option<usize> {
    if value.len() < 3 {
        return None;
    }
    if value.len() >= 4 && (&value[..4] == b"ChST" || &value[..4] == b"MeST") {
        return Some(4);
    }
    if &value[..3] == b"GMT" {
        let rest = &value[3..];
        if rest.is_empty() {
            return Some(3);
        }
        return Some(3 + parse_signed_offset(rest));
    }
    if value[0] == b'+' || value[0] == b'-' {
        let l = parse_signed_offset(value);
        return if l > 0 { Some(l) } else { None };
    }
    let mut n_upper = 0;
    while n_upper < 6 {
        if n_upper >= value.len() {
            break;
        }
        let c = value[n_upper];
        if !c.is_ascii_uppercase() {
            break;
        }
        n_upper += 1;
    }
    match n_upper {
        5 => {
            if value[4] == b'T' {
                return Some(5);
            }
        }
        4 => {
            if value[3] == b'T' || &value[..4] == b"WITA" {
                return Some(4);
            }
        }
        3 => return Some(3),
        _ => {}
    }
    None
}

fn days_before(m: i64) -> i64 {
    const DB: [i64; 13] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334, 365];
    DB[(m - 1) as usize]
}

fn days_in(m: i64, year: i64) -> i64 {
    if m == 2 && is_leap(year) {
        return 29;
    }
    days_before(m + 1) - days_before(m)
}

fn parse_error(layout: &[u8], value: &[u8], layout_elem: &[u8], value_elem: &[u8], message: &[u8]) -> Vec<u8> {
    let mut m = b"parsing time ".to_vec();
    m.extend_from_slice(&time_quote(value));
    if message.is_empty() {
        m.extend_from_slice(b" as ");
        m.extend_from_slice(&time_quote(layout));
        m.extend_from_slice(b": cannot parse ");
        m.extend_from_slice(&time_quote(value_elem));
        m.extend_from_slice(b" as ");
        m.extend_from_slice(&time_quote(layout_elem));
    } else {
        m.extend_from_slice(message);
    }
    m
}

/// time.Parse(layout, value) with UTC as the default location and Local (UTC) as local.
pub fn parse(alayout: &[u8], avalue: &[u8]) -> Result<GoTime, Vec<u8>> {
    let mut layout = alayout;
    let mut value = avalue;
    let mut range_err: &str = "";
    let mut am_set = false;
    let mut pm_set = false;
    let (mut year, mut month, mut day, mut yday) = (0i64, -1i64, -1i64, -1i64);
    let (mut hour, mut min, mut sec, mut nsec) = (0i64, 0i64, 0i64, 0i64);
    let mut z: Option<Loc> = None;
    let mut zone_offset: i64 = -1;
    let mut zone_name: Vec<u8> = Vec::new();

    loop {
        let (prefix, std, suffix) = next_std_chunk(layout);
        let stdstr = &layout[prefix.len()..layout.len() - suffix.len()];
        value = match skip(value, prefix) {
            Ok(v) => v,
            Err(v) => return Err(parse_error(alayout, avalue, prefix, v, b"")),
        };
        let std = match std {
            None => {
                if !value.is_empty() {
                    let mut msg = b": extra text: ".to_vec();
                    msg.extend_from_slice(&time_quote(value));
                    return Err(parse_error(alayout, avalue, b"", value, &msg));
                }
                break;
            }
            Some(s) => s,
        };
        layout = suffix;
        let hold = value;
        let mut bad = false;
        match std {
            Std::Year => {
                if value.len() < 2 {
                    bad = true;
                } else {
                    let p = &value[..2];
                    value = &value[2..];
                    match atoi(p) {
                        Ok(y) => year = if y >= 69 { y + 1900 } else { y + 2000 },
                        Err(()) => bad = true,
                    }
                }
            }
            Std::LongYear => {
                if value.len() < 4 || !is_digit(value, 0) {
                    bad = true;
                } else {
                    let p = &value[..4];
                    value = &value[4..];
                    match atoi(p) {
                        Ok(y) => year = y,
                        Err(()) => bad = true,
                    }
                }
            }
            Std::Month | Std::LongMonth => {
                let tab: &[&[u8]] = if std == Std::Month { &SHORT_MONTH } else { &LONG_MONTH };
                match lookup(tab, value) {
                    Ok((m, rest)) => {
                        month = m + 1;
                        value = rest;
                    }
                    Err(()) => {
                        month = 0;
                        bad = true;
                    }
                }
            }
            Std::NumMonth | Std::ZeroMonth => match getnum(value, std == Std::ZeroMonth) {
                Ok((m, rest)) => {
                    month = m;
                    value = rest;
                    if month <= 0 || 12 < month {
                        range_err = "month";
                    }
                }
                Err(()) => {
                    month = 0;
                    bad = true;
                }
            },
            Std::WeekDay | Std::LongWeekDay => {
                let tab: &[&[u8]] = if std == Std::WeekDay { &SHORT_DAY } else { &LONG_DAY };
                match lookup(tab, value) {
                    Ok((_, rest)) => value = rest,
                    Err(()) => bad = true,
                }
            }
            Std::Day | Std::UnderDay | Std::ZeroDay => {
                if std == Std::UnderDay && !value.is_empty() && value[0] == b' ' {
                    value = &value[1..];
                }
                match getnum(value, std == Std::ZeroDay) {
                    Ok((d, rest)) => {
                        day = d;
                        value = rest;
                    }
                    Err(()) => {
                        day = 0;
                        bad = true;
                    }
                }
            }
            Std::UnderYearDay | Std::ZeroYearDay => {
                for _ in 0..2 {
                    if std == Std::UnderYearDay && !value.is_empty() && value[0] == b' ' {
                        value = &value[1..];
                    }
                }
                match getnum3(value, std == Std::ZeroYearDay) {
                    Ok((d, rest)) => {
                        yday = d;
                        value = rest;
                    }
                    Err(()) => {
                        yday = 0;
                        bad = true;
                    }
                }
            }
            Std::Hour => match getnum(value, false) {
                Ok((h, rest)) => {
                    hour = h;
                    value = rest;
                    if !(0..24).contains(&hour) {
                        range_err = "hour";
                    }
                }
                Err(()) => {
                    hour = 0;
                    bad = true;
                }
            },
            Std::Hour12 | Std::ZeroHour12 => match getnum(value, std == Std::ZeroHour12) {
                Ok((h, rest)) => {
                    hour = h;
                    value = rest;
                    if !(0..=12).contains(&hour) {
                        range_err = "hour";
                    }
                }
                Err(()) => {
                    hour = 0;
                    bad = true;
                }
            },
            Std::Minute | Std::ZeroMinute => match getnum(value, std == Std::ZeroMinute) {
                Ok((m, rest)) => {
                    min = m;
                    value = rest;
                    if !(0..60).contains(&min) {
                        range_err = "minute";
                    }
                }
                Err(()) => {
                    min = 0;
                    bad = true;
                }
            },
            Std::Second | Std::ZeroSecond => match getnum(value, std == Std::ZeroSecond) {
                Ok((s, rest)) => {
                    sec = s;
                    value = rest;
                    if !(0..60).contains(&sec) {
                        range_err = "second";
                    } else if value.len() >= 2 && (value[0] == b'.' || value[0] == b',') && is_digit(value, 1) {
                        let (_, nstd, _) = next_std_chunk(layout);
                        let is_frac = matches!(nstd, Some(Std::FracSecond0(..)) | Some(Std::FracSecond9(..)));
                        if !is_frac {
                            let mut n = 2;
                            while n < value.len() && is_digit(value, n) {
                                n += 1;
                            }
                            match parse_nanoseconds(value, n) {
                                Ok((ns, re)) => {
                                    nsec = ns;
                                    range_err = re;
                                }
                                Err(()) => bad = true,
                            }
                            value = &value[n..];
                        }
                    }
                }
                Err(()) => {
                    sec = 0;
                    bad = true;
                }
            },
            Std::PM | Std::Pm => {
                if value.len() < 2 {
                    bad = true;
                } else {
                    let p = &value[..2];
                    value = &value[2..];
                    let (pm, am): (&[u8], &[u8]) = if std == Std::PM { (b"PM", b"AM") } else { (b"pm", b"am") };
                    if p == pm {
                        pm_set = true;
                    } else if p == am {
                        am_set = true;
                    } else {
                        bad = true;
                    }
                }
            }
            Std::ISO8601TZ
            | Std::ISO8601ShortTZ
            | Std::ISO8601ColonTZ
            | Std::ISO8601SecondsTZ
            | Std::ISO8601ColonSecondsTZ
            | Std::NumTZ
            | Std::NumShortTZ
            | Std::NumColonTZ
            | Std::NumSecondsTz
            | Std::NumColonSecondsTZ => {
                let iso = matches!(
                    std,
                    Std::ISO8601TZ
                        | Std::ISO8601ShortTZ
                        | Std::ISO8601ColonTZ
                        | Std::ISO8601SecondsTZ
                        | Std::ISO8601ColonSecondsTZ
                );
                if iso && !value.is_empty() && value[0] == b'Z' {
                    value = &value[1..];
                    z = Some(Loc::Utc);
                } else {
                    let parts: Option<(&[u8], &[u8], &[u8], &[u8], &[u8])> = match std {
                        Std::ISO8601ColonTZ | Std::NumColonTZ => {
                            if value.len() < 6 || value[3] != b':' {
                                None
                            } else {
                                Some((&value[0..1], &value[1..3], &value[4..6], b"00", &value[6..]))
                            }
                        }
                        Std::NumShortTZ | Std::ISO8601ShortTZ => {
                            if value.len() < 3 {
                                None
                            } else {
                                Some((&value[0..1], &value[1..3], b"00", b"00", &value[3..]))
                            }
                        }
                        Std::ISO8601ColonSecondsTZ | Std::NumColonSecondsTZ => {
                            if value.len() < 9 || value[3] != b':' || value[6] != b':' {
                                None
                            } else {
                                Some((&value[0..1], &value[1..3], &value[4..6], &value[7..9], &value[9..]))
                            }
                        }
                        Std::ISO8601SecondsTZ | Std::NumSecondsTz => {
                            if value.len() < 7 {
                                None
                            } else {
                                Some((&value[0..1], &value[1..3], &value[3..5], &value[5..7], &value[7..]))
                            }
                        }
                        _ => {
                            if value.len() < 5 {
                                None
                            } else {
                                Some((&value[0..1], &value[1..3], &value[3..5], b"00", &value[5..]))
                            }
                        }
                    };
                    match parts {
                        None => bad = true,
                        Some((sign, h, m, s, rest)) => {
                            value = rest;
                            let (mut hr, mut mm, mut ss) = (0i64, 0i64, 0i64);
                            let mut err = false;
                            match getnum(h, true) {
                                Ok((v, _)) => {
                                    hr = v;
                                    match getnum(m, true) {
                                        Ok((v, _)) => {
                                            mm = v;
                                            match getnum(s, true) {
                                                Ok((v, _)) => ss = v,
                                                Err(()) => err = true,
                                            }
                                        }
                                        Err(()) => err = true,
                                    }
                                }
                                Err(()) => err = true,
                            }
                            if hr > 24 {
                                range_err = "time zone offset hour";
                            }
                            if mm > 60 {
                                range_err = "time zone offset minute";
                            }
                            if ss > 60 {
                                range_err = "time zone offset second";
                            }
                            zone_offset = (hr * 60 + mm) * 60 + ss;
                            match sign[0] {
                                b'+' => {}
                                b'-' => zone_offset = -zone_offset,
                                _ => err = true,
                            }
                            if err {
                                bad = true;
                            }
                        }
                    }
                }
            }
            Std::TZ => {
                if value.len() >= 3 && &value[..3] == b"UTC" {
                    z = Some(Loc::Utc);
                    value = &value[3..];
                } else {
                    match parse_time_zone(value) {
                        Some(n) => {
                            zone_name = value[..n].to_vec();
                            value = &value[n..];
                        }
                        None => bad = true,
                    }
                }
            }
            Std::FracSecond0(digits, _) => {
                let ndigit = 1 + digits;
                if value.len() < ndigit {
                    bad = true;
                } else {
                    match parse_nanoseconds(value, ndigit) {
                        Ok((ns, re)) => {
                            nsec = ns;
                            range_err = re;
                        }
                        Err(()) => bad = true,
                    }
                    value = &value[ndigit..];
                }
            }
            Std::FracSecond9(..) => {
                if value.len() >= 2 && (value[0] == b'.' || value[0] == b',') && value[1].is_ascii_digit() {
                    let mut i = 0;
                    while i + 1 < value.len() && value[i + 1].is_ascii_digit() {
                        i += 1;
                    }
                    match parse_nanoseconds(value, 1 + i) {
                        Ok((ns, re)) => {
                            nsec = ns;
                            range_err = re;
                        }
                        Err(()) => bad = true,
                    }
                    value = &value[1 + i..];
                }
            }
        }
        if !range_err.is_empty() {
            let msg = format!(": {} out of range", range_err);
            return Err(parse_error(alayout, avalue, stdstr, value, msg.as_bytes()));
        }
        if bad {
            return Err(parse_error(alayout, avalue, stdstr, hold, b""));
        }
    }
    if pm_set && hour < 12 {
        hour += 12;
    } else if am_set && hour == 12 {
        hour = 0;
    }
    if yday >= 0 {
        let mut d = 0i64;
        let mut m = 0i64;
        if is_leap(year) {
            if yday == 31 + 29 {
                m = 2;
                d = 29;
            } else if yday > 31 + 29 {
                yday -= 1;
            }
        }
        if !(1..=365).contains(&yday) {
            return Err(parse_error(alayout, avalue, b"", value, b": day-of-year out of range"));
        }
        if m == 0 {
            m = (yday - 1) / 31 + 1;
            if days_before(m + 1) < yday {
                m += 1;
            }
            d = yday - days_before(m);
        }
        if month >= 0 && month != m {
            return Err(parse_error(alayout, avalue, b"", value, b": day-of-year does not match month"));
        }
        month = m;
        if day >= 0 && day != d {
            return Err(parse_error(alayout, avalue, b"", value, b": day-of-year does not match day"));
        }
        day = d;
    } else {
        if month < 0 {
            month = 1;
        }
        if day < 0 {
            day = 1;
        }
    }
    if day < 1 || day > days_in(month, year) {
        return Err(parse_error(alayout, avalue, b"", value, b": day out of range"));
    }
    if let Some(loc) = z {
        return Ok(GoTime::date(year, month, day, hour, min, sec, nsec, loc));
    }
    if zone_offset != -1 {
        let t = GoTime::date(year, month, day, hour, min, sec, nsec, Loc::Utc);
        let t = GoTime { sec: t.sec - zone_offset, nsec: t.nsec, loc: Loc::Utc };
        // Local is UTC here: name "UTC", offset 0.
        if zone_offset == 0 && (zone_name.is_empty() || zone_name == b"UTC") {
            return Ok(t.with_loc(Loc::Local));
        }
        return Ok(t.with_loc(Loc::Fixed { name: zone_name, offset: zone_offset }));
    }
    if !zone_name.is_empty() {
        let t = GoTime::date(year, month, day, hour, min, sec, nsec, Loc::Utc);
        if zone_name == b"UTC" {
            return Ok(t.with_loc(Loc::Local));
        }
        let mut offset = 0;
        if zone_name.len() > 3 && &zone_name[..3] == b"GMT" {
            offset = atoi(&zone_name[3..]).unwrap_or(0) * 3600;
        }
        return Ok(t.with_loc(Loc::Fixed { name: zone_name, offset }));
    }
    Ok(GoTime::date(year, month, day, hour, min, sec, nsec, Loc::Utc))
}
