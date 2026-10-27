//! strconv.FormatFloat and strconv.ParseFloat with Go's exact output.
//!
//! Formatting follows Go's bigFtoa: the float's exact decimal expansion (the `Decimal` type
//! below, a port of strconv/decimal.go), shortest digits from roundShortest, and
//! round-half-even for fixed precision. Parsing accepts Go's syntax (underscores after a base
//! prefix, hex mantissas with p exponents, "inf"/"infinity"/"nan" in any case).

use crate::gostr::{underscore_ok, NumErr, NumError};

#[derive(Clone, Copy)]
struct FloatInfo {
    mantbits: u32,
    expbits: u32,
    bias: i32,
}

const FLOAT32: FloatInfo = FloatInfo { mantbits: 23, expbits: 8, bias: -127 };
const FLOAT64: FloatInfo = FloatInfo { mantbits: 52, expbits: 11, bias: -1023 };

const MAX_DIGITS: usize = 800;

#[derive(Clone)]
struct Decimal {
    d: Vec<u8>,
    nd: usize,
    dp: i32,
    trunc: bool,
}

impl Decimal {
    fn new() -> Self {
        Decimal { d: vec![0u8; MAX_DIGITS], nd: 0, dp: 0, trunc: false }
    }

    fn trim(&mut self) {
        while self.nd > 0 && self.d[self.nd - 1] == b'0' {
            self.nd -= 1;
        }
        if self.nd == 0 {
            self.dp = 0;
        }
    }

    fn assign(&mut self, mut v: u64) {
        let mut buf = [0u8; 24];
        let mut n = 0;
        while v > 0 {
            let v1 = v / 10;
            buf[n] = (v - 10 * v1) as u8 + b'0';
            n += 1;
            v = v1;
        }
        self.nd = 0;
        while n > 0 {
            n -= 1;
            self.d[self.nd] = buf[n];
            self.nd += 1;
        }
        self.dp = self.nd as i32;
        self.trim();
    }

    fn right_shift(&mut self, k: u32) {
        let mut r = 0usize;
        let mut w = 0usize;
        let mut n: u64 = 0;
        while n >> k == 0 {
            if r >= self.nd {
                if n == 0 {
                    self.nd = 0;
                    return;
                }
                while n >> k == 0 {
                    n *= 10;
                    r += 1;
                }
                break;
            }
            let c = self.d[r] as u64;
            n = n * 10 + c - b'0' as u64;
            r += 1;
        }
        self.dp -= r as i32 - 1;
        let mask: u64 = (1u64 << k) - 1;
        while r < self.nd {
            let c = self.d[r] as u64;
            let dig = n >> k;
            n &= mask;
            self.d[w] = dig as u8 + b'0';
            w += 1;
            n = n * 10 + c - b'0' as u64;
            r += 1;
        }
        while n > 0 {
            let dig = n >> k;
            n &= mask;
            if w < MAX_DIGITS {
                self.d[w] = dig as u8 + b'0';
                w += 1;
            } else if dig > 0 {
                self.trunc = true;
            }
            n *= 10;
        }
        self.nd = w;
        self.trim();
    }

    fn left_shift(&mut self, k: u32) {
        let (mut delta, cutoff) = LEFTCHEATS[k as usize];
        if prefix_is_less_than(&self.d[..self.nd], cutoff.as_bytes()) {
            delta -= 1;
        }
        let mut r = self.nd as isize;
        let mut w = self.nd as isize + delta as isize;
        let mut n: u64 = 0;
        r -= 1;
        while r >= 0 {
            n += ((self.d[r as usize] - b'0') as u64) << k;
            let quo = n / 10;
            let rem = n - 10 * quo;
            w -= 1;
            if (w as usize) < MAX_DIGITS {
                self.d[w as usize] = rem as u8 + b'0';
            } else if rem != 0 {
                self.trunc = true;
            }
            n = quo;
            r -= 1;
        }
        while n > 0 {
            let quo = n / 10;
            let rem = n - 10 * quo;
            w -= 1;
            if (w as usize) < MAX_DIGITS {
                self.d[w as usize] = rem as u8 + b'0';
            } else if rem != 0 {
                self.trunc = true;
            }
            n = quo;
        }
        self.nd += delta as usize;
        if self.nd >= MAX_DIGITS {
            self.nd = MAX_DIGITS;
        }
        self.dp += delta;
        self.trim();
    }

