//! The text module (stdlib/text.go, text_regexp.go) over ports of Go's strings and strconv
//! functions.

use crate::ftoa::{format_float, parse_float};
use crate::goregex::{regexp_compile, GoRegexp};
use crate::gostr::*;
use crate::objects::*;
use crate::stdlib::*;
use std::rc::Rc;

// ---------------------------------------------------------------------------------------------
// strings package

fn index(s: &[u8], sep: &[u8]) -> i64 {
    if sep.is_empty() {
        return 0;
    }
    if sep.len() > s.len() {
        return -1;
    }
    for i in 0..=s.len() - sep.len() {
        if &s[i..i + sep.len()] == sep {
            return i as i64;
        }
    }
    -1
}

fn last_index(s: &[u8], sep: &[u8]) -> i64 {
    if sep.is_empty() {
        return s.len() as i64;
    }
    if sep.len() > s.len() {
        return -1;
    }
    let mut i = s.len() - sep.len();
    loop {
        if &s[i..i + sep.len()] == sep {
            return i as i64;
        }
        if i == 0 {
            return -1;
        }
        i -= 1;
    }
}

pub fn count(s: &[u8], sep: &[u8]) -> i64 {
    if sep.is_empty() {
        return rune_count(s) as i64 + 1;
    }
    let mut n = 0;
    let mut s = s;
    loop {
        let i = index(s, sep);
        if i == -1 {
            return n;
        }
        n += 1;
        s = &s[i as usize + sep.len()..];
    }
}

fn contains_rune(s: &[u8], r: i32) -> bool {
    let mut i = 0;
    while i < s.len() {
        let (c, w) = decode_rune(&s[i..]);
        if c == r {
            return true;
        }
        i += w;
    }
    false
}

fn index_any(s: &[u8], chars: &[u8]) -> i64 {
    if chars.is_empty() {
        return -1;
    }
    let mut i = 0;
    while i < s.len() {
        let (c, w) = decode_rune(&s[i..]);
        if contains_rune(chars, c) {
            return i as i64;
        }
        i += w;
    }
    -1
}

fn last_index_any(s: &[u8], chars: &[u8]) -> i64 {
    if chars.is_empty() {
        return -1;
    }
    let mut i = s.len();
    while i > 0 {
        let (r, size) = decode_last_rune(&s[..i]);
        i -= size;
        if contains_rune(chars, r) {
            return i as i64;
        }
    }
    -1
}

fn equal_fold(s: &[u8], t: &[u8]) -> bool {
    let mut s = s;
    let mut t = t;
    while !s.is_empty() && !t.is_empty() {
        let (mut sr, mut tr);
        if s[0] < 0x80 {
            sr = s[0] as i32;
            s = &s[1..];
        } else {
            let (r, size) = decode_rune(s);
            sr = r;
            s = &s[size..];
        }
        if t[0] < 0x80 {
            tr = t[0] as i32;
            t = &t[1..];
        } else {
            let (r, size) = decode_rune(t);
            tr = r;
            t = &t[size..];
        }
        if tr == sr {
            continue;
        }
        if tr < sr {
            std::mem::swap(&mut tr, &mut sr);
        }
        if tr < 0x80 {
            if ('A' as i32..='Z' as i32).contains(&sr) && tr == sr + 'a' as i32 - 'A' as i32 {
                continue;
            }
            return false;
        }
        let mut r = simple_fold(sr);
        while r != sr && r < tr {
            r = simple_fold(r);
        }
        if r == tr {
            continue;
        }
        return false;
    }
    s.len() == t.len()
}

fn fields(s: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut start: Option<usize> = None;
    while i < s.len() {
        let (r, w) = decode_rune(&s[i..]);
        if is_space(r) {
            if let Some(st) = start {
                out.push(s[st..i].to_vec());
                start = None;
            }
        } else if start.is_none() {
            start = Some(i);
        }
        i += w;
    }
    if let Some(st) = start {
        out.push(s[st..].to_vec());
    }
    out
}

fn explode(s: &[u8], n: i64) -> Vec<Vec<u8>> {
    let l = rune_count(s) as i64;
    let n = if n < 0 || n > l { l } else { n };
    let mut a = Vec::with_capacity(n as usize);
    let mut s = s;
    for _ in 0..(n - 1).max(0) {
        let (_, size) = decode_rune(s);
        a.push(s[..size].to_vec());
        s = &s[size..];
    }
    if n > 0 {
        a.push(s.to_vec());
    }
    a
}

