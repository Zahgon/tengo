//! The tokenizer (parser/scanner.go).

use crate::gostr::{decode_rune, is_digit, is_letter, RUNE_ERROR};
use crate::source::{FileSetRef, Pos, SourceFilePos};
use crate::token::Token;

const BOM: i32 = 0xFEFF;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub pos: SourceFilePos,
    pub msg: String,
}

pub struct Scanner {
    fset: FileSetRef,
    file: usize,
    base: i64,
    src: Vec<u8>,
    ch: i32,
    offset: usize,
    read_offset: usize,
    insert_semi: bool,
    pub error_count: usize,
}

/// fmt's %#U rendering of a rune: U+0041 'A' (the quoted form only for printable runes).
pub fn sharp_u(ch: i32) -> String {
    let mut s = format!("U+{:04X}", ch);
    if ch >= 0 && crate::gostr::strconv_is_print(ch) {
        s.push_str(" '");
        s.push_str(&String::from_utf8_lossy(&crate::gostr::rune_string(ch)));
        s.push('\'');
    }
    s
}

impl Scanner {
    pub fn new(fset: FileSetRef, file: usize, src: Vec<u8>, errs: &mut Vec<ParseError>) -> Scanner {
        let base = fset.borrow().files[file].base;
        let mut s = Scanner {
            fset,
            file,
            base,
            src,
            ch: ' ' as i32,
            offset: 0,
            read_offset: 0,
            insert_semi: false,
            error_count: 0,
        };
        s.next(errs);
        if s.ch == BOM {
            s.next(errs);
        }
        s
    }

    fn file_pos(&self, offset: usize) -> Pos {
        self.base + offset as i64
    }

    fn error(&mut self, errs: &mut Vec<ParseError>, offset: usize, msg: String) {
        let p = self.file_pos(offset);
        let pos = self.fset.borrow().files[self.file].position(p);
        errs.push(ParseError { pos, msg });
        self.error_count += 1;
    }

    fn add_line(&mut self, offset: usize) {
        self.fset.borrow_mut().files[self.file].add_line(offset as i64);
    }

    fn next(&mut self, errs: &mut Vec<ParseError>) {
        if self.read_offset < self.src.len() {
            self.offset = self.read_offset;
            if self.ch == '\n' as i32 {
                self.add_line(self.offset);
            }
            let mut r = self.src[self.read_offset] as i32;
            let mut w = 1;
            if r == 0 {
                self.error(errs, self.offset, "illegal character NUL".to_string());
            } else if r >= 0x80 {
                let (rr, ww) = decode_rune(&self.src[self.read_offset..]);
                r = rr;
                w = ww;
                if r == RUNE_ERROR && w == 1 {
                    self.error(errs, self.offset, "illegal UTF-8 encoding".to_string());
                } else if r == BOM && self.offset > 0 {
                    self.error(errs, self.offset, "illegal byte order mark".to_string());
                }
            }
            self.read_offset += w;
            self.ch = r;
        } else {
            self.offset = self.src.len();
            if self.ch == '\n' as i32 {
                self.add_line(self.offset);
            }
            self.ch = -1;
        }
    }

    fn peek(&self) -> u8 {
        if self.read_offset < self.src.len() {
            self.src[self.read_offset]
        } else {
            0
        }
    }

