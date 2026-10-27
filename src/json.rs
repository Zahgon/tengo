//! The json module (stdlib/json.go, stdlib/json/*.go) and Go's json.Indent / HTMLEscape.

use crate::ftoa::{format_float, parse_float};
use crate::goslice::GoSlice;
use crate::gostr::{decode_rune, parse_int, quote, rune_string, RUNE_ERROR};
use crate::objects::*;
use crate::stdlib::{array_of, check_args};

const HEX: &[u8] = b"0123456789abcdef";

// ---------------------------------------------------------------------------------------------
// scanner (shared by the Tengo decoder and Go's Indent)

#[derive(Clone, Copy, PartialEq, Debug)]
enum Step {
    BeginValue,
    BeginValueOrEmpty,
    BeginStringOrEmpty,
    BeginString,
    EndValue,
    EndTop,
    InString,
    InStringEsc,
    InStringEscU,
    InStringEscU1,
    InStringEscU12,
    InStringEscU123,
    Neg,
    One,
    Zero,
    Dot,
    Dot0,
    E,
    ESign,
    E0,
    T,
    Tr,
    Tru,
    F,
    Fa,
    Fal,
    Fals,
    N,
    Nu,
    Nul,
    Error,
}

const SCAN_CONTINUE: i32 = 0;
const SCAN_BEGIN_LITERAL: i32 = 1;
const SCAN_BEGIN_OBJECT: i32 = 2;
const SCAN_OBJECT_KEY: i32 = 3;
const SCAN_OBJECT_VALUE: i32 = 4;
const SCAN_END_OBJECT: i32 = 5;
const SCAN_BEGIN_ARRAY: i32 = 6;
const SCAN_ARRAY_VALUE: i32 = 7;
const SCAN_END_ARRAY: i32 = 8;
const SCAN_SKIP_SPACE: i32 = 9;
const SCAN_END: i32 = 10;
const SCAN_ERROR: i32 = 11;

const PARSE_OBJECT_KEY: i32 = 0;
const PARSE_OBJECT_VALUE: i32 = 1;
const PARSE_ARRAY_VALUE: i32 = 2;

struct Scanner {
    step: Step,
    end_top: bool,
    parse_state: Vec<i32>,
    err: Option<Vec<u8>>,
    max_depth: Option<usize>,
}

fn is_space(c: u8) -> bool {
    c == b' ' || c == b'\t' || c == b'\r' || c == b'\n'
}

fn is_hex(c: u8) -> bool {
    c.is_ascii_digit() || (b'a'..=b'f').contains(&c) || (b'A'..=b'F').contains(&c)
}

fn quote_char(c: u8) -> Vec<u8> {
    if c == b'\'' {
        return b"'\\''".to_vec();
    }
    if c == b'"' {
        return b"'\"'".to_vec();
    }
    let q = quote(&rune_string(c as i32));
    let mut out = b"'".to_vec();
    out.extend_from_slice(&q[1..q.len() - 1]);
    out.push(b'\'');
    out
}

impl Scanner {
    fn new(max_depth: Option<usize>) -> Scanner {
        Scanner { step: Step::BeginValue, end_top: false, parse_state: Vec::new(), err: None, max_depth }
    }

    fn error(&mut self, c: u8, context: &str) -> i32 {
        self.step = Step::Error;
        let mut m = b"invalid character ".to_vec();
        m.extend_from_slice(&quote_char(c));
        m.push(b' ');
        m.extend_from_slice(context.as_bytes());
        self.err = Some(m);
        SCAN_ERROR
    }

    fn eof(&mut self) -> i32 {
        if self.err.is_some() {
            return SCAN_ERROR;
        }
        if self.end_top {
            return SCAN_END;
        }
        self.step(b' ');
        if self.end_top {
            return SCAN_END;
        }
        if self.err.is_none() {
            self.err = Some(b"unexpected end of JSON input".to_vec());
        }
        SCAN_ERROR
    }

    fn push_parse_state(&mut self, c: u8, p: i32, success: i32) -> i32 {
        self.parse_state.push(p);
        if let Some(max) = self.max_depth {
            if self.parse_state.len() > max {
                return self.error(c, "exceeded max depth");
            }
        }
        success
    }