    fn shift(&mut self, mut k: i32) {
        const MAX_SHIFT: i32 = 60;
        if self.nd == 0 {
            return;
        }
        if k > 0 {
            while k > MAX_SHIFT {
                self.left_shift(MAX_SHIFT as u32);
                k -= MAX_SHIFT;
            }
            self.left_shift(k as u32);
        } else if k < 0 {
            while k < -MAX_SHIFT {
                self.right_shift(MAX_SHIFT as u32);
                k += MAX_SHIFT;
            }
            self.right_shift((-k) as u32);
        }
    }

    fn should_round_up(&self, nd: i32) -> bool {
        if nd < 0 || nd as usize >= self.nd {
            return false;
        }
        let nd = nd as usize;
        if self.d[nd] == b'5' && nd + 1 == self.nd {
            if self.trunc {
                return true;
            }
            return nd > 0 && (self.d[nd - 1] - b'0') % 2 != 0;
        }
        self.d[nd] >= b'5'
    }

    fn round(&mut self, nd: i32) {
        if nd < 0 || nd as usize >= self.nd {
            return;
        }
        if self.should_round_up(nd) {
            self.round_up(nd);
        } else {
            self.round_down(nd);
        }
    }

    fn round_down(&mut self, nd: i32) {
        if nd < 0 || nd as usize >= self.nd {
            return;
        }
        self.nd = nd as usize;
        self.trim();
    }

    fn round_up(&mut self, nd: i32) {
        if nd < 0 || nd as usize >= self.nd {
            return;
        }
        let mut i = nd - 1;
        while i >= 0 {
            let c = self.d[i as usize];
            if c < b'9' {
                self.d[i as usize] += 1;
                self.nd = i as usize + 1;
                return;
            }
            i -= 1;
        }
        self.d[0] = b'1';
        self.nd = 1;
        self.dp += 1;
    }
}

fn prefix_is_less_than(b: &[u8], s: &[u8]) -> bool {
    for i in 0..s.len() {
        if i >= b.len() {
            return true;
        }
        if b[i] != s[i] {
            return b[i] < s[i];
        }
    }
    false
}

const LEFTCHEATS: [(i32, &str); 61] = [
    (0, ""),
    (1, "5"),
    (1, "25"),
    (1, "125"),
    (2, "625"),
    (2, "3125"),
    (2, "15625"),
    (3, "78125"),
    (3, "390625"),
    (3, "1953125"),
    (4, "9765625"),
    (4, "48828125"),
    (4, "244140625"),
    (4, "1220703125"),
    (5, "6103515625"),
    (5, "30517578125"),
    (5, "152587890625"),
    (6, "762939453125"),
    (6, "3814697265625"),
    (6, "19073486328125"),
    (7, "95367431640625"),
    (7, "476837158203125"),
    (7, "2384185791015625"),
    (7, "11920928955078125"),
    (8, "59604644775390625"),
    (8, "298023223876953125"),
    (8, "1490116119384765625"),
    (9, "7450580596923828125"),
    (9, "37252902984619140625"),
    (9, "186264514923095703125"),
    (10, "931322574615478515625"),
    (10, "4656612873077392578125"),
    (10, "23283064365386962890625"),
    (10, "116415321826934814453125"),
    (11, "582076609134674072265625"),
    (11, "2910383045673370361328125"),
    (11, "14551915228366851806640625"),
    (12, "72759576141834259033203125"),
    (12, "363797880709171295166015625"),
    (12, "1818989403545856475830078125"),
    (13, "9094947017729282379150390625"),
    (13, "45474735088646411895751953125"),
    (13, "227373675443232059478759765625"),
    (13, "1136868377216160297393798828125"),
    (14, "5684341886080801486968994140625"),
    (14, "28421709430404007434844970703125"),
    (14, "142108547152020037174224853515625"),
    (15, "710542735760100185871124267578125"),
    (15, "3552713678800500929355621337890625"),
    (15, "17763568394002504646778106689453125"),
    (16, "88817841970012523233890533447265625"),
    (16, "444089209850062616169452667236328125"),
    (16, "2220446049250313080847263336181640625"),
    (16, "11102230246251565404236316680908203125"),
    (17, "55511151231257827021181583404541015625"),
    (17, "277555756156289135105907917022705078125"),
    (17, "1387778780781445675529539585113525390625"),
    (18, "6938893903907228377647697925567626953125"),
    (18, "34694469519536141888238489627838134765625"),
    (18, "173472347597680709441192448139190673828125"),
    (19, "867361737988403547205962240695953369140625"),
];

