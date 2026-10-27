//! Go string semantics: UTF-8 decoding with U+FFFD for invalid bytes, the unicode package's
//! predicates and simple case mappings, and the strconv quoting and integer parsers.

use crate::gotables;

pub const RUNE_ERROR: i32 = 0xFFFD;
pub const MAX_RUNE: i32 = 0x10FFFF;
pub const RUNE_SELF: i32 = 0x80;

// ---------------------------------------------------------------------------------------------
// UTF-8

/// utf8.DecodeRune: the rune at the start of `b` and its width; (RuneError, 1) for invalid
/// encodings and (RuneError, 0) for empty input.
pub fn decode_rune(b: &[u8]) -> (i32, usize) {
    if b.is_empty() {
        return (RUNE_ERROR, 0);
    }
    let b0 = b[0];
    if b0 < 0x80 {
        return (b0 as i32, 1);
    }
    let (need, min, init) = match b0 {
        0xC2..=0xDF => (1, 0x80, (b0 & 0x1F) as i32),
        0xE0..=0xEF => (2, 0x800, (b0 & 0x0F) as i32),
        0xF0..=0xF4 => (3, 0x10000, (b0 & 0x07) as i32),
        _ => return (RUNE_ERROR, 1),
    };
    if b.len() < need + 1 {
        // Go reports width 1 for any incomplete sequence.
        return (RUNE_ERROR, 1);
    }
    // Second-byte ranges are restricted for E0, ED, F0 and F4 (overlongs and surrogates).
    let (lo, hi) = match b0 {
        0xE0 => (0xA0, 0xBF),
        0xED => (0x80, 0x9F),
        0xF0 => (0x90, 0xBF),
        0xF4 => (0x80, 0x8F),
        _ => (0x80, 0xBF),
    };
    let b1 = b[1];
    if b1 < lo || b1 > hi {
        return (RUNE_ERROR, 1);
    }
    let mut r = (init << 6) | (b1 & 0x3F) as i32;
    for k in 2..=need {
        let c = b[k];
        if !(0x80..=0xBF).contains(&c) {
            return (RUNE_ERROR, 1);
        }
        r = (r << 6) | (c & 0x3F) as i32;
    }
    let _ = min;
    (r, need + 1)
}

/// utf8.DecodeLastRune.
pub fn decode_last_rune(b: &[u8]) -> (i32, usize) {
    let end = b.len();
    if end == 0 {
        return (RUNE_ERROR, 0);
    }
    let r = b[end - 1];
    if r < 0x80 {
        return (r as i32, 1);
    }
    let lim: isize = end as isize - 4;
    let lim = lim.max(0);
    let mut start: isize = end as isize - 2;
    while start >= lim {
        if (b[start as usize] & 0xC0) != 0x80 {
            break;
        }
        start -= 1;
    }
    if start < 0 {
        start = 0;
    }
    let start = start as usize;
    let (r, size) = decode_rune(&b[start..end]);
    if start + size != end {
        return (RUNE_ERROR, 1);
    }
    (r, size)
}

pub fn valid_rune(r: i32) -> bool {
    (0..0xD800).contains(&r) || (0xE000..=MAX_RUNE).contains(&r)
}

/// utf8.AppendRune / string(rune): invalid runes encode as U+FFFD.
pub fn append_rune(out: &mut Vec<u8>, r: i32) {
    let r = if valid_rune(r) { r as u32 } else { RUNE_ERROR as u32 };
    let c = char::from_u32(r).unwrap();
    let mut buf = [0u8; 4];
    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
}

pub fn rune_len(r: i32) -> i32 {
    if r < 0 {
        -1
    } else if r < 0x80 {
        1
    } else if r < 0x800 {
        2
    } else if (0xD800..=0xDFFF).contains(&r) {
        -1
    } else if r < 0x10000 {
        3
    } else if r <= MAX_RUNE {
        4
    } else {
        -1
    }
}

pub fn rune_string(r: i32) -> Vec<u8> {
    let mut v = Vec::with_capacity(4);
    append_rune(&mut v, r);
    v
}