    fn pop_parse_state(&mut self) {
        self.parse_state.pop();
        if self.parse_state.is_empty() {
            self.step = Step::EndTop;
            self.end_top = true;
        } else {
            self.step = Step::EndValue;
        }
    }

    fn step(&mut self, c: u8) -> i32 {
        match self.step {
            Step::BeginValueOrEmpty => {
                if is_space(c) {
                    return SCAN_SKIP_SPACE;
                }
                if c == b']' {
                    return self.end_value(c);
                }
                self.begin_value(c)
            }
            Step::BeginValue => self.begin_value(c),
            Step::BeginStringOrEmpty => {
                if is_space(c) {
                    return SCAN_SKIP_SPACE;
                }
                if c == b'}' {
                    let n = self.parse_state.len();
                    self.parse_state[n - 1] = PARSE_OBJECT_VALUE;
                    return self.end_value(c);
                }
                self.begin_string(c)
            }
            Step::BeginString => self.begin_string(c),
            Step::EndValue => self.end_value(c),
            Step::EndTop => self.end_top_step(c),
            Step::InString => {
                if c == b'"' {
                    self.step = Step::EndValue;
                    return SCAN_CONTINUE;
                }
                if c == b'\\' {
                    self.step = Step::InStringEsc;
                    return SCAN_CONTINUE;
                }
                if c < 0x20 {
                    return self.error(c, "in string literal");
                }
                SCAN_CONTINUE
            }
            Step::InStringEsc => match c {
                b'b' | b'f' | b'n' | b'r' | b't' | b'\\' | b'/' | b'"' => {
                    self.step = Step::InString;
                    SCAN_CONTINUE
                }
                b'u' => {
                    self.step = Step::InStringEscU;
                    SCAN_CONTINUE
                }
                _ => self.error(c, "in string escape code"),
            },
            Step::InStringEscU | Step::InStringEscU1 | Step::InStringEscU12 | Step::InStringEscU123 => {
                if is_hex(c) {
                    self.step = match self.step {
                        Step::InStringEscU => Step::InStringEscU1,
                        Step::InStringEscU1 => Step::InStringEscU12,
                        Step::InStringEscU12 => Step::InStringEscU123,
                        _ => Step::InString,
                    };
                    return SCAN_CONTINUE;
                }
                self.error(c, "in \\u hexadecimal character escape")
            }
            Step::Neg => {
                if c == b'0' {
                    self.step = Step::Zero;
                    return SCAN_CONTINUE;
                }
                if (b'1'..=b'9').contains(&c) {
                    self.step = Step::One;
                    return SCAN_CONTINUE;
                }
                self.error(c, "in numeric literal")
            }
            Step::One => {
                if c.is_ascii_digit() {
                    self.step = Step::One;
                    return SCAN_CONTINUE;
                }
                self.state0(c)
            }
            Step::Zero => self.state0(c),
            Step::Dot => {
                if c.is_ascii_digit() {
                    self.step = Step::Dot0;
                    return SCAN_CONTINUE;
                }
                self.error(c, "after decimal point in numeric literal")
            }
            Step::Dot0 => {
                if c.is_ascii_digit() {
                    return SCAN_CONTINUE;
                }
                if c == b'e' || c == b'E' {
                    self.step = Step::E;
                    return SCAN_CONTINUE;
                }
                self.end_value(c)
            }
            Step::E => {
                if c == b'+' || c == b'-' {
                    self.step = Step::ESign;
                    return SCAN_CONTINUE;
                }
                self.e_sign(c)
            }
            Step::ESign => self.e_sign(c),
            Step::E0 => {
                if c.is_ascii_digit() {
                    return SCAN_CONTINUE;
                }
                self.end_value(c)
            }
            Step::T => self.lit(c, b'r', Step::Tr, "in literal true (expecting 'r')"),
            Step::Tr => self.lit(c, b'u', Step::Tru, "in literal true (expecting 'u')"),
            Step::Tru => self.lit(c, b'e', Step::EndValue, "in literal true (expecting 'e')"),
            Step::F => self.lit(c, b'a', Step::Fa, "in literal false (expecting 'a')"),
            Step::Fa => self.lit(c, b'l', Step::Fal, "in literal false (expecting 'l')"),
            Step::Fal => self.lit(c, b's', Step::Fals, "in literal false (expecting 's')"),
            Step::Fals => self.lit(c, b'e', Step::EndValue, "in literal false (expecting 'e')"),
            Step::N => self.lit(c, b'u', Step::Nu, "in literal null (expecting 'u')"),
            Step::Nu => self.lit(c, b'l', Step::Nul, "in literal null (expecting 'l')"),
            Step::Nul => self.lit(c, b'l', Step::EndValue, "in literal null (expecting 'l')"),
            Step::Error => SCAN_ERROR,
        }
    }