fn round_shortest(d: &mut Decimal, mant: u64, exp: i32, flt: &FloatInfo) {
    if mant == 0 {
        d.nd = 0;
        return;
    }
    let minexp = flt.bias + 1;
    if exp > minexp && 332 * (d.dp - d.nd as i32) >= 100 * (exp - flt.mantbits as i32) {
        return;
    }
    let mut upper = Decimal::new();
    upper.assign(mant * 2 + 1);
    upper.shift(exp - flt.mantbits as i32 - 1);
    let (mantlo, explo) = if mant > 1u64 << flt.mantbits || exp == minexp {
        (mant - 1, exp)
    } else {
        (mant * 2 - 1, exp - 1)
    };
    let mut lower = Decimal::new();
    lower.assign(mantlo * 2 + 1);
    lower.shift(explo - flt.mantbits as i32 - 1);
    let inclusive = mant % 2 == 0;
    let mut upperdelta = 0u8;
    let mut ui: i32 = 0;
    loop {
        let mi = ui - upper.dp + d.dp;
        if mi >= d.nd as i32 {
            break;
        }
        let li = ui - upper.dp + lower.dp;
        let l = if li >= 0 && (li as usize) < lower.nd { lower.d[li as usize] } else { b'0' };
        let m = if mi >= 0 { d.d[mi as usize] } else { b'0' };
        let u = if (ui as usize) < upper.nd { upper.d[ui as usize] } else { b'0' };
        let okdown = l != m || inclusive && li + 1 == lower.nd as i32;
        if upperdelta == 0 && m + 1 < u {
            upperdelta = 2;
        } else if upperdelta == 0 && m != u {
            upperdelta = 1;
        } else if upperdelta == 1 && (m != b'9' || u != b'0') {
            upperdelta = 2;
        }
        let okup = upperdelta > 0 && (inclusive || upperdelta > 1 || ui + 1 < upper.nd as i32);
        if okdown && okup {
            d.round(mi + 1);
            return;
        } else if okdown {
            d.round_down(mi + 1);
            return;
        } else if okup {
            d.round_up(mi + 1);
            return;
        }
        ui += 1;
    }
}

struct DigitSlice<'a> {
    d: &'a [u8],
    nd: usize,
    dp: i32,
}

fn fmt_e(dst: &mut Vec<u8>, neg: bool, d: &DigitSlice, prec: i32, fmt: u8) {
    if neg {
        dst.push(b'-');
    }
    let ch = if d.nd != 0 { d.d[0] } else { b'0' };
    dst.push(ch);
    if prec > 0 {
        dst.push(b'.');
        let mut i = 1usize;
        let m = d.nd.min(prec as usize + 1);
        if i < m {
            dst.extend_from_slice(&d.d[i..m]);
            i = m;
        }
        while i as i32 <= prec {
            dst.push(b'0');
            i += 1;
        }
    }
    dst.push(fmt);
    let mut exp = d.dp - 1;
    if d.nd == 0 {
        exp = 0;
    }
    if exp < 0 {
        dst.push(b'-');
        exp = -exp;
    } else {
        dst.push(b'+');
    }
    if exp < 10 {
        dst.push(b'0');
        dst.push(exp as u8 + b'0');
    } else if exp < 100 {
        dst.push((exp / 10) as u8 + b'0');
        dst.push((exp % 10) as u8 + b'0');
    } else {
        dst.push((exp / 100) as u8 + b'0');
        dst.push(((exp / 10) % 10) as u8 + b'0');
        dst.push((exp % 10) as u8 + b'0');
    }
}