fn gen_split(s: &[u8], sep: &[u8], sep_save: usize, n: i64) -> Vec<Vec<u8>> {
    if n == 0 {
        return Vec::new();
    }
    if sep.is_empty() {
        return explode(s, n);
    }
    let mut n = n;
    if n < 0 {
        n = count(s, sep) + 1;
    }
    if n > s.len() as i64 + 1 {
        n = s.len() as i64 + 1;
    }
    let mut a = Vec::new();
    let mut s = s;
    n -= 1;
    let mut i = 0;
    while i < n {
        let m = index(s, sep);
        if m < 0 {
            break;
        }
        let m = m as usize;
        a.push(s[..m + sep_save].to_vec());
        s = &s[m + sep.len()..];
        i += 1;
    }
    a.push(s.to_vec());
    a
}

/// strings.Map
fn go_map(s: &[u8], mapping: impl Fn(i32) -> i32) -> Vec<u8> {
    let mut b: Option<Vec<u8>> = None;
    let mut rest_start = 0;
    let mut i = 0;
    while i < s.len() {
        let (c, w) = decode_rune(&s[i..]);
        let r = mapping(c);
        if r == c && c != RUNE_ERROR {
            i += w;
            continue;
        }
        let width;
        if c == RUNE_ERROR {
            let (c2, w2) = decode_rune(&s[i..]);
            width = w2;
            if w2 != 1 && r == c2 {
                i += w;
                continue;
            }
        } else {
            width = rune_len(c) as usize;
        }
        let mut out = s[..i].to_vec();
        if r >= 0 {
            append_rune(&mut out, r);
        }
        b = Some(out);
        rest_start = i + width;
        break;
    }
    let mut out = match b {
        None => return s.to_vec(),
        Some(o) => o,
    };
    let rest = &s[rest_start..];
    let mut j = 0;
    while j < rest.len() {
        let (c, w) = decode_rune(&rest[j..]);
        let r = mapping(c);
        if r >= 0 {
            append_rune(&mut out, r);
        }
        j += w;
    }
    out
}

fn to_upper_s(s: &[u8]) -> Vec<u8> {
    if s.iter().all(|&c| c < 0x80) {
        return s.iter().map(|c| c.to_ascii_uppercase()).collect();
    }
    go_map(s, to_upper)
}

fn to_lower_s(s: &[u8]) -> Vec<u8> {
    if s.iter().all(|&c| c < 0x80) {
        return s.iter().map(|c| c.to_ascii_lowercase()).collect();
    }
    go_map(s, to_lower)
}

fn to_title_s(s: &[u8]) -> Vec<u8> {
    go_map(s, to_title)
}

fn is_separator(r: i32) -> bool {
    if r <= 0x7F {
        let c = r as u8;
        return !(c.is_ascii_alphanumeric() || c == b'_');
    }
    if is_letter(r) || is_digit(r) {
        return false;
    }
    is_space(r)
}

fn title(s: &[u8]) -> Vec<u8> {
    let prev = std::cell::Cell::new(' ' as i32);
    go_map(s, |r| {
        if is_separator(prev.get()) {
            prev.set(r);
            return to_title(r);
        }
        prev.set(r);
        r
    })
}

fn trim_left_func(s: &[u8], f: impl Fn(i32) -> bool) -> &[u8] {
    let mut i = 0;
    while i < s.len() {
        let (r, w) = decode_rune(&s[i..]);
        if !f(r) {
            return &s[i..];
        }
        i += w;
    }
    &s[s.len()..]
}

fn trim_right_func(s: &[u8], f: impl Fn(i32) -> bool) -> &[u8] {
    let mut i = s.len();
    while i > 0 {
        let (r, size) = decode_last_rune(&s[..i]);
        if !f(r) {
            return &s[..i];
        }
        i -= size;
    }
    &s[..0]
}

fn trim_space(s: &[u8]) -> Vec<u8> {
    trim_right_func(trim_left_func(s, is_space), is_space).to_vec()
}