/// []rune(s)
pub fn runes(s: &[u8]) -> Vec<i32> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let (r, w) = decode_rune(&s[i..]);
        out.push(r);
        i += w;
    }
    out
}

pub fn rune_count(s: &[u8]) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i < s.len() {
        let (_, w) = decode_rune(&s[i..]);
        i += w;
        n += 1;
    }
    n
}

pub fn valid_string(s: &[u8]) -> bool {
    let mut i = 0;
    while i < s.len() {
        let (r, w) = decode_rune(&s[i..]);
        if r == RUNE_ERROR && w == 1 {
            return false;
        }
        i += w;
    }
    true
}

// ---------------------------------------------------------------------------------------------
// unicode package

fn in_table(t: &[(u32, u32)], r: i32) -> bool {
    if r < 0 {
        return false;
    }
    let r = r as u32;
    let mut lo = 0usize;
    let mut hi = t.len();
    while lo < hi {
        let m = (lo + hi) / 2;
        let (a, b) = t[m];
        if r < a {
            hi = m;
        } else if r > b {
            lo = m + 1;
        } else {
            return true;
        }
    }
    false
}

pub fn is_letter(r: i32) -> bool {
    in_table(gotables::IS_LETTER, r)
}
pub fn is_digit(r: i32) -> bool {
    in_table(gotables::IS_DIGIT, r)
}
pub fn is_number(r: i32) -> bool {
    in_table(gotables::IS_NUMBER, r)
}
pub fn is_space(r: i32) -> bool {
    in_table(gotables::IS_SPACE, r)
}
pub fn is_upper(r: i32) -> bool {
    in_table(gotables::IS_UPPER, r)
}
pub fn is_lower(r: i32) -> bool {
    in_table(gotables::IS_LOWER, r)
}
pub fn is_title(r: i32) -> bool {
    in_table(gotables::IS_TITLE, r)
}
pub fn is_punct(r: i32) -> bool {
    in_table(gotables::IS_PUNCT, r)
}
pub fn is_mark(r: i32) -> bool {
    in_table(gotables::IS_MARK, r)
}
pub fn is_symbol(r: i32) -> bool {
    in_table(gotables::IS_SYMBOL, r)
}
pub fn strconv_is_print(r: i32) -> bool {
    in_table(gotables::STRCONV_IS_PRINT, r)
}
pub fn strconv_is_graphic(r: i32) -> bool {
    in_table(gotables::STRCONV_IS_GRAPHIC, r)
}

fn case_entry(r: i32) -> Option<(u32, u32, u32, u32)> {
    if r < 0 {
        return None;
    }
    let t = gotables::CASE_MAP;
    let r = r as u32;
    t.binary_search_by_key(&r, |e| e.0).ok().map(|i| t[i])
}

pub fn to_upper(r: i32) -> i32 {
    case_entry(r).map(|e| e.1 as i32).unwrap_or(r)
}
pub fn to_lower(r: i32) -> i32 {
    case_entry(r).map(|e| e.2 as i32).unwrap_or(r)
}
pub fn to_title(r: i32) -> i32 {
    case_entry(r).map(|e| e.3 as i32).unwrap_or(r)
}

/// unicode.SimpleFold.
pub fn simple_fold(r: i32) -> i32 {
    if r < 0 || r > MAX_RUNE {
        return r;
    }
    let t = gotables::SIMPLE_FOLD;
    match t.binary_search_by_key(&(r as u32), |e| e.0) {
        Ok(i) => t[i].1 as i32,
        Err(_) => r,
    }
}

// ---------------------------------------------------------------------------------------------
// strconv quoting

const LOWERHEX: &[u8] = b"0123456789abcdef";

