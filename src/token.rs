//! Tengo tokens (token/token.go).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Token {
    Illegal = 0,
    EOF,
    Comment,
    LiteralBeg,
    Ident,
    Int,
    Float,
    Char,
    String,
    LiteralEnd,
    OperatorBeg,
    Add,
    Sub,
    Mul,
    Quo,
    Rem,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    AndNot,
    AddAssign,
    SubAssign,
    MulAssign,
    QuoAssign,
    RemAssign,
    AndAssign,
    OrAssign,
    XorAssign,
    ShlAssign,
    ShrAssign,
    AndNotAssign,
    LAnd,
    LOr,
    Inc,
    Dec,
    Equal,
    Less,
    Greater,
    Assign,
    Not,
    NotEqual,
    LessEq,
    GreaterEq,
    Define,
    Ellipsis,
    LParen,
    LBrack,
    LBrace,
    Comma,
    Period,
    RParen,
    RBrack,
    RBrace,
    Semicolon,
    Colon,
    Question,
    OperatorEnd,
    KeywordBeg,
    Break,
    Continue,
    Else,
    For,
    Func,
    Error,
    Immutable,
    If,
    Return,
    Export,
    True,
    False,
    In,
    Undefined,
    Import,
    KeywordEnd,
}

use Token::*;

const ALL: [Token; 76] = [
    Illegal, EOF, Comment, LiteralBeg, Ident, Int, Float, Char, String, LiteralEnd, OperatorBeg, Add, Sub, Mul, Quo,
    Rem, And, Or, Xor, Shl, Shr, AndNot, AddAssign, SubAssign, MulAssign, QuoAssign, RemAssign, AndAssign, OrAssign,
    XorAssign, ShlAssign, ShrAssign, AndNotAssign, LAnd, LOr, Inc, Dec, Equal, Less, Greater, Assign, Not, NotEqual,
    LessEq, GreaterEq, Define, Ellipsis, LParen, LBrack, LBrace, Comma, Period, RParen, RBrack, RBrace, Semicolon,
    Colon, Question, OperatorEnd, KeywordBeg, Break, Continue, Else, For, Func, Error, Immutable, If, Return, Export,
    True, False, In, Undefined, Import, KeywordEnd,
];

impl Token {
    pub fn from_u8(b: u8) -> Option<Token> {
        ALL.get(b as usize).copied()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Illegal => "ILLEGAL",
            EOF => "EOF",
            Comment => "COMMENT",
            Ident => "IDENT",
            Int => "INT",
            Float => "FLOAT",
            Char => "CHAR",
            String => "STRING",
            Add => "+",
            Sub => "-",
            Mul => "*",
            Quo => "/",
            Rem => "%",
            And => "&",
            Or => "|",
            Xor => "^",
            Shl => "<<",
            Shr => ">>",
            AndNot => "&^",
            AddAssign => "+=",
            SubAssign => "-=",
            MulAssign => "*=",
            QuoAssign => "/=",
            RemAssign => "%=",
            AndAssign => "&=",
            OrAssign => "|=",
            XorAssign => "^=",
            ShlAssign => "<<=",
            ShrAssign => ">>=",
            AndNotAssign => "&^=",
            LAnd => "&&",
            LOr => "||",
            Inc => "++",
            Dec => "--",
            Equal => "==",
            Less => "<",
            Greater => ">",
            Assign => "=",
            Not => "!",
            NotEqual => "!=",
            LessEq => "<=",
            GreaterEq => ">=",
            Define => ":=",
            Ellipsis => "...",
            LParen => "(",
            LBrack => "[",
            LBrace => "{",
            Comma => ",",
            Period => ".",
            RParen => ")",
            RBrack => "]",
            RBrace => "}",
            Semicolon => ";",
            Colon => ":",
            Question => "?",
            Break => "break",
            Continue => "continue",
            Else => "else",
            For => "for",
            Func => "func",
            Error => "error",
            Immutable => "immutable",
            If => "if",
            Return => "return",
            Export => "export",
            True => "true",
            False => "false",
            In => "in",
            Undefined => "undefined",
            Import => "import",
            LiteralBeg | LiteralEnd | OperatorBeg | OperatorEnd | KeywordBeg | KeywordEnd => "",
        }
    }

    /// Token.String(): the spelling, or token(N) for the unnamed markers.
    pub fn string(self) -> std::string::String {
        let s = self.as_str();
        if s.is_empty() {
            format!("token({})", self as u8)
        } else {
            s.to_string()
        }
    }

    pub fn precedence(self) -> i32 {
        match self {
            LOr => 1,
            LAnd => 2,
            Equal | NotEqual | Less | LessEq | Greater | GreaterEq => 3,
            Add | Sub | Or | Xor => 4,
            Mul | Quo | Rem | Shl | Shr | And | AndNot => 5,
            _ => 0,
        }
    }

    pub fn is_literal(self) -> bool {
        (LiteralBeg as u8) < (self as u8) && (self as u8) < (LiteralEnd as u8)
    }

    pub fn is_operator(self) -> bool {
        (OperatorBeg as u8) < (self as u8) && (self as u8) < (OperatorEnd as u8)
    }

    pub fn is_keyword(self) -> bool {
        (KeywordBeg as u8) < (self as u8) && (self as u8) < (KeywordEnd as u8)
    }

    pub fn lookup(ident: &[u8]) -> Token {
        match ident {
            b"break" => Break,
            b"continue" => Continue,
            b"else" => Else,
            b"for" => For,
            b"func" => Func,
            b"error" => Error,
            b"immutable" => Immutable,
            b"if" => If,
            b"return" => Return,
            b"export" => Export,
            b"true" => True,
            b"false" => False,
            b"in" => In,
            b"undefined" => Undefined,
            b"import" => Import,
            _ => Ident,
        }
    }
}

pub const LOWEST_PREC: i32 = 0;