    pub fn scan(&mut self, errs: &mut Vec<ParseError>) -> (Token, Vec<u8>, Pos) {
        self.skip_whitespace(errs);
        let pos = self.file_pos(self.offset);
        let mut insert_semi = false;
        let tok;
        let mut literal: Vec<u8> = Vec::new();
        let ch = self.ch;
        if is_letter_ch(ch) {
            literal = self.scan_identifier(errs);
            tok = Token::lookup(&literal);
            if matches!(
                tok,
                Token::Ident | Token::Break | Token::Continue | Token::Return | Token::Export | Token::True | Token::False | Token::Undefined
            ) {
                insert_semi = true;
            }
        } else if ('0' as i32 <= ch && ch <= '9' as i32) || (ch == '.' as i32 && self.peek().is_ascii_digit()) {
            insert_semi = true;
            let (t, l) = self.scan_number(errs);
            tok = t;
            literal = l;
        } else {
            self.next(errs);
            match ch {
                -1 => {
                    if self.insert_semi {
                        self.insert_semi = false;
                        return (Token::Semicolon, b"\n".to_vec(), pos);
                    }
                    tok = Token::EOF;
                }
                c if c == '\n' as i32 => {
                    self.insert_semi = false;
                    return (Token::Semicolon, b"\n".to_vec(), pos);
                }
                c if c == '"' as i32 => {
                    insert_semi = true;
                    tok = Token::String;
                    literal = self.scan_string(errs);
                }
                c if c == '\'' as i32 => {
                    insert_semi = true;
                    tok = Token::Char;
                    literal = self.scan_rune(errs);
                }
                c if c == '`' as i32 => {
                    insert_semi = true;
                    tok = Token::String;
                    literal = self.scan_raw_string(errs);
                }
                c if c == ':' as i32 => tok = self.switch2(errs, Token::Colon, Token::Define),
                c if c == '.' as i32 => {
                    if self.ch == '.' as i32 && self.peek() == b'.' {
                        self.next(errs);
                        self.next(errs);
                        tok = Token::Ellipsis;
                    } else {
                        tok = Token::Period;
                    }
                }
                c if c == ',' as i32 => tok = Token::Comma,
                c if c == '?' as i32 => tok = Token::Question,
                c if c == ';' as i32 => {
                    tok = Token::Semicolon;
                    literal = b";".to_vec();
                }
                c if c == '(' as i32 => tok = Token::LParen,
                c if c == ')' as i32 => {
                    insert_semi = true;
                    tok = Token::RParen;
                }
                c if c == '[' as i32 => tok = Token::LBrack,
                c if c == ']' as i32 => {
                    insert_semi = true;
                    tok = Token::RBrack;
                }
                c if c == '{' as i32 => tok = Token::LBrace,
                c if c == '}' as i32 => {
                    insert_semi = true;
                    tok = Token::RBrace;
                }
                c if c == '+' as i32 => {
                    tok = self.switch3(errs, Token::Add, Token::AddAssign, '+' as i32, Token::Inc);
                    if tok == Token::Inc {
                        insert_semi = true;
                    }
                }
                c if c == '-' as i32 => {
                    tok = self.switch3(errs, Token::Sub, Token::SubAssign, '-' as i32, Token::Dec);
                    if tok == Token::Dec {
                        insert_semi = true;
                    }
                }
                c if c == '*' as i32 => tok = self.switch2(errs, Token::Mul, Token::MulAssign),
                c if c == '/' as i32 => {
                    if self.ch == '/' as i32 || self.ch == '*' as i32 {
                        if self.insert_semi && self.find_line_end(errs) {
                            self.ch = '/' as i32;
                            self.offset = (pos - self.base) as usize;
                            self.read_offset = self.offset + 1;
                            self.insert_semi = false;
                            return (Token::Semicolon, b"\n".to_vec(), pos);
                        }
                        let _comment = self.scan_comment(errs);
                        // ScanComments is never set: comments are skipped.
                        self.insert_semi = false;
                        return self.scan(errs);
                    }
                    tok = self.switch2(errs, Token::Quo, Token::QuoAssign);
                }
                c if c == '%' as i32 => tok = self.switch2(errs, Token::Rem, Token::RemAssign),
                c if c == '^' as i32 => tok = self.switch2(errs, Token::Xor, Token::XorAssign),
                c if c == '<' as i32 => tok = self.switch4(errs, Token::Less, Token::LessEq, '<' as i32, Token::Shl, Token::ShlAssign),
                c if c == '>' as i32 => {
                    tok = self.switch4(errs, Token::Greater, Token::GreaterEq, '>' as i32, Token::Shr, Token::ShrAssign)
                }
                c if c == '=' as i32 => tok = self.switch2(errs, Token::Assign, Token::Equal),
                c if c == '!' as i32 => tok = self.switch2(errs, Token::Not, Token::NotEqual),
                c if c == '&' as i32 => {
                    if self.ch == '^' as i32 {
                        self.next(errs);
                        tok = self.switch2(errs, Token::AndNot, Token::AndNotAssign);
                    } else {
                        tok = self.switch3(errs, Token::And, Token::AndAssign, '&' as i32, Token::LAnd);
                    }
                }
                c if c == '|' as i32 => tok = self.switch3(errs, Token::Or, Token::OrAssign, '|' as i32, Token::LOr),
                _ => {
                    if ch != BOM {
                        let off = (pos - self.base) as usize;
                        self.error(errs, off, format!("illegal character {}", sharp_u(ch)));
                    }
                    insert_semi = self.insert_semi;
                    tok = Token::Illegal;
                    literal = crate::gostr::rune_string(ch);
                }
            }
        }
        self.insert_semi = insert_semi;
        (tok, literal, pos)
    }