fn fmt_f(dst: &mut Vec<u8>, neg: bool, d: &DigitSlice, prec: i32) {
    if neg {
        dst.push(b'-');
    }
    if d.dp > 0 {
        let mut m = d.nd.min(d.dp as usize);
        dst.extend_from_slice(&d.d[..m]);
        while (m as i32) < d.dp {
            dst.push(b'0');
            m += 1;
        }
    } else {
        dst.push(b'0');
    }
    if prec > 0 {
        dst.push(b'.');
        for i in 0..prec {
            let j = d.dp + i;
            let ch = if j >= 0 && (j as usize) < d.nd { d.d[j as usize] } else { b'0' };
            dst.push(ch);
        }
    }
}

fn format_digits(dst: &mut Vec<u8>, shortest: bool, neg: bool, d: &DigitSlice, mut prec: i32, fmt: u8) {
    match fmt {
        b'e' | b'E' => fmt_e(dst, neg, d, prec, fmt),
        b'f' => fmt_f(dst, neg, d, prec),
        b'g' | b'G' => {
            let mut eprec = prec;
            if eprec > d.nd as i32 && d.nd as i32 >= d.dp {
                eprec = d.nd as i32;
            }
            if shortest {
                eprec = 6;
            }
            let exp = d.dp - 1;
            if exp < -4 || exp >= eprec {
                if prec > d.nd as i32 {
                    prec = d.nd as i32;
                }
                fmt_e(dst, neg, d, prec - 1, fmt + b'e' - b'g');
                return;
            }
            if prec > d.dp {
                prec = d.nd as i32;
            }
            fmt_f(dst, neg, d, (prec - d.dp).max(0));
        }
        _ => {
            dst.push(b'%');
            dst.push(fmt);
        }
    }
}

fn fmt_b(dst: &mut Vec<u8>, neg: bool, mant: u64, exp: i32, flt: &FloatInfo) {
    if neg {
        dst.push(b'-');
    }
    dst.extend_from_slice(mant.to_string().as_bytes());
    dst.push(b'p');
    let exp = exp - flt.mantbits as i32;
    if exp >= 0 {
        dst.push(b'+');
    }
    dst.extend_from_slice(exp.to_string().as_bytes());
}

fn fmt_x(dst: &mut Vec<u8>, prec: i32, fmt: u8, neg: bool, mut mant: u64, mut exp: i32, flt: &FloatInfo) {
    if mant == 0 {
        exp = 0;
    }
    mant <<= 60 - flt.mantbits;
    while mant != 0 && mant & (1u64 << 60) == 0 {
        mant <<= 1;
        exp -= 1;
    }
    if (0..15).contains(&prec) {
        let shift = (prec * 4) as u32;
        let extra = (mant << shift) & ((1u64 << 60) - 1);
        mant >>= 60 - shift;
        if extra | (mant & 1) > 1u64 << 59 {
            mant += 1;
        }
        mant <<= 60 - shift;
        if mant & (1u64 << 61) != 0 {
            mant >>= 1;
            exp += 1;
        }
    }
    let hex: &[u8] = if fmt == b'X' { b"0123456789ABCDEF" } else { b"0123456789abcdef" };
    if neg {
        dst.push(b'-');
    }
    dst.push(b'0');
    dst.push(fmt);
    dst.push(b'0' + ((mant >> 60) & 1) as u8);
    mant <<= 4;
    if prec < 0 && mant != 0 {
        dst.push(b'.');
        while mant != 0 {
            dst.push(hex[((mant >> 60) & 15) as usize]);
            mant <<= 4;
        }
    } else if prec > 0 {
        dst.push(b'.');
        for _ in 0..prec {
            dst.push(hex[((mant >> 60) & 15) as usize]);
            mant <<= 4;
        }
    }
    dst.push(if fmt == b'x' { b'p' } else { b'P' });
    if exp < 0 {
        dst.push(b'-');
        exp = -exp;
    } else {
        dst.push(b'+');
    }
    if exp < 100 {
        dst.push((exp / 10) as u8 + b'0');
        dst.push((exp % 10) as u8 + b'0');
    } else if exp < 1000 {
        dst.push((exp / 100) as u8 + b'0');
        dst.push(((exp / 10) % 10) as u8 + b'0');
        dst.push((exp % 10) as u8 + b'0');
    } else {
        dst.push((exp / 1000) as u8 + b'0');
        dst.push(((exp / 100) % 10) as u8 + b'0');
        dst.push(((exp / 10) % 10) as u8 + b'0');
        dst.push((exp % 10) as u8 + b'0');
    }
}