fn append_escaped_rune(buf: &mut Vec<u8>, r: i32, quote: u8, ascii_only: bool) {
    if r == quote as i32 || r == '\\' as i32 {
        buf.push(b'\\');
        buf.push(r as u8);
        return;
    }
    if ascii_only {
        if r < RUNE_SELF && strconv_is_print(r) {
            buf.push(r as u8);
            return;
        }
    } else if strconv_is_print(r) {
        append_rune(buf, r);
        return;
    }
    match r {
        7 => buf.extend_from_slice(b"\\a"),
        8 => buf.extend_from_slice(b"\\b"),
        12 => buf.extend_from_slice(b"\\f"),
        10 => buf.extend_from_slice(b"\\n"),
        13 => buf.extend_from_slice(b"\\r"),
        9 => buf.extend_from_slice(b"\\t"),
        11 => buf.extend_from_slice(b"\\v"),
        _ => {
            if r < ' ' as i32 || r == 0x7f {
                buf.extend_from_slice(b"\\x");
                buf.push(LOWERHEX[((r as u8) >> 4) as usize]);
                buf.push(LOWERHEX[((r as u8) & 0xF) as usize]);
            } else {
                let r = if valid_rune(r) { r } else { 0xFFFD };
                if r < 0x10000 {
                    buf.extend_from_slice(b"\\u");
                    let mut s = 12i32;
                    while s >= 0 {
                        buf.push(LOWERHEX[((r >> s) & 0xF) as usize]);
                        s -= 4;
                    }
                } else {
                    buf.extend_from_slice(b"\\U");
                    let mut s = 28i32;
                    while s >= 0 {
                        buf.push(LOWERHEX[((r >> s) & 0xF) as usize]);
                        s -= 4;
                    }
                }
            }
        }
    }
}

fn append_quoted_with(buf: &mut Vec<u8>, s: &[u8], quote: u8, ascii_only: bool) {
    buf.push(quote);
    let mut i = 0;
    while i < s.len() {
        let mut r = s[i] as i32;
        let mut width = 1;
        if r >= RUNE_SELF {
            let (rr, w) = decode_rune(&s[i..]);
            r = rr;
            width = w;
        }
        if width == 1 && r == RUNE_ERROR {
            buf.extend_from_slice(b"\\x");
            buf.push(LOWERHEX[(s[i] >> 4) as usize]);
            buf.push(LOWERHEX[(s[i] & 0xF) as usize]);
            i += 1;
            continue;
        }
        append_escaped_rune(buf, r, quote, ascii_only);
        i += width;
    }
    buf.push(quote);
}

/// strconv.Quote
pub fn quote(s: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(s.len() + 2);
    append_quoted_with(&mut b, s, b'"', false);
    b
}

/// strconv.QuoteToASCII
pub fn quote_to_ascii(s: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(s.len() + 2);
    append_quoted_with(&mut b, s, b'"', true);
    b
}

/// strconv.QuoteRune / QuoteRuneToASCII
pub fn quote_rune(r: i32, ascii_only: bool) -> Vec<u8> {
    let mut b = Vec::new();
    b.push(b'\'');
    let r = if valid_rune(r) { r } else { RUNE_ERROR };
    append_escaped_rune(&mut b, r, b'\'', ascii_only);
    b.push(b'\'');
    b
}

/// strconv.CanBackquote
pub fn can_backquote(s: &[u8]) -> bool {
    let mut i = 0;
    while i < s.len() {
        let (r, wid) = decode_rune(&s[i..]);
        i += wid;
        if wid > 1 {
            if r == 0xFEFF {
                return false;
            }
            continue;
        }
        if r == RUNE_ERROR {
            return false;
        }
        if (r < ' ' as i32 && r != '\t' as i32) || r == '`' as i32 || r == 0x7F {
            return false;
        }
    }
    true
}

fn unhex(c: u8) -> Option<i32> {
    match c {
        b'0'..=b'9' => Some((c - b'0') as i32),
        b'a'..=b'f' => Some((c - b'a' + 10) as i32),
        b'A'..=b'F' => Some((c - b'A' + 10) as i32),
        _ => None,
    }
}