fn trim_left(s: &[u8], cutset: &[u8]) -> Vec<u8> {
    if s.is_empty() || cutset.is_empty() {
        return s.to_vec();
    }
    if cutset.len() == 1 && cutset[0] < 0x80 {
        let c = cutset[0];
        let mut i = 0;
        while i < s.len() && s[i] == c {
            i += 1;
        }
        return s[i..].to_vec();
    }
    if cutset.iter().all(|&c| c < 0x80) {
        let mut i = 0;
        while i < s.len() && cutset.contains(&s[i]) {
            i += 1;
        }
        return s[i..].to_vec();
    }
    let mut s = s;
    while !s.is_empty() {
        let (r, n) = if s[0] < 0x80 { (s[0] as i32, 1) } else { decode_rune(s) };
        if !contains_rune(cutset, r) {
            break;
        }
        s = &s[n..];
    }
    s.to_vec()
}

fn trim_right(s: &[u8], cutset: &[u8]) -> Vec<u8> {
    if s.is_empty() || cutset.is_empty() {
        return s.to_vec();
    }
    if cutset.len() == 1 && cutset[0] < 0x80 {
        let c = cutset[0];
        let mut i = s.len();
        while i > 0 && s[i - 1] == c {
            i -= 1;
        }
        return s[..i].to_vec();
    }
    if cutset.iter().all(|&c| c < 0x80) {
        let mut i = s.len();
        while i > 0 && cutset.contains(&s[i - 1]) {
            i -= 1;
        }
        return s[..i].to_vec();
    }
    let mut s = s;
    while !s.is_empty() {
        let (r, n) = if s[s.len() - 1] < 0x80 {
            (s[s.len() - 1] as i32, 1)
        } else {
            decode_last_rune(s)
        };
        if !contains_rune(cutset, r) {
            break;
        }
        s = &s[..s.len() - n];
    }
    s.to_vec()
}

fn trim(s: &[u8], cutset: &[u8]) -> Vec<u8> {
    if s.is_empty() || cutset.is_empty() {
        return s.to_vec();
    }
    trim_right(&trim_left(s, cutset), cutset)
}