/// strconv.FormatFloat(val, fmt, prec, bitSize). Returns None for an illegal bitSize (Go
/// panics). An unknown verb yields "%" followed by the verb, as Go does.
pub fn format_float(val: f64, fmt: u8, prec: i64, bit_size: i64) -> Option<Vec<u8>> {
    let (bits, flt): (u64, FloatInfo) = match bit_size {
        32 => ((val as f32).to_bits() as u64, FLOAT32),
        64 => (val.to_bits(), FLOAT64),
        _ => return None,
    };
    let mut dst = Vec::new();
    let neg = bits >> (flt.expbits + flt.mantbits) != 0;
    let mut exp = ((bits >> flt.mantbits) & ((1u64 << flt.expbits) - 1)) as i32;
    let mut mant = bits & ((1u64 << flt.mantbits) - 1);
    if exp == (1 << flt.expbits) - 1 {
        let s: &[u8] = if mant != 0 {
            b"NaN"
        } else if neg {
            b"-Inf"
        } else {
            b"+Inf"
        };
        dst.extend_from_slice(s);
        return Some(dst);
    } else if exp == 0 {
        exp += 1;
    } else {
        mant |= 1u64 << flt.mantbits;
    }
    exp += flt.bias;
    if fmt == b'b' {
        fmt_b(&mut dst, neg, mant, exp, &flt);
        return Some(dst);
    }
    // A precision this large cannot change the digits but must not overflow i32.
    let mut prec: i32 = prec.clamp(-1_000_000, 1_000_000) as i32;
    if prec < -1 {
        prec = -1;
    }
    if fmt == b'x' || fmt == b'X' {
        fmt_x(&mut dst, prec, fmt, neg, mant, exp, &flt);
        return Some(dst);
    }
    let mut d = Decimal::new();
    d.assign(mant);
    d.shift(exp - flt.mantbits as i32);
    let shortest = prec < 0;
    if shortest {
        round_shortest(&mut d, mant, exp, &flt);
        match fmt {
            b'e' | b'E' => prec = d.nd as i32 - 1,
            b'f' => prec = (d.nd as i32 - d.dp).max(0),
            b'g' | b'G' => prec = d.nd as i32,
            _ => {}
        }
    } else {
        match fmt {
            b'e' | b'E' => d.round(prec + 1),
            b'f' => d.round(d.dp + prec),
            b'g' | b'G' => {
                if prec == 0 {
                    prec = 1;
                }
                d.round(prec);
            }
            _ => {}
        }
    }
    let digs = DigitSlice { d: &d.d, nd: d.nd, dp: d.dp };
    format_digits(&mut dst, shortest, neg, &digs, prec, fmt);
    Some(dst)
}

/// The Go `%v`/String() rendering of a float: FormatFloat(f, 'f', -1, 64).
pub fn float_string(f: f64) -> Vec<u8> {
    format_float(f, b'f', -1, 64).unwrap()
}

// ---------------------------------------------------------------------------------------------
// parsing

fn lower(c: u8) -> u8 {
    c | (b'x' - b'X')
}

fn common_prefix_len_ignore_case(s: &[u8], prefix: &[u8]) -> usize {
    let n = prefix.len().min(s.len());
    for i in 0..n {
        let mut c = s[i];
        if c.is_ascii_uppercase() {
            c += b'a' - b'A';
        }
        if c != prefix[i] {
            return i;
        }
    }
    n
}

