//! Go's `regexp` package (Go 1.25: `regexp.Compile`, i.e. `regexp/syntax.Parse` with
//! `syntax.Perl` flags) for the text module.
//!
//! This is a port rather than a translation onto another engine:
//! - `regexp/syntax/parse.go`: the parser with Go's exact error texts, its size / height /
//!   repeat / rune limits (including the free-list node reuse those limits are computed
//!   over), alternation factoring, case folding and the `\p{..}` lookups of Go 1.25
//!   (canonicalised names, Any/Assigned/ASCII/LC, category aliases);
//! - `simplify.go`, `compile.go` and `prog.go`: the program Go executes;
//! - `exec.go`: the Pike-VM NFA (leftmost-first, Perl-style submatches, ASCII `\b`);
//! - `regexp.go`: `allMatches`, `pad`, `Split`, `expand`/`extract`.
//!
//! Running Go's own automaton keeps every observable detail identical to Go, including
//! haystacks with invalid UTF-8 (each bad byte is U+FFFD of width 1, as `utf8.DecodeRune`
//! does). Go's backtracker and one-pass engines are not ported: they are optimisations that
//! return the same results as the NFA. The Unicode data (the `\p{..}` tables and
//! `unicode.SimpleFold`) is generated from Go's unicode package (Unicode 15.0.0) by
//! `work/regex/unigen` and sits at the end of the file.

use std::collections::HashMap;
use std::sync::OnceLock;

const MAX_RUNE: i32 = 0x10FFFF;
const RUNE_ERROR: i32 = 0xFFFD;
const MIN_FOLD: i32 = 0x0041;
const MAX_FOLD: i32 = 0x1e943;

// syntax.Flags. Compile always parses with syntax.Perl; the pattern can only toggle i, m, s
// and U, so ClassNL, PerlX and UnicodeGroups are constant and their branches are omitted.
const FOLD_CASE: u16 = 1 << 0;
const CLASS_NL: u16 = 1 << 2;
const DOT_NL: u16 = 1 << 3;
const ONE_LINE: u16 = 1 << 4;
const NON_GREEDY: u16 = 1 << 5;
const PERL_X: u16 = 1 << 6;
const UNICODE_GROUPS: u16 = 1 << 7;
const WAS_DOLLAR: u16 = 1 << 8;
const PERL: u16 = CLASS_NL | ONE_LINE | PERL_X | UNICODE_GROUPS;

// syntax.Op, in Go's order (the parser compares ops numerically).
const OP_NO_MATCH: u8 = 1;
const OP_EMPTY_MATCH: u8 = 2;
const OP_LITERAL: u8 = 3;
const OP_CHAR_CLASS: u8 = 4;
const OP_ANY_CHAR_NOT_NL: u8 = 5;
const OP_ANY_CHAR: u8 = 6;
const OP_BEGIN_LINE: u8 = 7;
const OP_END_LINE: u8 = 8;
const OP_BEGIN_TEXT: u8 = 9;
const OP_END_TEXT: u8 = 10;
const OP_WORD_BOUNDARY: u8 = 11;
const OP_NO_WORD_BOUNDARY: u8 = 12;
const OP_CAPTURE: u8 = 13;
const OP_STAR: u8 = 14;
const OP_PLUS: u8 = 15;
const OP_QUEST: u8 = 16;
const OP_REPEAT: u8 = 17;
const OP_CONCAT: u8 = 18;
const OP_ALTERNATE: u8 = 19;
const OP_PSEUDO: u8 = 128;
const OP_LEFT_PAREN: u8 = 128;
const OP_VERTICAL_BAR: u8 = 129;

const ERR_INVALID_CHAR_RANGE: &str = "invalid character class range";
const ERR_INVALID_ESCAPE: &str = "invalid escape sequence";
const ERR_INVALID_NAMED_CAPTURE: &str = "invalid named capture";
const ERR_INVALID_PERL_OP: &str = "invalid or unsupported Perl syntax";
const ERR_INVALID_REPEAT_OP: &str = "invalid nested repetition operator";
const ERR_INVALID_REPEAT_SIZE: &str = "invalid repeat count";
const ERR_INVALID_UTF8: &str = "invalid UTF-8";
const ERR_MISSING_BRACKET: &str = "missing closing ]";
const ERR_MISSING_PAREN: &str = "missing closing )";
const ERR_MISSING_REPEAT_ARGUMENT: &str = "missing argument to repetition operator";
const ERR_TRAILING_BACKSLASH: &str = "trailing backslash at end of expression";
const ERR_UNEXPECTED_PAREN: &str = "unexpected )";
const ERR_NESTING_DEPTH: &str = "expression nests too deeply";
const ERR_LARGE: &str = "expression too large";

const MAX_HEIGHT: i64 = 1000;
const MAX_SIZE: i64 = (128 << 20) / 40; // instSize = 5 * 8
const MAX_RUNES: i64 = (128 << 20) / 4;

// ---------------------------------------------------------------------------------------------
// public API

/// A compiled Go regular expression.
pub struct GoRegexp {
    expr_len: usize,
    prog: Prog,
    num_subexp: usize,
    names: Vec<Vec<u8>>,
    cond: u8,
}

/// regexp.Compile(pattern). Err holds Go's err.Error() bytes, e.g.
/// "error parsing regexp: missing closing ): `(ab`".
pub fn regexp_compile(pattern: &[u8]) -> Result<GoRegexp, Vec<u8>> {
    let mut p = Parser {
        n: Vec::new(),
        free: Vec::new(),
        flags: PERL,
        stack: Vec::new(),
        num_cap: 0,
        whole: pattern,
        num_regexp: 0,
        num_runes: 0,
        repeats: 0,
        height: None,
        size: None,
    };
    let root = match p.parse() {
        Ok(r) => r,
        Err(e) => {
            let (code, expr) = match e {
                PErr::Syntax(c, x) => (c, x),
                PErr::Large => (ERR_LARGE, pattern.to_vec()),
                PErr::Nest => (ERR_NESTING_DEPTH, pattern.to_vec()),
            };
            let mut m = b"error parsing regexp: ".to_vec();
            m.extend_from_slice(code.as_bytes());
            m.extend_from_slice(b": `");
            m.extend_from_slice(&expr);
            m.push(b'`');
            return Err(m);
        }
    };
    let mut nodes = p.n;
    let num_subexp = max_cap(&nodes, root) as usize;
    let mut names = vec![Vec::new(); num_subexp + 1];
    cap_names(&nodes, root, &mut names);
    let sroot = simplify(&mut nodes, root);
    let prog = Compiler::compile_prog(&mut nodes, sroot);
    let cond = prog.start_cond();
    Ok(GoRegexp {
        expr_len: pattern.len(),
        prog,
        num_subexp,
        names,
        cond,
    })
}

impl GoRegexp {
    /// MatchString.
    pub fn is_match(&self, s: &[u8]) -> bool {
        self.exec(s, 0, 0).is_some()
    }

    /// FindStringSubmatchIndex (unmatched groups are -1).
    pub fn find_submatch_index(&self, s: &[u8]) -> Option<Vec<i64>> {
        self.exec(s, 0, self.prog.num_cap).map(|m| self.pad(m))
    }

    /// FindAllStringSubmatchIndex(s, n); n < 0 means all; no match gives an empty Vec.
    pub fn find_all_submatch_index(&self, s: &[u8], n: i64) -> Vec<Vec<i64>> {
        let n = if n < 0 { s.len() as i64 + 1 } else { n };
        let mut out = Vec::new();
        self.all_matches(s, n, |m| out.push(m));
        out
    }

    /// ExpandString(nil, template, src, m).
    pub fn expand(&self, template: &[u8], src: &[u8], m: &[i64]) -> Vec<u8> {
        let mut dst = Vec::new();
        let mut template = template;
        let piece = |dst: &mut Vec<u8>, b: i64, e: i64| {
            if 0 <= b && b <= e && (e as usize) <= src.len() {
                dst.extend_from_slice(&src[b as usize..e as usize]);
            }
        };
        while let Some(i) = template.iter().position(|&b| b == b'$') {
            dst.extend_from_slice(&template[..i]);
            template = &template[i + 1..];
            if !template.is_empty() && template[0] == b'$' {
                // $$ is a literal $.
                dst.push(b'$');
                template = &template[1..];
                continue;
            }
            let Some((name, num, rest)) = extract(template) else {
                // Malformed; the $ is raw text.
                dst.push(b'$');
                continue;
            };
            template = rest;
            if num >= 0 {
                let k = 2 * num as usize;
                if k + 1 < m.len() && m[k] >= 0 {
                    piece(&mut dst, m[k], m[k + 1]);
                }
            } else {
                for (i, namei) in self.names.iter().enumerate() {
                    if name == namei.as_slice() && 2 * i + 1 < m.len() && m[2 * i] >= 0 {
                        piece(&mut dst, m[2 * i], m[2 * i + 1]);
                        break;
                    }
                }
            }
        }
        dst.extend_from_slice(template);
        dst
    }

    /// Split(s, n).
    pub fn split(&self, s: &[u8], n: i64) -> Vec<Vec<u8>> {
        if n == 0 {
            return Vec::new();
        }
        if self.expr_len > 0 && s.is_empty() {
            return vec![Vec::new()];
        }
        // FindAllStringIndex(s, n)
        let limit = if n < 0 { s.len() as i64 + 1 } else { n };
        let mut matches = Vec::new();
        self.all_matches(s, limit, |m| matches.push((m[0] as usize, m[1] as usize)));
        let mut out: Vec<Vec<u8>> = Vec::new();
        let (mut beg, mut end) = (0usize, 0usize);
        for &(m0, m1) in &matches {
            if n > 0 && out.len() as i64 >= n - 1 {
                break;
            }
            end = m0;
            if m1 != 0 {
                out.push(s[beg..end].to_vec());
            }
            beg = m1;
        }
        if end != s.len() {
            out.push(s[beg..].to_vec());
        }
        out
    }

    /// Regexp.pad: the program may have fewer capture slots than the regexp has groups
    /// (`(a){0}` compiles to nothing), so unreported groups are -1.
    fn pad(&self, mut a: Vec<i64>) -> Vec<i64> {
        let n = (1 + self.num_subexp) * 2;
        while a.len() < n {
            a.push(-1);
        }
        a
    }

    /// Regexp.allMatches: successive matches, skipping an empty match right after the
    /// previous match and stepping one character past every empty match.
    fn all_matches(&self, s: &[u8], n: i64, mut deliver: impl FnMut(Vec<i64>)) {
        let end = s.len();
        let (mut pos, mut i, mut prev_match_end) = (0usize, 0i64, -1i64);
        while i < n && pos <= end {
            let Some(m) = self.exec(s, pos, self.prog.num_cap) else {
                break;
            };
            let mut accept = true;
            if m[1] == pos as i64 {
                // An empty match.
                if m[0] == prev_match_end {
                    accept = false;
                }
                let (_, width) = step(s, pos);
                if width > 0 {
                    pos += width;
                } else {
                    pos = end + 1;
                }
            } else {
                pos = m[1] as usize;
            }
            prev_match_end = m[1];
            if accept {
                deliver(self.pad(m));
                i += 1;
            }
        }
    }
}

/// regexp.extract: the name in a leading "name" or "{name}" (the $ already removed), its
/// number (or -1) and the rest of the template.
fn extract(s: &[u8]) -> Option<(&[u8], i64, &[u8])> {
    if s.is_empty() {
        return None;
    }
    let mut s = s;
    let brace = s[0] == b'{';
    if brace {
        s = &s[1..];
    }
    let mut i = 0;
    while i < s.len() {
        let (r, size) = decode_rune(&s[i..]);
        if !is_letter(r) && !is_digit(r) && r != '_' as i32 {
            break;
        }
        i += size;
    }
    if i == 0 {
        return None; // empty name is not okay
    }
    let name = &s[..i];
    if brace {
        if i >= s.len() || s[i] != b'}' {
            return None; // missing closing brace
        }
        i += 1;
    }
    let mut num: i64 = 0;
    for &b in name {
        if !b.is_ascii_digit() || num >= 100_000_000 {
            num = -1;
            break;
        }
        num = num * 10 + (b - b'0') as i64;
    }
    if name[0] == b'0' && name.len() > 1 {
        num = -1; // leading zeros
    }
    Some((name, num, &s[i..]))
}

// ---------------------------------------------------------------------------------------------
// UTF-8 as package unicode/utf8 decodes it

/// utf8.DecodeRune: (RuneError, 1) for any invalid or truncated sequence.
fn decode_rune(s: &[u8]) -> (i32, usize) {
    let b0 = s[0];
    if b0 < 0x80 {
        return (b0 as i32, 1);
    }
    let n = match b0 {
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return (RUNE_ERROR, 1),
    };
    if s.len() < n {
        return (RUNE_ERROR, 1);
    }
    match std::str::from_utf8(&s[..n]) {
        Ok(t) => (t.chars().next().unwrap() as i32, n),
        Err(_) => (RUNE_ERROR, 1),
    }
}

/// inputString.step: the rune at pos and its width; endOfText (-1, 0) at the end.
fn step(s: &[u8], pos: usize) -> (i32, usize) {
    if pos < s.len() {
        decode_rune(&s[pos..])
    } else {
        (-1, 0)
    }
}

// ---------------------------------------------------------------------------------------------
// parser (regexp/syntax/parse.go)

#[derive(Clone, Default)]
struct Node {
    op: u8,
    flags: u16,
    sub: Vec<usize>,
    runes: Vec<i32>,
    min: i32,
    max: i32,
    cap: i32,
    name: Vec<u8>,
}

enum PErr {
    Syntax(&'static str, Vec<u8>),
    Large,
    Nest,
}

type PRes<T> = Result<T, PErr>;

fn err<T>(code: &'static str, expr: &[u8]) -> PRes<T> {
    Err(PErr::Syntax(code, expr.to_vec()))
}

/// The parser state. Nodes live in an arena; `free` mirrors Go's free list because the
/// size and height limits are computed over node identities and allocation counts.
struct Parser<'a> {
    n: Vec<Node>,
    free: Vec<usize>,
    flags: u16,
    stack: Vec<usize>,
    num_cap: i32,
    whole: &'a [u8],
    num_regexp: i64,
    num_runes: i64,
    repeats: i64,
    height: Option<HashMap<usize, i64>>,
    size: Option<HashMap<usize, i64>>,
}

fn find(s: &[u8], pat: &[u8]) -> Option<usize> {
    s.windows(pat.len()).position(|w| w == pat)
}

/// nextRune: the first rune of s; an empty s yields RuneError without error, like Go.
fn next_rune(s: &[u8]) -> PRes<(i32, &[u8])> {
    if s.is_empty() {
        return Ok((RUNE_ERROR, s));
    }
    let (c, size) = decode_rune(s);
    if c == RUNE_ERROR && size == 1 {
        return err(ERR_INVALID_UTF8, s);
    }
    Ok((c, &s[size..]))
}

fn check_utf8(mut s: &[u8]) -> PRes<()> {
    while !s.is_empty() {
        let (c, size) = decode_rune(s);
        if c == RUNE_ERROR && size == 1 {
            return err(ERR_INVALID_UTF8, s);
        }
        s = &s[size..];
    }
    Ok(())
}

fn isalnum(c: i32) -> bool {
    (c as u32) < 0x80 && (c as u8).is_ascii_alphanumeric()
}

fn unhex(c: i32) -> i32 {
    match c {
        0x30..=0x39 => c - 0x30,
        0x61..=0x66 => c - 0x61 + 10,
        0x41..=0x46 => c - 0x41 + 10,
        _ => -1,
    }
}

impl<'a> Parser<'a> {
    fn new_regexp(&mut self, op: u8) -> usize {
        let i = match self.free.pop() {
            Some(i) => {
                self.n[i] = Node::default();
                i
            }
            None => {
                self.n.push(Node::default());
                self.num_regexp += 1;
                self.n.len() - 1
            }
        };
        self.n[i].op = op;
        i
    }

    fn reuse(&mut self, re: usize) {
        if let Some(h) = self.height.as_mut() {
            h.remove(&re);
        }
        self.free.push(re);
    }

    fn check_limits(&mut self, re: usize) -> PRes<()> {
        if self.num_runes > MAX_RUNES {
            return Err(PErr::Large);
        }
        self.check_size(re)?;
        self.check_height(re)
    }

    fn check_size(&mut self, re: usize) -> PRes<()> {
        if self.size.is_none() {
            // Cheap check first: track sizes only once nodes x repeats could exceed the budget.
            if self.repeats == 0 {
                self.repeats = 1;
            }
            if self.n[re].op == OP_REPEAT {
                let mut n = self.n[re].max;
                if n == -1 {
                    n = self.n[re].min;
                }
                if n <= 0 {
                    n = 1;
                }
                if n as i64 > MAX_SIZE / self.repeats {
                    self.repeats = MAX_SIZE;
                } else {
                    self.repeats *= n as i64;
                }
            }
            if self.num_regexp < MAX_SIZE / self.repeats {
                return Ok(());
            }
            self.size = Some(HashMap::new());
            for k in 0..self.stack.len() {
                let s = self.stack[k];
                self.check_size(s)?;
            }
        }
        if self.calc_size(re, true) > MAX_SIZE {
            return Err(PErr::Large);
        }
        Ok(())
    }

    fn calc_size(&mut self, re: usize, force: bool) -> i64 {
        if !force {
            if let Some(&s) = self.size.as_ref().unwrap().get(&re) {
                return s;
            }
        }
        let node = &self.n[re];
        let (op, min, max) = (node.op, node.min as i64, node.max as i64);
        let subs = node.sub.clone();
        let mut size: i64 = 0;
        match op {
            OP_LITERAL => size = node.runes.len() as i64,
            OP_CAPTURE | OP_STAR => size = 2 + self.calc_size(subs[0], false),
            OP_PLUS | OP_QUEST => size = 1 + self.calc_size(subs[0], false),
            OP_CONCAT | OP_ALTERNATE => {
                for &s in &subs {
                    size += self.calc_size(s, false);
                }
                if op == OP_ALTERNATE && subs.len() > 1 {
                    size += subs.len() as i64 - 1;
                }
            }
            OP_REPEAT => {
                let sub = self.calc_size(subs[0], false);
                size = if max == -1 {
                    if min == 0 {
                        2 + sub
                    } else {
                        1 + min * sub
                    }
                } else {
                    max * sub + (max - min)
                };
            }
            _ => {}
        }
        let size = size.max(1);
        self.size.as_mut().unwrap().insert(re, size);
        size
    }

    fn check_height(&mut self, re: usize) -> PRes<()> {
        if self.num_regexp < MAX_HEIGHT {
            return Ok(());
        }
        if self.height.is_none() {
            self.height = Some(HashMap::new());
            for k in 0..self.stack.len() {
                let s = self.stack[k];
                self.check_height(s)?;
            }
        }
        if self.calc_height(re, true) > MAX_HEIGHT {
            return Err(PErr::Nest);
        }
        Ok(())
    }

    fn calc_height(&mut self, re: usize, force: bool) -> i64 {
        if !force {
            if let Some(&h) = self.height.as_ref().unwrap().get(&re) {
                return h;
            }
        }
        let mut h = 1;
        for s in self.n[re].sub.clone() {
            h = h.max(1 + self.calc_height(s, false));
        }
        self.height.as_mut().unwrap().insert(re, h);
        h
    }