fn compare(a: &[u8], b: &[u8]) -> i64 {
    match a.cmp(b) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

fn bool_s(b: bool) -> Vec<u8> {
    if b {
        b"true".to_vec()
    } else {
        b"false".to_vec()
    }
}

fn do_text_replace(s: &[u8], old: &[u8], new: &[u8], n: i64) -> Vec<u8> {
    if old == new || n == 0 {
        return s.to_vec();
    }
    let m = count(s, old);
    if m == 0 {
        return s.to_vec();
    }
    let n = if n < 0 || m < n { m } else { n };
    let mut t = Vec::new();
    let mut start = 0usize;
    for i in 0..n {
        let mut j = start;
        if old.is_empty() {
            if i > 0 {
                let (_, wid) = decode_rune(&s[start..]);
                j += wid;
            }
        } else {
            j += index(&s[start..], old) as usize;
        }
        t.extend_from_slice(&s[start..j]);
        t.extend_from_slice(new);
        start = j + old.len();
    }
    t.extend_from_slice(&s[start..]);
    t
}

// ---------------------------------------------------------------------------------------------
// module functions

fn text_join(args: &[Object]) -> TResult<Object> {
    check_args(args, 2)?;
    let items = match array_elems(&args[0]) {
        Some(v) => v,
        None => return Err(TErr::arg("first", "array", &args[0])),
    };
    let mut parts = Vec::new();
    for (idx, a) in items.iter().enumerate() {
        let (s, ok) = to_string(a)?;
        if !ok {
            return Err(TErr::InvalidArgumentType {
                name: format!("first[{}]", idx),
                expected: "string(compatible)".into(),
                found: a.type_name(),
            });
        }
        parts.push(s);
    }
    let sep = arg_string(args, 1, "second")?;
    Ok(new_str(parts.join(&sep[..])))
}

fn text_repeat(args: &[Object]) -> TResult<Object> {
    check_args(args, 2)?;
    let s = arg_string(args, 0, "first")?;
    let n = arg_int(args, 1, "second")?;
    if (s.len() as i64).wrapping_mul(n) > MAX_STRING_LEN as i64 {
        return Err(TErr::StringLimit);
    }
    if n < 0 {
        return Err(TErr::panic("strings: negative Repeat count"));
    }
    Ok(new_str(s.repeat(n as usize)))
}

fn text_replace(args: &[Object]) -> TResult<Object> {
    check_args(args, 4)?;
    let s1 = arg_string(args, 0, "first")?;
    let s2 = arg_string(args, 1, "second")?;
    let s3 = arg_string(args, 2, "third")?;
    let i4 = arg_int(args, 3, "fourth")?;
    Ok(new_str(do_text_replace(&s1, &s2, &s3, i4)))
}

fn text_substr(args: &[Object]) -> TResult<Object> {
    if args.len() != 2 && args.len() != 3 {
        return Err(TErr::WrongNumArguments);
    }
    let s1 = arg_string(args, 0, "first")?;
    let mut i2 = arg_int(args, 1, "second")?;
    let strlen = s1.len() as i64;
    let mut i3 = strlen;
    if args.len() == 3 {
        i3 = arg_int(args, 2, "third")?;
    }
    if i2 > i3 {
        return Err(TErr::InvalidIndexType);
    }
    i2 = i2.clamp(0, strlen);
    i3 = i3.clamp(0, strlen);
    Ok(new_str(&s1[i2 as usize..i3 as usize]))
}

fn text_pad(args: &[Object], left: bool) -> TResult<Object> {
    if args.len() != 2 && args.len() != 3 {
        return Err(TErr::WrongNumArguments);
    }
    let s1 = arg_string(args, 0, "first")?;
    let i2 = arg_int(args, 1, "second")?;
    if i2 > MAX_STRING_LEN as i64 {
        return Err(TErr::StringLimit);
    }
    if s1.len() as i64 >= i2 {
        return Ok(new_str(s1));
    }
    let mut s3 = b" ".to_vec();
    if args.len() == 3 {
        s3 = arg_string(args, 2, "third")?;
    }
    let pad_len = s3.len() as i64;
    if pad_len == 0 {
        return Ok(new_str(s1));
    }
    let pad_count = ((i2 - pad_len) / pad_len) + 1;
    let rep = s3.repeat(pad_count as usize);
    if left {
        let mut r = rep;
        r.extend_from_slice(&s1);
        let lo = r.len() as i64 - i2;
        if lo < 0 {
            return Err(TErr::panic(format!("runtime error: slice bounds out of range [{}:]", lo)));
        }
        Ok(new_str(&r[lo as usize..]))
    } else {
        let mut r = s1.clone();
        r.extend_from_slice(&rep);
        if i2 as usize > r.len() {
            return Err(TErr::panic(format!(
                "runtime error: slice bounds out of range [:{}] with length {}",
                i2,
                r.len()
            )));
        }
        Ok(new_str(&r[..i2 as usize]))
    }
}

fn text_format_bool(args: &[Object]) -> TResult<Object> {
    check_args(args, 1)?;
    match &args[0] {
        Object::Bool(b) => Ok(new_str(bool_s(*b))),
        o => Err(TErr::arg("first", "bool", o)),
    }
}

fn text_format_float(args: &[Object]) -> TResult<Object> {
    check_args(args, 4)?;
    let f1 = match &args[0] {
        Object::Float(f) => *f,
        o => return Err(TErr::arg("first", "float", o)),
    };
    let s2 = arg_string(args, 1, "second")?;
    let i3 = arg_int(args, 2, "third")?;
    let i4 = arg_int(args, 3, "fourth")?;
    if s2.is_empty() {
        return Err(TErr::panic("runtime error: index out of range [0] with length 0"));
    }
    match format_float(f1, s2[0], i3, i4) {
        Some(s) => Ok(new_str(s)),
        None => Err(TErr::panic("strconv: illegal AppendFloat/FormatFloat bitSize")),
    }
}

fn text_format_int(args: &[Object]) -> TResult<Object> {
    check_args(args, 2)?;
    let i1 = match &args[0] {
        Object::Int(i) => *i,
        o => return Err(TErr::arg("first", "int", o)),
    };
    let i2 = arg_int(args, 1, "second")?;
    match format_int(i1, i2) {
        Some(s) => Ok(new_str(s)),
        None => Err(TErr::panic("strconv: illegal AppendInt/FormatInt base")),
    }
}

fn text_parse_bool(args: &[Object]) -> TResult<Object> {
    check_args(args, 1)?;
    let s = match &args[0] {
        Object::String(s) => s.clone(),
        o => return Err(TErr::arg("first", "string", o)),
    };
    // the error is returned as err (a runtime error), not wrapped
    match parse_bool(&s) {
        Ok(b) => Ok(Object::Bool(b)),
        Err(e) => Err(TErr::Msg(e.message())),
    }
}

fn text_parse_float(args: &[Object]) -> TResult<Object> {
    check_args(args, 2)?;
    let s = match &args[0] {
        Object::String(s) => s.clone(),
        o => return Err(TErr::arg("first", "string", o)),
    };
    let bits = arg_int(args, 1, "second")?;
    match parse_float(&s, bits) {
        Ok(f) => Ok(Object::Float(f)),
        Err((_, e)) => Err(TErr::Msg(e.message())),
    }
}

fn text_parse_int(args: &[Object]) -> TResult<Object> {
    check_args(args, 3)?;
    let s = match &args[0] {
        Object::String(s) => s.clone(),
        o => return Err(TErr::arg("first", "string", o)),
    };
    let base = arg_int(args, 1, "second")?;
    let bits = arg_int(args, 2, "third")?;
    match parse_int(&s, base, bits) {
        Ok(i) => Ok(Object::Int(i)),
        Err((_, e)) => Err(TErr::Msg(e.message())),
    }
}

// ---------------------------------------------------------------------------------------------
// regular expressions

/// regexp.Compile; the text functions return the compile error itself (Go assigns it to the
/// named err result), which the VM reports as a runtime error.
fn compile_re(pattern: &[u8]) -> TResult<Rc<GoRegexp>> {
    match regexp_compile(pattern) {
        Ok(re) => Ok(Rc::new(re)),
        Err(e) => Err(TErr::Msg(e)),
    }
}

/// One submatch as {text, begin, end}.
fn match_map(s: &[u8], b: i64, e: i64) -> TResult<Object> {
    if b < 0 || e < 0 || b > e || e as usize > s.len() {
        return Err(slice_panic(b, e, s.len()));
    }
    let mut m = MapData::new();
    m.insert(b"text".to_vec(), new_str(&s[b as usize..e as usize]));
    m.insert(b"begin".to_vec(), Object::Int(b));
    m.insert(b"end".to_vec(), Object::Int(e));
    Ok(new_imm_map(m))
}

/// The Go runtime panic for s[b:e] out of range (negative bounds omit the length).
fn slice_panic(b: i64, e: i64, len: usize) -> TErr {
    if e < 0 {
        TErr::panic(format!("runtime error: slice bounds out of range [:{}]", e))
    } else if e as usize > len {
        TErr::panic(format!("runtime error: slice bounds out of range [:{}] with length {}", e, len))
    } else if b < 0 {
        TErr::panic(format!("runtime error: slice bounds out of range [{}:]", b))
    } else {
        TErr::panic(format!("runtime error: slice bounds out of range [{}:{}]", b, e))
    }
}

/// The submatch array of one match; check_unmatched skips groups that did not participate
/// (the re_find path) while the compiled object's find slices them and panics.
fn submatch_array(s: &[u8], m: &[i64], skip_unmatched: bool) -> TResult<Object> {
    let mut items = Vec::new();
    let mut i = 0;
    while i < m.len() {
        if !skip_unmatched || (m[i] >= 0 && m[i + 1] >= 0) {
            items.push(match_map(s, m[i], m[i + 1])?);
        }
        i += 2;
    }
    Ok(array_of(items))
}

fn re_find_impl(re: &GoRegexp, s: &[u8], count: Option<i64>, skip_unmatched: bool) -> TResult<Object> {
    match count {
        None => match re.find_submatch_index(s) {
            None => Ok(Object::Undefined),
            Some(m) => {
                let arr = submatch_array(s, &m, skip_unmatched)?;
                Ok(array_of(vec![arr]))
            }
        },
        Some(n) => {
            let all = re.find_all_submatch_index(s, n);
            if all.is_empty() {
                return Ok(Object::Undefined);
            }
            let mut items = Vec::new();
            for m in all {
                items.push(submatch_array(s, &m, skip_unmatched)?);
            }
            Ok(array_of(items))
        }
    }
}

fn re_replace_impl(re: &GoRegexp, src: &[u8], repl: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut idx = 0usize;
    for m in re.find_all_submatch_index(src, -1) {
        let exp = re.expand(repl, src, &m);
        out.extend_from_slice(&src[idx..m[0] as usize]);
        out.extend_from_slice(&exp);
        idx = m[1] as usize;
    }
    if idx < src.len() {
        out.extend_from_slice(&src[idx..]);
    }
    out
}

fn re_split_impl(re: &GoRegexp, s: &[u8], n: i64) -> Object {
    str_array(re.split(s, n))
}

fn text_re_match(args: &[Object]) -> TResult<Object> {
    check_args(args, 2)?;
    let s1 = arg_string(args, 0, "first")?;
    let s2 = arg_string(args, 1, "second")?;
    let re = compile_re(&s1)?;
    Ok(Object::Bool(re.is_match(&s2)))
}

fn text_re_find(args: &[Object]) -> TResult<Object> {
    if args.len() != 2 && args.len() != 3 {
        return Err(TErr::WrongNumArguments);
    }
    let s1 = arg_string(args, 0, "first")?;
    let re = compile_re(&s1)?;
    let s2 = arg_string(args, 1, "second")?;
    if args.len() < 3 {
        return re_find_impl(&re, &s2, None, true);
    }
    let n = arg_int(args, 2, "third")?;
    re_find_impl(&re, &s2, Some(n), true)
}

fn text_re_replace(args: &[Object]) -> TResult<Object> {
    check_args(args, 3)?;
    let s1 = arg_string(args, 0, "first")?;
    let s2 = arg_string(args, 1, "second")?;
    let s3 = arg_string(args, 2, "third")?;
    let re = compile_re(&s1)?;
    Ok(new_str(re_replace_impl(&re, &s2, &s3)))
}

fn text_re_split(args: &[Object]) -> TResult<Object> {
    if args.len() != 2 && args.len() != 3 {
        return Err(TErr::WrongNumArguments);
    }
    let s1 = arg_string(args, 0, "first")?;
    let s2 = arg_string(args, 1, "second")?;
    let mut n = -1;
    if args.len() > 2 {
        n = arg_int(args, 2, "third")?;
    }
    let re = compile_re(&s1)?;
    Ok(re_split_impl(&re, &s2, n))
}

fn text_re_compile(args: &[Object]) -> TResult<Object> {
    check_args(args, 1)?;
    let s1 = arg_string(args, 0, "first")?;
    let re = compile_re(&s1)?;
    Ok(make_text_regexp(re))
}

fn make_text_regexp(re: Rc<GoRegexp>) -> Object {
    let mut m = MapData::new();
    let r = re.clone();
    m.insert(
        b"match".to_vec(),
        user_fn("", move |args| {
            check_args(args, 1)?;
            let s1 = arg_string(args, 0, "first")?;
            Ok(Object::Bool(r.is_match(&s1)))
        }),
    );
    let r = re.clone();
    m.insert(
        b"find".to_vec(),
        user_fn("", move |args| {
            if args.len() != 1 && args.len() != 2 {
                return Err(TErr::WrongNumArguments);
            }
            let s1 = arg_string(args, 0, "first")?;
            if args.len() == 1 {
                return re_find_impl(&r, &s1, None, false);
            }
            let n = arg_int(args, 1, "second")?;
            re_find_impl(&r, &s1, Some(n), false)
        }),
    );
    let r = re.clone();
    m.insert(
        b"replace".to_vec(),
        user_fn("", move |args| {
            check_args(args, 2)?;
            let s1 = arg_string(args, 0, "first")?;
            let s2 = arg_string(args, 1, "second")?;
            Ok(new_str(re_replace_impl(&r, &s1, &s2)))
        }),
    );
    let r = re;
    m.insert(
        b"split".to_vec(),
        user_fn("", move |args| {
            if args.len() != 1 && args.len() != 2 {
                return Err(TErr::WrongNumArguments);
            }
            let s1 = arg_string(args, 0, "first")?;
            let mut n = -1;
            if args.len() > 1 {
                n = arg_int(args, 1, "second")?;
            }
            Ok(re_split_impl(&r, &s1, n))
        }),
    );
    new_imm_map(m)
}

pub fn text_module() -> Vec<(&'static str, Object)> {
    vec![
        ("re_match", user_fn("re_match", text_re_match)),
        ("re_find", user_fn("re_find", text_re_find)),
        ("re_replace", user_fn("re_replace", text_re_replace)),
        ("re_split", user_fn("re_split", text_re_split)),
        ("re_compile", user_fn("re_compile", text_re_compile)),
        ("compare", f_assri("compare", compare)),
        ("contains", f_assrb("contains", |s, sub| index(s, sub) >= 0)),
        ("contains_any", f_assrb("contains_any", |s, chars| index_any(s, chars) >= 0)),
        ("count", f_assri("count", count)),
        ("equal_fold", f_assrb("equal_fold", equal_fold)),
        ("fields", f_asrss("fields", fields)),
        ("has_prefix", f_assrb("has_prefix", |s, p| s.starts_with(p))),
        ("has_suffix", f_assrb("has_suffix", |s, p| s.ends_with(p))),
        ("index", f_assri("index", index)),
        ("index_any", f_assri("index_any", index_any)),
        ("join", user_fn("join", text_join)),
        ("last_index", f_assri("last_index", last_index)),
        ("last_index_any", f_assri("last_index_any", last_index_any)),
        ("repeat", user_fn("repeat", text_repeat)),
        ("replace", user_fn("replace", text_replace)),
        ("substr", user_fn("substr", text_substr)),
        ("split", f_assrss("split", |s, sep| gen_split(s, sep, 0, -1))),
        ("split_after", f_assrss("split_after", |s, sep| gen_split(s, sep, sep.len(), -1))),
        ("split_after_n", f_assirss("split_after_n", |s, sep, n| gen_split(s, sep, sep.len(), n))),
        ("split_n", f_assirss("split_n", |s, sep, n| gen_split(s, sep, 0, n))),
        ("title", f_asrs("title", title)),
        ("to_lower", f_asrs("to_lower", to_lower_s)),
        ("to_title", f_asrs("to_title", to_title_s)),
        ("to_upper", f_asrs("to_upper", to_upper_s)),
        ("pad_left", user_fn("pad_left", |a| text_pad(a, true))),
        ("pad_right", user_fn("pad_right", |a| text_pad(a, false))),
        ("trim", f_assrs("trim", trim)),
        ("trim_left", f_assrs("trim_left", trim_left)),
        ("trim_prefix", f_assrs("trim_prefix", |s, p| s.strip_prefix(p).unwrap_or(s).to_vec())),
        ("trim_right", f_assrs("trim_right", trim_right)),
        ("trim_space", f_asrs("trim_space", trim_space)),
        ("trim_suffix", f_assrs("trim_suffix", |s, p| s.strip_suffix(p).unwrap_or(s).to_vec())),
        (
            "atoi",
            user_fn("atoi", |args| {
                check_args(args, 1)?;
                let s = arg_string(args, 0, "first")?;
                match atoi(&s) {
                    Ok(i) => Ok(Object::Int(i)),
                    Err(e) => Ok(wrap_error(e.message())),
                }
            }),
        ),
        ("format_bool", user_fn("format_bool", text_format_bool)),
        ("format_float", user_fn("format_float", text_format_float)),
        ("format_int", user_fn("format_int", text_format_int)),
        (
            "itoa",
            user_fn("itoa", |args| {
                check_args(args, 1)?;
                let i = arg_int(args, 0, "first")?;
                Ok(new_str(itoa(i)))
            }),
        ),
        ("parse_bool", user_fn("parse_bool", text_parse_bool)),
        ("parse_float", user_fn("parse_float", text_parse_float)),
        ("parse_int", user_fn("parse_int", text_parse_int)),
        ("quote", f_asrs("quote", quote)),
        (
            "unquote",
            user_fn("unquote", |args| {
                check_args(args, 1)?;
                let s = arg_string(args, 0, "first")?;
                match unquote(&s) {
                    Ok(r) => Ok(new_str(r)),
                    Err(()) => Ok(wrap_error("invalid syntax")),
                }
            }),
        ),
    ]
}
