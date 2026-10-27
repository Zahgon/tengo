//! Tengo's Format (formatter.go): Go's fmt.Sprintf reworked for Objects. %v prints String(),
//! bad verbs print the value's String(), and only ints, floats, strings, bytes and bools have
//! verb-specific formatting.

use crate::ftoa::format_float;
use crate::gostr::{
    append_rune, can_backquote, decode_rune, quote, quote_rune, quote_to_ascii, rune_count, strconv_is_print,
    MAX_RUNE, RUNE_ERROR,
};
use crate::objects::{to_int, Object, TResult};

const LDIGITS: &[u8] = b"0123456789abcdefx";
const UDIGITS: &[u8] = b"0123456789ABCDEFX";

#[derive(Default, Clone, Copy)]
struct Flags {
    wid_present: bool,
    prec_present: bool,
    minus: bool,
    plus: bool,
    sharp: bool,
    space: bool,
    zero: bool,
    plus_v: bool,
    sharp_v: bool,
}

struct Printer {
    buf: Vec<u8>,
    f: Flags,
    wid: i64,
    prec: i64,
    arg: Option<Object>,
    reordered: bool,
    good_arg_num: bool,
}

fn too_large(x: i64) -> bool {
    x > 1_000_000 || x < -1_000_000
}

fn parsenum(s: &[u8], start: usize, end: usize) -> (i64, bool, usize) {
    if start >= end {
        return (0, false, end);
    }
    let mut num = 0i64;
    let mut isnum = false;
    let mut newi = start;
    while newi < end && s[newi].is_ascii_digit() {
        if too_large(num) {
            return (0, false, end);
        }
        num = num * 10 + (s[newi] - b'0') as i64;
        isnum = true;
        newi += 1;
    }
    (num, isnum, newi)
}

fn parse_arg_number(format: &[u8]) -> (i64, usize, bool) {
    if format.len() < 3 {
        return (0, 1, false);
    }
    for i in 1..format.len() {
        if format[i] == b']' {
            let (width, ok, newi) = parsenum(format, 1, i);
            if !ok || newi != i {
                return (0, i + 1, false);
            }
            return (width - 1, i + 1, true);
        }
    }
    (0, 1, false)
}

fn int_from_arg(a: &[Object], arg_num: usize) -> (i64, bool, usize) {
    let mut num = 0;
    let mut is_int = false;
    let mut new_arg_num = arg_num;
    if arg_num < a.len() {
        if let Some(v) = to_int(&a[arg_num]) {
            num = v;
            is_int = true;
        }
        new_arg_num = arg_num + 1;
        if too_large(num) {
            num = 0;
            is_int = false;
        }
    }
    (num, is_int, new_arg_num)
}

/// Truncates s to prec runes when a precision is present (invalid bytes count as one rune).
fn truncate_prec(s: &[u8], prec_present: bool, prec: i64) -> &[u8] {
    if prec_present {
        let mut n = prec;
        let mut i = 0;
        while i < s.len() {
            n -= 1;
            if n < 0 {
                return &s[..i];
            }
            let (_, w) = decode_rune(&s[i..]);
            i += w;
        }
    }
    s
}

impl Printer {
    fn new() -> Printer {
        Printer {
            buf: Vec::new(),
            f: Flags::default(),
            wid: 0,
            prec: 0,
            arg: None,
            reordered: false,
            good_arg_num: true,
        }
    }

    fn write_padding(&mut self, n: i64) {
        if n <= 0 {
            return;
        }
        let pad = if self.f.zero { b'0' } else { b' ' };
        for _ in 0..n {
            self.buf.push(pad);
        }
    }

    fn pad(&mut self, b: &[u8]) {
        if !self.f.wid_present || self.wid == 0 {
            self.buf.extend_from_slice(b);
            return;
        }
        let width = self.wid - rune_count(b) as i64;
        if !self.f.minus {
            self.write_padding(width);
            self.buf.extend_from_slice(b);
        } else {
            self.buf.extend_from_slice(b);
            self.write_padding(width);
        }
    }

    fn fmt_boolean(&mut self, v: bool) {
        self.pad(if v { b"true" } else { b"false" });
    }