    fn lit(&mut self, c: u8, want: u8, next: Step, ctx: &str) -> i32 {
        if c == want {
            self.step = next;
            return SCAN_CONTINUE;
        }
        self.error(c, ctx)
    }

    fn begin_value(&mut self, c: u8) -> i32 {
        if is_space(c) {
            return SCAN_SKIP_SPACE;
        }
        match c {
            b'{' => {
                self.step = Step::BeginStringOrEmpty;
                self.push_parse_state(c, PARSE_OBJECT_KEY, SCAN_BEGIN_OBJECT)
            }
            b'[' => {
                self.step = Step::BeginValueOrEmpty;
                self.push_parse_state(c, PARSE_ARRAY_VALUE, SCAN_BEGIN_ARRAY)
            }
            b'"' => {
                self.step = Step::InString;
                SCAN_BEGIN_LITERAL
            }
            b'-' => {
                self.step = Step::Neg;
                SCAN_BEGIN_LITERAL
            }
            b'0' => {
                self.step = Step::Zero;
                SCAN_BEGIN_LITERAL
            }
            b't' => {
                self.step = Step::T;
                SCAN_BEGIN_LITERAL
            }
            b'f' => {
                self.step = Step::F;
                SCAN_BEGIN_LITERAL
            }
            b'n' => {
                self.step = Step::N;
                SCAN_BEGIN_LITERAL
            }
            b'1'..=b'9' => {
                self.step = Step::One;
                SCAN_BEGIN_LITERAL
            }
            _ => self.error(c, "looking for beginning of value"),
        }
    }

    fn begin_string(&mut self, c: u8) -> i32 {
        if is_space(c) {
            return SCAN_SKIP_SPACE;
        }
        if c == b'"' {
            self.step = Step::InString;
            return SCAN_BEGIN_LITERAL;
        }
        self.error(c, "looking for beginning of object key string")
    }

    fn end_value(&mut self, c: u8) -> i32 {
        let n = self.parse_state.len();
        if n == 0 {
            self.step = Step::EndTop;
            self.end_top = true;
            return self.end_top_step(c);
        }
        if is_space(c) {
            self.step = Step::EndValue;
            return SCAN_SKIP_SPACE;
        }
        let ps = self.parse_state[n - 1];
        match ps {
            PARSE_OBJECT_KEY => {
                if c == b':' {
                    self.parse_state[n - 1] = PARSE_OBJECT_VALUE;
                    self.step = Step::BeginValue;
                    return SCAN_OBJECT_KEY;
                }
                self.error(c, "after object key")
            }
            PARSE_OBJECT_VALUE => {
                if c == b',' {
                    self.parse_state[n - 1] = PARSE_OBJECT_KEY;
                    self.step = Step::BeginString;
                    return SCAN_OBJECT_VALUE;
                }
                if c == b'}' {
                    self.pop_parse_state();
                    return SCAN_END_OBJECT;
                }
                self.error(c, "after object key:value pair")
            }
            PARSE_ARRAY_VALUE => {
                if c == b',' {
                    self.step = Step::BeginValue;
                    return SCAN_ARRAY_VALUE;
                }
                if c == b']' {
                    self.pop_parse_state();
                    return SCAN_END_ARRAY;
                }
                self.error(c, "after array element")
            }
            _ => self.error(c, ""),
        }
    }

    fn end_top_step(&mut self, c: u8) -> i32 {
        if !is_space(c) {
            self.error(c, "after top-level value");
        }
        SCAN_END
    }

    fn state0(&mut self, c: u8) -> i32 {
        if c == b'.' {
            self.step = Step::Dot;
            return SCAN_CONTINUE;
        }
        if c == b'e' || c == b'E' {
            self.step = Step::E;
            return SCAN_CONTINUE;
        }
        self.end_value(c)
    }