fn special(s: &[u8]) -> Option<(f64, usize)> {
    if s.is_empty() {
        return None;
    }
    let mut sign = 1.0;
    let mut nsign = 0;
    let mut t = s;
    match t[0] {
        b'+' | b'-' => {
            if t[0] == b'-' {
                sign = -1.0;
            }
            nsign = 1;
            t = &t[1..];
            let mut n = common_prefix_len_ignore_case(t, b"infinity");
            if 3 < n && n < 8 {
                n = 3;
            }
            if n == 3 || n == 8 {
                return Some((sign * f64::INFINITY, nsign + n));
            }
        }
        b'i' | b'I' => {
            let mut n = common_prefix_len_ignore_case(t, b"infinity");
            if 3 < n && n < 8 {
                n = 3;
            }
            if n == 3 || n == 8 {
                return Some((f64::INFINITY, n));
            }
        }
        b'n' | b'N' => {
            if common_prefix_len_ignore_case(t, b"nan") == 3 {
                return Some((f64::NAN, 3));
            }
        }
        _ => {}
    }
    let _ = nsign;
    None
}

struct ReadFloat {
    mantissa: u64,
    exp: i32,
    neg: bool,
    trunc: bool,
    hex: bool,
    i: usize,
    ok: bool,
}

fn read_float(s: &[u8]) -> ReadFloat {
    let mut r = ReadFloat { mantissa: 0, exp: 0, neg: false, trunc: false, hex: false, i: 0, ok: false };
    let mut underscores = false;
    let mut i = 0usize;
    if i >= s.len() {
        return r;
    }
    if s[i] == b'+' {
        i += 1;
    } else if s[i] == b'-' {
        i += 1;
        r.neg = true;
    }
    let mut base: u64 = 10;
    let mut max_mant_digits = 19;
    let mut exp_char = b'e';
    if i + 2 < s.len() && s[i] == b'0' && lower(s[i + 1]) == b'x' {
        base = 16;
        max_mant_digits = 16;
        i += 2;
        exp_char = b'p';
        r.hex = true;
    }
    let mut sawdot = false;
    let mut sawdigits = false;
    let mut nd: i32 = 0;
    let mut nd_mant: i32 = 0;
    let mut dp: i32 = 0;
    while i < s.len() {
        let c = s[i];
        if c == b'_' {
            underscores = true;
            i += 1;
            continue;
        } else if c == b'.' {
            if sawdot {
                break;
            }
            sawdot = true;
            dp = nd;
            i += 1;
            continue;
        } else if c.is_ascii_digit() {
            sawdigits = true;
            if c == b'0' && nd == 0 {
                dp -= 1;
                i += 1;
                continue;
            }
            nd += 1;
            if nd_mant < max_mant_digits {
                r.mantissa = r.mantissa * base + (c - b'0') as u64;
                nd_mant += 1;
            } else if c != b'0' {
                r.trunc = true;
            }
            i += 1;
            continue;
        } else if base == 16 && (b'a'..=b'f').contains(&lower(c)) {
            sawdigits = true;
            nd += 1;
            if nd_mant < max_mant_digits {
                r.mantissa = r.mantissa * 16 + (lower(c) - b'a' + 10) as u64;
                nd_mant += 1;
            } else {
                r.trunc = true;
            }
            i += 1;
            continue;
        }
        break;
    }
    r.i = i;
    if !sawdigits {
        return r;
    }
    if !sawdot {
        dp = nd;
    }
    if base == 16 {
        dp *= 4;
        nd_mant *= 4;
    }
    if i < s.len() && lower(s[i]) == exp_char {
        i += 1;
        r.i = i;
        if i >= s.len() {
            return r;
        }
        let mut esign = 1;
        if s[i] == b'+' {
            i += 1;
        } else if s[i] == b'-' {
            i += 1;
            esign = -1;
        }
        r.i = i;
        if i >= s.len() || !s[i].is_ascii_digit() {
            return r;
        }
        let mut e: i32 = 0;
        while i < s.len() && (s[i].is_ascii_digit() || s[i] == b'_') {
            if s[i] == b'_' {
                underscores = true;
                i += 1;
                continue;
            }
            if e < 10000 {
                e = e * 10 + (s[i] - b'0') as i32;
            }
            i += 1;
        }
        dp += e * esign;
    } else if base == 16 {
        r.i = i;
        return r;
    }
    r.i = i;
    if r.mantissa != 0 {
        r.exp = dp - nd_mant;
    }
    if underscores && !underscore_ok(&s[..i]) {
        return r;
    }
    r.ok = true;
    r
}