    fn fmt_unicode(&mut self, u: u64) {
        let mut prec: i64 = 4;
        if self.f.prec_present && self.prec > 4 {
            prec = self.prec;
        }
        // built right-to-left
        let mut rev: Vec<u8> = Vec::new();
        if self.f.sharp && u <= MAX_RUNE as u64 && strconv_is_print(u as i32) {
            rev.push(b'\'');
            let mut enc = Vec::new();
            append_rune(&mut enc, u as i32);
            for &c in enc.iter().rev() {
                rev.push(c);
            }
            rev.push(b'\'');
            rev.push(b' ');
        }
        let mut u = u;
        while u >= 16 {
            rev.push(UDIGITS[(u & 0xF) as usize]);
            prec -= 1;
            u >>= 4;
        }
        rev.push(UDIGITS[u as usize]);
        prec -= 1;
        while prec > 0 {
            rev.push(b'0');
            prec -= 1;
        }
        rev.push(b'+');
        rev.push(b'U');
        rev.reverse();
        let old_zero = self.f.zero;
        self.f.zero = false;
        self.pad(&rev);
        self.f.zero = old_zero;
    }

    fn fmt_integer(&mut self, u: u64, base: u32, is_signed: bool, verb: i32, digits: &[u8]) {
        let negative = is_signed && (u as i64) < 0;
        let mut u = if negative { u.wrapping_neg() } else { u };
        let mut prec: i64 = 0;
        if self.f.prec_present {
            prec = self.prec;
            if prec == 0 && u == 0 {
                let old_zero = self.f.zero;
                self.f.zero = false;
                let w = self.wid;
                self.write_padding(w);
                self.f.zero = old_zero;
                return;
            }
        } else if self.f.zero && self.f.wid_present {
            prec = self.wid;
            if negative || self.f.plus || self.f.space {
                prec -= 1;
            }
        }
        let mut rev: Vec<u8> = Vec::new();
        let b = base as u64;
        while u >= b {
            rev.push(digits[(u % b) as usize]);
            u /= b;
        }
        rev.push(digits[u as usize]);
        while prec > rev.len() as i64 {
            rev.push(b'0');
        }
        if self.f.sharp {
            match base {
                2 => {
                    rev.push(b'b');
                    rev.push(b'0');
                }
                8 => {
                    if *rev.last().unwrap() != b'0' {
                        rev.push(b'0');
                    }
                }
                16 => {
                    rev.push(digits[16]);
                    rev.push(b'0');
                }
                _ => {}
            }
        }
        if verb == 'O' as i32 {
            rev.push(b'o');
            rev.push(b'0');
        }
        if negative {
            rev.push(b'-');
        } else if self.f.plus {
            rev.push(b'+');
        } else if self.f.space {
            rev.push(b' ');
        }
        rev.reverse();
        let old_zero = self.f.zero;
        self.f.zero = false;
        self.pad(&rev);
        self.f.zero = old_zero;
    }

    fn fmt_s(&mut self, s: &[u8]) {
        let s = truncate_prec(s, self.f.prec_present, self.prec).to_vec();
        self.pad(&s);
    }

    fn fmt_sbx(&mut self, b: &[u8], digits: &[u8]) {
        let mut length = b.len() as i64;
        if self.f.prec_present && self.prec < length {
            length = self.prec;
        }
        let mut width = 2 * length;
        if width > 0 {
            if self.f.space {
                if self.f.sharp {
                    width *= 2;
                }
                width += length - 1;
            } else if self.f.sharp {
                width += 2;
            }
        } else {
            if self.f.wid_present {
                let w = self.wid;
                self.write_padding(w);
            }
            return;
        }
        if self.f.wid_present && self.wid > width && !self.f.minus {
            let w = self.wid - width;
            self.write_padding(w);
        }
        if self.f.sharp {
            self.buf.push(b'0');
            self.buf.push(digits[16]);
        }
        for i in 0..length as usize {
            if self.f.space && i > 0 {
                self.buf.push(b' ');
                if self.f.sharp {
                    self.buf.push(b'0');
                    self.buf.push(digits[16]);
                }
            }
            let c = b[i];
            self.buf.push(digits[(c >> 4) as usize]);
            self.buf.push(digits[(c & 0xF) as usize]);
        }
        if self.f.wid_present && self.wid > width && self.f.minus {
            let w = self.wid - width;
            self.write_padding(w);
        }
    }

    fn fmt_q(&mut self, s: &[u8]) {
        let s = truncate_prec(s, self.f.prec_present, self.prec).to_vec();
        if self.f.sharp && can_backquote(&s) {
            let mut q = b"`".to_vec();
            q.extend_from_slice(&s);
            q.push(b'`');
            self.pad(&q);
            return;
        }
        let q = if self.f.plus { quote_to_ascii(&s) } else { quote(&s) };
        self.pad(&q);
    }