    fn scan_comment(&mut self, errs: &mut Vec<ParseError>) -> Vec<u8> {
        let offs = self.offset - 1;
        if self.ch == '/' as i32 {
            self.next(errs);
            while self.ch != '\n' as i32 && self.ch >= 0 {
                self.next(errs);
            }
        } else {
            self.next(errs);
            let mut terminated = false;
            while self.ch >= 0 {
                let ch = self.ch;
                self.next(errs);
                if ch == '*' as i32 && self.ch == '/' as i32 {
                    self.next(errs);
                    terminated = true;
                    break;
                }
            }
            if !terminated {
                self.error(errs, offs, "comment not terminated".to_string());
            }
        }
        self.src[offs..self.offset].to_vec()
    }

    fn find_line_end(&mut self, errs: &mut Vec<ParseError>) -> bool {
        let offs = self.offset - 1;
        let result = self.find_line_end_inner(errs);
        self.ch = '/' as i32;
        self.offset = offs;
        self.read_offset = offs + 1;
        self.next(errs);
        result
    }

    fn find_line_end_inner(&mut self, errs: &mut Vec<ParseError>) -> bool {
        while self.ch == '/' as i32 || self.ch == '*' as i32 {
            if self.ch == '/' as i32 {
                return true;
            }
            self.next(errs);
            while self.ch >= 0 {
                let ch = self.ch;
                if ch == '\n' as i32 {
                    return true;
                }
                self.next(errs);
                if ch == '*' as i32 && self.ch == '/' as i32 {
                    self.next(errs);
                    break;
                }
            }
            self.skip_whitespace(errs);
            if self.ch < 0 || self.ch == '\n' as i32 {
                return true;
            }
            if self.ch != '/' as i32 {
                return false;
            }
            self.next(errs);
        }
        false
    }

    fn scan_identifier(&mut self, errs: &mut Vec<ParseError>) -> Vec<u8> {
        let offs = self.offset;
        while is_letter_ch(self.ch) || is_digit_ch(self.ch) {
            self.next(errs);
        }
        self.src[offs..self.offset].to_vec()
    }

    fn scan_digits(&mut self, errs: &mut Vec<ParseError>, base: i32) {
        while self.ch == '_' as i32 || digit_val(self.ch) < base {
            self.next(errs);
        }
    }

    fn scan_number(&mut self, errs: &mut Vec<ParseError>) -> (Token, Vec<u8>) {
        let offs = self.offset;
        let mut tok = Token::Int;
        let mut base = 10;
        let lp = self.peek() | (b'x' - b'X');
        if self.ch == '0' as i32 && lp == b'b' {
            base = 2;
            self.next(errs);
            self.next(errs);
        } else if self.ch == '0' as i32 && lp == b'o' {
            base = 8;
            self.next(errs);
            self.next(errs);
        } else if self.ch == '0' as i32 && lp == b'x' {
            base = 16;
            self.next(errs);
            self.next(errs);
        }
        self.scan_digits(errs, base);
        if self.ch == '.' as i32 && (base == 10 || base == 16) {
            tok = Token::Float;
            self.next(errs);
            self.scan_digits(errs, base);
        }
        if self.ch == 'e' as i32 || self.ch == 'E' as i32 || self.ch == 'p' as i32 || self.ch == 'P' as i32 {
            tok = Token::Float;
            self.next(errs);
            if self.ch == '-' as i32 || self.ch == '+' as i32 {
                self.next(errs);
            }
            let offs2 = self.offset;
            self.scan_digits(errs, 10);
            if offs2 == self.offset {
                self.error(errs, offs2, "exponent has no digits".to_string());
            }
        }
        (tok, self.src[offs..self.offset].to_vec())
    }

    fn scan_escape(&mut self, errs: &mut Vec<ParseError>, quote: i32) -> bool {
        let offs = self.offset;
        let (mut n, base, max): (i32, u32, u32);
        let c = self.ch;
        if c == 'a' as i32
            || c == 'b' as i32
            || c == 'f' as i32
            || c == 'n' as i32
            || c == 'r' as i32
            || c == 't' as i32
            || c == 'v' as i32
            || c == '\\' as i32
            || c == quote
        {
            self.next(errs);
            return true;
        } else if ('0' as i32..='7' as i32).contains(&c) {
            n = 3;
            base = 8;
            max = 255;
        } else if c == 'x' as i32 {
            self.next(errs);
            n = 2;
            base = 16;
            max = 255;
        } else if c == 'u' as i32 {
            self.next(errs);
            n = 4;
            base = 16;
            max = 0x10FFFF;
        } else if c == 'U' as i32 {
            self.next(errs);
            n = 8;
            base = 16;
            max = 0x10FFFF;
        } else {
            let msg = if self.ch < 0 { "escape sequence not terminated" } else { "unknown escape sequence" };
            self.error(errs, offs, msg.to_string());
            return false;
        }
        let mut x: u32 = 0;
        while n > 0 {
            let d = digit_val(self.ch) as u32;
            if d >= base {
                let msg = if self.ch < 0 {
                    "escape sequence not terminated".to_string()
                } else {
                    format!("illegal character {} in escape sequence", sharp_u(self.ch))
                };
                let off = self.offset;
                self.error(errs, off, msg);
                return false;
            }
            x = x.wrapping_mul(base).wrapping_add(d);
            self.next(errs);
            n -= 1;
        }
        if x > max || (0xD800..0xE000).contains(&x) {
            self.error(errs, offs, "escape sequence is invalid Unicode code point".to_string());
            return false;
        }
        true
    }