fn atof_hex(flt: &FloatInfo, mut mantissa: u64, mut exp: i32, neg: bool, trunc: bool) -> (f64, bool) {
    let max_exp = (1i32 << flt.expbits) + flt.bias - 2;
    let min_exp = flt.bias + 1;
    exp += flt.mantbits as i32;
    while mantissa != 0 && mantissa >> (flt.mantbits + 2) == 0 {
        mantissa <<= 1;
        exp -= 1;
    }
    if trunc {
        mantissa |= 1;
    }
    while mantissa >> (1 + flt.mantbits + 2) != 0 {
        mantissa = (mantissa >> 1) | (mantissa & 1);
        exp += 1;
    }
    while mantissa > 1 && exp < min_exp - 2 {
        mantissa = (mantissa >> 1) | (mantissa & 1);
        exp += 1;
    }
    let mut round = mantissa & 3;
    mantissa >>= 2;
    round |= mantissa & 1;
    exp += 2;
    if round == 3 {
        mantissa += 1;
        if mantissa == 1u64 << (1 + flt.mantbits) {
            mantissa >>= 1;
            exp += 1;
        }
    }
    if mantissa >> flt.mantbits == 0 {
        exp = flt.bias;
    }
    let mut overflow = false;
    if exp > max_exp {
        mantissa = 1u64 << flt.mantbits;
        exp = max_exp + 1;
        overflow = true;
    }
    let mut bits = mantissa & ((1u64 << flt.mantbits) - 1);
    bits |= (((exp - flt.bias) as u64) & ((1u64 << flt.expbits) - 1)) << flt.mantbits;
    if neg {
        bits |= 1u64 << flt.mantbits << flt.expbits;
    }
    if flt.mantbits == 23 {
        (f32::from_bits(bits as u32) as f64, overflow)
    } else {
        (f64::from_bits(bits), overflow)
    }
}

/// strconv.ParseFloat(s, bitSize): Ok(f) or Err((f, error)) with Go's error value.
pub fn parse_float(s: &[u8], bit_size: i64) -> Result<f64, (f64, NumError)> {
    let syntax = |s: &[u8]| NumError { func: "ParseFloat", num: s.to_vec(), err: NumErr::Syntax };
    let range = |s: &[u8]| NumError { func: "ParseFloat", num: s.to_vec(), err: NumErr::Range };
    let is32 = bit_size == 32;
    let (val, n, err): (f64, usize, Option<NumError>) = if let Some((v, n)) = special(s) {
        (v, n, None)
    } else {
        let r = read_float(s);
        if !r.ok {
            (0.0, r.i, Some(syntax(s)))
        } else if r.hex {
            let flt = if is32 { FLOAT32 } else { FLOAT64 };
            let (f, ovf) = atof_hex(&flt, r.mantissa, r.exp, r.neg, r.trunc);
            (f, r.i, if ovf { Some(range(&s[..r.i])) } else { None })
        } else {
            // A decimal literal: Rust's parser is correctly rounded, as Go's is.
            let lit: String = s[..r.i].iter().filter(|&&c| c != b'_').map(|&c| c as char).collect();
            if is32 {
                let f: f32 = lit.parse().unwrap_or(0.0);
                let ovf = f.is_infinite();
                (f as f64, r.i, if ovf { Some(range(&s[..r.i])) } else { None })
            } else {
                let f: f64 = lit.parse().unwrap_or(0.0);
                let ovf = f.is_infinite();
                (f, r.i, if ovf { Some(range(&s[..r.i])) } else { None })
            }
        }
    };
    if n != s.len() && (err.is_none() || err.as_ref().map(|e| e.err != NumErr::Syntax).unwrap_or(false)) {
        return Err((0.0, syntax(s)));
    }
    match err {
        None => Ok(val),
        Some(mut e) => {
            e.num = s.to_vec();
            Err((val, e))
        }
    }
}