    fn fmt_c(&mut self, c: u64) {
        let r = if c > MAX_RUNE as u64 { RUNE_ERROR } else { c as i32 };
        let mut b = Vec::new();
        append_rune(&mut b, r);
        self.pad(&b);
    }

    fn fmt_qc(&mut self, c: u64) {
        let r = if c > MAX_RUNE as u64 { RUNE_ERROR } else { c as i32 };
        let q = quote_rune(r, self.f.plus);
        self.pad(&q);
    }

    fn fmt_float_raw(&mut self, v: f64, size: i64, verb: u8, prec: i64) {
        let prec = if self.f.prec_present { self.prec } else { prec };
        let mut num = vec![0u8];
        num.extend_from_slice(&format_float(v, verb, prec, size).unwrap());
        if num[1] == b'-' || num[1] == b'+' {
            num.remove(0);
        } else {
            num[0] = b'+';
        }
        if self.f.space && num[0] == b'+' && !self.f.plus {
            num[0] = b' ';
        }
        if num[1] == b'I' || num[1] == b'N' {
            let old_zero = self.f.zero;
            self.f.zero = false;
            if num[1] == b'N' && !self.f.space && !self.f.plus {
                num.remove(0);
            }
            self.pad(&num);
            self.f.zero = old_zero;
            return;
        }
        if self.f.sharp && verb != b'b' {
            let mut digits: i64 = 0;
            if matches!(verb, b'v' | b'g' | b'G' | b'x') {
                digits = prec;
                if digits == -1 {
                    digits = 6;
                }
            }
            let mut tail: Vec<u8> = Vec::new();
            let mut has_decimal_point = false;
            let mut i = 1;
            while i < num.len() {
                match num[i] {
                    b'.' => has_decimal_point = true,
                    b'p' | b'P' => {
                        tail.extend_from_slice(&num[i..]);
                        num.truncate(i);
                    }
                    b'e' | b'E' if verb != b'x' && verb != b'X' => {
                        tail.extend_from_slice(&num[i..]);
                        num.truncate(i);
                    }
                    _ => digits -= 1,
                }
                i += 1;
            }
            if !has_decimal_point {
                num.push(b'.');
            }
            while digits > 0 {
                num.push(b'0');
                digits -= 1;
            }
            num.extend_from_slice(&tail);
        }
        if self.f.plus || num[0] != b'+' {
            if self.f.zero && self.f.wid_present && self.wid > num.len() as i64 {
                self.buf.push(num[0]);
                let w = self.wid - num.len() as i64;
                self.write_padding(w);
                self.buf.extend_from_slice(&num[1..]);
                return;
            }
            self.pad(&num);
            return;
        }
        self.pad(&num[1..]);
    }

    fn write_rune(&mut self, r: i32) {
        append_rune(&mut self.buf, r);
    }

    fn bad_verb(&mut self, verb: i32) -> TResult<()> {
        self.buf.extend_from_slice(b"%!");
        self.write_rune(verb);
        self.buf.push(b'(');
        match self.arg.clone() {
            Some(arg) => {
                let s = arg.string()?;
                self.buf.extend_from_slice(&s);
                self.buf.push(b'=');
                self.print_arg(&arg, 'v' as i32)?;
            }
            None => self.buf.extend_from_slice(b"<undefined>"),
        }
        self.buf.push(b')');
        Ok(())
    }

    fn fmt_bool(&mut self, v: bool, verb: i32) -> TResult<()> {
        if verb == 't' as i32 || verb == 'v' as i32 {
            self.fmt_boolean(v);
            Ok(())
        } else {
            self.bad_verb(verb)
        }
    }

    fn fmt_int(&mut self, v: u64, is_signed: bool, verb: i32) -> TResult<()> {
        match char::from_u32(verb as u32).unwrap_or('\u{FFFD}') {
            'v' | 'd' => self.fmt_integer(v, 10, is_signed, verb, LDIGITS),
            'b' => self.fmt_integer(v, 2, is_signed, verb, LDIGITS),
            'o' | 'O' => self.fmt_integer(v, 8, is_signed, verb, LDIGITS),
            'x' => self.fmt_integer(v, 16, is_signed, verb, LDIGITS),
            'X' => self.fmt_integer(v, 16, is_signed, verb, UDIGITS),
            'c' => self.fmt_c(v),
            'q' => {
                if v <= MAX_RUNE as u64 {
                    self.fmt_qc(v)
                } else {
                    return self.bad_verb(verb);
                }
            }
            'U' => self.fmt_unicode(v),
            _ => return self.bad_verb(verb),
        }
        Ok(())
    }

