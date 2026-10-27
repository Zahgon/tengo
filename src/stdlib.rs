//! The standard library modules (stdlib/*.go) and the argument-checking wrappers of
//! func_typedefs.go. os and rand are not provided.

use std::rc::Rc;

use crate::goslice::GoSlice;
use crate::objects::*;

pub enum Module {
    Builtin(Object),
    Source(&'static str),
}

pub const ENUM_SRC: &str = include_str!("enum.tengo");

/// A module's attributes as Go's BuiltinModule.AsImmutableMap builds them: every value
/// copied, plus __module_name__.
fn as_immutable_map(name: &str, attrs: Vec<(&str, Object)>) -> Object {
    let mut m = MapData::new();
    for (k, v) in attrs {
        m.insert(k.as_bytes().to_vec(), v.copy());
    }
    m.insert(b"__module_name__".to_vec(), new_str(name));
    new_imm_map(m)
}

pub fn get_module(name: &str) -> Option<Module> {
    let attrs = match name {
        "fmt" => fmt_module(),
        "text" => crate::text::text_module(),
        "math" => crate::gomath::math_module(),
        "base64" => base64_module(),
        "hex" => hex_module(),
        "json" => crate::json::json_module(),
        "times" => crate::times::times_module(),
        "enum" => return Some(Module::Source(ENUM_SRC)),
        _ => return None,
    };
    Some(Module::Builtin(as_immutable_map(name, attrs)))
}

/// Wraps a Go error value as a Tengo error object (wrapError).
pub fn wrap_error<S: AsRef<[u8]>>(msg: S) -> Object {
    error_str(msg)
}

/// &tengo.Array{} built by appending (nil when empty).
pub fn array_of(items: Vec<Object>) -> Object {
    new_array(GoSlice::from_appends(items))
}

pub fn str_array(items: Vec<Vec<u8>>) -> Object {
    array_of(items.into_iter().map(new_str).collect())
}

// ---------------------------------------------------------------------------------------------
// argument helpers

pub fn arg_string(args: &[Object], i: usize, name: &str) -> TResult<Vec<u8>> {
    let (s, ok) = to_string(&args[i])?;
    if !ok {
        return Err(TErr::arg(name, "string(compatible)", &args[i]));
    }
    Ok(s)
}

pub fn arg_int(args: &[Object], i: usize, name: &str) -> TResult<i64> {
    match to_int(&args[i]) {
        Some(v) => Ok(v),
        None => Err(TErr::arg(name, "int(compatible)", &args[i])),
    }
}

pub fn arg_float(args: &[Object], i: usize, name: &str) -> TResult<f64> {
    match to_float(&args[i]) {
        Some(v) => Ok(v),
        None => Err(TErr::arg(name, "float(compatible)", &args[i])),
    }
}

pub fn arg_bytes(args: &[Object], i: usize, name: &str) -> TResult<Vec<u8>> {
    match to_bytes_vec(&args[i]) {
        Some(v) => Ok(v),
        None => Err(TErr::arg(name, "bytes(compatible)", &args[i])),
    }
}

pub fn check_args(args: &[Object], n: usize) -> TResult<()> {
    if args.len() != n {
        return Err(TErr::WrongNumArguments);
    }
    Ok(())
}

pub const ORD: [&str; 4] = ["first", "second", "third", "fourth"];

/// FuncASRS
pub fn f_asrs(name: &str, f: fn(&[u8]) -> Vec<u8>) -> Object {
    user_fn(name, move |args| {
        check_args(args, 1)?;
        let s = arg_string(args, 0, "first")?;
        Ok(new_str(f(&s)))
    })
}

/// FuncASSRS
pub fn f_assrs(name: &str, f: fn(&[u8], &[u8]) -> Vec<u8>) -> Object {
    user_fn(name, move |args| {
        check_args(args, 2)?;
        let s1 = arg_string(args, 0, "first")?;
        let s2 = arg_string(args, 1, "second")?;
        Ok(new_str(f(&s1, &s2)))
    })
}

/// FuncASSRB
pub fn f_assrb(name: &str, f: fn(&[u8], &[u8]) -> bool) -> Object {
    user_fn(name, move |args| {
        check_args(args, 2)?;
        let s1 = arg_string(args, 0, "first")?;
        let s2 = arg_string(args, 1, "second")?;
        Ok(Object::Bool(f(&s1, &s2)))
    })
}

/// FuncASSRI: the second argument's error reports the first argument's type.
pub fn f_assri(name: &str, f: fn(&[u8], &[u8]) -> i64) -> Object {
    user_fn(name, move |args| {
        check_args(args, 2)?;
        let s1 = arg_string(args, 0, "first")?;
        let (s2, ok) = to_string(&args[1])?;
        if !ok {
            return Err(TErr::arg("second", "string(compatible)", &args[0]));
        }
        Ok(Object::Int(f(&s1, &s2)))
    })
}

/// FuncASRSs
pub fn f_asrss(name: &str, f: fn(&[u8]) -> Vec<Vec<u8>>) -> Object {
    user_fn(name, move |args| {
        check_args(args, 1)?;
        let s1 = arg_string(args, 0, "first")?;
        Ok(str_array(f(&s1)))
    })
}

/// FuncASSRSs: the second argument's error is named "first".
pub fn f_assrss(name: &str, f: fn(&[u8], &[u8]) -> Vec<Vec<u8>>) -> Object {
    user_fn(name, move |args| {
        check_args(args, 2)?;
        let s1 = arg_string(args, 0, "first")?;
        let s2 = arg_string(args, 1, "first")?;
        Ok(str_array(f(&s1, &s2)))
    })
}

/// FuncASSIRSs
pub fn f_assirss(name: &str, f: fn(&[u8], &[u8], i64) -> Vec<Vec<u8>>) -> Object {
    user_fn(name, move |args| {
        check_args(args, 3)?;
        let s1 = arg_string(args, 0, "first")?;
        let s2 = arg_string(args, 1, "second")?;
        let i3 = arg_int(args, 2, "third")?;
        Ok(str_array(f(&s1, &s2, i3)))
    })
}

/// FuncAYRS
pub fn f_ayrs(name: &str, f: fn(&[u8]) -> Vec<u8>) -> Object {
    user_fn(name, move |args| {
        check_args(args, 1)?;
        let y = arg_bytes(args, 0, "first")?;
        Ok(new_str(f(&y)))
    })
}

/// FuncASRYE
pub fn f_asrye(name: &str, f: fn(&[u8]) -> Result<Vec<u8>, Vec<u8>>) -> Object {
    user_fn(name, move |args| {
        check_args(args, 1)?;
        let s = arg_string(args, 0, "first")?;
        match f(&s) {
            Ok(b) => Ok(Object::Bytes(GoSlice::from_vec_exact(b))),
            Err(e) => Ok(wrap_error(e)),
        }
    })
}

// ---------------------------------------------------------------------------------------------
// fmt

fn print_args(args: &[Object]) -> TResult<Vec<u8>> {
    let mut out = Vec::new();
    for a in args {
        let (s, _) = to_string(a)?;
        out.extend_from_slice(&s);
    }
    Ok(out)
}

fn fmt_module() -> Vec<(&'static str, Object)> {
    vec![
        (
            "print",
            user_fn("print", |args| {
                let s = print_args(args)?;
                crate::out::write(&s);
                Ok(Object::Nil)
            }),
        ),
        (
            "printf",
            user_fn("printf", |args| {
                if args.is_empty() {
                    return Err(TErr::WrongNumArguments);
                }
                let f = match &args[0] {
                    Object::String(s) => s.clone(),
                    o => return Err(TErr::arg("format", "string", o)),
                };
                if args.len() == 1 {
                    crate::out::write(&args[0].string()?);
                    return Ok(Object::Nil);
                }
                let s = crate::formatter::format(&f, &args[1..])?;
                crate::out::write(&s);
                Ok(Object::Nil)
            }),
        ),
        (
            "println",
            user_fn("println", |args| {
                let mut s = print_args(args)?;
                s.push(b'\n');
                crate::out::write(&s);
                Ok(Object::Nil)
            }),
        ),
        (
            "sprintf",
            user_fn("sprintf", |args| {
                if args.is_empty() {
                    return Err(TErr::WrongNumArguments);
                }
                let f = match &args[0] {
                    Object::String(s) => s.clone(),
                    o => return Err(TErr::arg("format", "string", o)),
                };
                if args.len() == 1 {
                    return Ok(args[0].clone());
                }
                Ok(new_str(crate::formatter::format(&f, &args[1..])?))
            }),
        ),
    ]
}

// ---------------------------------------------------------------------------------------------
// base64 and hex

const STD_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn b64_encode(src: &[u8], alphabet: &[u8; 64], pad: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= src.len() {
        let v = (src[i] as u32) << 16 | (src[i + 1] as u32) << 8 | src[i + 2] as u32;
        for k in 0..4 {
            out.push(alphabet[((v >> (18 - 6 * k)) & 0x3F) as usize]);
        }
        i += 3;
    }
    let rem = src.len() - i;
    if rem > 0 {
        let mut v = (src[i] as u32) << 16;
        if rem == 2 {
            v |= (src[i + 1] as u32) << 8;
        }
        out.push(alphabet[((v >> 18) & 0x3F) as usize]);
        out.push(alphabet[((v >> 12) & 0x3F) as usize]);
        if rem == 2 {
            out.push(alphabet[((v >> 6) & 0x3F) as usize]);
            if pad {
                out.push(b'=');
            }
        } else if pad {
            out.push(b'=');
            out.push(b'=');
        }
    }
    out
}

/// encoding/base64 DecodeString (strict=false): Ok(bytes) or the CorruptInputError text.
fn b64_decode(src: &[u8], alphabet: &[u8; 64], pad: bool) -> Result<Vec<u8>, Vec<u8>> {
    let mut decode_map = [0xFFu8; 256];
    for (i, &c) in alphabet.iter().enumerate() {
        decode_map[c as usize] = i as u8;
    }
    let corrupt = |off: usize| format!("illegal base64 data at input byte {}", off).into_bytes();
    let mut dst = Vec::new();
    let mut si = 0usize;
    while si < src.len() {
        // decodeQuantum
        let mut dbuf = [0u8; 4];
        let mut dlen = 4usize;
        let mut j = 0usize;
        while j < dbuf.len() {
            if src.len() == si {
                if j == 0 {
                    return Ok(dst);
                } else if j == 1 || pad {
                    return Err(corrupt(si - j));
                }
                dlen = j;
                break;
            }
            let input = src[si];
            si += 1;
            let out = decode_map[input as usize];
            if out != 0xFF {
                dbuf[j] = out;
                j += 1;
                continue;
            }
            if input == b'\n' || input == b'\r' {
                continue;
            }
            if input != b'=' || !pad {
                return Err(corrupt(si - 1));
            }
            // padding
            match j {
                0 | 1 => return Err(corrupt(si - 1)),
                2 => {
                    while si < src.len() && (src[si] == b'\n' || src[si] == b'\r') {
                        si += 1;
                    }
                    if si == src.len() {
                        return Err(corrupt(src.len()));
                    }
                    if src[si] != b'=' {
                        return Err(corrupt(si - 1));
                    }
                    si += 1;
                }
                _ => {}
            }
            while si < src.len() && (src[si] == b'\n' || src[si] == b'\r') {
                si += 1;
            }
            if si < src.len() {
                return Err(corrupt(si));
            }
            dlen = j;
            break;
        }
        let val = (dbuf[0] as u32) << 18 | (dbuf[1] as u32) << 12 | (dbuf[2] as u32) << 6 | dbuf[3] as u32;
        let (b0, b1, b2) = ((val >> 16) as u8, (val >> 8) as u8, val as u8);
        match dlen {
            4 => {
                dst.push(b0);
                dst.push(b1);
                dst.push(b2);
            }
            3 => {
                dst.push(b0);
                dst.push(b1);
            }
            2 => dst.push(b0),
            _ => {}
        }
        if dlen < 4 {
            return Ok(dst);
        }
    }
    Ok(dst)
}

fn base64_module() -> Vec<(&'static str, Object)> {
    vec![
        ("encode", f_ayrs("encode", |b| b64_encode(b, STD_ALPHABET, true))),
        ("decode", f_asrye("decode", |s| b64_decode(s, STD_ALPHABET, true))),
        ("raw_encode", f_ayrs("raw_encode", |b| b64_encode(b, STD_ALPHABET, false))),
        ("raw_decode", f_asrye("raw_decode", |s| b64_decode(s, STD_ALPHABET, false))),
        ("url_encode", f_ayrs("url_encode", |b| b64_encode(b, URL_ALPHABET, true))),
        ("url_decode", f_asrye("url_decode", |s| b64_decode(s, URL_ALPHABET, true))),
        ("raw_url_encode", f_ayrs("raw_url_encode", |b| b64_encode(b, URL_ALPHABET, false))),
        ("raw_url_decode", f_asrye("raw_url_decode", |s| b64_decode(s, URL_ALPHABET, false))),
    ]
}

fn hex_encode(b: &[u8]) -> Vec<u8> {
    const H: &[u8] = b"0123456789abcdef";
    let mut out = Vec::with_capacity(b.len() * 2);
    for &c in b {
        out.push(H[(c >> 4) as usize]);
        out.push(H[(c & 0xF) as usize]);
    }
    out
}

fn from_hex_char(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// encoding/hex DecodeString
fn hex_decode(s: &[u8]) -> Result<Vec<u8>, Vec<u8>> {
    let invalid = |c: u8| format!("encoding/hex: invalid byte: {}", go_quote_rune_u(c)).into_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    let mut i = 1;
    while i < s.len() {
        let a = match from_hex_char(s[i - 1]) {
            Some(a) => a,
            None => return Err(invalid(s[i - 1])),
        };
        let b = match from_hex_char(s[i]) {
            Some(b) => b,
            None => return Err(invalid(s[i])),
        };
        out.push(a << 4 | b);
        i += 2;
    }
    if s.len() % 2 == 1 {
        if from_hex_char(s[i - 1]).is_none() {
            return Err(invalid(s[i - 1]));
        }
        return Err(b"encoding/hex: odd length hex string".to_vec());
    }
    Ok(out)
}

/// fmt's %#U of a byte value: U+0067 'g'
fn go_quote_rune_u(c: u8) -> String {
    let r = c as i32;
    if crate::gostr::strconv_is_print(r) {
        let mut s = format!("U+{:04X} '", r);
        s.push(char::from_u32(r as u32).unwrap());
        s.push('\'');
        s
    } else {
        format!("U+{:04X}", r)
    }
}

fn hex_module() -> Vec<(&'static str, Object)> {
    vec![("encode", f_ayrs("encode", hex_encode)), ("decode", f_asrye("decode", hex_decode))]
}

#[allow(dead_code)]
pub fn rc_str(s: &[u8]) -> Rc<Vec<u8>> {
    Rc::new(s.to_vec())
}