    fn scan_rune(&mut self, errs: &mut Vec<ParseError>) -> Vec<u8> {
        let offs = self.offset - 1;
        let mut valid = true;
        let mut n = 0;
        loop {
            let ch = self.ch;
            if ch == '\n' as i32 || ch < 0 {
                if valid {
                    self.error(errs, offs, "rune literal not terminated".to_string());
                    valid = false;
                }
                break;
            }
            self.next(errs);
            if ch == '\'' as i32 {
                break;
            }
            n += 1;
            if ch == '\\' as i32 && !self.scan_escape(errs, '\'' as i32) {
                valid = false;
            }
        }
        if valid && n != 1 {
            self.error(errs, offs, "illegal rune literal".to_string());
        }
        self.src[offs..self.offset].to_vec()
    }

    fn scan_string(&mut self, errs: &mut Vec<ParseError>) -> Vec<u8> {
        let offs = self.offset - 1;
        loop {
            let ch = self.ch;
            if ch == '\n' as i32 || ch < 0 {
                self.error(errs, offs, "string literal not terminated".to_string());
                break;
            }
            self.next(errs);
            if ch == '"' as i32 {
                break;
            }
            if ch == '\\' as i32 {
                self.scan_escape(errs, '"' as i32);
            }
        }
        self.src[offs..self.offset].to_vec()
    }

    fn scan_raw_string(&mut self, errs: &mut Vec<ParseError>) -> Vec<u8> {
        let offs = self.offset - 1;
        let mut has_cr = false;
        loop {
            let ch = self.ch;
            if ch < 0 {
                self.error(errs, offs, "raw string literal not terminated".to_string());
                break;
            }
            self.next(errs);
            if ch == '`' as i32 {
                break;
            }
            if ch == '\r' as i32 {
                has_cr = true;
            }
        }
        let lit = self.src[offs..self.offset].to_vec();
        if has_cr {
            return lit.into_iter().filter(|&c| c != b'\r').collect();
        }
        lit
    }

    fn skip_whitespace(&mut self, errs: &mut Vec<ParseError>) {
        while self.ch == ' ' as i32 || self.ch == '\t' as i32 || (self.ch == '\n' as i32 && !self.insert_semi) || self.ch == '\r' as i32 {
            self.next(errs);
        }
    }

    fn switch2(&mut self, errs: &mut Vec<ParseError>, tok0: Token, tok1: Token) -> Token {
        if self.ch == '=' as i32 {
            self.next(errs);
            return tok1;
        }
        tok0
    }

    fn switch3(&mut self, errs: &mut Vec<ParseError>, tok0: Token, tok1: Token, ch2: i32, tok2: Token) -> Token {
        if self.ch == '=' as i32 {
            self.next(errs);
            return tok1;
        }
        if self.ch == ch2 {
            self.next(errs);
            return tok2;
        }
        tok0
    }

    fn switch4(&mut self, errs: &mut Vec<ParseError>, tok0: Token, tok1: Token, ch2: i32, tok2: Token, tok3: Token) -> Token {
        if self.ch == '=' as i32 {
            self.next(errs);
            return tok1;
        }
        if self.ch == ch2 {
            self.next(errs);
            if self.ch == '=' as i32 {
                self.next(errs);
                return tok3;
            }
            return tok2;
        }
        tok0
    }
}

fn is_letter_ch(ch: i32) -> bool {
    ('a' as i32..='z' as i32).contains(&ch)
        || ('A' as i32..='Z' as i32).contains(&ch)
        || ch == '_' as i32
        || (ch >= 0x80 && is_letter(ch))
}

fn is_digit_ch(ch: i32) -> bool {
    ('0' as i32..='9' as i32).contains(&ch) || (ch >= 0x80 && is_digit(ch))
}

fn digit_val(ch: i32) -> i32 {
    if ('0' as i32..='9' as i32).contains(&ch) {
        ch - '0' as i32
    } else if ('a' as i32..='f' as i32).contains(&ch) {
        ch - 'a' as i32 + 10
    } else if ('A' as i32..='F' as i32).contains(&ch) {
        ch - 'A' as i32 + 10
    } else {
        16
    }
}