    fn fmt_float(&mut self, v: f64, verb: i32) -> TResult<()> {
        match char::from_u32(verb as u32).unwrap_or('\u{FFFD}') {
            'v' => self.fmt_float_raw(v, 64, b'g', -1),
            'b' | 'g' | 'G' | 'x' | 'X' => self.fmt_float_raw(v, 64, verb as u8, -1),
            'f' | 'e' | 'E' => self.fmt_float_raw(v, 64, verb as u8, 6),
            'F' => self.fmt_float_raw(v, 64, b'f', 6),
            _ => return self.bad_verb(verb),
        }
        Ok(())
    }

    fn fmt_string(&mut self, v: &[u8], verb: i32) -> TResult<()> {
        match char::from_u32(verb as u32).unwrap_or('\u{FFFD}') {
            'v' => {
                if self.f.sharp_v {
                    self.fmt_q(v)
                } else {
                    self.fmt_s(v)
                }
            }
            's' => self.fmt_s(v),
            'x' => self.fmt_sbx(v, LDIGITS),
            'X' => self.fmt_sbx(v, UDIGITS),
            'q' => self.fmt_q(v),
            _ => return self.bad_verb(verb),
        }
        Ok(())
    }

    fn fmt_bytes(&mut self, v: &[u8], verb: i32) {
        match char::from_u32(verb as u32).unwrap_or('\u{FFFD}') {
            'v' | 'd' => {
                self.buf.push(b'[');
                for (i, &c) in v.iter().enumerate() {
                    if i > 0 {
                        self.buf.push(b' ');
                    }
                    self.fmt_integer(c as u64, 10, false, verb, LDIGITS);
                }
                self.buf.push(b']');
            }
            's' => {
                let s = truncate_prec(v, self.f.prec_present, self.prec).to_vec();
                self.pad(&s);
            }
            'x' => self.fmt_sbx(v, LDIGITS),
            'X' => self.fmt_sbx(v, UDIGITS),
            'q' => self.fmt_q(v),
            _ => {}
        }
    }

    fn print_arg(&mut self, arg: &Object, verb: i32) -> TResult<()> {
        self.arg = Some(arg.clone());
        if verb == 'T' as i32 {
            let t = arg.type_name();
            self.fmt_s(t.as_bytes());
            return Ok(());
        }
        if verb == 'v' as i32 {
            let s = arg.string()?;
            self.fmt_s(&s);
            return Ok(());
        }
        match arg {
            Object::Bool(_) => self.fmt_bool(!arg.is_falsy(), verb),
            Object::Float(f) => self.fmt_float(*f, verb),
            Object::Int(i) => self.fmt_int(*i as u64, true, verb),
            Object::String(s) => self.fmt_string(s, verb),
            Object::Bytes(b) => {
                self.fmt_bytes(&b.to_vec(), verb);
                Ok(())
            }
            other => {
                let s = other.string()?;
                self.fmt_string(&s, verb)
            }
        }
    }

    fn arg_number(&mut self, arg_num: usize, format: &[u8], i: usize, num_args: usize) -> (usize, usize, bool) {
        if format.len() <= i || format[i] != b'[' {
            return (arg_num, i, false);
        }
        self.reordered = true;
        let (index, wid, ok) = parse_arg_number(&format[i..]);
        if ok && 0 <= index && (index as usize) < num_args {
            return (index as usize, i + wid, true);
        }
        self.good_arg_num = false;
        (arg_num, i + wid, ok)
    }