    fn e_sign(&mut self, c: u8) -> i32 {
        if c.is_ascii_digit() {
            self.step = Step::E0;
            return SCAN_CONTINUE;
        }
        self.error(c, "in exponent of numeric literal")
    }
}

fn check_valid(data: &[u8]) -> Result<(), Vec<u8>> {
    let mut s = Scanner::new(None);
    for &c in data {
        if s.step(c) == SCAN_ERROR {
            return Err(s.err.clone().unwrap());
        }
    }
    if s.eof() == SCAN_ERROR {
        return Err(s.err.clone().unwrap());
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// decoder

struct Decoder<'a> {
    data: &'a [u8],
    off: usize,
    opcode: i32,
    scan: Scanner,
}

impl<'a> Decoder<'a> {
    fn read_index(&self) -> usize {
        self.off - 1
    }

    fn scan_next(&mut self) {
        if self.off < self.data.len() {
            self.opcode = self.scan.step(self.data[self.off]);
            self.off += 1;
        } else {
            self.opcode = self.scan.eof();
            self.off = self.data.len() + 1;
        }
    }

    fn scan_while(&mut self, op: i32) -> bool {
        let mut is_float = false;
        let mut i = self.off;
        while i < self.data.len() {
            let c = self.data[i];
            if c == b'.' || c == b'e' || c == b'E' {
                is_float = true;
            }
            let new_op = self.scan.step(c);
            i += 1;
            if new_op != op {
                self.opcode = new_op;
                self.off = i;
                return is_float;
            }
        }
        self.off = self.data.len() + 1;
        self.opcode = self.scan.eof();
        is_float
    }

    fn value(&mut self) -> Result<Object, Vec<u8>> {
        match self.opcode {
            SCAN_BEGIN_ARRAY => {
                let o = self.array()?;
                self.scan_next();
                Ok(o)
            }
            SCAN_BEGIN_OBJECT => {
                let o = self.object()?;
                self.scan_next();
                Ok(o)
            }
            SCAN_BEGIN_LITERAL => self.literal(),
            _ => Err(b"JSON decoder out of sync - data changing underfoot?".to_vec()),
        }
    }

    fn array(&mut self) -> Result<Object, Vec<u8>> {
        let mut arr = Vec::new();
        loop {
            self.scan_while(SCAN_SKIP_SPACE);
            if self.opcode == SCAN_END_ARRAY {
                break;
            }
            let o = self.value()?;
            arr.push(o);
            if self.opcode == SCAN_SKIP_SPACE {
                self.scan_while(SCAN_SKIP_SPACE);
            }
            if self.opcode == SCAN_END_ARRAY {
                break;
            }
        }
        Ok(array_of(arr))
    }

    fn object(&mut self) -> Result<Object, Vec<u8>> {
        let mut m = MapData::new();
        loop {
            self.scan_while(SCAN_SKIP_SPACE);
            if self.opcode == SCAN_END_OBJECT {
                break;
            }
            let start = self.read_index();
            self.scan_while(SCAN_CONTINUE);
            let item = &self.data[start..self.read_index()];
            let key = unquote_bytes(item).unwrap_or_default();
            if self.opcode == SCAN_SKIP_SPACE {
                self.scan_while(SCAN_SKIP_SPACE);
            }
            self.scan_while(SCAN_SKIP_SPACE);
            let o = self.value()?;
            m.insert(key, o);
            if self.opcode == SCAN_SKIP_SPACE {
                self.scan_while(SCAN_SKIP_SPACE);
            }
            if self.opcode == SCAN_END_OBJECT {
                break;
            }
        }
        Ok(new_map(m))
    }

    fn literal(&mut self) -> Result<Object, Vec<u8>> {
        let start = self.read_index();
        let is_float = self.scan_while(SCAN_CONTINUE);
        let item = &self.data[start..self.read_index()];
        match item[0] {
            b'n' => Ok(Object::Undefined),
            b't' => Ok(Object::Bool(true)),
            b'f' => Ok(Object::Bool(false)),
            b'"' => Ok(new_str(unquote_bytes(item).unwrap_or_default())),
            _ => {
                if is_float {
                    let n = match parse_float(item, 64) {
                        Ok(f) => f,
                        Err((f, _)) => f,
                    };
                    return Ok(Object::Float(n));
                }
                match parse_int(item, 10, 64) {
                    Ok(n) => Ok(Object::Int(n)),
                    Err((_, e)) => Err(e.message()),
                }
            }
        }
    }
}

fn getu4(s: &[u8]) -> i32 {
    if s.len() < 6 || s[0] != b'\\' || s[1] != b'u' {
        return -1;
    }
    let mut r: i32 = 0;
    for &c in &s[2..6] {
        let v = match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => return -1,
        };
        r = r * 16 + v as i32;
    }
    r
}