/// strconv.UnquoteChar: (value, multibyte, tail) or a syntax error.
pub fn unquote_char(s: &[u8], quote: u8) -> Result<(i32, bool, &[u8]), ()> {
    if s.is_empty() {
        return Err(());
    }
    let c = s[0];
    if c == quote && (quote == b'\'' || quote == b'"') {
        return Err(());
    }
    if c >= 0x80 {
        let (r, size) = decode_rune(s);
        return Ok((r, true, &s[size..]));
    }
    if c != b'\\' {
        return Ok((c as i32, false, &s[1..]));
    }
    if s.len() <= 1 {
        return Err(());
    }
    let c = s[1];
    let mut s = &s[2..];
    let mut multibyte = false;
    let value: i32 = match c {
        b'a' => 7,
        b'b' => 8,
        b'f' => 12,
        b'n' => 10,
        b'r' => 13,
        b't' => 9,
        b'v' => 11,
        b'x' | b'u' | b'U' => {
            let n = match c {
                b'x' => 2,
                b'u' => 4,
                _ => 8,
            };
            if s.len() < n {
                return Err(());
            }
            let mut v: i32 = 0;
            for j in 0..n {
                let x = unhex(s[j]).ok_or(())?;
                v = (v << 4) | x;
            }
            s = &s[n..];
            if c == b'x' {
                v
            } else {
                if !valid_rune(v) {
                    return Err(());
                }
                multibyte = true;
                v
            }
        }
        b'0'..=b'7' => {
            let mut v = (c - b'0') as i32;
            if s.len() < 2 {
                return Err(());
            }
            for j in 0..2 {
                let x = s[j] as i32 - '0' as i32;
                if !(0..=7).contains(&x) {
                    return Err(());
                }
                v = (v << 3) | x;
            }
            s = &s[2..];
            if v > 255 {
                return Err(());
            }
            v
        }
        b'\\' => '\\' as i32,
        b'\'' | b'"' => {
            if c != quote {
                return Err(());
            }
            c as i32
        }
        _ => return Err(()),
    };
    Ok((value, multibyte, s))
}

/// strconv.Unquote
pub fn unquote(input: &[u8]) -> Result<Vec<u8>, ()> {
    let (out, rem) = unquote_inner(input)?;
    if !rem.is_empty() {
        return Err(());
    }
    Ok(out)
}

fn unquote_inner(input: &[u8]) -> Result<(Vec<u8>, &[u8]), ()> {
    if input.len() < 2 {
        return Err(());
    }
    let quote = input[0];
    let end = match input[1..].iter().position(|&c| c == quote) {
        Some(e) => e + 2,
        None => return Err(()),
    };
    match quote {
        b'`' => {
            let body = &input[1..end - 1];
            let out: Vec<u8> = body.iter().copied().filter(|&c| c != b'\r').collect();
            Ok((out, &input[end..]))
        }
        b'"' | b'\'' => {
            let head = &input[..end];
            if !head.contains(&b'\\') && !head.contains(&b'\n') {
                let valid = if quote == b'"' {
                    valid_string(&input[1..end - 1])
                } else {
                    let (r, n) = decode_rune(&input[1..end - 1]);
                    1 + n + 1 == end && (r != RUNE_ERROR || n != 1)
                };
                if valid {
                    return Ok((input[1..end - 1].to_vec(), &input[end..]));
                }
            }
            let mut buf = Vec::new();
            let mut s = &input[1..];
            while !s.is_empty() && s[0] != quote {
                if s[0] == b'\n' {
                    return Err(());
                }
                let (r, multibyte, rem) = unquote_char(s, quote)?;
                s = rem;
                if r < RUNE_SELF || !multibyte {
                    buf.push(r as u8);
                } else {
                    append_rune(&mut buf, r);
                }
                if quote == b'\'' {
                    break;
                }
            }
            if !(!s.is_empty() && s[0] == quote) {
                return Err(());
            }
            Ok((buf, &s[1..]))
        }
        _ => Err(()),
    }
}

// ---------------------------------------------------------------------------------------------
// strconv integers