    /// push pushes re onto the parse stack (single runes and [Aa]-style folds become
    /// literals and are concatenated). None when the rune was merged into a literal.
    fn push(&mut self, re: usize) -> PRes<Option<usize>> {
        self.num_runes += self.n[re].runes.len() as i64;
        let node = &self.n[re];
        let r = &node.runes;
        if node.op == OP_CHAR_CLASS && r.len() == 2 && r[0] == r[1] {
            let r0 = r[0];
            if self.maybe_concat(r0, self.flags & !FOLD_CASE) {
                return Ok(None);
            }
            let node = &mut self.n[re];
            node.op = OP_LITERAL;
            node.runes.truncate(1);
            node.flags = self.flags & !FOLD_CASE;
        } else if node.op == OP_CHAR_CLASS
            && ((r.len() == 4
                && r[0] == r[1]
                && r[2] == r[3]
                && simple_fold(r[0]) == r[2]
                && simple_fold(r[2]) == r[0])
                || (r.len() == 2
                    && r[0] + 1 == r[1]
                    && simple_fold(r[0]) == r[1]
                    && simple_fold(r[1]) == r[0]))
        {
            // Case-insensitive rune like [Aa] or [Δδ].
            let r0 = r[0];
            if self.maybe_concat(r0, self.flags | FOLD_CASE) {
                return Ok(None);
            }
            let node = &mut self.n[re];
            node.op = OP_LITERAL;
            node.runes.truncate(1);
            node.flags = self.flags | FOLD_CASE;
        } else {
            self.maybe_concat(-1, 0);
        }
        self.stack.push(re);
        self.check_limits(re)?;
        Ok(Some(re))
    }

    /// maybeConcat merges the top two literals; with r >= 0 the freed top node is reused
    /// for r. Reports whether r was pushed.
    fn maybe_concat(&mut self, r: i32, flags: u16) -> bool {
        let n = self.stack.len();
        if n < 2 {
            return false;
        }
        let (re1, re2) = (self.stack[n - 1], self.stack[n - 2]);
        if self.n[re1].op != OP_LITERAL
            || self.n[re2].op != OP_LITERAL
            || self.n[re1].flags & FOLD_CASE != self.n[re2].flags & FOLD_CASE
        {
            return false;
        }
        let rs = std::mem::take(&mut self.n[re1].runes);
        self.n[re2].runes.extend_from_slice(&rs);
        if r >= 0 {
            self.n[re1].runes = vec![r];
            self.n[re1].flags = flags;
            return true;
        }
        self.stack.pop();
        self.reuse(re1);
        false
    }

    fn literal(&mut self, r: i32) -> PRes<()> {
        let re = self.new_regexp(OP_LITERAL);
        self.n[re].flags = self.flags;
        let r = if self.flags & FOLD_CASE != 0 {
            min_fold_rune(r)
        } else {
            r
        };
        self.n[re].runes = vec![r];
        self.push(re)?;
        Ok(())
    }

    /// op pushes a node with the given op (never a char class, so push keeps it).
    fn op(&mut self, op: u8) -> PRes<usize> {
        let re = self.new_regexp(op);
        self.n[re].flags = self.flags;
        Ok(self.push(re)?.unwrap_or(re))
    }