fn unquote_bytes(s: &[u8]) -> Option<Vec<u8>> {
    if s.len() < 2 || s[0] != b'"' || s[s.len() - 1] != b'"' {
        return None;
    }
    let s = &s[1..s.len() - 1];
    let mut r = 0;
    while r < s.len() {
        let c = s[r];
        if c == b'\\' || c == b'"' || c < b' ' {
            break;
        }
        if c < 0x80 {
            r += 1;
            continue;
        }
        let (rr, size) = decode_rune(&s[r..]);
        if rr == RUNE_ERROR && size == 1 {
            break;
        }
        r += size;
    }
    if r == s.len() {
        return Some(s.to_vec());
    }
    let mut b = s[..r].to_vec();
    while r < s.len() {
        let c = s[r];
        if c == b'\\' {
            r += 1;
            if r >= s.len() {
                return None;
            }
            match s[r] {
                b'"' | b'\\' | b'/' | b'\'' => {
                    b.push(s[r]);
                    r += 1;
                }
                b'b' => {
                    b.push(8);
                    r += 1;
                }
                b'f' => {
                    b.push(12);
                    r += 1;
                }
                b'n' => {
                    b.push(b'\n');
                    r += 1;
                }
                b'r' => {
                    b.push(b'\r');
                    r += 1;
                }
                b't' => {
                    b.push(b'\t');
                    r += 1;
                }
                b'u' => {
                    r -= 1;
                    let mut rr = getu4(&s[r..]);
                    if rr < 0 {
                        return None;
                    }
                    r += 6;
                    if (0xD800..0xE000).contains(&rr) {
                        let rr1 = getu4(&s[r..]);
                        let dec = if (0xD800..0xDC00).contains(&rr) && (0xDC00..0xE000).contains(&rr1) {
                            (((rr - 0xD800) << 10) | (rr1 - 0xDC00)) + 0x10000
                        } else {
                            RUNE_ERROR
                        };
                        if dec != RUNE_ERROR {
                            r += 6;
                            b.extend_from_slice(&rune_string(dec));
                            continue;
                        }
                        rr = RUNE_ERROR;
                    }
                    b.extend_from_slice(&rune_string(rr));
                }
                _ => return None,
            }
        } else if c == b'"' || c < b' ' {
            return None;
        } else if c < 0x80 {
            b.push(c);
            r += 1;
        } else {
            let (rr, size) = decode_rune(&s[r..]);
            r += size;
            b.extend_from_slice(&rune_string(rr));
        }
    }
    Some(b)
}

/// json.Decode
pub fn decode(data: &[u8]) -> Result<Object, Vec<u8>> {
    check_valid(data)?;
    let mut d = Decoder { data, off: 0, opcode: 0, scan: Scanner::new(None) };
    d.scan_while(SCAN_SKIP_SPACE);
    d.value()
}

// ---------------------------------------------------------------------------------------------
// encoder

fn safe(b: u8) -> bool {
    b >= 0x20 && b != b'"' && b != b'\\'
}