/// A *strconv.NumError rendered as its Error() string.
#[derive(Debug, Clone)]
pub struct NumError {
    pub func: &'static str,
    pub num: Vec<u8>,
    pub err: NumErr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NumErr {
    Syntax,
    Range,
    Base(i64),
    BitSize(i64),
}

impl NumError {
    pub fn message(&self) -> Vec<u8> {
        let mut m = b"strconv.".to_vec();
        m.extend_from_slice(self.func.as_bytes());
        m.extend_from_slice(b": parsing ");
        m.extend_from_slice(&quote(&self.num));
        m.extend_from_slice(b": ");
        match &self.err {
            NumErr::Syntax => m.extend_from_slice(b"invalid syntax"),
            NumErr::Range => m.extend_from_slice(b"value out of range"),
            NumErr::Base(b) => m.extend_from_slice(format!("invalid base {}", b).as_bytes()),
            NumErr::BitSize(b) => m.extend_from_slice(format!("invalid bit size {}", b).as_bytes()),
        }
        m
    }
}

fn lower(c: u8) -> u8 {
    c | (b'x' - b'X')
}

pub fn underscore_ok(s: &[u8]) -> bool {
    let mut saw = b'^';
    let mut i = 0;
    let mut s = s;
    if !s.is_empty() && (s[0] == b'-' || s[0] == b'+') {
        s = &s[1..];
    }
    let mut hex = false;
    if s.len() >= 2 && s[0] == b'0' && (lower(s[1]) == b'b' || lower(s[1]) == b'o' || lower(s[1]) == b'x') {
        i = 2;
        saw = b'0';
        hex = lower(s[1]) == b'x';
    }
    while i < s.len() {
        if s[i].is_ascii_digit() || hex && (b'a'..=b'f').contains(&lower(s[i])) {
            saw = b'0';
            i += 1;
            continue;
        }
        if s[i] == b'_' {
            if saw != b'0' {
                return false;
            }
            saw = b'_';
            i += 1;
            continue;
        }
        if saw == b'_' {
            return false;
        }
        saw = b'!';
        i += 1;
    }
    saw != b'_'
}

fn num_err(func: &'static str, s: &[u8], err: NumErr) -> NumError {
    NumError { func, num: s.to_vec(), err }
}

/// strconv.ParseUint
pub fn parse_uint(s: &[u8], base: i64, bit_size: i64) -> Result<u64, (u64, NumError)> {
    const F: &str = "ParseUint";
    if s.is_empty() {
        return Err((0, num_err(F, s, NumErr::Syntax)));
    }
    let base0 = base == 0;
    let s0 = s;
    let mut s = s;
    let mut base = base;
    if (2..=36).contains(&base) {
    } else if base == 0 {
        base = 10;
        if s[0] == b'0' {
            if s.len() >= 3 && lower(s[1]) == b'b' {
                base = 2;
                s = &s[2..];
            } else if s.len() >= 3 && lower(s[1]) == b'o' {
                base = 8;
                s = &s[2..];
            } else if s.len() >= 3 && lower(s[1]) == b'x' {
                base = 16;
                s = &s[2..];
            } else {
                base = 8;
                s = &s[1..];
            }
        }
    } else {
        return Err((0, num_err(F, s0, NumErr::Base(base))));
    }
    let mut bit_size = bit_size;
    if bit_size == 0 {
        bit_size = 64;
    } else if !(0..=64).contains(&bit_size) {
        return Err((0, num_err(F, s0, NumErr::BitSize(bit_size))));
    }
    let cutoff: u64 = match base {
        10 => u64::MAX / 10 + 1,
        16 => u64::MAX / 16 + 1,
        _ => u64::MAX / base as u64 + 1,
    };
    let max_val: u64 = if bit_size == 64 { u64::MAX } else { (1u64 << bit_size) - 1 };
    let mut underscores = false;
    let mut n: u64 = 0;
    for &c in s {
        let d: u8;
        if c == b'_' && base0 {
            underscores = true;
            continue;
        } else if c.is_ascii_digit() {
            d = c - b'0';
        } else if (b'a'..=b'z').contains(&lower(c)) {
            d = lower(c) - b'a' + 10;
        } else {
            return Err((0, num_err(F, s0, NumErr::Syntax)));
        }
        if d as i64 >= base {
            return Err((0, num_err(F, s0, NumErr::Syntax)));
        }
        if n >= cutoff {
            return Err((max_val, num_err(F, s0, NumErr::Range)));
        }
        n = n.wrapping_mul(base as u64);
        let n1 = n.wrapping_add(d as u64);
        if n1 < n || n1 > max_val {
            return Err((max_val, num_err(F, s0, NumErr::Range)));
        }
        n = n1;
    }
    if underscores && !underscore_ok(s0) {
        return Err((0, num_err(F, s0, NumErr::Syntax)));
    }
    Ok(n)
}

/// strconv.ParseInt: Ok(value) or Err((value Go returns alongside the error, error)).
pub fn parse_int(s: &[u8], base: i64, bit_size: i64) -> Result<i64, (i64, NumError)> {
    const F: &str = "ParseInt";
    if s.is_empty() {
        return Err((0, num_err(F, s, NumErr::Syntax)));
    }
    let s0 = s;
    let mut s = s;
    let mut neg = false;
    if s[0] == b'+' {
        s = &s[1..];
    } else if s[0] == b'-' {
        s = &s[1..];
        neg = true;
    }
    let un = match parse_uint(s, base, bit_size) {
        Ok(v) => v,
        Err((v, e)) => {
            if e.err != NumErr::Range {
                return Err((0, NumError { func: F, num: s0.to_vec(), err: e.err }));
            }
            v
        }
    };
    let bit_size = if bit_size == 0 { 64 } else { bit_size };
    let cutoff: u64 = 1u64 << (bit_size - 1);
    if !neg && un >= cutoff {
        return Err(((cutoff - 1) as i64, num_err(F, s0, NumErr::Range)));
    }
    if neg && un > cutoff {
        return Err(((cutoff as i64).wrapping_neg(), num_err(F, s0, NumErr::Range)));
    }
    let n = un as i64;
    Ok(if neg { n.wrapping_neg() } else { n })
}

/// strconv.Atoi
pub fn atoi(s: &[u8]) -> Result<i64, NumError> {
    const F: &str = "Atoi";
    let slen = s.len();
    if 0 < slen && slen < 19 {
        let s0 = s;
        let mut s = s;
        if s[0] == b'-' || s[0] == b'+' {
            s = &s[1..];
            if s.is_empty() {
                return Err(num_err(F, s0, NumErr::Syntax));
            }
        }
        let mut n: i64 = 0;
        for &ch in s {
            let d = ch.wrapping_sub(b'0');
            if d > 9 {
                return Err(num_err(F, s0, NumErr::Syntax));
            }
            n = n * 10 + d as i64;
        }
        if s0[0] == b'-' {
            n = -n;
        }
        return Ok(n);
    }
    match parse_int(s, 10, 0) {
        Ok(v) => Ok(v),
        Err((_, mut e)) => {
            e.func = F;
            Err(e)
        }
    }
}

/// strconv.ParseBool
pub fn parse_bool(s: &[u8]) -> Result<bool, NumError> {
    match s {
        b"1" | b"t" | b"T" | b"true" | b"TRUE" | b"True" => Ok(true),
        b"0" | b"f" | b"F" | b"false" | b"FALSE" | b"False" => Ok(false),
        _ => Err(num_err("ParseBool", s, NumErr::Syntax)),
    }
}

const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// strconv.FormatInt; None for an illegal base (Go panics).
pub fn format_int(v: i64, base: i64) -> Option<Vec<u8>> {
    if !(2..=36).contains(&base) {
        return None;
    }
    let neg = v < 0;
    let mut u: u64 = if neg { (v as u64).wrapping_neg() } else { v as u64 };
    let mut buf = Vec::new();
    if u == 0 {
        buf.push(b'0');
    }
    while u > 0 {
        buf.push(DIGITS[(u % base as u64) as usize]);
        u /= base as u64;
    }
    if neg {
        buf.push(b'-');
    }
    buf.reverse();
    Some(buf)
}

pub fn itoa(v: i64) -> Vec<u8> {
    v.to_string().into_bytes()
}