    /// repeat replaces the stack top with itself repeated; before starts at the operator,
    /// after follows it. Returns the updated after.
    fn repeat(
        &mut self,
        op: u8,
        min: i32,
        max: i32,
        before: &'a [u8],
        after: &'a [u8],
        last_repeat: Option<&'a [u8]>,
    ) -> PRes<&'a [u8]> {
        let mut after = after;
        let mut flags = self.flags;
        if !after.is_empty() && after[0] == b'?' {
            after = &after[1..];
            flags ^= NON_GREEDY;
        }
        if let Some(lr) = last_repeat {
            // a** is a syntax error, not a doubled star.
            return err(ERR_INVALID_REPEAT_OP, &lr[..lr.len() - after.len()]);
        }
        let n = self.stack.len();
        let expr = &before[..before.len() - after.len()];
        if n == 0 || self.n[self.stack[n - 1]].op >= OP_PSEUDO {
            return err(ERR_MISSING_REPEAT_ARGUMENT, expr);
        }
        let sub = self.stack[n - 1];
        let re = self.new_regexp(op);
        let node = &mut self.n[re];
        node.min = min;
        node.max = max;
        node.flags = flags;
        node.sub = vec![sub];
        self.stack[n - 1] = re;
        self.check_limits(re)?;
        if op == OP_REPEAT && (min >= 2 || max >= 2) && !self.repeat_is_valid(re, 1000) {
            return err(ERR_INVALID_REPEAT_SIZE, expr);
        }
        Ok(after)
    }

    /// repeatIsValid: nested repetitions may not exceed n copies of the innermost thing.
    fn repeat_is_valid(&self, re: usize, mut n: i32) -> bool {
        let node = &self.n[re];
        if node.op == OP_REPEAT {
            let mut m = node.max;
            if m == 0 {
                return true;
            }
            if m < 0 {
                m = node.min;
            }
            if m > n {
                return false;
            }
            if m > 0 {
                n /= m;
            }
        }
        node.sub.iter().all(|&s| self.repeat_is_valid(s, n))
    }

    /// The start of the run of real (non-pseudo) operators at the top of the stack.
    fn run_start(&self) -> usize {
        let mut i = self.stack.len();
        while i > 0 && self.n[self.stack[i - 1]].op < OP_PSEUDO {
            i -= 1;
        }
        i
    }

    /// concat replaces the top of the stack (above the topmost '|' or '(') with its
    /// concatenation.
    fn concat(&mut self) -> PRes<()> {
        self.maybe_concat(-1, 0);
        let i = self.run_start();
        let subs = self.stack.split_off(i);
        let re = if subs.is_empty() {
            self.new_regexp(OP_EMPTY_MATCH)
        } else {
            self.collapse(&subs, OP_CONCAT)?
        };
        self.push(re)?;
        Ok(())
    }

    /// alternate replaces the top of the stack (above the topmost '(') with its alternation.
    fn alternate(&mut self) -> PRes<()> {
        let i = self.run_start();
        let subs = self.stack.split_off(i);
        if let Some(&last) = subs.last() {
            self.clean_alt(last);
        }
        let re = if subs.is_empty() {
            self.new_regexp(OP_NO_MATCH)
        } else {
            self.collapse(&subs, OP_ALTERNATE)?
        };
        self.push(re)?;
        Ok(())
    }

    fn clean_alt(&mut self, re: usize) {
        let node = &mut self.n[re];
        if node.op == OP_CHAR_CLASS {
            clean_class(&mut node.runes);
            let r = &node.runes;
            if r.len() == 2 && r[0] == 0 && r[1] == MAX_RUNE {
                node.runes = Vec::new();
                node.op = OP_ANY_CHAR;
            } else if r.len() == 4 && r[0] == 0 && r[1] == 9 && r[2] == 11 && r[3] == MAX_RUNE {
                node.runes = Vec::new();
                node.op = OP_ANY_CHAR_NOT_NL;
            }
        }
    }

    /// collapse applies op to subs, hoisting nested ops of the same kind; alternations are
    /// factored.
    fn collapse(&mut self, subs: &[usize], op: u8) -> PRes<usize> {
        if subs.len() == 1 {
            return Ok(subs[0]);
        }
        let mut re = self.new_regexp(op);
        let mut list = Vec::new();
        for &sub in subs {
            if self.n[sub].op == op {
                list.extend_from_slice(&self.n[sub].sub.clone());
                self.reuse(sub);
            } else {
                list.push(sub);
            }
        }
        if op == OP_ALTERNATE {
            list = self.factor(list)?;
            if list.len() == 1 {
                let old = re;
                re = list[0];
                self.reuse(old);
                return Ok(re);
            }
        }
        self.n[re].sub = list;
        Ok(re)
    }

    /// factor factors common prefixes out of an alternation list.
    fn factor(&mut self, mut sub: Vec<usize>) -> PRes<Vec<usize>> {
        if sub.len() < 2 {
            return Ok(sub);
        }

        // Round 1: factor out common literal prefixes.
        let mut str: Vec<i32> = Vec::new();
        let mut strflags: u16 = 0;
        let mut start = 0;
        let mut out = Vec::new();
        let mut i = 0;
        while i <= sub.len() {
            let mut istr = Vec::new();
            let mut iflags = 0;
            if i < sub.len() {
                let (s, f) = self.leading_string(sub[i]);
                istr = s;
                iflags = f;
                if iflags == strflags {
                    let mut same = 0;
                    while same < str.len() && same < istr.len() && str[same] == istr[same] {
                        same += 1;
                    }
                    if same > 0 {
                        // Matches at least one rune in current range.
                        str.truncate(same);
                        i += 1;
                        continue;
                    }
                }
            }
            // sub[start..i] all begin with str; sub[i] does not.
            if i == start + 1 {
                out.push(sub[start]);
            } else if i > start + 1 {
                let prefix = self.new_regexp(OP_LITERAL);
                self.n[prefix].flags = strflags;
                self.n[prefix].runes = str.clone();
                for s in &mut sub[start..i] {
                    *s = self.remove_leading_string(*s, str.len());
                    self.check_limits(*s)?;
                }
                let suffix = self.collapse(&sub[start..i], OP_ALTERNATE)?;
                let re = self.new_regexp(OP_CONCAT);
                self.n[re].sub = vec![prefix, suffix];
                out.push(re);
            }
            start = i;
            str = istr;
            strflags = iflags;
            i += 1;
        }
        sub = out;

        // Round 2: factor out common simple prefixes (the first piece of each
        // concatenation) when it is a char class or a fixed repeat of one.
        let mut start = 0;
        let mut out = Vec::new();
        let mut first: Option<usize> = None;
        let mut i = 0;
        while i <= sub.len() {
            let mut ifirst = None;
            if i < sub.len() {
                ifirst = self.leading_regexp(sub[i]);
                if let (Some(f), Some(fi)) = (first, ifirst) {
                    let fnode = &self.n[f];
                    if self.equal(f, fi)
                        && (self.is_char_class(f)
                            || (fnode.op == OP_REPEAT
                                && fnode.min == fnode.max
                                && self.is_char_class(fnode.sub[0])))
                    {
                        i += 1;
                        continue;
                    }
                }
            }
            if i == start + 1 {
                out.push(sub[start]);
            } else if i > start + 1 {
                let prefix = first.unwrap();
                for (k, s) in sub[start..i].iter_mut().enumerate() {
                    let reuse = k != 0; // prefix came from sub[start]
                    *s = self.remove_leading_regexp(*s, reuse);
                    self.check_limits(*s)?;
                }
                let suffix = self.collapse(&sub[start..i], OP_ALTERNATE)?;
                let re = self.new_regexp(OP_CONCAT);
                self.n[re].sub = vec![prefix, suffix];
                out.push(re);
            }
            start = i;
            first = ifirst;
            i += 1;
        }
        sub = out;

        // Round 3: collapse runs of single literals into character classes.
        let mut start = 0;
        let mut out = Vec::new();
        let mut i = 0;
        while i <= sub.len() {
            if i < sub.len() && self.is_char_class(sub[i]) {
                i += 1;
                continue;
            }
            if i == start + 1 {
                out.push(sub[start]);
            } else if i > start + 1 {
                // Start with the most complex regexp in sub[start..i].
                let mut max = start;
                for j in start + 1..i {
                    let (a, b) = (&self.n[sub[max]], &self.n[sub[j]]);
                    if a.op < b.op || (a.op == b.op && a.runes.len() < b.runes.len()) {
                        max = j;
                    }
                }
                sub.swap(start, max);
                for j in start + 1..i {
                    self.merge_char_class(sub[start], sub[j]);
                    self.reuse(sub[j]);
                }
                self.clean_alt(sub[start]);
                out.push(sub[start]);
            }
            if i < sub.len() {
                out.push(sub[i]);
            }
            start = i + 1;
            i += 1;
        }
        sub = out;

        // Round 4: collapse runs of empty matches into a single empty match.
        let mut out = Vec::new();
        for i in 0..sub.len() {
            if i + 1 < sub.len()
                && self.n[sub[i]].op == OP_EMPTY_MATCH
                && self.n[sub[i + 1]].op == OP_EMPTY_MATCH
            {
                continue;
            }
            out.push(sub[i]);
        }
        Ok(out)
    }

    fn leading_string(&self, re: usize) -> (Vec<i32>, u16) {
        let mut re = re;
        if self.n[re].op == OP_CONCAT && !self.n[re].sub.is_empty() {
            re = self.n[re].sub[0];
        }
        if self.n[re].op != OP_LITERAL {
            return (Vec::new(), 0);
        }
        (self.n[re].runes.clone(), self.n[re].flags & FOLD_CASE)
    }

    fn remove_leading_string(&mut self, re: usize, n: usize) -> usize {
        if self.n[re].op == OP_CONCAT && !self.n[re].sub.is_empty() {
            let sub = self.remove_leading_string(self.n[re].sub[0], n);
            self.n[re].sub[0] = sub;
            if self.n[sub].op == OP_EMPTY_MATCH {
                self.reuse(sub);
                match self.n[re].sub.len() {
                    0 | 1 => {
                        self.n[re].op = OP_EMPTY_MATCH;
                        self.n[re].sub = Vec::new();
                    }
                    2 => {
                        let next = self.n[re].sub[1];
                        self.reuse(re);
                        return next;
                    }
                    _ => {
                        self.n[re].sub.remove(0);
                    }
                }
            }
            return re;
        }
        if self.n[re].op == OP_LITERAL {
            let node = &mut self.n[re];
            node.runes.drain(..n.min(node.runes.len()));
            if node.runes.is_empty() {
                node.op = OP_EMPTY_MATCH;
            }
        }
        re
    }

    fn leading_regexp(&self, re: usize) -> Option<usize> {
        let node = &self.n[re];
        if node.op == OP_EMPTY_MATCH {
            return None;
        }
        if node.op == OP_CONCAT && !node.sub.is_empty() {
            let sub = node.sub[0];
            if self.n[sub].op == OP_EMPTY_MATCH {
                return None;
            }
            return Some(sub);
        }
        Some(re)
    }

    fn remove_leading_regexp(&mut self, re: usize, reuse: bool) -> usize {
        if self.n[re].op == OP_CONCAT && !self.n[re].sub.is_empty() {
            if reuse {
                self.reuse(self.n[re].sub[0]);
            }
            self.n[re].sub.remove(0);
            match self.n[re].sub.len() {
                0 => {
                    self.n[re].op = OP_EMPTY_MATCH;
                    self.n[re].sub = Vec::new();
                }
                1 => {
                    let only = self.n[re].sub[0];
                    self.reuse(re);
                    return only;
                }
                _ => {}
            }
            return re;
        }
        if reuse {
            self.reuse(re);
        }
        self.new_regexp(OP_EMPTY_MATCH)
    }

    /// Regexp.Equal (note: literals and classes compare runes only, not flags).
    fn equal(&self, x: usize, y: usize) -> bool {
        let (a, b) = (&self.n[x], &self.n[y]);
        if a.op != b.op {
            return false;
        }
        match a.op {
            OP_END_TEXT => a.flags & WAS_DOLLAR == b.flags & WAS_DOLLAR,
            OP_LITERAL | OP_CHAR_CLASS => a.runes == b.runes,
            OP_ALTERNATE | OP_CONCAT => {
                a.sub.len() == b.sub.len()
                    && a.sub.iter().zip(&b.sub).all(|(&p, &q)| self.equal(p, q))
            }
            OP_STAR | OP_PLUS | OP_QUEST => {
                a.flags & NON_GREEDY == b.flags & NON_GREEDY && self.equal(a.sub[0], b.sub[0])
            }
            OP_REPEAT => {
                a.flags & NON_GREEDY == b.flags & NON_GREEDY
                    && a.min == b.min
                    && a.max == b.max
                    && self.equal(a.sub[0], b.sub[0])
            }
            OP_CAPTURE => a.cap == b.cap && a.name == b.name && self.equal(a.sub[0], b.sub[0]),
            _ => true,
        }
    }

    /// Single-rune literal, char class, . or (?s). -- representable as a char class.
    fn is_char_class(&self, re: usize) -> bool {
        let node = &self.n[re];
        (node.op == OP_LITERAL && node.runes.len() == 1)
            || node.op == OP_CHAR_CLASS
            || node.op == OP_ANY_CHAR_NOT_NL
            || node.op == OP_ANY_CHAR
    }

    fn match_rune(&self, re: usize, r: i32) -> bool {
        let node = &self.n[re];
        match node.op {
            OP_LITERAL => node.runes.len() == 1 && node.runes[0] == r,
            OP_CHAR_CLASS => node.runes.chunks(2).any(|p| p[0] <= r && r <= p[1]),
            OP_ANY_CHAR_NOT_NL => r != '\n' as i32,
            OP_ANY_CHAR => true,
            _ => false,
        }
    }

    /// mergeCharClass makes dst = dst|src (dst.op >= src.op).
    fn merge_char_class(&mut self, dst: usize, src: usize) {
        match self.n[dst].op {
            OP_ANY_CHAR_NOT_NL => {
                if self.match_rune(src, '\n' as i32) {
                    self.n[dst].op = OP_ANY_CHAR;
                }
            }
            OP_CHAR_CLASS => {
                let s = self.n[src].clone();
                let d = &mut self.n[dst].runes;
                if s.op == OP_LITERAL {
                    append_literal(d, s.runes[0], s.flags);
                } else {
                    append_class(d, &s.runes);
                }
            }
            OP_LITERAL => {
                let s = self.n[src].clone();
                let d = &mut self.n[dst];
                if s.runes[0] == d.runes[0] && s.flags == d.flags {
                    return;
                }
                d.op = OP_CHAR_CLASS;
                let (d0, dflags) = (d.runes[0], d.flags);
                d.runes.clear();
                append_literal(&mut d.runes, d0, dflags);
                append_literal(&mut d.runes, s.runes[0], s.flags);
            }
            _ => {}
        }
    }

    /// If the stack top is an element above a '|', swap them (merging char classes).
    fn swap_vertical_bar(&mut self) -> bool {
        let n = self.stack.len();
        if n >= 3
            && self.n[self.stack[n - 2]].op == OP_VERTICAL_BAR
            && self.is_char_class(self.stack[n - 1])
            && self.is_char_class(self.stack[n - 3])
        {
            let (mut re1, mut re3) = (self.stack[n - 1], self.stack[n - 3]);
            // Make re3 the more complex of the two.
            if self.n[re1].op > self.n[re3].op {
                std::mem::swap(&mut re1, &mut re3);
                self.stack[n - 3] = re3;
            }
            self.merge_char_class(re3, re1);
            self.reuse(re1);
            self.stack.pop();
            return true;
        }
        if n >= 2 {
            let (re1, re2) = (self.stack[n - 1], self.stack[n - 2]);
            if self.n[re2].op == OP_VERTICAL_BAR {
                if n >= 3 {
                    // Now out of reach; clean opportunistically.
                    self.clean_alt(self.stack[n - 3]);
                }
                self.stack[n - 2] = re1;
                self.stack[n - 1] = re2;
                return true;
            }
        }
        false
    }

    fn parse_vertical_bar(&mut self) -> PRes<()> {
        self.concat()?;
        if !self.swap_vertical_bar() {
            self.op(OP_VERTICAL_BAR)?;
        }
        Ok(())
    }

    fn parse_right_paren(&mut self) -> PRes<()> {
        self.concat()?;
        if self.swap_vertical_bar() {
            self.stack.pop();
        }
        self.alternate()?;
        let n = self.stack.len();
        if n < 2 {
            return err(ERR_UNEXPECTED_PAREN, self.whole);
        }
        let (re1, re2) = (self.stack[n - 1], self.stack[n - 2]);
        self.stack.truncate(n - 2);
        if self.n[re2].op != OP_LEFT_PAREN {
            return err(ERR_UNEXPECTED_PAREN, self.whole);
        }
        // Restore flags at time of paren.
        self.flags = self.n[re2].flags;
        if self.n[re2].cap == 0 {
            self.push(re1)?;
        } else {
            self.n[re2].op = OP_CAPTURE;
            self.n[re2].sub = vec![re1];
            self.push(re2)?;
        }
        Ok(())
    }

    fn parse(&mut self) -> PRes<usize> {
        let s = self.whole;
        let mut t: &'a [u8] = s;
        let mut last_repeat: Option<&'a [u8]> = None;
        while !t.is_empty() {
            let mut repeat = None;
            match t[0] {
                b'(' => {
                    if t.len() >= 2 && t[1] == b'?' {
                        // Flag changes and non-capturing groups.
                        t = self.parse_perl_flags(t)?;
                    } else {
                        self.num_cap += 1;
                        let re = self.op(OP_LEFT_PAREN)?;
                        self.n[re].cap = self.num_cap;
                        t = &t[1..];
                    }
                }
                b'|' => {
                    self.parse_vertical_bar()?;
                    t = &t[1..];
                }
                b')' => {
                    self.parse_right_paren()?;
                    t = &t[1..];
                }
                b'^' => {
                    self.op(if self.flags & ONE_LINE != 0 {
                        OP_BEGIN_TEXT
                    } else {
                        OP_BEGIN_LINE
                    })?;
                    t = &t[1..];
                }
                b'$' => {
                    if self.flags & ONE_LINE != 0 {
                        let re = self.op(OP_END_TEXT)?;
                        self.n[re].flags |= WAS_DOLLAR;
                    } else {
                        self.op(OP_END_LINE)?;
                    }
                    t = &t[1..];
                }
                b'.' => {
                    self.op(if self.flags & DOT_NL != 0 {
                        OP_ANY_CHAR
                    } else {
                        OP_ANY_CHAR_NOT_NL
                    })?;
                    t = &t[1..];
                }
                b'[' => t = self.parse_class(t)?,
                b'*' | b'+' | b'?' => {
                    let op = match t[0] {
                        b'*' => OP_STAR,
                        b'+' => OP_PLUS,
                        _ => OP_QUEST,
                    };
                    let after = self.repeat(op, 0, 0, t, &t[1..], last_repeat)?;
                    repeat = Some(t);
                    t = after;
                }
                b'{' => match parse_repeat(t) {
                    // If the repeat cannot be parsed, { is a literal.
                    None => {
                        self.literal('{' as i32)?;
                        t = &t[1..];
                    }
                    Some((min, max, after)) => {
                        if !(0..=1000).contains(&min) || max > 1000 || (max >= 0 && min > max) {
                            // Numbers were too big, or max is present and min > max.
                            return err(ERR_INVALID_REPEAT_SIZE, &t[..t.len() - after.len()]);
                        }
                        let after = self.repeat(OP_REPEAT, min, max, t, after, last_repeat)?;
                        repeat = Some(t);
                        t = after;
                    }
                },
                b'\\' => t = self.parse_backslash(t)?,
                _ => {
                    let (c, rest) = next_rune(t)?;
                    self.literal(c)?;
                    t = rest;
                }
            }
            last_repeat = repeat;
        }
        self.concat()?;
        if self.swap_vertical_bar() {
            self.stack.pop(); // pop vertical bar
        }
        self.alternate()?;
        if self.stack.len() != 1 {
            return err(ERR_MISSING_PAREN, s);
        }
        Ok(self.stack[0])
    }

    fn parse_backslash(&mut self, t: &'a [u8]) -> PRes<&'a [u8]> {
        if t.len() >= 2 {
            match t[1] {
                b'A' => {
                    self.op(OP_BEGIN_TEXT)?;
                    return Ok(&t[2..]);
                }
                b'b' => {
                    self.op(OP_WORD_BOUNDARY)?;
                    return Ok(&t[2..]);
                }
                b'B' => {
                    self.op(OP_NO_WORD_BOUNDARY)?;
                    return Ok(&t[2..]);
                }
                b'C' => return err(ERR_INVALID_ESCAPE, &t[..2]), // any byte; not supported
                b'Q' => {
                    // \Q ... \E: the ... is always literals
                    let rest = &t[2..];
                    let (mut lit, after) = match find(rest, b"\\E") {
                        Some(i) => (&rest[..i], &rest[i + 2..]),
                        None => (rest, &rest[rest.len()..]),
                    };
                    while !lit.is_empty() {
                        let (c, r) = next_rune(lit)?;
                        self.literal(c)?;
                        lit = r;
                    }
                    return Ok(after);
                }
                b'z' => {
                    self.op(OP_END_TEXT)?;
                    return Ok(&t[2..]);
                }
                _ => {}
            }
        }
        let re = self.new_regexp(OP_CHAR_CLASS);
        self.n[re].flags = self.flags;
        let mut class = Vec::new();
        // Unicode character group like \p{Han}.
        if t.len() >= 2 && (t[1] == b'p' || t[1] == b'P') {
            if let Some(rest) = self.parse_unicode_class(t, &mut class)? {
                self.n[re].runes = class;
                self.push(re)?;
                return Ok(rest);
            }
        }
        // Perl character class escape.
        if let Some(rest) = self.parse_perl_class_escape(t, &mut class) {
            self.n[re].runes = class;
            self.push(re)?;
            return Ok(rest);
        }
        self.reuse(re);
        // Ordinary single-character escape.
        let (c, rest) = self.parse_escape(t)?;
        self.literal(c)?;
        Ok(rest)
    }

    /// parsePerlFlags parses (?i), (?:, (?i:, (?P<name>, (?<name> and the like.
    fn parse_perl_flags(&mut self, s: &'a [u8]) -> PRes<&'a [u8]> {
        let t = s;
        let starts_with_p = t.len() > 4 && t[2] == b'P' && t[3] == b'<';
        let starts_with_name = t.len() > 3 && t[2] == b'<';
        if starts_with_p || starts_with_name {
            let expr_start = if starts_with_name { 3 } else { 4 };
            let Some(end) = t.iter().position(|&b| b == b'>') else {
                check_utf8(t)?;
                return err(ERR_INVALID_NAMED_CAPTURE, s);
            };
            let capture = &t[..end + 1];
            let name = &t[expr_start..end];
            check_utf8(name)?;
            if name.is_empty() || !name.iter().all(|&c| c == b'_' || c.is_ascii_alphanumeric()) {
                return err(ERR_INVALID_NAMED_CAPTURE, capture);
            }
            self.num_cap += 1;
            let re = self.op(OP_LEFT_PAREN)?;
            self.n[re].cap = self.num_cap;
            self.n[re].name = name.to_vec();
            return Ok(&t[end + 1..]);
        }

        // Non-capturing group, possibly with flag changes.
        let mut t = &s[2..];
        let mut flags = self.flags;
        let mut sign = 1;
        let mut saw_flag = false;
        while !t.is_empty() {
            let (c, rest) = next_rune(t)?;
            t = rest;
            match char::from_u32(c as u32).unwrap_or('\0') {
                'i' => {
                    flags |= FOLD_CASE;
                    saw_flag = true;
                }
                'm' => {
                    flags &= !ONE_LINE;
                    saw_flag = true;
                }
                's' => {
                    flags |= DOT_NL;
                    saw_flag = true;
                }
                'U' => {
                    flags |= NON_GREEDY;
                    saw_flag = true;
                }
                '-' => {
                    if sign < 0 {
                        break;
                    }
                    sign = -1;
                    // Invert flags so that | above turn into &^ and vice versa.
                    flags = !flags;
                    saw_flag = false;
                }
                ch @ (':' | ')') => {
                    if sign < 0 {
                        if !saw_flag {
                            break;
                        }
                        flags = !flags;
                    }
                    if ch == ':' {
                        self.op(OP_LEFT_PAREN)?; // open new group
                    }
                    self.flags = flags;
                    return Ok(t);
                }
                _ => break,
            }
        }
        err(ERR_INVALID_PERL_OP, &s[..s.len() - t.len()])
    }

    /// parseEscape parses an escape sequence at the beginning of s.
    fn parse_escape(&self, s: &'a [u8]) -> PRes<(i32, &'a [u8])> {
        let t = &s[1..];
        if t.is_empty() {
            return err(ERR_TRAILING_BACKSLASH, b"");
        }
        let (c, mut t) = next_rune(t)?;
        'sw: {
            match c {
                0x30..=0x37 => {
                    // Single non-zero digit is a backreference; not supported.
                    if c != 0x30 && (t.is_empty() || t[0] < b'0' || t[0] > b'7') {
                        break 'sw;
                    }
                    // Consume up to three octal digits; already have one.
                    let mut r = c - 0x30;
                    for _ in 1..3 {
                        if t.is_empty() || t[0] < b'0' || t[0] > b'7' {
                            break;
                        }
                        r = r * 8 + (t[0] - b'0') as i32;
                        t = &t[1..];
                    }
                    return Ok((r, t));
                }
                0x78 => {
                    // x: hexadecimal escapes
                    if t.is_empty() {
                        break 'sw;
                    }
                    let (c1, rest) = next_rune(t)?;
                    t = rest;
                    if c1 == '{' as i32 {
                        // Any number of hex digits in braces, at least one.
                        let mut nhex = 0;
                        let mut r = 0i32;
                        loop {
                            if t.is_empty() {
                                break 'sw;
                            }
                            let (c2, rest) = next_rune(t)?;
                            t = rest;
                            if c2 == '}' as i32 {
                                break;
                            }
                            let v = unhex(c2);
                            if v < 0 {
                                break 'sw;
                            }
                            r = r * 16 + v;
                            if r > MAX_RUNE {
                                break 'sw;
                            }
                            nhex += 1;
                        }
                        if nhex == 0 {
                            break 'sw;
                        }
                        return Ok((r, t));
                    }
                    // Easy case: two hex digits.
                    let x = unhex(c1);
                    let (c2, rest) = next_rune(t)?;
                    t = rest;
                    let y = unhex(c2);
                    if x < 0 || y < 0 {
                        break 'sw;
                    }
                    return Ok((x * 16 + y, t));
                }
                // C escapes (no \b: that is the word boundary).
                0x61 => return Ok((7, t)),
                0x66 => return Ok((12, t)),
                0x6E => return Ok((10, t)),
                0x72 => return Ok((13, t)),
                0x74 => return Ok((9, t)),
                0x76 => return Ok((11, t)),
                _ => {
                    // Escaped non-word characters are always themselves.
                    if c < 0x80 && !isalnum(c) {
                        return Ok((c, t));
                    }
                }
            }
        }
        err(ERR_INVALID_ESCAPE, &s[..s.len() - t.len()])
    }

    fn parse_class_char(&self, s: &'a [u8], whole_class: &'a [u8]) -> PRes<(i32, &'a [u8])> {
        if s.is_empty() {
            return err(ERR_MISSING_BRACKET, whole_class);
        }
        // Allow regular escape sequences even though many need not be escaped here.
        if s[0] == b'\\' {
            return self.parse_escape(s);
        }
        next_rune(s)
    }

    /// parsePerlClassEscape: \d \D \s \S \w \W, appended to r.
    fn parse_perl_class_escape(&self, s: &'a [u8], r: &mut Vec<i32>) -> Option<&'a [u8]> {
        if s.len() < 2 || s[0] != b'\\' {
            return None;
        }
        let (sign, class): (i32, &[i32]) = match s[1] {
            b'd' => (1, &[0x30, 0x39]),
            b'D' => (-1, &[0x30, 0x39]),
            b's' => (1, &[0x9, 0xa, 0xc, 0xd, 0x20, 0x20]),
            b'S' => (-1, &[0x9, 0xa, 0xc, 0xd, 0x20, 0x20]),
            b'w' => (1, &[0x30, 0x39, 0x41, 0x5a, 0x5f, 0x5f, 0x61, 0x7a]),
            b'W' => (-1, &[0x30, 0x39, 0x41, 0x5a, 0x5f, 0x5f, 0x61, 0x7a]),
            _ => return None,
        };
        self.append_group(r, sign, class);
        Some(&s[2..])
    }

    /// parseNamedClass: a POSIX class like [:alnum:], appended to r.
    fn parse_named_class(&self, s: &'a [u8], r: &mut Vec<i32>) -> PRes<Option<&'a [u8]>> {
        if s.len() < 2 || s[0] != b'[' || s[1] != b':' {
            return Ok(None);
        }
        let Some(i) = find(&s[2..], b":]") else {
            return Ok(None);
        };
        let i = i + 2;
        let (name, rest) = (&s[..i + 2], &s[i + 2..]);
        let (neg, base) = match name.strip_prefix(b"[:^") {
            Some(b) => (true, b),
            None => (false, &name[2..]),
        };
        let class: &[i32] = match base {
            b"alnum:]" => &[0x30, 0x39, 0x41, 0x5a, 0x61, 0x7a],
            b"alpha:]" => &[0x41, 0x5a, 0x61, 0x7a],
            b"ascii:]" => &[0x0, 0x7f],
            b"blank:]" => &[0x9, 0x9, 0x20, 0x20],
            b"cntrl:]" => &[0x0, 0x1f, 0x7f, 0x7f],
            b"digit:]" => &[0x30, 0x39],
            b"graph:]" => &[0x21, 0x7e],
            b"lower:]" => &[0x61, 0x7a],
            b"print:]" => &[0x20, 0x7e],
            b"punct:]" => &[0x21, 0x2f, 0x3a, 0x40, 0x5b, 0x60, 0x7b, 0x7e],
            b"space:]" => &[0x9, 0xd, 0x20, 0x20],
            b"upper:]" => &[0x41, 0x5a],
            b"word:]" => &[0x30, 0x39, 0x41, 0x5a, 0x5f, 0x5f, 0x61, 0x7a],
            b"xdigit:]" => &[0x30, 0x39, 0x41, 0x46, 0x61, 0x66],
            _ => return err(ERR_INVALID_CHAR_RANGE, name),
        };
        self.append_group(r, if neg { -1 } else { 1 }, class);
        Ok(Some(rest))
    }

    fn append_group(&self, r: &mut Vec<i32>, sign: i32, class: &[i32]) {
        if self.flags & FOLD_CASE == 0 {
            if sign < 0 {
                append_negated_class(r, class);
            } else {
                append_class(r, class);
            }
        } else {
            let mut tmp = Vec::new();
            append_folded_class(&mut tmp, class);
            clean_class(&mut tmp);
            if sign < 0 {
                append_negated_class(r, &tmp);
            } else {
                append_class(r, &tmp);
            }
        }
    }

    /// parseUnicodeClass: \p{Han}, \pL, \P{^Greek} ..., appended to r.
    fn parse_unicode_class(&self, s: &'a [u8], r: &mut Vec<i32>) -> PRes<Option<&'a [u8]>> {
        if s.len() < 2 || s[0] != b'\\' || (s[1] != b'p' && s[1] != b'P') {
            return Ok(None);
        }
        // Committed to parse or return error.
        let mut sign = if s[1] == b'P' { -1 } else { 1 };
        let (c, t) = next_rune(&s[2..])?;
        let (seq, mut name, rest);
        if c != '{' as i32 {
            // Single-letter name.
            seq = &s[..s.len() - t.len()];
            name = &seq[2..];
            rest = t;
        } else {
            // Name is in braces.
            let Some(end) = s.iter().position(|&b| b == b'}') else {
                check_utf8(s)?;
                return err(ERR_INVALID_CHAR_RANGE, s);
            };
            seq = &s[..end + 1];
            rest = &s[end + 1..];
            name = &s[3..end];
            check_utf8(name)?;
        }
        // Group can have leading negation too. \p{^Han} == \P{Han}, \P{^Han} == \p{Han}.
        if !name.is_empty() && name[0] == b'^' {
            sign = -sign;
            name = &name[1..];
        }
        let Some((tab, fold, tsign)) = unicode_table(name) else {
            return err(ERR_INVALID_CHAR_RANGE, seq);
        };
        if tsign < 0 {
            sign = -sign;
        }
        match fold {
            Some(fold) if self.flags & FOLD_CASE != 0 => {
                let mut tmp = Vec::new();
                append_table(&mut tmp, tab);
                append_table(&mut tmp, fold);
                clean_class(&mut tmp);
                if sign > 0 {
                    append_class(r, &tmp);
                } else {
                    append_negated_class(r, &tmp);
                }
            }
            _ => {
                if sign > 0 {
                    append_table(r, tab);
                } else {
                    append_negated_table(r, tab);
                }
            }
        }
        Ok(Some(rest))
    }

    /// parseClass parses a bracketed character class and pushes it.
    fn parse_class(&mut self, s: &'a [u8]) -> PRes<&'a [u8]> {
        let mut t = &s[1..]; // chop [
        let re = self.new_regexp(OP_CHAR_CLASS);
        self.n[re].flags = self.flags;
        let mut sign = 1;
        if !t.is_empty() && t[0] == b'^' {
            // ClassNL is set (Perl), so [^...] may match \n: nothing to add here.
            sign = -1;
            t = &t[1..];
        }
        let mut class = Vec::new();
        let mut first = true; // ] and - are okay as first char in class
        while t.is_empty() || t[0] != b']' || first {
            first = false;
            // POSIX [:alnum:] etc.
            if t.len() > 2 && t[0] == b'[' && t[1] == b':' {
                if let Some(nt) = self.parse_named_class(t, &mut class)? {
                    t = nt;
                    continue;
                }
            }
            // Unicode character group like \p{Han}.
            if let Some(nt) = self.parse_unicode_class(t, &mut class)? {
                t = nt;
                continue;
            }
            // Perl character class symbols.
            if let Some(nt) = self.parse_perl_class_escape(t, &mut class) {
                t = nt;
                continue;
            }
            // Single character or simple range.
            let rng = t;
            let (lo, nt) = self.parse_class_char(t, s)?;
            t = nt;
            let mut hi = lo;
            // [a-] means (a|-) so check for final ].
            if t.len() >= 2 && t[0] == b'-' && t[1] != b']' {
                let (h, nt) = self.parse_class_char(&t[1..], s)?;
                t = nt;
                hi = h;
                if hi < lo {
                    return err(ERR_INVALID_CHAR_RANGE, &rng[..rng.len() - t.len()]);
                }
            }
            if self.flags & FOLD_CASE == 0 {
                append_range(&mut class, lo, hi);
            } else {
                append_folded_range(&mut class, lo, hi);
            }
        }
        t = &t[1..]; // chop ]
        clean_class(&mut class);
        if sign < 0 {
            class = negate_class(&class);
        }
        self.n[re].runes = class;
        self.push(re)?;
        Ok(t)
    }
}

/// parseRepeat parses {min} (max=min), {min,} (max=-1) or {min,max}; None if s is not of
/// that form, min = -1 if a number is too big.
fn parse_repeat(s: &[u8]) -> Option<(i32, i32, &[u8])> {
    if s.is_empty() || s[0] != b'{' {
        return None;
    }
    let (mut min, mut s) = parse_int(&s[1..])?;
    if s.is_empty() {
        return None;
    }
    let max;
    if s[0] != b',' {
        max = min;
    } else {
        s = &s[1..];
        if s.is_empty() {
            return None;
        }
        if s[0] == b'}' {
            max = -1;
        } else {
            let (m, rest) = parse_int(s)?;
            s = rest;
            max = m;
            if max < 0 {
                min = -1; // parseInt found too big a number
            }
        }
    }
    if s.is_empty() || s[0] != b'}' {
        return None;
    }
    Some((min, max, &s[1..]))
}

/// parseInt parses a decimal integer without leading zeros; -1 when it is too big.
fn parse_int(s: &[u8]) -> Option<(i32, &[u8])> {
    if s.is_empty() || !s[0].is_ascii_digit() {
        return None;
    }
    if s.len() >= 2 && s[0] == b'0' && s[1].is_ascii_digit() {
        return None;
    }
    let nd = s.iter().take_while(|b| b.is_ascii_digit()).count();
    let mut n: i64 = 0;
    for &d in &s[..nd] {
        if n >= 100_000_000 {
            n = -1;
            break;
        }
        n = n * 10 + (d - b'0') as i64;
    }
    Some((n as i32, &s[nd..]))
}

/// minFoldRune: the minimum rune fold-equivalent to r.
fn min_fold_rune(r: i32) -> i32 {
    if !(MIN_FOLD..=MAX_FOLD).contains(&r) {
        return r;
    }
    let (mut m, r0) = (r, r);
    let mut r = simple_fold(r);
    while r != r0 {
        m = m.min(r);
        r = simple_fold(r);
    }
    m
}

fn max_cap(n: &[Node], re: usize) -> i32 {
    let mut m = if n[re].op == OP_CAPTURE { n[re].cap } else { 0 };
    for &s in &n[re].sub {
        m = m.max(max_cap(n, s));
    }
    m
}

fn cap_names(n: &[Node], re: usize, names: &mut [Vec<u8>]) {
    if n[re].op == OP_CAPTURE {
        names[n[re].cap as usize] = n[re].name.clone();
    }
    for &s in &n[re].sub {
        cap_names(n, s, names);
    }
}

// ---------------------------------------------------------------------------------------------
// character class helpers (runes are lo,hi pairs)

/// cleanClass sorts the ranges (lo ascending, hi descending) and merges them.
fn clean_class(r: &mut Vec<i32>) {
    let mut p: Vec<(i32, i32)> = r.chunks(2).map(|c| (c[0], c[1])).collect();
    p.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    let mut w = 0;
    for i in 0..p.len() {
        let (lo, hi) = p[i];
        if w > 0 && lo <= p[w - 1].1 + 1 {
            if hi > p[w - 1].1 {
                p[w - 1].1 = hi;
            }
            continue;
        }
        p[w] = (lo, hi);
        w += 1;
    }
    r.clear();
    for &(lo, hi) in &p[..w] {
        r.push(lo);
        r.push(hi);
    }
}

fn append_literal(r: &mut Vec<i32>, x: i32, flags: u16) {
    if flags & FOLD_CASE != 0 {
        append_folded_range(r, x, x);
    } else {
        append_range(r, x, x);
    }
}

/// appendRange appends lo-hi, extending the last or next-to-last range when they touch.
fn append_range(r: &mut Vec<i32>, lo: i32, hi: i32) {
    let n = r.len();
    for i in [2, 4] {
        if n >= i {
            let (rlo, rhi) = (r[n - i], r[n - i + 1]);
            if lo <= rhi + 1 && rlo <= hi + 1 {
                if lo < rlo {
                    r[n - i] = lo;
                }
                if hi > rhi {
                    r[n - i + 1] = hi;
                }
                return;
            }
        }
    }
    r.push(lo);
    r.push(hi);
}

/// appendFoldedRange appends lo-hi and its case-folding equivalents.
fn append_folded_range(r: &mut Vec<i32>, mut lo: i32, mut hi: i32) {
    if (lo <= MIN_FOLD && hi >= MAX_FOLD) || hi < MIN_FOLD || lo > MAX_FOLD {
        // Range is full, or outside folding possibilities.
        append_range(r, lo, hi);
        return;
    }
    if lo < MIN_FOLD {
        append_range(r, lo, MIN_FOLD - 1);
        lo = MIN_FOLD;
    }
    if hi > MAX_FOLD {
        append_range(r, MAX_FOLD + 1, hi);
        hi = MAX_FOLD;
    }
    for c in lo..=hi {
        append_range(r, c, c);
        let mut f = simple_fold(c);
        while f != c {
            append_range(r, f, f);
            f = simple_fold(f);
        }
    }
}

fn append_class(r: &mut Vec<i32>, x: &[i32]) {
    for p in x.chunks(2) {
        append_range(r, p[0], p[1]);
    }
}

fn append_folded_class(r: &mut Vec<i32>, x: &[i32]) {
    for p in x.chunks(2) {
        append_folded_range(r, p[0], p[1]);
    }
}

fn append_negated_class(r: &mut Vec<i32>, x: &[i32]) {
    let mut next_lo = 0;
    for p in x.chunks(2) {
        if next_lo < p[0] {
            append_range(r, next_lo, p[0] - 1);
        }
        next_lo = p[1] + 1;
    }
    if next_lo <= MAX_RUNE {
        append_range(r, next_lo, MAX_RUNE);
    }
}

fn append_table(r: &mut Vec<i32>, t: &[(i32, i32)]) {
    for &(lo, hi) in t {
        append_range(r, lo, hi);
    }
}

fn append_negated_table(r: &mut Vec<i32>, t: &[(i32, i32)]) {
    let mut next_lo = 0;
    for &(lo, hi) in t {
        if next_lo < lo {
            append_range(r, next_lo, lo - 1);
        }
        next_lo = hi + 1;
    }
    if next_lo <= MAX_RUNE {
        append_range(r, next_lo, MAX_RUNE);
    }
}

/// negateClass returns the negation of a clean class.
fn negate_class(r: &[i32]) -> Vec<i32> {
    let mut out = Vec::new();
    let mut next_lo = 0;
    for p in r.chunks(2) {
        if next_lo < p[0] {
            out.push(next_lo);
            out.push(p[0] - 1);
        }
        next_lo = p[1] + 1;
    }
    if next_lo <= MAX_RUNE {
        out.push(next_lo);
        out.push(MAX_RUNE);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Unicode tables (package unicode via the generated data at the end of the file)

type Table = Vec<(i32, i32)>;

struct UniData {
    tables: HashMap<&'static str, Table>,
    /// (lo, hi, upper delta, lower delta) from unicode.CaseRanges.
    case_ranges: Vec<(i32, i32, i32, i32)>,
    /// SimpleFold results that differ from the ToLower/ToUpper rule.
    orbit: Vec<(i32, i32)>,
}

/// Decodes the base-26 numbers of the generated data ('a'..'z' continue, 'A'..'Z' end).
fn decode_nums(s: &str) -> Vec<i64> {
    let mut out = Vec::new();
    let mut acc: i64 = 0;
    for b in s.bytes() {
        match b {
            b'a'..=b'z' => acc = acc * 26 + (b - b'a') as i64,
            b'A'..=b'Z' => {
                out.push(acc * 26 + (b - b'A') as i64);
                acc = 0;
            }
            _ => {}
        }
    }
    out
}

fn unzig(n: i64) -> i32 {
    (if n & 1 != 0 { -(n + 1) / 2 } else { n / 2 }) as i32
}

fn uni() -> &'static UniData {
    static DATA: OnceLock<UniData> = OnceLock::new();
    DATA.get_or_init(|| {
        let mut tables = HashMap::new();
        for &(key, enc) in UNI_TABLES {
            let nums = decode_nums(enc);
            let mut t = Vec::with_capacity(nums.len() / 2);
            let mut next: i64 = 0;
            for p in nums.chunks(2) {
                let lo = next + p[0];
                let hi = lo + p[1];
                t.push((lo as i32, hi as i32));
                next = hi + 1;
            }
            tables.insert(key, t);
        }
        let mut case_ranges = Vec::new();
        let mut prev: i64 = 0;
        for c in decode_nums(UNI_CASE_RANGES).chunks(4) {
            let lo = prev + c[0];
            let hi = lo + c[1];
            case_ranges.push((lo as i32, hi as i32, unzig(c[2]), unzig(c[3])));
            prev = hi + 1;
        }
        let mut orbit = Vec::new();
        let mut r: i64 = 0;
        for c in decode_nums(UNI_FOLD_ORBIT).chunks(2) {
            r += c[0];
            orbit.push((r as i32, c[1] as i32));
        }
        UniData {
            tables,
            case_ranges,
            orbit,
        }
    })
}

fn uni_table(kind: &str, name: &[u8]) -> Option<&'static [(i32, i32)]> {
    let mut key = kind.as_bytes().to_vec();
    key.push(b':');
    key.extend_from_slice(name);
    let key = std::str::from_utf8(&key).ok()?;
    uni().tables.get(key).map(|t| t.as_slice())
}

fn in_table(t: &[(i32, i32)], r: i32) -> bool {
    t.binary_search_by(|&(lo, hi)| {
        if hi < r {
            std::cmp::Ordering::Less
        } else if lo > r {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    })
    .is_ok()
}

/// unicode.IsLetter.
fn is_letter(r: i32) -> bool {
    in_table(uni_table("c", b"L").unwrap(), r)
}

/// unicode.IsDigit.
fn is_digit(r: i32) -> bool {
    if r <= 0xFF {
        return (0x30..=0x39).contains(&r);
    }
    in_table(uni_table("c", b"Nd").unwrap(), r)
}

/// unicode.To(UpperCase/LowerCase) through CaseRanges (`upper` selects the delta).
fn to_case(upper: bool, r: i32) -> i32 {
    let cr = &uni().case_ranges;
    let (mut lo, mut hi) = (0, cr.len());
    while lo < hi {
        let m = (lo + hi) / 2;
        let (clo, chi, du, dl) = cr[m];
        if clo <= r && r <= chi {
            let delta = if upper { du } else { dl };
            if delta > MAX_RUNE {
                // UpperLower sequence: even offsets upper case, odd lower case.
                return clo + (((r - clo) & !1) | if upper { 0 } else { 1 });
            }
            return r + delta;
        }
        if r < clo {
            hi = m;
        } else {
            lo = m + 1;
        }
    }
    r
}

/// unicode.SimpleFold.
fn simple_fold(r: i32) -> i32 {
    if !(0..=MAX_RUNE).contains(&r) {
        return r;
    }
    let orbit = &uni().orbit;
    if let Ok(i) = orbit.binary_search_by(|p| p.0.cmp(&r)) {
        return orbit[i].1;
    }
    let l = to_case(false, r);
    if l != r {
        return l;
    }
    to_case(true, r)
}

static ANY_TABLE: &[(i32, i32)] = &[(0, MAX_RUNE)];
static ASCII_TABLE: &[(i32, i32)] = &[(0, 0x7F)];
static ASCII_FOLD_TABLE: &[(i32, i32)] = &[(0, 0x7F), (0x017F, 0x017F), (0x212A, 0x212A)];

/// canonicalName: separators (_ - space) dropped, first letter upper, rest lower.
fn canonical_name(name: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(name.len());
    for &c in name {
        if c == b'_' || c == b'-' || c == b' ' {
            continue;
        }
        b.push(if b.is_empty() {
            c.to_ascii_uppercase()
        } else {
            c.to_ascii_lowercase()
        });
    }
    b
}

type UniLookup = (&'static [(i32, i32)], Option<&'static [(i32, i32)]>, i32);

/// unicodeTable: the table named by name, its fold-equivalent additions and a sign
/// (-1 when the table must be inverted).
fn unicode_table(name: &[u8]) -> Option<UniLookup> {
    let name = canonical_name(name);
    match name.as_slice() {
        b"Any" => return Some((ANY_TABLE, Some(ANY_TABLE), 1)),
        b"Assigned" => {
            let cn = uni_table("c", b"Cn").unwrap();
            return Some((cn, Some(cn), -1));
        }
        b"Ascii" => return Some((ASCII_TABLE, Some(ASCII_FOLD_TABLE), 1)),
        b"Lc" => return Some((uni_table("c", b"LC")?, uni_table("fc", b"LC"), 1)),
        _ => {}
    }
    if let Some(t) = uni_table("c", &name) {
        return Some((t, uni_table("fc", &name), 1));
    }
    if let Some(t) = uni_table("s", &name) {
        return Some((t, uni_table("fs", &name), 1));
    }
    for &(alias, actual) in UNI_ALIASES {
        if canonical_name(alias.as_bytes()) == name {
            return Some((
                uni_table("c", actual.as_bytes())?,
                uni_table("fc", actual.as_bytes()),
                1,
            ));
        }
    }
    None
}

// ---------------------------------------------------------------------------------------------
// simplify.go

fn add_node(n: &mut Vec<Node>, node: Node) -> usize {
    n.push(node);
    n.len() - 1
}

/// Regexp.Simplify: removes counted repetitions (x{2,5} = xx(x(x(x)?)?)?) and the like.
fn simplify(n: &mut Vec<Node>, re: usize) -> usize {
    match n[re].op {
        OP_CAPTURE | OP_CONCAT | OP_ALTERNATE => {
            let subs = n[re].sub.clone();
            let mut nsubs = Vec::with_capacity(subs.len());
            let mut changed = false;
            for &s in &subs {
                let ns = simplify(n, s);
                changed |= ns != s;
                nsubs.push(ns);
            }
            if !changed {
                return re;
            }
            let mut node = n[re].clone();
            node.runes = Vec::new();
            node.sub = nsubs;
            add_node(n, node)
        }
        OP_STAR | OP_PLUS | OP_QUEST => {
            let sub = simplify(n, n[re].sub[0]);
            let (op, flags) = (n[re].op, n[re].flags);
            simplify1(n, op, flags, sub, Some(re))
        }
        OP_REPEAT => {
            let (min, max, flags) = (n[re].min, n[re].max, n[re].flags);
            // x{0} matches the empty string and doesn't even need to consider x.
            if min == 0 && max == 0 {
                return add_node(
                    n,
                    Node {
                        op: OP_EMPTY_MATCH,
                        ..Node::default()
                    },
                );
            }
            let sub = simplify(n, n[re].sub[0]);
            if max == -1 {
                if min == 0 {
                    return simplify1(n, OP_STAR, flags, sub, None);
                }
                if min == 1 {
                    return simplify1(n, OP_PLUS, flags, sub, None);
                }
                // x{4,} is xxxx+.
                let mut subs = vec![sub; (min - 1) as usize];
                subs.push(simplify1(n, OP_PLUS, flags, sub, None));
                return add_node(
                    n,
                    Node {
                        op: OP_CONCAT,
                        sub: subs,
                        ..Node::default()
                    },
                );
            }
            if min == 1 && max == 1 {
                return sub;
            }
            let mut prefix = if min > 0 {
                Some(vec![sub; min as usize])
            } else {
                None
            };
            if max > min {
                let mut suffix = simplify1(n, OP_QUEST, flags, sub, None);
                for _ in min + 1..max {
                    let c = add_node(
                        n,
                        Node {
                            op: OP_CONCAT,
                            sub: vec![sub, suffix],
                            ..Node::default()
                        },
                    );
                    suffix = simplify1(n, OP_QUEST, flags, c, None);
                }
                match prefix.as_mut() {
                    None => return suffix,
                    Some(p) => p.push(suffix),
                }
            }
            match prefix {
                Some(p) => add_node(
                    n,
                    Node {
                        op: OP_CONCAT,
                        sub: p,
                        ..Node::default()
                    },
                ),
                // Some degenerate case like min > max: impossible match.
                None => add_node(
                    n,
                    Node {
                        op: OP_NO_MATCH,
                        ..Node::default()
                    },
                ),
            }
        }
        _ => re,
    }
}

/// simplify1 builds op(sub), reusing sub or re where Go does.
fn simplify1(n: &mut Vec<Node>, op: u8, flags: u16, sub: usize, re: Option<usize>) -> usize {
    if n[sub].op == OP_EMPTY_MATCH {
        return sub;
    }
    if op == n[sub].op && flags & NON_GREEDY == n[sub].flags & NON_GREEDY {
        return sub;
    }
    if let Some(re) = re {
        if n[re].op == op && n[re].flags & NON_GREEDY == flags & NON_GREEDY && n[re].sub[0] == sub {
            return re;
        }
    }
    add_node(
        n,
        Node {
            op,
            flags,
            sub: vec![sub],
            ..Node::default()
        },
    )
}

// ---------------------------------------------------------------------------------------------
// compile.go / prog.go

#[derive(Clone, Copy, PartialEq)]
enum IOp {
    Alt,
    Capture,
    EmptyWidth,
    Match,
    Fail,
    Nop,
    Rune,
    Rune1,
    RuneAny,
    RuneAnyNotNL,
}

const EMPTY_BEGIN_LINE: u8 = 1;
const EMPTY_END_LINE: u8 = 2;
const EMPTY_BEGIN_TEXT: u8 = 4;
const EMPTY_END_TEXT: u8 = 8;
const EMPTY_WORD_BOUNDARY: u8 = 16;
const EMPTY_NO_WORD_BOUNDARY: u8 = 32;

#[derive(Clone, Copy)]
struct Inst {
    op: IOp,
    out: u32,
    arg: u32,
    /// The instruction's runes: runes[rs..rs + rn] of the program.
    rs: u32,
    rn: u32,
}

struct Prog {
    inst: Vec<Inst>,
    runes: Vec<i32>,
    start: u32,
    num_cap: usize,
}

impl Prog {
    fn inst_runes(&self, i: &Inst) -> &[i32] {
        &self.runes[i.rs as usize..(i.rs + i.rn) as usize]
    }

    /// StartCond: the empty-width conditions every match must satisfy at its start
    /// (0xFF when no match is possible).
    fn start_cond(&self) -> u8 {
        let mut flag = 0;
        let mut pc = self.start as usize;
        loop {
            let i = &self.inst[pc];
            match i.op {
                IOp::EmptyWidth => flag |= i.arg as u8,
                IOp::Fail => return 0xFF,
                IOp::Capture | IOp::Nop => {}
                _ => break,
            }
            pc = i.out as usize;
        }
        flag
    }
}

#[derive(Clone, Copy, Default)]
struct PatchList {
    head: u32,
    tail: u32,
}

#[derive(Clone, Copy, Default)]
struct Frag {
    i: u32,
    out: PatchList,
    nullable: bool,
}

struct Compiler<'n> {
    /// The simplified tree; class runes are moved into the program as they are compiled
    /// (Go shares those slices between tree and program).
    n: &'n mut [Node],
    p: Prog,
    class_runes: HashMap<usize, (u32, u32)>,
}

impl<'n> Compiler<'n> {
    fn compile_prog(n: &'n mut [Node], re: usize) -> Prog {
        let mut c = Compiler {
            n,
            p: Prog {
                inst: Vec::new(),
                runes: Vec::new(),
                start: 0,
                num_cap: 2,
            },
            class_runes: HashMap::new(),
        };
        c.inst(IOp::Fail);
        let f = c.compile(re);
        let m = c.inst(IOp::Match).i;
        c.patch(f.out, m);
        c.p.start = f.i;
        c.p
    }

    fn patch(&mut self, l: PatchList, val: u32) {
        let mut head = l.head;
        while head != 0 {
            let i = &mut self.p.inst[(head >> 1) as usize];
            if head & 1 == 0 {
                head = i.out;
                i.out = val;
            } else {
                head = i.arg;
                i.arg = val;
            }
        }
    }

    fn append(&mut self, l1: PatchList, l2: PatchList) -> PatchList {
        if l1.head == 0 {
            return l2;
        }
        if l2.head == 0 {
            return l1;
        }
        let i = &mut self.p.inst[(l1.tail >> 1) as usize];
        if l1.tail & 1 == 0 {
            i.out = l2.head;
        } else {
            i.arg = l2.head;
        }
        PatchList {
            head: l1.head,
            tail: l2.tail,
        }
    }

    fn inst(&mut self, op: IOp) -> Frag {
        let f = Frag {
            i: self.p.inst.len() as u32,
            out: PatchList::default(),
            nullable: true,
        };
        self.p.inst.push(Inst {
            op,
            out: 0,
            arg: 0,
            rs: 0,
            rn: 0,
        });
        f
    }

    fn single(&mut self, op: IOp, arg: u32) -> Frag {
        let mut f = self.inst(op);
        self.p.inst[f.i as usize].arg = arg;
        f.out = PatchList {
            head: f.i << 1,
            tail: f.i << 1,
        };
        f
    }

    fn cap(&mut self, arg: u32) -> Frag {
        if self.p.num_cap < arg as usize + 1 {
            self.p.num_cap = arg as usize + 1;
        }
        self.single(IOp::Capture, arg)
    }

    fn cat(&mut self, f1: Frag, f2: Frag) -> Frag {
        // concat of failure is failure
        if f1.i == 0 || f2.i == 0 {
            return Frag::default();
        }
        self.patch(f1.out, f2.i);
        Frag {
            i: f1.i,
            out: f2.out,
            nullable: f1.nullable && f2.nullable,
        }
    }

    fn alt(&mut self, f1: Frag, f2: Frag) -> Frag {
        // alt of failure is other
        if f1.i == 0 {
            return f2;
        }
        if f2.i == 0 {
            return f1;
        }
        let mut f = self.inst(IOp::Alt);
        let i = &mut self.p.inst[f.i as usize];
        i.out = f1.i;
        i.arg = f2.i;
        f.out = self.append(f1.out, f2.out);
        f.nullable = f1.nullable || f2.nullable;
        f
    }

    fn quest(&mut self, f1: Frag, nongreedy: bool) -> Frag {
        let mut f = self.inst(IOp::Alt);
        let i = &mut self.p.inst[f.i as usize];
        let l = if nongreedy {
            i.arg = f1.i;
            f.i << 1
        } else {
            i.out = f1.i;
            f.i << 1 | 1
        };
        f.out = self.append(PatchList { head: l, tail: l }, f1.out);
        f
    }

    /// loop: the main loop of a plus or star.
    fn lp(&mut self, f1: Frag, nongreedy: bool) -> Frag {
        let mut f = self.inst(IOp::Alt);
        let i = &mut self.p.inst[f.i as usize];
        let l = if nongreedy {
            i.arg = f1.i;
            f.i << 1
        } else {
            i.out = f1.i;
            f.i << 1 | 1
        };
        f.out = PatchList { head: l, tail: l };
        self.patch(f1.out, f.i);
        f
    }

    fn star(&mut self, f1: Frag, nongreedy: bool) -> Frag {
        if f1.nullable {
            // Use (f1+)? to get priority match order correct (golang.org/issue/46123).
            let p = self.plus(f1, nongreedy);
            return self.quest(p, nongreedy);
        }
        self.lp(f1, nongreedy)
    }

    fn plus(&mut self, f1: Frag, nongreedy: bool) -> Frag {
        Frag {
            i: f1.i,
            out: self.lp(f1, nongreedy).out,
            nullable: f1.nullable,
        }
    }

    fn rune(&mut self, rs: u32, rn: u32, flags: u16) -> Frag {
        let mut f = self.inst(IOp::Rune);
        f.nullable = false;
        let r = &self.p.runes[rs as usize..(rs + rn) as usize];
        let mut flags = flags & FOLD_CASE;
        if r.len() != 1 || simple_fold(r[0]) == r[0] {
            flags &= !FOLD_CASE; // and sometimes not even that
        }
        let op = if flags & FOLD_CASE == 0 && (r.len() == 1 || (r.len() == 2 && r[0] == r[1])) {
            IOp::Rune1
        } else if r == [0, MAX_RUNE] {
            IOp::RuneAny
        } else if r == [0, 9, 11, MAX_RUNE] {
            IOp::RuneAnyNotNL
        } else {
            IOp::Rune
        };
        let i = &mut self.p.inst[f.i as usize];
        i.op = op;
        i.rs = rs;
        i.rn = rn;
        i.arg = flags as u32;
        f.out = PatchList {
            head: f.i << 1,
            tail: f.i << 1,
        };
        f
    }

    fn add_runes(&mut self, r: &[i32]) -> (u32, u32) {
        let rs = self.p.runes.len() as u32;
        self.p.runes.extend_from_slice(r);
        (rs, r.len() as u32)
    }

    fn compile(&mut self, re: usize) -> Frag {
        let (op, flags, cap) = (self.n[re].op, self.n[re].flags, self.n[re].cap);
        let subs = self.n[re].sub.clone();
        match op {
            OP_NO_MATCH => Frag::default(),
            OP_EMPTY_MATCH => self.single(IOp::Nop, 0),
            OP_LITERAL => {
                let runes = self.n[re].runes.clone();
                if runes.is_empty() {
                    return self.single(IOp::Nop, 0);
                }
                let mut f = Frag::default();
                for (j, &r) in runes.iter().enumerate() {
                    let (rs, rn) = self.add_runes(&[r]);
                    let f1 = self.rune(rs, rn, flags);
                    f = if j == 0 { f1 } else { self.cat(f, f1) };
                }
                f
            }
            OP_CHAR_CLASS => {
                let (rs, rn) = match self.class_runes.get(&re) {
                    Some(&v) => v,
                    None => {
                        let runes = std::mem::take(&mut self.n[re].runes);
                        let v = self.add_runes(&runes);
                        self.class_runes.insert(re, v);
                        v
                    }
                };
                self.rune(rs, rn, flags)
            }
            OP_ANY_CHAR_NOT_NL => {
                let (rs, rn) = self.add_runes(&[0, 9, 11, MAX_RUNE]);
                self.rune(rs, rn, 0)
            }
            OP_ANY_CHAR => {
                let (rs, rn) = self.add_runes(&[0, MAX_RUNE]);
                self.rune(rs, rn, 0)
            }
            OP_BEGIN_LINE => self.single(IOp::EmptyWidth, EMPTY_BEGIN_LINE as u32),
            OP_END_LINE => self.single(IOp::EmptyWidth, EMPTY_END_LINE as u32),
            OP_BEGIN_TEXT => self.single(IOp::EmptyWidth, EMPTY_BEGIN_TEXT as u32),
            OP_END_TEXT => self.single(IOp::EmptyWidth, EMPTY_END_TEXT as u32),
            OP_WORD_BOUNDARY => self.single(IOp::EmptyWidth, EMPTY_WORD_BOUNDARY as u32),
            OP_NO_WORD_BOUNDARY => self.single(IOp::EmptyWidth, EMPTY_NO_WORD_BOUNDARY as u32),
            OP_CAPTURE => {
                let bra = self.cap((cap << 1) as u32);
                let sub = self.compile(subs[0]);
                let ket = self.cap((cap << 1 | 1) as u32);
                let f = self.cat(bra, sub);
                self.cat(f, ket)
            }
            OP_STAR | OP_PLUS | OP_QUEST => {
                let f1 = self.compile(subs[0]);
                let ng = flags & NON_GREEDY != 0;
                match op {
                    OP_STAR => self.star(f1, ng),
                    OP_PLUS => self.plus(f1, ng),
                    _ => self.quest(f1, ng),
                }
            }
            OP_CONCAT => {
                if subs.is_empty() {
                    return self.single(IOp::Nop, 0);
                }
                let mut f = Frag::default();
                for (i, &s) in subs.iter().enumerate() {
                    let fs = self.compile(s);
                    f = if i == 0 { fs } else { self.cat(f, fs) };
                }
                f
            }
            OP_ALTERNATE => {
                let mut f = Frag::default();
                for &s in &subs {
                    let fs = self.compile(s);
                    f = self.alt(f, fs);
                }
                f
            }
            _ => unreachable!("regexp: unhandled case in compile"),
        }
    }
}

/// Inst.MatchRunePos != noMatch, as Go searches the rune list.
fn match_rune(runes: &[i32], fold: bool, r: i32) -> bool {
    match runes.len() {
        0 => false,
        1 => {
            // Single-rune slice is from a literal string, not a char class.
            let r0 = runes[0];
            if r == r0 {
                return true;
            }
            if fold {
                let mut r1 = simple_fold(r0);
                while r1 != r0 {
                    if r == r1 {
                        return true;
                    }
                    r1 = simple_fold(r1);
                }
            }
            false
        }
        2 => r >= runes[0] && r <= runes[1],
        4 | 6 | 8 => {
            for p in runes.chunks(2) {
                if r < p[0] {
                    return false;
                }
                if r <= p[1] {
                    return true;
                }
            }
            false
        }
        _ => {
            let (mut lo, mut hi) = (0, runes.len() / 2);
            while lo < hi {
                let m = (lo + hi) / 2;
                if runes[2 * m] <= r {
                    if r <= runes[2 * m + 1] {
                        return true;
                    }
                    lo = m + 1;
                } else {
                    hi = m;
                }
            }
            false
        }
    }
}

// ---------------------------------------------------------------------------------------------
// exec.go: the Pike VM

fn is_word_char(r: i32) -> bool {
    (r as u32) < 0x80 && ((r as u8).is_ascii_alphanumeric() || r == '_' as i32)
}

/// lazyFlag: the runes around a position, evaluated against an EmptyOp on demand.
#[derive(Clone, Copy)]
struct LazyFlag(i32, i32);

impl LazyFlag {
    fn matches(self, mut op: u8) -> bool {
        if op == 0 {
            return true;
        }
        let LazyFlag(r1, r2) = self;
        if op & EMPTY_BEGIN_LINE != 0 {
            if r1 != '\n' as i32 && r1 >= 0 {
                return false;
            }
            op &= !EMPTY_BEGIN_LINE;
        }
        if op & EMPTY_BEGIN_TEXT != 0 {
            if r1 >= 0 {
                return false;
            }
            op &= !EMPTY_BEGIN_TEXT;
        }
        if op == 0 {
            return true;
        }
        if op & EMPTY_END_LINE != 0 {
            if r2 != '\n' as i32 && r2 >= 0 {
                return false;
            }
            op &= !EMPTY_END_LINE;
        }
        if op & EMPTY_END_TEXT != 0 {
            if r2 >= 0 {
                return false;
            }
            op &= !EMPTY_END_TEXT;
        }
        if op == 0 {
            return true;
        }
        if is_word_char(r1) != is_word_char(r2) {
            op &= !EMPTY_WORD_BOUNDARY;
        } else {
            op &= !EMPTY_NO_WORD_BOUNDARY;
        }
        op == 0
    }
}

/// inputString.context: the runes before and at pos. Only ASCII values matter to the
/// empty-width tests, so a non-ASCII neighbour is reported as RuneError.
fn context(s: &[u8], pos: usize) -> LazyFlag {
    let ascii = |b: u8| if b < 0x80 { b as i32 } else { RUNE_ERROR };
    let r1 = if pos > 0 && pos <= s.len() {
        ascii(s[pos - 1])
    } else {
        -1
    };
    let r2 = if pos < s.len() { ascii(s[pos]) } else { -1 };
    LazyFlag(r1, r2)
}

const NO_THREAD: u32 = u32::MAX;

/// A sparse set of pcs in priority order; each entry may own a thread (a capture slot).
struct Queue {
    sparse: Vec<u32>,
    dense: Vec<(u32, u32)>,
}

enum Frame {
    Explore(u32),
    Restore(usize, i64),
}

struct Machine<'p> {
    prog: &'p Prog,
    ncap: usize,
    caps: Vec<i64>,
    free: Vec<u32>,
    nslots: u32,
    stack: Vec<Frame>,
    matched: bool,
    matchcap: Vec<i64>,
}

impl<'p> Machine<'p> {
    fn alloc(&mut self) -> u32 {
        if let Some(s) = self.free.pop() {
            return s;
        }
        self.nslots += 1;
        self.caps.resize(self.nslots as usize * self.ncap, -1);
        self.nslots - 1
    }

    fn slot(&mut self, t: u32) -> &mut [i64] {
        let k = t as usize * self.ncap;
        &mut self.caps[k..k + self.ncap]
    }

    /// add: adds pc and everything reachable from it by empty-width steps satisfied by
    /// cond, in priority order (an explicit stack replaces Go's recursion).
    fn add(&mut self, q: &mut Queue, pc0: u32, pos: usize, cap: &mut [i64], cond: LazyFlag) {
        let prog = self.prog;
        self.stack.push(Frame::Explore(pc0));
        while let Some(frame) = self.stack.pop() {
            let mut pc = match frame {
                Frame::Restore(k, v) => {
                    cap[k] = v;
                    continue;
                }
                Frame::Explore(pc) => pc,
            };
            loop {
                if pc == 0 {
                    break;
                }
                let j = q.sparse[pc as usize] as usize;
                if j < q.dense.len() && q.dense[j].0 == pc {
                    break;
                }
                let j = q.dense.len();
                q.dense.push((pc, NO_THREAD));
                q.sparse[pc as usize] = j as u32;
                let i = &prog.inst[pc as usize];
                match i.op {
                    IOp::Fail => break,
                    IOp::Alt => {
                        self.stack.push(Frame::Explore(i.arg));
                        pc = i.out;
                    }
                    IOp::EmptyWidth => {
                        if !cond.matches(i.arg as u8) {
                            break;
                        }
                        pc = i.out;
                    }
                    IOp::Nop => pc = i.out,
                    IOp::Capture => {
                        let k = i.arg as usize;
                        if k < cap.len() {
                            self.stack.push(Frame::Restore(k, cap[k]));
                            cap[k] = pos as i64;
                        }
                        pc = i.out;
                    }
                    _ => {
                        let t = self.alloc();
                        self.slot(t).copy_from_slice(cap);
                        q.dense[j].1 = t;
                        break;
                    }
                }
            }
        }
    }

    /// step runs the threads of runq over rune c at pos, adding successors to nextq.
    #[allow(clippy::too_many_arguments)]
    fn step(
        &mut self,
        runq: &mut Queue,
        nextq: &mut Queue,
        pos: usize,
        next_pos: usize,
        c: i32,
        next_cond: LazyFlag,
        scratch: &mut Vec<i64>,
    ) {
        let prog = self.prog;
        let mut j = 0;
        while j < runq.dense.len() {
            let (pc, t) = runq.dense[j];
            j += 1;
            if t == NO_THREAD {
                continue;
            }
            let i = &prog.inst[pc as usize];
            let add = match i.op {
                IOp::Match => {
                    if self.ncap > 0 {
                        let k = t as usize * self.ncap;
                        self.caps[k + 1] = pos as i64;
                        self.matchcap.copy_from_slice(&self.caps[k..k + self.ncap]);
                    }
                    // First-match mode: cut off all lower-priority threads.
                    for &(_, t2) in &runq.dense[j..] {
                        if t2 != NO_THREAD {
                            self.free.push(t2);
                        }
                    }
                    runq.dense.clear();
                    self.matched = true;
                    false
                }
                IOp::Rune => match_rune(prog.inst_runes(i), i.arg as u16 & FOLD_CASE != 0, c),
                IOp::Rune1 => c == prog.runes[i.rs as usize],
                IOp::RuneAny => true,
                IOp::RuneAnyNotNL => c != '\n' as i32,
                _ => unreachable!("bad inst"),
            };
            if add {
                scratch.clear();
                scratch.extend_from_slice(self.slot(t));
                self.add(nextq, i.out, next_pos, scratch, next_cond);
            }
            self.free.push(t);
        }
        runq.dense.clear();
    }
}

impl GoRegexp {
    /// doExecute with the NFA: the leftmost-first match at or after pos, with ncap
    /// capture positions (0 = just report whether there is one).
    fn exec(&self, s: &[u8], pos: usize, ncap: usize) -> Option<Vec<i64>> {
        if self.cond == 0xFF {
            return None; // impossible
        }
        let prog = &self.prog;
        let ninst = prog.inst.len();
        let mut m = Machine {
            prog,
            ncap,
            caps: Vec::new(),
            free: Vec::new(),
            nslots: 0,
            stack: Vec::new(),
            matched: false,
            matchcap: vec![-1; ncap],
        };
        let mut runq = Queue {
            sparse: vec![0; ninst],
            dense: Vec::with_capacity(ninst),
        };
        let mut nextq = Queue {
            sparse: vec![0; ninst],
            dense: Vec::with_capacity(ninst),
        };
        let mut scratch = Vec::with_capacity(ncap);
        let mut pos = pos;
        let (mut r, mut width) = step(s, pos);
        let (mut r1, mut width1) = (-1, 0);
        if r != -1 {
            (r1, width1) = step(s, pos + width);
        }
        let mut flag = if pos == 0 {
            LazyFlag(-1, r)
        } else {
            context(s, pos)
        };
        loop {
            if runq.dense.is_empty() {
                if self.cond & EMPTY_BEGIN_TEXT != 0 && pos != 0 {
                    break; // anchored match, past beginning of text
                }
                if m.matched {
                    break; // have match; finished exploring alternatives
                }
            }
            if !m.matched {
                let mut cap = std::mem::take(&mut m.matchcap);
                if ncap > 0 {
                    cap[0] = pos as i64;
                }
                m.add(&mut runq, prog.start, pos, &mut cap, flag);
                m.matchcap = cap;
            }
            flag = LazyFlag(r, r1);
            m.step(
                &mut runq,
                &mut nextq,
                pos,
                pos + width,
                r,
                flag,
                &mut scratch,
            );
            if width == 0 {
                break;
            }
            if ncap == 0 && m.matched {
                break; // any match will do
            }
            pos += width;
            (r, width) = (r1, width1);
            if r != -1 {
                (r1, width1) = step(s, pos + width);
            }
            std::mem::swap(&mut runq, &mut nextq);
        }
        if m.matched {
            Some(m.matchcap)
        } else {
            None
        }
    }
}
// ---- Generated by work/regex/unigen from Go go1.25.14's unicode package (Unicode 15.0.0). ----

/// (kind:name, ranges) for unicode.Categories (c:), Scripts (s:), FoldCategory (fc:)
/// and FoldScript (fs:); ranges are (gap from previous end, length - 1) pairs.
static UNI_TABLES: &[(&str, &str)] = &[
    ("c:C", "AbFdRbGNAbbMBGDHABAUApHAbMBbYBDAcDHbBDGQWAhKAbWBcHBdXNcHBbXBPAbCBBALEbFIcWAgFAIBCBWAHABCEBJBCBEH\
    BDCAFBZBDAGDCBWAHACACACBBAFDCBDCBGEABGRJDAJADAWAHACAFBKADADBBOEBMGHADAIBCBWAHACAFBJBCBDGDDCAFBSJ\
    CAGCDAECCABACCCCDCMDFCDAEBBFBNVENADAXAQBJADAEGCADBBBEBKGWADAXAKAFBJADAEGCFCAEBKADLNADAbZADAGDQBb\
    AADASCYAJABBHCBDGABAIFKBDLcGDbDbKCABAFAYABAXBFABAHAKBEbFcUAbKDbNAbKAPANbKhQABEBBoNAEBHABAEBbPAEB\
    bHAEBHABAEBPAcFAEBcPBbGCbAFdIBGBzTCdLGWIYIULNADACLdQBKFKFOALFdLGbREcSJbFAMDMDBCbQBFKbSDbAFLCcKBc\
    NAbDBLFKFOBbFbWcZCbVAeMHcICPCcIGbRBLHbREuOBGBbMBGBIABABABAbFBcBAPAOBGATBDAJALEbAEbXPCBbBANCbHObH\
    OfKDzNYLUcrSBbGAnLEbTABEBBcEGCNYIHAHAHAHAHAHAHAHAeWbHbAAdLLiGZMDcMAdIBdZEbRAdQAdGLbVAbriRCcDInKT\
    hCHhVECABAFXcHCKFcEHcSHMFeMKbECdAALDbHAcDIOBKBdZXbCJGBGBGIHAHAcIDeWBKFqnSLXDbXmnBoCBeCbLHLFEbAAF\
    ABACACAeVPrDBcCGBbFbQFbZATAEDFAfFDhICGBGBGBDCHAHMCBMAbAATACAPBObHeTEDDbTCdKANCBbUbUeZbDCbXObCDbK\
    IbEEbREbEAbLDObPgCBKFbKDbKDbOHcAKMAPAHACALAPAHACcOlZIWJIXGAbQAJcQGBBAbSACCBBXAcUHJbVTACEbHCbBEBc\
    LcEDUBbYACEIADAbDBDDKGJGcMbFbNDMIcCCbDBbBEbAGELHdBcVcCbZMbZGbUHKlHbFAbQADBCcWbRHbQVbAbLbCTXIdADb\
    KIcKAFMZGKFcBASHbNIdSAUKSAbVcJHABAEAPALFcHEKFEAIBCBWAHACAFAKBCBDBBFBEHBHCFfIdOAFbDcUHKgJcCBbMbHc\
    RKKFNScGFKcBbBBPDXhCcIdVdFLIBBBIACAbEACBMIKcRIBbUBLbAcUHdFMcVGKjLJAbTAOJbDCbGBWAOcUHACAbSCBACAJH\
    KFGACAbLACAGGKlXZGRAbPCbCdHBObYMbjNdXeHAFKhOdxJdVMbpGPWfyFwLmuAvXGbFAKDdDAKFbEBGJcSJKAHAVETbaLdN\
    dWcXDcFGRcLFKCNjcAHbvQbPJnfQEAHACAlFOBbCDBBNEHpGdkTeDENCJGKBEgyXbUBXIeMcHjMJbNBcWHeIUcSeRULULdJI\
    ZfEdHAcTACBBBCBEAMABAHAcNAEBIAHAbCAEAFABCHAnCBlGBbbAOFAPbqLbFFGiEHARBHACAFEcKbGBeHbTCOBKDCmHbFQc\
    GEBrVbQbcNHAEACAPAhPBQbOcYDKDCbeEcQcXcJhLEAbBACABBBAKAEABABFBDBABABADACABBBABABABABACABBEAHAEAEA\
    BAKAREDAFARbZCkJbSDdWLPBPAPAbLJgScDbDMbSDJGCNGfXblWDRCNCePDdRFMDBOMDcEHKFbOHbEBCcZnCLOBNCJGbUAHH\
    ODJGJGfRAcDbKKbnPclfCbFgdUFiOBinQNlbLepIuWcfXhhZEgfGbonhFjGlecT"),
    ("c:Cc", "AbFdRbG"),
    ("c:Cf", "gRAcaKFWAhKAbXAoUBdCAftJAdaQEbAEbXEBJdghJAjPCgiZAPAnkKPbzqUDhwXHbtkzMAbEdR"),
    ("c:Cn", "biEBGDHABAUApHAbMBbYBDAcDHbBDGKkKAcIBdXNcHBbXBPAbCBBALEbFACFjCAIBCBWAHABCEBJBCBEHBDCAFBZBDAGDCBW\
    AHACACACBBAFDCBDCBGEABGRJDAJADAWAHACAFBKADADBBOEBMGHADAIBCBWAHACAFBJBCBDGDDCAFBSJCAGCDAECCABACCC\
    CDCMDFCDAEBBFBNVENADAXAQBJADAEGCADBBBEBKGWADAXAKAFBJADAEGCFCAEBKADLNADAbZADAGDQBbAADASCYAJABBHCB\
    DGABAIFKBDLcGDbDbKCABAFAYABAXBFABAHAKBEbFcUAbKDbNAbKAPANbKhQABEBBoNAEBHABAEBbPAEBbHAEBHABAEBPAcF\
    AEBcPBbGCbAFdIBGBzTCdLGWIYIULNADACLdQBKFKFbAFdLGbREcSJbFAMDMDBCbQBFKbSDbAFLCcKBcNAbDBLFKFOBbFbWc\
    ZCbVAeMHcICPCcIGbRBLHbREuOBGBbMBGBIABABABAbFBcBAPAOBGATBDAJAdXAMBbBANCbHObHOfKDzNYLUcrSBbGAnLEbT\
    ABEBBcEGCNYIHAHAHAHAHAHAHAHAeWbHbAAdLLiGZMDcMAdIBdZEbRAdQAdGLbVAbriRCcDInKThCHhVECABAFXcHCKFcEHc\
    SHMFeMKbECdAALDbHAcDIOBKBdZXbCJGBGBGIHAHAcIDeWBKFqnSLXDbXDnbABeCbLHLFEbAAFABACACAeVPrDBcCGBbFbQF\
    bZATAEDFAfFBBAhICGBGBGBDCHAHJFBMAbAATACAPBObHeTEDDbTCdKANCBbUbUeZbDCbXObCDbKIbEEbREbEAbLDObPgCBK\
    FbKDbKDbOHcAKMAPAHACALAPAHACcOlZIWJIXGAbQAJcQGBBAbSACCBBXAcUHJbVTACEbHCbBEBcLcEDUBbYACEIADAbDBDD\
    KGJGcMbFbNDMIcCCbDBbBEbAGELHdBcVcCbZMbZGbUHKlHbFAbQADBCcWbRHbQVbAbLbCTXIdADbKIcQJBBZGKFcBASHbNId\
    SAUKSAbVcJHABAEAPALFcHEKFEAIBCBWAHACAFAKBCBDBBFBEHBHCFfIdOAFbDcUHKgJcCBbMbHcRKKFNScGFKcBbBBPDXhC\
    cIdVdFLIBBBIACAbEACBMIKcRIBbUBLbAcUHdFMcVGKjLJAbTAOJbDCbGBWAOcUHACAbSCBACAJHKFGACAbLACAGGKlXZGRA\
    bPCbCdHBObYMbjNdXeHAFKhOdxJdVMbqSfyFwLmuAvXGbFAKDdDAKFbEBGJcSJKAHAVETbaLdNdWcXDcFGRcLFKCNjcAHbvQ\
    bPJnfQEAHACAlFOBbCDBBNEHpGdkTeDENCJGKBIgyTbUBXIeMcHjMJbNBhMUcSeRULULdJIZfEdHAcTACBBBCBEAMABAHAcN\
    AEBIAHAbCAEAFABCHAnCBlGBbbAOFAPbqLbFFGiEHARBHACAFEcKbGBeHbTCOBKDCmHbFQcGEBrVbQbcNHAEACAPAhPBQbOc\
    YDKDCbeEcQcXcJhLEAbBACABBBAKAEABABFBDBABABADACABBBABABABABACABBEAHAEAEABAKAREDAFARbZCkJbSDdWLPBP\
    APAbLJgScDbDMbSDJGCNGfXblWDRCNCePDdRFMDBOMDcEHKFbOHbEBCcZnCLOBNCJGbUAHHODJGJGfRAcDbKKbnPclfCbFgd\
    UFiOBinQNlbLepIuWcfXhhZEgfGbomxKBbDdSeXjGdsfNdsyOBdsyOB"),
    ("c:Co", "dgvOjmDcahxOdsyNCdsyN"),
    ("c:Cs", "dduUdaT"),
    ("c:L", "cNZGZbVAKAEAFWBbEBrPELOEHABAeZEBBCDBAGABCBABTBdEBfIIgJBbLCAGbOcTbAEDbTbQbJBBdUBAPBHBKCCAQABbDbDd\
    KLAYbGJBEAFVEAJADAXYHKFXBFRbPcGcBDASAHJPPEHCBCVBGBADDDAQANBBCOBKAIFEBCVBGBBBBBBbFDBATCQIBCBVBGBB\
    BEDASAPBXALHCBCVBGBBBEDAbEBBCPARABFDCBDDBBABBDBDCDLWAcAHBCBWBPDAbACCACBbEAEHBCBWBJBEDAbFBBBPBRIB\
    CBbOCAQAFCICYFFRDXBIBACGcGbVBBMGcGBBABEBXBABJBBJACEBAVDbGAcLHBbJbBEeLbQUAQFEDDADBHCEMMARbLBAFACb\
    QBmUBDCGBABDCbOBDCbGBDCGBABDCOBcEBDCcObLPQdHCFDxVCQBZFcWGHHRNSOROMBCPbZbJAEAcPdKHECbHBAFcRKbEbXb\
    DCELbREZcCWJcAdEAdPbURHcCbDNBKbRbAbJbPCKbJCIHbQCCbPDBFBBDAFhJcMkRCFCbLCFCHBABABABbECcABGBADCBGDD\
    CFEMFCBGeMANAQMdXAEACJBADEGABABABDBKCDFEEAcABdzFiUGDDBMbLBAFACcDHAQWJGBGBGBGBGBGBGBGdCAsBBbQEFBE\
    dHGCBdLBDFbQBdPRbFbWPtSjtNcMbguEcPbTCkIDPKBUbUQbECcRbXICdYCcLFBBABEYPBCBDBWbDbZObXcKFDABBLbBKWZb\
    CHbUbCAQEBJKEBbOXCBHUWDADbXBADBCECABAYCCKHCMFCFCFJGBGBbQBNGeKbDqnRMWEbWmnCoBCeBbMGMEFABJBMBEBABB\
    BBBeDbHnYScLCcBbOLeMEBfEbKZGZLdKDFCFCFCCbJLBZBSBBBOCNbIeSoZbCDbWbVbFNTBHGbLKbDCbJEHbWgBSbJEbJEbN\
    IbZMKBOBGBBBKBOBGBBcPlYJVKHYFBbPBIcRFCABbRBBDACWKWJbEcNSBBKVKZcScDGBcMAPDBCBbCbQbCDbCbJHBbBbBcBK\
    VKSNReGcUcDbYNbYNbJnKbPGBdAbCKAIVbQRbUUbBWMcAcFBCANbSbGYbAbJbDACAIbIDAMbVODVABAbJRBYTBcLGBABDBOB\
    JHbUbMHCBCVBGBBBEDASAMEgCcASDUCbEbVUBBAhCbUbPDbKbVUAcHbQNAcTbAbLGhDbReMcLbFHCACHBBBXPABAdQHCbMQA\
    BAbCAKbNHAVALbTTAScUkDIBbKRAbXbDeIGBBBbLVAZFBBBbFOAmPSPABMBbHeUAdBbjLiWhNdxKdSPbpFRFfyVwKmuBvWHb\
    ERdARbDSbVQDbFUFSbaMcLeYcWFAcOMcMBBAbCjbZIbvPbQInfRDBGBBBlEPAbDCCAODIpFdkUeCFMDIHJiwKdGBcSBBCACB\
    CDBLBABGBcMBDCHBGBbBBDBEBADGBnBCYBYBbEBYBbEBYBbEBYBbEBYBHcsYbEGFkBcJfQbSKGQAmJbDSbRsQbBbdCGBDBBB\
    OBhOcHcPHAbuIDBbABBBACABJBDBABAGAEABABABCBBBACABABABABABBBACDBGBDBDBABJBQFCBEBQgoAclfBbGgdTGiNCi\
    nPOlbKepJuVcfYhhYFgfF"),
    ("c:LC", "cNZGZcGAKWBbEBhMBDEhZBbAhKDCBDCBAGABCBABTBdEBfIIgJBbLJbOefFbLBAFACbQCCzWdHCFdfUIHbQCCcMbRcLMBbHd\
    XkRCFCbLCFCHBABABABbECcABGBADCBGDDCFEMFCBGkBAEACJBADEGABABABDBFEACDFEEAcABdzFeTCdYGDDBMbLBAFAbtw\
    CbTSbBfEcZBWDDBcGFBBABEbBBDAbfPbQFIHdBbeaIGMEbnTZGZbtTdBdSbJEbJeMKBOBGBBBKBOBGBBcoPbYNbYekZcLbgi\
    QcLbmlKdGBcSBBCACBCDBLBABGBcMBDCHBGBbBBDBEBADGBnBCYBYBbEBYBbEBYBbEBYBbEBYBHcsYJBTGFdsVcP"),
    ("c:Ll", "dTZcGAbPXBHBABABABABABABABABABABABABABABABABABABABABABABABABABABABBBABABABABABABABBBABABABABABAB\
    ABABABABABABABABABABABABABABABABACABABCCABACADBEACADCCACABABACABBBACADABACBCCGACACABABABABABABAB\
    ABBBABABABABABABABABBCABADABABABABABABABABABABABABABABABABABABABABABABABABABABABABABGCACBBAEABAB\
    ABABcQBbAhLABADADCSAbBbIBBDCBABABABABABABABABABABABEBACACBbZbVBABABABABABABABABABABABABABABABABA\
    JABABABABABABABABABABABABABABABABABABABABABABABABABABACABABABABABABBBABABABABABABABABABABABABABA\
    BABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABAbWbOehBbQCCbdGFdfUIePbRcLMBb\
    HdYABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABA\
    BABABABABABABABABABABABABABABABABABABABABABABABABABABABIBABABABABABABABABABABABABABABABABABABABA\
    BABABABABABABABABABABABABABABABABABABABABABABABABABABABABIIFKHIHIFKHIHINCHIHIHIEBBGADCBBIDCBIHKC\
    BBkOADBDAbBAEAEACBIDEAcBAebBbVBADBBABABAEABBBFFABABABABABABABABABABABABABABABABABABABABABABABABA\
    BABABABABABABABABABABABABABABABABABABABABABABABABBHABAEAMbLBAFAbtwDABABABABABABABABABABABABABABA\
    BABABABABABABABATABABABABABABABABABABABABABAfFABABABABABABCBABABABABABABABABABABABABABABABABABAB\
    ABABABABABABABABABABABABABHBABACABABABABAEABACABCBABABABABABABABABABAFAFABABABABABABABAEABAGABAB\
    ABABAbCADAbfPbQFIHdBbeaIGMEboZZbvHbNfGbJfZKBOBGBBcrBbYemFbFbgjWbFbmmKZbAGBRbAZbADBABGBKbAZbAZbAZ\
    bAZbAZbAZbAZbAZbAbBbCYBFbAYBFbAYBFbAYBFbAYBFBAcsYJBTGFduDbH"),
    ("c:Lm", "baMRELOEHABAfDAFAsKAiWAgIBkJBEAbFAJADAgEAgLAbvOAeXAvTAcpMAeDAxNArWFgScKNAbIbKbaNANAQMemXBjHAhJAs\
    BAbREFAdTBdPCbqbUAbwCFkKAeKAbCBeRIdCAXAeBCDBsBAWAfHAeEAVBdZDJAbfvIAbTBczOFBbPBIblrXDbqLMcMBBAygQ\
    DBGBBsgNcJhTGbkFAbrBA"),
    ("c:Lo", "gOAPAjWAEDiAAbfVbAEDbTbFBJbJBBdUBAYBKCCAQABbDbDdKLAYbGVVbQYHKFXBFRbOcHcBDASAHJQOEHCBCVBGBADDDAQA\
    NBBCOBKAIFEBCVBGBBBBBBbFDBATCQIBCBVBGBBBEDASAPBXALHCBCVBGBBBEDAbEBBCPARABFDCBDDBBABBDBDCDLWAcAHB\
    CBWBPDAbACCACBbEAEHBCBWBJBEDAbFBBBPBRIBCBbOCAQAFCICYFFRDXBIBACGcGbVBBMFcHBBABEBXBABJBBJACEXDbGAc\
    LHBbJbBEeLbQUAQFEDDADBHCEMMAeJmQBDCGBABDCbOBDCbGBDCGBABDCOBcEBDCcObLPeJxVCQBZFcWGHHRNSOROMBCPbZb\
    OAcPbIBcAHECbHBAFcRKbEbXbDCELbREZcCWJcAgUbURHcCbDNBKbRbAbJbPCKbDeJDBFBBDAbpQDenVcDYWJGBGBGBGBGBG\
    BGBGvFAcBAEdHIABdLEAFbQBdPRbFbWPtSjtNcMbfcEBbrYcPbNIkHEPKBcOAbXcRgNAdZADGBCBDBWbDbZObXcKFDABBLbB\
    KWZbCHbUbTECIKEBbOXCBHUPBFDADbXBADBCECABAYBDKHAOFCFCFJGBGfPbIbDqnRMWEbWmnCoBCeBcPABJBMBEBABBBBBe\
    DbHnYScLCcBbOLeMEBfEeBJBbSCbEDFCFCFCCbJLBZBSBBBOCNbIeSoZbCDbWbVbFNTBHGbLKbDCbJEHeYcZdUbNIbZgAlYJ\
    VKHfWFCABbRBBDACWKWJbEcNSBBKVKZcScDGBcMAPDBCBbCbQbCDbCbJHBbBbBcBKVKSNReGcUhBbJnKbPGBdAbCKAIVbQRb\
    UUbBWMcAcFBCANbSbGYbAbJbDACAIbIDAMbVODVABAbJRBYTBcLGBABDBOBJHbUbMHCBCVBGBBBEDASAMEgCcASDUCbEbVUB\
    BAhCbUbPDbKbVUAcHbQNAcTbAbLGhDbRiDHCACHBBBXPABAdQHCbMQABAbCAKbNHAVALbTTAScUkDIBbKRAbXbDeIGBBBbLV\
    AZFBBBbFOAmPSPABMBbHeUAdBbjLiWhNdxKdSPbpFRFfyVwKmuBvWHbERdARbDSbVbZUFSbhWcWFAgTjbZIbvPbQIngHlEPA\
    bDCCAODIpFdkUeCFMDIHJnbCAtHbSbHAmJbDSbRsQbAbdDGBDBBBOBhObzNDBbABBBACABJBDBABAGAEABABABCBBBACABAB\
    ABABABBBACDBGBDBDBABJBQFCBEBQgoAclfBbGgdTGiNCinPOlbKepJuVcfYhhYFgfF"),
    ("c:Lt", "rLACACAbMAlfHHIHIHMAPAbVA"),
    ("c:Lu", "cNZdXWBGbHABABABABABABABABABABABABABABABABABABABABABABABABABABABACABABABABABABABACABABABABABABAB\
    ABABABABABABABABABABABABABABABABBBABADBBABBBCCDBBBCDBBBBABABBBACABBBCBABBDAHACACACABABABABABABAB\
    ACABABABABABABABABACACABCBABABABABABABABABABABABABABABABABABABABABABABABABABABABABAHBBBCABDBABAB\
    ABAlDABADAIAGABCBABBBQBIbJACCDABABABABABABABABABABABAFACABBCbYbWABABABABABABABABABABABABABABABAB\
    AJABABABABABABABABABABABABABABABABABABABABABABABABABABABBBABABABABABACABABABABABABABABABABABABAB\
    ABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABACbLehDbLBAFAbbUdHdgSbQCCmIA\
    BABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABA\
    BABABABABABABABABABABABABABABABABABABABABABABABABABAJABABABABABABABABABABABABABABABABABABABABABA\
    BABABABABABABABABABABABABABABABABABABABABABABABABABABAJHIFKHIHIFLABABABAIHcUDMDMDMELDkCAEADCCCCA\
    DEGABABABDCDKBFAcJAdzGbVbWABCCABABABDBACAICBABABABABABABABABABABABABABABABABABABABABABABABABABAB\
    ABABABABABABABABABABABABABABABABABABABABABABAIABAEAbtyJABABABABABABABABABABABABABABABABABABABABA\
    BABATABABABABABABABABABABABABABAfFABABABABABABADABABABABABABABABABABABABABABABABABABABABABABABAB\
    ABABABABABABAKABABBBABABABAEABACABADABABABABABABABABABABEBEBABABABABABABABDBAGAFABAbCAbhaHZbuZbN\
    fGbJgAKBOBGBBcqCbYenLbFbgjWbFbmmQZbAZbAZbAABBCACBCDBHbAZbABBDCHBGbBBBDBEBADGbBZbAZbAZbAZbAZbAZbE\
    YbHYbHYbHYbHYbHAgnLbH"),
    ("c:M", "bdOeHkPGkDbSBABBBBBAcUKbWUQAdXGCFCBBDbJAbEbAdNKcGIJAYDBIBCBEbRCcIHbQXBbGcCCBRBGKBbDCcEABGCBCCJAK\
    BbAACCcEABEEBCCDAbEBDALCcEABHBCBCUBWFBCcEABGCBCCHCKBbEAcHEDCBDJAbOEcDABGBCBDHBLBbDCcEABGBCBDHBLB\
    PAMDcDBBGBCBDJAKBbDCcSAEFBABHSBcJACGMHdUACILGcVBbBABABAEBbXTBBFKBbJJAdWTXDECBCCGDDNLBAKDbbBCbkKD\
    bCCbDBbEBcMbFJAbTCBAeNBbIAeOLELiLEcFJBbCCAbWbEbXEbVQbMIMCbEMcENbWTfWCBUEAGACChQcLbbSbGeoCCfLAdSb\
    FvIFeBBbsqEDBJbGBdCBkMADAEAXEEAdFBbYRbARNAbMHZMbSDbVNbKAcPNMAIBbTCbYABCCBFBBAbPEFBjCHBBbdzSAbcJP\
    QPblLAiSAfTEcmGCBBFDbOCEAgJBwBDoXBdCCcSKbXDeSCcBObPACBKDbTKHAcJCbKNQBbSAMCbWNIDBBdOLGACAgBLVDcDB\
    BGCBCCJAKBCGDEhKRXAdDTjBGCIbBBdEQeCMdXOjWOjLFBBCDBABBfLGCGDAbCJbOGBDIAJKbUPpPHBHdEVBNeSFDABBBGBA\
    cOEBBBEnJDJBBAbWGDEhyRAGOurQEcHGboIABcCHDdDALBbdaXBgyZbTCWuWEDFIHCGbEDfSCcyDcCEbXIAOAWEBOcaIGBQC\
    GBBBEdWAgEGoLAcJDtODbmEGeFGbtciVjF"),
    ("c:Mc", "dkTAcDACCIDBBbYBcGCGBCBKAbRAcGCcOAcGCIABBcBBcGABAGBCBKAdYBBBDCBCKAbPCcJDcJBcGABECBBBJBbCAOBcGCFC\
    BCKAbQBcXCGHSBmSBcLAgPBEAGACBZBKCCGVBCFCAKCclSAbEAeZAHHBBnIDCCEBBFiQBcGABAJABBIFfPAbWAFABEBBcJAb\
    EAEBCAcIACCBADBbWHIBgPAVAhhEBbtlBBCAdKBbYPfMBbVAbWBEBCCeGBCBYAbTABAeFACBFAjDBBBBBBAblxJABAeXAbTC\
    EBeLAYBcHAbWCJBNAdPCDBBAgOCbFBcGBBDCBCCJAKBiBCIBDAeCCGABDCAjDCGDCAeJCIBBAeFABBGAeBBEAkBCJAjNFBBE\
    ACABAfMCIDEAdGAbDBcKApRAOAeCAHACAiFEEBBAnMBMAbWBIBBAbeiHcCeABbkxNBGF"),
    ("c:Me", "bsQBikQAciGDBCbyoXC"),
    ("c:Mn", "bdOeHkPEkFbSBABBBBBAcUKbWUQAdXGCFCBBDbJAbEbAdNKcGIJAYDBIBCBEbRCcIHbQXBbFcDABAEHEADGKBbDAcGAEDIAU\
    BbAACBcFAEBEBCCDAbEBDALBcFAEEBBEAUBWFBAcGACABDIAHBLBbEAcJAMAbYADAcDABCFCBDHBLBbDAcGACAGAFBUBbCBc\
    FBEDIAUBbDAcUAHCBAdMACGMHdUACILGcVBbBABABAcDNBEBBFKBbJJAdYDBFBBCBZBECQDNACBGAPAbbBCbkKCbDBbEBbEB\
    cMBBGIACKJAbTCBAeNBbIAeOCEBJAGCiLBCAcGABGBABACHGJCAbWNBPbXDbWABEBAFAbOIMBbGDCBBCcEABBDABCcGHCBfW\
    CBMBGEAGADBhQcLbbSMEADLeoCCfLAdSbFvIDeDBbsqEAEJbGBdCBkMADAEAZBFAfVBbARNAbMHZKbUCbWACDCBbNAcPFCBC\
    BMAIAbVAbZABCCBFBBAbQBIAjEACAEAbdzSAbcJPQPblLAiSAfTEcmGCBBFDbOCEAgJBwBDoXBdCCcSKbXDeTAcCObPACBKC\
    bXDCBHAcJCbKEBHcKAMBcAIKDCAdRCCABBGACAgBADHVBcFBDAbLGDEhNHCCBAXAdGFBAEBBBjEDGBBBbBBdHHCABBeCABAC\
    FBAdXCCDBEjZIBBjWBBAEAfODCBEAbGJbOFCDIAJFCCbUMBBpQGBFBAdEVCGBBBBeSFDABBBGBAcUBDABAnJBLBcAEFABAhy\
    RAGOurQEcHGboIAcLDdDAbdbKBgyZbTCWuYCRHCGbEDfSCcyDcCEbXIAOAWEBOcaIGBQCGBBBEdWAgEGoLAcJDtODbmEGeFG\
    btciVjF"),
    ("c:N", "bWJeQBFACCcdLJfEJhQJpWJeOJEFeEJeOJeOJCFeGMeLJIGdZJeAGHSeFJdSJeOJcSTkIJcSJbbRTbhXCjFJGJWJlOJeYKgJ\
    JGJhAJdIJfEJGJboGADFGJhQbYCEbbYcHdAVyGbDcbHAbdXAZIOCnFDfIJbEHBObGJbNObrsAJhGJmIFfYJbMJhQJWJdIJpQ\
    JbfmCJsZbSMcERBnDbAbKDbDAIAfEEhUJbkGHZGbNIcXEWFgEBCPCbTcMIcABbECcXEeAHYHbPGmSFbWJlIbEgCJbQDeIGfE\
    bDeYJcIJfOJHTjRJnEJeOJoKJdYJdYLqESdPJbdEbCiTJcSJqGJdYUbpBeGbamRJdIJfEJBGbeSWblyFTMTeEYbqRbXdnCJq\
    GJtIJblLIeYJbeLcGBCBDcYbSBOblAMedFJ"),
    ("c:Nd", "bWJciOJfEJhQJpWJeOJeOJeOJeOJeOJeOJeOJeOJeOJdSJeOJcSJkSJcSJctQJbMJlOJeYJgKJGJhAJdIJfEJGJcaeOJbaCJ\
    bMJhQJWJdIJpQJbfmCJccKJdfYJbfGJeYJcIJfOJkSJnEJeOJoKJdYJdYJqGJdYJbdEJjMJcSJqGJbckSJdIJfEJbpbWbXdn\
    CJqGJtIJbqSJhbAJ"),
    ("c:Nl", "irUCdyTbICDfmSAZIOCbsyLJbifGcArSAIAfEEmeSeG"),
    ("c:No", "gWBFACCdmRFoMFeQCfDGiJGRIqRJbpLTbrVJsMAcmVADFGJhQPbPAbbYcHdAVyGbDcbHAbtCDfIJbEHBObGJbNObsmIFbhqN\
    bScNDRBnDbAbKDbzGHZGbNIcXEWFgEBCPCbTcMIcABbECcXEeAHYHbPGmSFnObEgCJbQDeIGfEToPTbzXBqOIbhNSbgTUbcp\
    QGbeSWblyFTMTeEYibUIbjTcGBCBDcYbSBOblAM"),
    ("c:P", "bHCBFBDKBDBbACBAbBABAbJAFADAKBDADAbbAAIArYFbPBbZABACACAbSBUBBBNABCcWDdYAbRNiZCcCObFAkBBKAfKAeQAe\
    RApAAMAoDAdMAKBgMOBAbLDcTAcWEEBeHFgPAxOIfVAxXAbSBdACcTBgBCBCbLKmBBiIBeYGBFgQGbCBeVDcHEcKBcMHLAbf\
    WXITBMBLbEBOByJDbDBbpTNdBBbFJpNVcLDbGBbdJDBBeIAfNbUBbFCLqDCEJCLQAMAdUAdMAbryGBkJCdVAKAeLFoQDdIBb\
    OCBAbXBbVAdTMQBeUDeWBQBjPAbeuSBiAJWbIBNBAEABBfTCBFBDKBDBbACBAbBABABGpUCzSAbWApYAbcPAhRAbFAkMIbMA\
    eIGcOGdLDbeEAgLEbSDhHGeFBBDeWDbWBdBDEANABCdKFeDAqBEKBBAeAAjQWeBCbCMcYAfACjSAkECfZAdOHdFCBEdPJlZE\
    bQByVBcWMgTAbrSEejCBwdVBfDAcNEIAbgSDmPAbdbOAliLEfpYB"),
    ("c:Pc", "dRAmbVBTAdgaOBYCjFA"),
    ("c:Pd", "bTAcaUAbZAfkJAbnPAdbDFfhXACAbFBEAbCArEATAeHAczwABbLAKAgNAfxVA"),
    ("c:Pe", "bPAbZAbFAfpDABAcuOAdrDAcDAPAyKABAbEAbpUABABABABABABAdCAbGABABABABApOABABABABABABABABABABAcMABAbH\
    AboVABABABAbSABABABAqMABABABABADABABABACBczrQAiJAbDABABABABABABABADARABABAgOAbZAbFACACA"),
    ("c:Pf", "hFAlwVADAbCAffSABAEACAPADA"),
    ("c:Pi", "gPAlxKACBCAZAffSABAEACAPADA"),
    ("c:Po", "bHCBCCABABBKBDBbBAcQAFAOBHAbbAAIArYFbPAcCACACAbSBUBBBNABCcWDdYAbRNiZCcCObFAkBBKAfKAeQAeRApAAMAoD\
    AdMAKBgMOBAeIAcWEEBeHFgPAxOIbdTAeUCcTBgBCBCbLFBDmBBiIBeYGBFgQGbCBeVDcHEcKBcMHLAbgCBIHIICDCCDKBAB\
    JeuCDBBeIAfNBECCACIBBBACBKEBJCDBABMCCqMCcFAhHAbryGBkJCdVAKAeLFoQDdIBbOCBAbXBbVAdTMQBeUDeWBQBjPAb\
    fcUGCAWAUBCDDCBDHCGABBfTCBCCABABBKBDBbBAbKACBpUCzSAbWApYAbcPAhRAbFAkMIbMAeIGcOGdLDbkQEbSDhHGeFBB\
    DeWDbWBdBDEANABCdKFeDAqBEKBBAeAAjQWeBCbCMcYAfACjSAkECfZAdOHdFCBEdPJlZEbQByVBcWMgTAbrSEejCBwdVBfD\
    AcNEIAbgSDmPAbdbOAliLEfpYB"),
    ("c:Ps", "bOAbYAbFAfpEABAcuOAdpMADAbMAcDAPAyKABAbEAbpUABABABABABABAdCAbGABABABABApOABABABABABABABABABABAcM\
    ABAbHAboVABABABAZASABABABAqMABABABABADABABABACAczrTAiHAbDABABABABABABABADARABABAgOAbYAbFADACA"),
    ("c:S", "bKAGAQCbFABAbBABAbJEBBCABDCADAbEAbFArQDMNFGBABQeNAOBeIAfJAkGCeOCCACBhYAKATBjNAHBfGAnXBGBjLAeWAfA\
    HfCAhZAbPAhPAhLCPABCCFUABABAfDHBFBBFDhPBbcYJbbVAoBAnSAgBbHnPJJIbpWABCLCNCNCNBcRANAbNCNCTbGcLBBDB\
    BKABCFFBABABAEALBEEFDBAcGBEoLEbCCjRZKdDcZWxRbSbWCbEKpMWcKEbFCoJCbFBeAiVFnTBbUZBdKMiFbALIANBMAVBG\
    BdNBjJBEJbGbJbCbELbDIAPbFKbMPmHjtOcLbguIcCvXWJBdZBgBDKDwBCiRAOBbefDAfGQoRPeXAbSDdUABCCAfYAGAQCbF\
    ABAbBABAeZGBGNBmBIcFQCCBMDAbVbSclUBwTAesSAdgNbCbcnIDFAbeuWAhcDeLcIjLKbMCcHFCWBHbDEcIVcNDAhEdIbhQ\
    AZAbFAZAbFAZAbFAZAbFAZAcItRcDDbYHBNBBcoUAqPAdrGADAeVArHBkKbREdVMOCOBOBbKXgEcEbCNbREIHBOFfYblVEQD\
    MDeOEdQGLEAPLEcDIJGbNIbDCBdAnBMNCMDIHbTBGINEIHIHfQBcC"),
    ("c:Sc", "bKAeVDbwJAeTAtEBtEBHAjLAkDAwJAdqPAdiIbGbzhVAbgmLAeEAfYAiLBDBmcSDcvxEAdrKA"),
    ("c:Sk", "dQABAcTAGAEADAuBDMNFGBABQeNAOBbxIAiuMABCLCNCNCNBghOBbsvRWJBdZBblOAOBbekKQbiHABAgGAdolJE"),
    ("c:Sm", "bRAQCcJABAbTAEAbLAbFAbdMAuHCjyHANAbNCNCfJAbNEGAcQEFBEACACAHAbFBCABAbFkHbGBdMAbEYbOFsBAJAcCHeHAmY\
    ECbEKPjWfAWcKEbFCjXbWUCFdasQAbfSABCgIAQCcJABAfBAGDddjGAZAbFAZAbFAZAbFAZAbFAZAiuEB"),
    ("c:So", "gKACAEABAblPAkGBeXBhYAKATBjNAtVAoJAfAFBAfCAhZAbPApBCPABCCFUABABAfDHBFBBFDhPBbcYJbbVAbbUAgBbHnPJJ\
    IccHBBDBBKABBGFBABABAEALBOABBBAcGBJECDBBBBBGBbECBBABbEkIHETCGCdCBbDZbNGcQZKdDcZWhABIBcBIeGBjNbSb\
    RcMjVtSbVVBGbMCbFBeAiVFnTBbUZBdKMiFbALIANBMAVBGBmYBEJbGbJbCbELbDIAPbFKbMPmHjtOcLbguIcCbhHDKBBAwB\
    CbfjAPeXAbTCsQADAEBNBmBIcFQCCBMDAbVbSclUBwTAesSAdgNHEQbcnIDFAbeuWAhcDeLcIjLKbMCcHFCWBHbDEcIVcNDA\
    hEdIbtXtRcDDbYHBNBBcoUAehWAeZAbbTbREdVMOCOBOBbKXgEcEbCNbREIHBOFfYjQFbbZEQDMDeOEdQGLEAPLEcDIJGbNI\
    bDCBdAnBMNCMDIHbTBGINEIHIHfQBcC"),
    ("c:Z", "bGAeXAihJAdpNKbDBFAbVAfxWA"),
    ("c:Zl", "meQA"),
    ("c:Zp", "meRA"),
    ("c:Zs", "bGAeXAihJAdpNKbKAbVAfxWA"),
    ("s:Adlam", "hdeUcXEJEB"),
    ("s:Ahom", "ebrCbACOEW"),
    ("s:Anatolian_Hieroglyphs", "essEwK"),
    ("s:Arabic", "chCEBFBNBCBbFBJLZBeDBbHdCbVjGbEBBGcVBbCdntWeKQnYCdBCcBHAbGPeIEBfEfvNbEeWCdglSDBbABBBACABJBDBABAG\
    AEABABABCBBBACABABABABABBBACDBGBDBDBABJBQFCBEBQcAB"),
    ("s:Armenian", "bzDbLCbXCCdozJE"),
    ("s:Avestan", "dxcYcBDG"),
    ("s:Balinese", "kfWcYDbU"),
    ("s:Bamum", "clcQdJcvcQvW"),
    ("s:Bassa_Vah", "fhkIbDCF"),
    ("s:Batak", "knGbZID"),
    ("s:Bengali", "dpODBHCBCVBGBADDCICBCDIAEBBECY"),
    ("s:Bhaiksuki", "edoIIBbSBNKbC"),
    ("s:Bopomofo", "bcSBrlXbQeIbF"),
    ("s:Brahmi", "dzaEcZEbJJA"),
    ("s:Braille", "pdWjV"),
    ("s:Buginese", "jwAbBCB"),
    ("s:Buhid", "iuYT"),
    ("s:Canadian_Aboriginal", "hoYyPvOcRdtpQP"),
    ("s:Carian", "dtyMbW"),
    ("s:Caucasian_Albanian", "duxSbZLA"),
    ("s:Chakma", "dzkAcABR"),
    ("s:Cham", "cmjWcCJNCJCD"),
    ("s:Cherokee", "hlGdHCFcfjEdB"),
    ("s:Chorasmian", "dyxCbB"),
    ("s:Common", "AcMbAFbAbUBOBEXAbFArHbMFECTeMAJAGABAyNAGAOADAbGAgAAtWAeZBbvTApPDlEAcgLCcTBhVBBAbvHANAHDBFBCCAbdT\
    LCdIBKDKBORbGcLbLBCCFBbABQbPCEzMZKVbjRjWyDCbFBeAtSdPpMLEEBABYPHEDdNBDAdMBfRPbGbJcIcLbFdCbVAdKgLj\
    tOcLbhsIbHdYCgJJjKAgEApFAOBbezQBiAJWbIBSBDfRABbFbAFbAKKAbTBcMGBGKEjYCEbSDIdCMbZbSiUbAcshWDhbWeLc\
    IjLKbMCcJDQIBHbDEcIiFTMTMdIJYfFdGBcSBBCACBCDBLBABGBcMBDCHBGBbBBDBEBADGBnBClFCbXhtHcPcYcIbbEbREdV\
    MOCOBOBbKKgRcEZBBNbREIHBOFfYblVEQDMDeOEdQGLEAPLEcDIJGbNIbDCBdAnBMNCMDIHbTBGINEIHIHfQBcCbLJbsuxBA\
    bEdR"),
    ("s:Coptic", "bmGNpjKeLFG"),
    ("s:Cuneiform", "efbSbjLdYeGBELhN"),
    ("s:Cypriot", "dvzKFCABbRBBDACA"),
    ("s:Cypro_Minoan", "ekyYdU"),
    ("s:Cyrillic", "bnKfCCgMivOIgGAcYAgfNbFbtoAdRbheCBdhpGcJbHA"),
    ("s:Deseret", "dumAdB"),
    ("s:Devanagari", "dkQdCEOCZcijGbFbreMJ"),
    ("s:Dives_Akuru", "eckUGCACHBBBbDBBCLJJ"),
    ("s:Dogra", "ecaYcH"),
    ("s:Duployan", "gmdSeCFMDIHJCD"),
    ("s:Egyptian_Hieroglyphs", "eldGbqR"),
    ("s:Elbasan", "duvWbN"),
    ("s:Elymaic", "dyyYW"),
    ("s:Ethiopic", "gvGcUBDCGBABDCbOBDCbGBDCGBABDCOBcEBDCcOCbFDZjvAWJGBGBGBGBGBGBGBGbvkCFCFCFJGBGepyFGBDBBBO"),
    ("s:Georgian", "ghSbLBAFACbQBDejWbQCCgeAbLBAFA"),
    ("s:Glagolitic", "qrGdRgizGGBQCGBBBE"),
    ("s:Gothic", "dueAbA"),
    ("s:Grantha", "eadSDBHCBCVBGBBBECICBCCCAGAFGCGDE"),
    ("s:Greek", "bhWDBCCDBAEABABCBABTBcKOPjnQEbYEEEdGAmIVCFCbLCFCHBABABABbECcABOBNCFBSCCBIlJAcajEAbgnIdARAdaxRcR"),
    ("s:Gujarati", "dzLCBIBCBVBGBBBECJBCBCCAPDCLHG"),
    ("s:Gunjala_Gondi", "eebWFBBBbKBBBFHJ"),
    ("s:Gurmukhi", "duNCBFEBCVBGBBBBBBCABEEBCCDAHDBAHQ"),
    ("s:Han", "rpWZBdKMiFbVABAZIODblCjtNcMbfbJbhsIoBCeBbsiIBMBccoKclfBbGgdTGiNCinPOlbKepJuVcfYhhYFgfF"),
    ("s:Hangul", "glKjVllEBjXdPeJbEcNbEbtaNbCyTqnRMWEbWpaIbEDFCFCFCC"),
    ("s:Hanifi_Rohingya", "dxwQbNIJ"),
    ("s:Hanunoo", "itSU"),
    ("s:Hatran", "dwiASBBFE"),
    ("s:Hebrew", "ccVcCIbAEFdovWZBEBABBBBBJ"),
    ("s:Hiragana", "shDdHGCfpeVlASAbDCymVA"),
    ("s:Imperial_Aramaic", "dwbWVBI"),
    ("s:Inherited", "bdOeHkRBrKKbAAbcIDgoXbEtTCBMBGEAGADBhQcLuEBhMbGftXDeDBczuJPQNblNAiSAgfAActcUbTCWuYCRHCGbEDbtlhIj\
    F"),
    ("s:Inscriptional_Pahlavi", "dxgQSFH"),
    ("s:Inscriptional_Parthian", "dxfKVCH"),
    ("s:Javanese", "cmeYcZCJEB"),
    ("s:Kaithi", "dzfCcOKA"),
    ("s:Kannada", "etCMBCBWBJBECIBCBDHBGBBDCJBC"),
    ("s:Katakana", "skVdLCCjGPiAbUBdJczhEJBbScouCDBGBBBAlBCbYAOD"),
    ("s:Kawi", "eerWQBbODbB"),
    ("s:Kayah_Li", "cmaAbTBA"),
    ("s:Kharoshthi", "dwtCDBBFHBCBbCCCEJHI"),
    ("s:Khitan_Small_Script", "fjiIAkgXsB"),
    ("s:Khmer", "ixKdPCJGJsSbF"),
    ("s:Khojki", "dztWRBbU"),
    ("s:Khudawadi", "eaaQcGFJ"),
    ("s:Lao", "fmVBBABEBXBABWCEBABGBJCD"),
    ("s:Latin", "cNZGZbVAPAFWBbEBrGbNEjxBbLGbWFDFMBcRcNjVoFANAQMfLBGAbBARbOecTbFbulMdXDcLFBBABEYNbfKbQBIBDbedQGbo\
    KZGZdcFFBbPBIddnPbEGF"),
    ("s:Lepcha", "kpScDDODC"),
    ("s:Limbu", "jmEbEBLELEADL"),
    ("s:Linear_A", "dvfSlYJVKH"),
    ("s:Linear_B", "dsyQLBZBSBBBOCNbIeS"),
    ("s:Lisu", "ckkUbVbumAA"),
    ("s:Lycian", "dtxGbC"),
    ("s:Lydian", "dwkMZFA"),
    ("s:Mahajani", "dznCbM"),
    ("s:Makasar", "eeqQY"),
    ("s:Malayalam", "eyAMBCBbYBCBFEPCZ"),
    ("s:Mandaic", "ddGbBCA"),
    ("s:Manichaean", "dxaMbMEL"),
    ("s:Marchen", "edsQbFCVBN"),
    ("s:Masaram_Gondi", "edyEGBBBbRDABBBIIJ"),
    ("s:Medefaidrin", "fisEdM"),
    ("s:Meetei_Mayek", "cmsMWhTbTCJ"),
    ("s:Mende_Kikakui", "hcuYhOCP"),
    ("s:Meroitic_Cursive", "dwpKXETCbT"),
    ("s:Meroitic_Hieroglyphs", "dwoEbF"),
    ("s:Miao", "fizOcWEcEHQ"),
    ("s:Modi", "ebhGcQLJ"),
    ("s:Mongolian", "jcIBCABTGdKHbQdscBM"),
    ("s:Mro", "fheUbEBJEB"),
    ("s:Multani", "dzyUGBABDBOBK"),
    ("s:Myanmar", "gbOgDcgaYbEdTbF"),
    ("s:Nabataean", "dweIbEII"),
    ("s:Nag_Mundari", "hbpObP"),
    ("s:Nandinagari", "ecqYHCbTCK"),
    ("s:New_Tai_Lue", "jrCbREZGKDB"),
    ("s:Newa", "eanOdNBE"),
    ("s:Nko", "cyIcGCC"),
    ("s:Nushu", "fjiFAyvMpF"),
    ("s:Nyiakeng_Puachue_Hmong", "haeAbSDNCJEB"),
    ("s:Ogham", "inObC"),
    ("s:Ol_Chiki", "ksUbV"),
    ("s:Old_Hungarian", "dxrSbYNbYHF"),
    ("s:Old_Italic", "ducEbJJC"),
    ("s:Old_North_Arabian", "dwyAbF"),
    ("s:Old_Permic", "dufGbQ"),
    ("s:Old_Persian", "duiIbJEN"),
    ("s:Old_Sogdian", "dyqIbN"),
    ("s:Old_South_Arabian", "dwwUbF"),
    ("s:Old_Turkic", "dxmUcU"),
    ("s:Old_Uyghur", "dyuQZ"),
    ("s:Oriya", "eeJCBHCBCVBGBBBECICBCCHCEBBECR"),
    ("s:Osage", "dusUbJEbJ"),
    ("s:Osmanya", "duqYbDCJ"),
    ("s:Pahawh_Hmong", "fhmEcRKJBGBUFS"),
    ("s:Palmyrene", "dwdCbF"),
    ("s:Pau_Cin_Hau", "edcAcE"),
    ("s:Phags_Pa", "clsQcD"),
    ("s:Phoenician", "dwjGbBDA"),
    ("s:Psalter_Pahlavi", "dxhWRHDMG"),
    ("s:Rejang", "cmbWbJLA"),
    ("s:Runic", "ioUcWDK"),
    ("s:Samaritan", "daUbTCO"),
    ("s:Saurashtra", "clvCcRIL"),
    ("s:Sharada", "dzoYdR"),
    ("s:Shavian", "dupCbV"),
    ("s:Siddham", "ebcIcBCbL"),
    ("s:SignWriting", "gwtKzBPEBO"),
    ("s:Sinhala", "fcZCBRDXBIBACGDAEFBABHGJCCdulGT"),
    ("s:Sogdian", "dysEbP"),
    ("s:Sora_Sompeng", "dziEYHJ"),
    ("s:Soyombo", "ecxSdE"),
    ("s:Sundanese", "kkUcLjWH"),
    ("s:Syloti_Nagri", "clqEbS"),
    ("s:Syriac", "cqYNBcHCCkMK"),
    ("s:Tagalog", "isMVJA"),
    ("s:Tagbanwa", "iwEMBCBB"),
    ("s:Tai_Le", "jpGbDCE"),
    ("s:Tai_Tham", "jxGcKBbCCKGJGN"),
    ("s:Tai_Viet", "cmoUcOYE"),
    ("s:Takri", "ebmEcFGJ"),
    ("s:Tamil", "ejIBBFDCBDDBBABBDBDCDLEEDCBDCAGAOUealHbXNA"),
    ("s:Tangsa", "fhgQdABJ"),
    ("s:Tangut", "fjiEAbFjbZIbdNtSI"),
    ("s:Telugu", "eoEMBCBWBPCIBCBDHBBCCACDCJHI"),
    ("s:Thaana", "cvWbX"),
    ("s:Thai", "fhXcFFbB"),
    ("s:Tibetan", "frScTBbJEbMBbJBOBGEB"),
    ("s:Tifinagh", "rcYcDHBOA"),
    ("s:Tirhuta", "easMcTIJ"),
    ("s:Toto", "hatKbE"),
    ("s:Ugaritic", "duhCbDBA"),
    ("s:Vai", "ckmQlN"),
    ("s:Vithkuqi", "dvaEKBOBGBBBKBOBGBB"),
    ("s:Wancho", "havGcFFA"),
    ("s:Warang_Citi", "echCdEMA"),
    ("s:Yezidi", "dylKbPBCCB"),
    ("s:Yi", "cipKbsUDcC"),
    ("s:Zanabazar_Square", "ecuQcT"),
    ("fc:L", "bgFA"),
    ("fc:Ll", "cNZdXWBGbHABABABABABABABABABABABABABABABABABABABABABABABADABABACABABABABABABABACABABABABABABABAB\
    ABABABABABABABABABABABABABABABBBABADBBABBBCCDBBBCDBBBBABABBBACABBBCBABBDAHBBBBBBABABABABABABABAC\
    ABABABABABABABABACBBABCBABABABABABABABABABABABABABABABABABABABABABABABABABABABABAHBBBCABDBABABAB\
    AjMAbQABADAIAGABCBABBBQBIbJAIABABABABABABABABABABABAFACABBCbYbWABABABABABABABABABABABABABABABABA\
    JABABABABABABABABABABABABABABABABABABABABABABABABABABABBBABABABABABACABABABABABABABABABABABABABA\
    BABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABACbLehDbLBAFAbbUdHdgSbQCCmIAB\
    ABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABAB\
    ABABABABABABABABABABABABABABABABABABABABABABABABABAJABABABABABABABABABABABABABABABABABABABABABAB\
    ABABABABABABABABABABABABABABABABABABABABABABABABABABAJHIFKHIHIFLABABABAIHYHIHIHIELELDMELElLADBGA\
    dCAdzGbVbWABCCABABABDBACAICBABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABAB\
    ABABABABABABABABABABABABABABAIABAEAbtyJABABABABABABABABABABABABABABABABABABABABABABATABABABABABA\
    BABABABABABABABAfFABABABABABABADABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABAKAB\
    ABBBABABABAEABACABADABABABABABABABABABABEBEBABABABABABABABDBAGAFABAbCAbhaHZbuZbNfGbJgAKBOBGBBcqC\
    bYenLbFbgjWbFbulKbH"),
    ("fc:Lt", "rKABBBBBAbKABAleYHIHIHLAPAbVA"),
    ("fc:Lu", "dTZcGAbPXBHBABABABABABABABABABABABABABABABABABABABABABABABADABABACABABABABABABABACABABABABABABAB\
    ABABABABABABABABABABABABABABABACABABCCABACADAFACADBDACABABACAEACADABACADABAFBBBBBBABABABABABABAB\
    BBABABABABABABABABACBBADABABABABABABABABABABABABABABABABABABABADABABABABABABABABAIACBBAEABABABAB\
    FBBBABBDBBABBBECABBCAHACABBDFFAKBgKAbRABADADCbUDBbDBBDCBABABABABABABABABABABABEBACACAcAbVBABABAB\
    ABABABABABABABABABABABABABAJABABABABABABABABABABABABABABABABABABABABABABABABABABACABABABABABABBB\
    ABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABAb\
    XbLehDbQCCbdGFdfUIjGADAQAeKABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABA\
    BABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABAFAFABABABABABABA\
    BABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABIIFKHIHIFLABABA\
    BAIHINbYBMARBOBDAnWAcBAebBbVBADBBABABAGACAKABABABABABABABABABABABABABABABABABABABABABABABABABABA\
    BABABABABABABABABABABABABABABABABABABABABABABAIABAEAMbLBAFAbtwDABABABABABABABABABABABABABABABABA\
    BABABABABABATABABABABABABABABABABABABABAfFABABABABABABADABABABABABABABABABABABABABABABABABABABAB\
    ABABABABABABABABABABAKABACABABABABAEAEABBCABABABABABABABABABALABABABABABABABAEABAGAFABAbCAbhCAbC\
    dBbfqFZbvHbNfGbJfZKBOBGBBcrBbYemFbFbgjWbFbulMbH"),
    ("fc:M", "bjLAbFAkpWA"),
    ("fc:Mn", "bjLAbFAkpWA"),
    ("fs:Common", "bjOAbFA"),
    ("fs:Greek", "gZAzFA"),
    ("fs:Inherited", "bjLAbFAkpWA"),
];

/// unicode.CategoryAliases (alias, category).
static UNI_ALIASES: &[(&str, &str)] = &[
    ("Cased_Letter", "LC"), ("Close_Punctuation", "Pe"), ("Combining_Mark", "M"), ("Connector_Punctuation", "Pc"),
    ("Control", "Cc"), ("Currency_Symbol", "Sc"), ("Dash_Punctuation", "Pd"), ("Decimal_Number", "Nd"),
    ("Enclosing_Mark", "Me"), ("Final_Punctuation", "Pf"), ("Format", "Cf"), ("Initial_Punctuation", "Pi"),
    ("Letter", "L"), ("Letter_Number", "Nl"), ("Line_Separator", "Zl"), ("Lowercase_Letter", "Ll"),
    ("Mark", "M"), ("Math_Symbol", "Sm"), ("Modifier_Letter", "Lm"), ("Modifier_Symbol", "Sk"),
    ("Nonspacing_Mark", "Mn"), ("Number", "N"), ("Open_Punctuation", "Ps"), ("Other", "C"),
    ("Other_Letter", "Lo"), ("Other_Number", "No"), ("Other_Punctuation", "Po"), ("Other_Symbol", "So"),
    ("Paragraph_Separator", "Zp"), ("Private_Use", "Co"), ("Punctuation", "P"), ("Separator", "Z"),
    ("Space_Separator", "Zs"), ("Spacing_Mark", "Mc"), ("Surrogate", "Cs"), ("Symbol", "S"),
    ("Titlecase_Letter", "Lt"), ("Unassigned", "Cn"), ("Uppercase_Letter", "Lu"), ("cntrl", "Cc"),
    ("digit", "Nd"), ("punct", "P"),
];

/// unicode.CaseRanges as (gap, hi-lo, zigzag upper delta, zigzag lower delta).
static UNI_CASE_RANGES: &str = "cNZAcMGZcLAcGAcfEAKWAcMBGAcMBWcLABGcLAAAjIAAbVewueYewueYAAApHAArVAAFewueYewueYBPewueYewueYBbTewu\
    eYewueYAAAjHAFewueYewueYAAxBAAApAAAAAqEADewueYewueYAAApWABewueYewueYABApUABewueYewueYBAAgCAAApOA\
    AApQABewueYewueYAAApUAAApYAAhMAAAAqGAAAqCABewueYewueYAAmOABAAqGAAAqKAAkAAAAAqMAFewueYewueYAAAqUA\
    BewueYewueYAAAqUCBewueYewueYAAAqUABewueYewueYABAqSADewueYewueYAAAqWABewueYewueYCBewueYewueYBAeIA\
    EAAEAABCAADAAAAEAABCAADAAAAEAABCAADAAPewueYewueYAAgBAARewueYewueYBAAEAABCAADAABewueYewueYAAAhLAA\
    AeHAbNewueYewueYAAAjZBRewueYewueYGAAbfyKABewueYewueYAAAmNAAAbfyEABbfzYAABewueYewueYAAAoZAAAfIAAA\
    fMAJewueYewueYAAbfxMAAAbfxGAAAbfxKAAAqDAAApVABBpTABApNABApPAAAevfIADApTAAAevfAABApXABAevcIAAAeve\
    MABAqBAAAqFAAAeveMAAAbfuKAAAeveGACAqFABAbfuWAAAqJACAqLAHAbftEACAqTABAeveKAAAqTADAevcMAAAqTAAAfHA\
    ABqRAAAfLAFAqVAKAevaWAAAevaQAgKAgMAbQDewueYewueYCBewueYewueYDCkAABAAiYGAAcYBCAcWBAAeYBBAeWBQAcMB\
    IAcMAAcXAACcVABQcLAAAcJAAIcLAAAeXAABeVAAAAQAAeTAAAeJADAdPAAAeDAAAPAAXewueYewueYAAgPAAAgDAAAOAAAi\
    XAAAAePAAhJABBewueYewueYAAANABewueYewueYBCAjZAPAgEAbFAcMAbFcLAAPgDAAbHewueYewueYIcBewueYewueYAAA\
    bEANewueYewueYAAbDAAdRewueYewueYBbLAdSKbLdRAefHbLAvmUBAAvmUFAAvmUCbQixKACCixKAzWdBAekzOAFAQCFPAd\
    fUAsnBAAAsmZAAAsmHAABsmDAAAsmFAAAslRAAAshLAAAeaiUAHbQAixJCCAixJhDAeanWADAlhKAQAearWAeJfTewueYewu\
    eYFAeNACAAwnTBdRewueYewueYAHQAAHAPAFQACFAPCHQAAHAPAHQAAHAPAFQACFAPDAQABAQABAQABAQABAAPBAAPBAAPBA\
    APAHQAAHAPABfSAADgQAABhSAABjWAABiQAABjSACHQAAHAPAHQAAHAPAHQAAHAPABQABASAEBAPABAfRAAARBAviFAEASAE\
    DAgPAAARDBQAGBAPABAhREBQADAOACBAPABAiPAAANGASAEBAjVABAjRAAARlLAAwgFDAAyuVAAAylNGAAcEbBAcDARPAbGA\
    PbFADBewueYewueYbfLZAcAAZbZAcrUbVAdSAbVdRAABewueYewueYAAAbfuJAAAlhJAAAbftDAAbfyJAAAbfyDAAFewueYe\
    wueYAAAbfxFAAAbfuVAAAbfxLAAAbfxJBBewueYewueYBBewueYewueYHBAbfzXAdVewueYewueYHDewueYewueYDBewueYe\
    wueYMbLvmTABAvmTAFAvmTAbtwCbTewueYewueYSbBewueYewueYfENewueYewueYCcJewueYewueYJDewueYewueYAAAean\
    VAJewueYewueYDBewueYewueYAAAevcHCDewueYewueYAAdSABTewueYewueYAAAeveLAAAevfHAAAeveZAAAeveFAAAeveL\
    BAAevaPAAAevcLAAAevaVAAActKAPewueYewueYAAAdRAAAeveJAAAearVADewueYewueYFBewueYewueYEDewueYewueYbB\
    BewueYewueYbhCActJAbCdBekzNAbfoZZAcMGZcLAbtTbNAdCAbNdBAdSbJAdCEbJdBAeMKAdABOAdABGAdABBAdABKcZABO\
    cZABGcZABBcZAcoPbYAeYNbYeXAekZbFAcMAbFcLAbgiQbFAcMAbFcLAbukEbHAcQAbHcPA";

/// SimpleFold exceptions (caseOrbit, asciiFold): (rune delta, fold) pairs.
static UNI_FOLD_ORBIT: &str = "eDmoOIoTeElpMGmoPcXlSBlTfRrLDrODrRbNtEqSblAPblODbmZDblPBmaOBbmUCgZEblUBbmVBblBEblTDmoKIbmYbJbjKc\
    KkuQCkuRKkuSDkuTBkuUIkuWZkuXjdPkuVsJlpJnLbgFnWbkBEcXBhPbykUkuY";