fn encode_string(b: &mut Vec<u8>, val: &[u8]) {
    b.push(b'"');
    let mut i = 0;
    while i < val.len() {
        let c = val[i];
        if c > 31 && c != b'"' && c != b'\\' {
            b.push(c);
            i += 1;
        } else {
            break;
        }
    }
    if i == val.len() {
        b.push(b'"');
        return;
    }
    let mut start = i;
    while i < val.len() {
        let c = val[i];
        if c < 0x80 {
            if safe(c) {
                i += 1;
                continue;
            }
            if start < i {
                b.extend_from_slice(&val[start..i]);
            }
            b.push(b'\\');
            match c {
                b'\\' | b'"' => b.push(c),
                b'\n' => b.push(b'n'),
                b'\r' => b.push(b'r'),
                b'\t' => b.push(b't'),
                _ => {
                    b.extend_from_slice(b"u00");
                    b.push(HEX[(c >> 4) as usize]);
                    b.push(HEX[(c & 0xF) as usize]);
                }
            }
            i += 1;
            start = i;
            continue;
        }
        i += 1;
    }
    if start < val.len() {
        b.extend_from_slice(&val[start..]);
    }
    b.push(b'"');
}

fn base64_std(src: &[u8]) -> Vec<u8> {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    for chunk in src.chunks(3) {
        let v = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(A[(v >> 18 & 63) as usize]);
        out.push(A[(v >> 12 & 63) as usize]);
        out.push(if chunk.len() > 1 { A[(v >> 6 & 63) as usize] } else { b'=' });
        out.push(if chunk.len() > 2 { A[(v & 63) as usize] } else { b'=' });
    }
    out
}

/// json.Encode
pub fn encode(o: &Object) -> Result<Vec<u8>, Vec<u8>> {
    let mut b = Vec::new();
    match o {
        Object::Array(_) | Object::ImmutableArray(_) => {
            let items = array_elems(o).unwrap();
            b.push(b'[');
            let n = items.len();
            for (idx, e) in items.iter().enumerate() {
                b.extend_from_slice(&encode(e)?);
                if idx + 1 < n {
                    b.push(b',');
                }
            }
            b.push(b']');
        }
        Object::Map(m) | Object::ImmutableMap(m) => {
            let entries: Vec<(Vec<u8>, Object)> = m.data.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            b.push(b'{');
            let n = entries.len();
            for (idx, (k, v)) in entries.iter().enumerate() {
                encode_string(&mut b, k);
                b.push(b':');
                b.extend_from_slice(&encode(v)?);
                if idx + 1 < n {
                    b.push(b',');
                }
            }
            b.push(b'}');
        }
        Object::Bool(v) => b.extend_from_slice(if *v { b"true" } else { b"false" }),
        Object::Bytes(v) => {
            b.push(b'"');
            b.extend_from_slice(&base64_std(&v.to_vec()));
            b.push(b'"');
        }
        Object::Char(c) => b.extend_from_slice(c.to_string().as_bytes()),
        Object::Float(f) => {
            let f = *f;
            if f.is_infinite() || f.is_nan() {
                return Err(b"unsupported float value".to_vec());
            }
            let abs = f.abs();
            let mut fmt = b'f';
            if abs != 0.0 && (abs < 1e-6 || abs >= 1e21) {
                fmt = b'e';
            }
            let mut y = format_float(f, fmt, -1, 64).unwrap();
            if fmt == b'e' {
                let n = y.len();
                if n >= 4 && y[n - 4] == b'e' && y[n - 3] == b'-' && y[n - 2] == b'0' {
                    y[n - 2] = y[n - 1];
                    y.truncate(n - 1);
                }
            }
            b.extend_from_slice(&y);
        }
        Object::Int(i) => b.extend_from_slice(i.to_string().as_bytes()),
        Object::String(s) => encode_string(&mut b, s),
        Object::Time(t) => b.extend_from_slice(&crate::times::marshal_json(t)?),
        Object::Undefined => b.extend_from_slice(b"null"),
        _ => {}
    }
    Ok(b)
}

// ---------------------------------------------------------------------------------------------
// Go's encoding/json Indent and HTMLEscape

fn append_newline(dst: &mut Vec<u8>, prefix: &[u8], indent: &[u8], depth: usize) {
    dst.push(b'\n');
    dst.extend_from_slice(prefix);
    for _ in 0..depth {
        dst.extend_from_slice(indent);
    }
}