    fn do_format(&mut self, format: &[u8], a: &[Object]) -> TResult<()> {
        let end = format.len();
        let mut arg_num: usize = 0;
        let mut after_index;
        self.reordered = false;
        let mut i = 0;
        'format_loop: while i < end {
            self.good_arg_num = true;
            let lasti = i;
            while i < end && format[i] != b'%' {
                i += 1;
            }
            if i > lasti {
                self.buf.extend_from_slice(&format[lasti..i]);
            }
            if i >= end {
                break;
            }
            i += 1;
            self.f = Flags::default();
            while i < end {
                let c = format[i];
                match c {
                    b'#' => self.f.sharp = true,
                    b'0' => self.f.zero = !self.f.minus,
                    b'+' => self.f.plus = true,
                    b'-' => {
                        self.f.minus = true;
                        self.f.zero = false;
                    }
                    b' ' => self.f.space = true,
                    _ => {
                        if c.is_ascii_lowercase() && arg_num < a.len() {
                            if c == b'v' {
                                self.f.sharp_v = self.f.sharp;
                                self.f.sharp = false;
                                self.f.plus_v = self.f.plus;
                                self.f.plus = false;
                            }
                            let arg = a[arg_num].clone();
                            self.print_arg(&arg, c as i32)?;
                            arg_num += 1;
                            i += 1;
                            continue 'format_loop;
                        }
                        break;
                    }
                }
                i += 1;
            }

            let r = self.arg_number(arg_num, format, i, a.len());
            arg_num = r.0;
            i = r.1;
            after_index = r.2;

            if i < end && format[i] == b'*' {
                i += 1;
                let (w, ok, n) = int_from_arg(a, arg_num);
                self.wid = w;
                self.f.wid_present = ok;
                arg_num = n;
                if !self.f.wid_present {
                    self.buf.extend_from_slice(b"%!(BADWIDTH)");
                }
                if self.wid < 0 {
                    self.wid = -self.wid;
                    self.f.minus = true;
                    self.f.zero = false;
                }
                after_index = false;
            } else {
                let (w, ok, ni) = parsenum(format, i, end);
                self.wid = w;
                self.f.wid_present = ok;
                i = ni;
                if after_index && self.f.wid_present {
                    self.good_arg_num = false;
                }
            }

            if i + 1 < end && format[i] == b'.' {
                i += 1;
                if after_index {
                    self.good_arg_num = false;
                }
                let r = self.arg_number(arg_num, format, i, a.len());
                arg_num = r.0;
                i = r.1;
                after_index = r.2;
                if i < end && format[i] == b'*' {
                    i += 1;
                    let (p, ok, n) = int_from_arg(a, arg_num);
                    self.prec = p;
                    self.f.prec_present = ok;
                    arg_num = n;
                    if self.prec < 0 {
                        self.prec = 0;
                        self.f.prec_present = false;
                    }
                    if !self.f.prec_present {
                        self.buf.extend_from_slice(b"%!(BADPREC)");
                    }
                    after_index = false;
                } else {
                    let (p, ok, ni) = parsenum(format, i, end);
                    self.prec = p;
                    self.f.prec_present = ok;
                    i = ni;
                    if !self.f.prec_present {
                        self.prec = 0;
                        self.f.prec_present = true;
                    }
                }
            }

            if !after_index {
                let r = self.arg_number(arg_num, format, i, a.len());
                arg_num = r.0;
                i = r.1;
            }

            if i >= end {
                self.buf.extend_from_slice(b"%!(NOVERB)");
                break;
            }

            let (verb, size) = if format[i] < 0x80 {
                (format[i] as i32, 1)
            } else {
                decode_rune(&format[i..])
            };
            i += size;

            if verb == '%' as i32 {
                self.buf.push(b'%');
            } else if !self.good_arg_num {
                self.buf.extend_from_slice(b"%!");
                self.write_rune(verb);
                self.buf.extend_from_slice(b"(BADINDEX)");
            } else if arg_num >= a.len() {
                self.buf.extend_from_slice(b"%!");
                self.write_rune(verb);
                self.buf.extend_from_slice(b"(MISSING)");
            } else {
                if verb == 'v' as i32 {
                    self.f.sharp_v = self.f.sharp;
                    self.f.sharp = false;
                    self.f.plus_v = self.f.plus;
                    self.f.plus = false;
                }
                let arg = a[arg_num].clone();
                self.print_arg(&arg, verb)?;
                arg_num += 1;
            }
        }

        if !self.reordered && arg_num < a.len() {
            self.f = Flags::default();
            self.buf.extend_from_slice(b"%!(EXTRA ");
            for (k, arg) in a[arg_num..].iter().enumerate() {
                if k > 0 {
                    self.buf.extend_from_slice(b", ");
                }
                self.buf.extend_from_slice(arg.type_name().as_bytes());
                self.buf.push(b'=');
                self.print_arg(arg, 'v' as i32)?;
            }
            self.buf.push(b')');
        }
        Ok(())
    }
}

/// tengo.Format
pub fn format(format: &[u8], a: &[Object]) -> TResult<Vec<u8>> {
    let mut p = Printer::new();
    p.do_format(format, a)?;
    Ok(p.buf)
}