pub fn indent(src: &[u8], prefix: &[u8], ind: &[u8]) -> Result<Vec<u8>, Vec<u8>> {
    let mut scan = Scanner::new(Some(10000));
    let mut dst = Vec::new();
    let mut need_indent = false;
    let mut depth: usize = 0;
    for &c in src {
        let v = scan.step(c);
        if v == SCAN_SKIP_SPACE {
            continue;
        }
        if v == SCAN_ERROR {
            break;
        }
        if need_indent && v != SCAN_END_OBJECT && v != SCAN_END_ARRAY {
            need_indent = false;
            depth += 1;
            append_newline(&mut dst, prefix, ind, depth);
        }
        if v == SCAN_CONTINUE {
            dst.push(c);
            continue;
        }
        match c {
            b'{' | b'[' => {
                need_indent = true;
                dst.push(c);
            }
            b',' => {
                dst.push(c);
                append_newline(&mut dst, prefix, ind, depth);
            }
            b':' => {
                dst.push(c);
                dst.push(b' ');
            }
            b'}' | b']' => {
                if need_indent {
                    need_indent = false;
                } else {
                    depth = depth.wrapping_sub(1);
                    append_newline(&mut dst, prefix, ind, depth);
                }
                dst.push(c);
            }
            _ => dst.push(c),
        }
    }
    if scan.eof() == SCAN_ERROR {
        return Err(scan.err.unwrap());
    }
    Ok(dst)
}

pub fn html_escape(src: &[u8]) -> Vec<u8> {
    let mut dst = Vec::new();
    let mut start = 0;
    for i in 0..src.len() {
        let c = src[i];
        if c == b'<' || c == b'>' || c == b'&' {
            dst.extend_from_slice(&src[start..i]);
            dst.extend_from_slice(&[b'\\', b'u', b'0', b'0', HEX[(c >> 4) as usize], HEX[(c & 0xF) as usize]]);
            start = i + 1;
        }
        if c == 0xE2 && i + 2 < src.len() && src[i + 1] == 0x80 && src[i + 2] & !1 == 0xA8 {
            dst.extend_from_slice(&src[start..i]);
            dst.extend_from_slice(&[b'\\', b'u', b'2', b'0', b'2', HEX[(src[i + 2] & 0xF) as usize]]);
            start = i + 3;
        }
    }
    dst.extend_from_slice(&src[start..]);
    dst
}

fn bytes_or_string(o: &Object) -> Option<Vec<u8>> {
    match o {
        Object::Bytes(b) => Some(b.to_vec()),
        Object::String(s) => Some((**s).clone()),
        _ => None,
    }
}

pub fn json_module() -> Vec<(&'static str, Object)> {
    vec![
        (
            "decode",
            user_fn("decode", |args| {
                check_args(args, 1)?;
                let data = match bytes_or_string(&args[0]) {
                    Some(d) => d,
                    None => return Err(TErr::arg("first", "bytes/string", &args[0])),
                };
                match decode(&data) {
                    Ok(v) => Ok(v),
                    Err(e) => Ok(error_str(e)),
                }
            }),
        ),
        (
            "encode",
            user_fn("encode", |args| {
                check_args(args, 1)?;
                match encode(&args[0]) {
                    Ok(b) => Ok(Object::Bytes(GoSlice::from_appends(b))),
                    Err(e) => Ok(error_str(e)),
                }
            }),
        ),
        (
            "indent",
            user_fn("encode", |args| {
                check_args(args, 3)?;
                let (prefix, ok) = to_string(&args[1])?;
                if !ok {
                    return Err(TErr::arg("prefix", "string(compatible)", &args[1]));
                }
                let (ind, ok) = to_string(&args[2])?;
                if !ok {
                    return Err(TErr::arg("indent", "string(compatible)", &args[2]));
                }
                let data = match bytes_or_string(&args[0]) {
                    Some(d) => d,
                    None => return Err(TErr::arg("first", "bytes/string", &args[0])),
                };
                match indent(&data, &prefix, &ind) {
                    Ok(b) => Ok(Object::Bytes(GoSlice::from_vec_exact(b))),
                    Err(e) => Ok(error_str(e)),
                }
            }),
        ),
        (
            "html_escape",
            user_fn("html_escape", |args| {
                check_args(args, 1)?;
                let data = match bytes_or_string(&args[0]) {
                    Some(d) => d,
                    None => return Err(TErr::arg("first", "bytes/string", &args[0])),
                };
                Ok(Object::Bytes(GoSlice::from_vec_exact(html_escape(&data))))
            }),
        ),
    ]
}
