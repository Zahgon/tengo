//! Syntax tree (parser/ast.go, expr.go, stmt.go). Positions follow the Go node methods so
//! compile errors and runtime source maps report the same line:column.

use crate::source::{Pos, NO_POS};
use crate::token::Token;

#[derive(Debug, Clone)]
pub struct Ident {
    pub name: Vec<u8>,
    pub pos: Pos,
}

impl Ident {
    pub fn end(&self) -> Pos {
        self.pos + self.name.len() as i64
    }
}

#[derive(Debug, Clone)]
pub struct IdentList {
    pub lparen: Pos,
    pub var_args: bool,
    pub list: Vec<Ident>,
    pub rparen: Pos,
}

impl IdentList {
    pub fn pos(&self) -> Pos {
        if self.lparen != NO_POS {
            return self.lparen;
        }
        if let Some(f) = self.list.first() {
            return f.pos;
        }
        NO_POS
    }

    pub fn end(&self) -> Pos {
        if self.rparen != NO_POS {
            return self.rparen + 1;
        }
        if let Some(l) = self.list.last() {
            return l.end();
        }
        NO_POS
    }
}

#[derive(Debug, Clone)]
pub struct MapElementLit {
    pub key: Vec<u8>,
    pub key_pos: Pos,
    pub colon_pos: Pos,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct BlockStmt {
    pub stmts: Vec<Stmt>,
    pub lbrace: Pos,
    pub rbrace: Pos,
}

impl BlockStmt {
    pub fn pos(&self) -> Pos {
        self.lbrace
    }
    pub fn end(&self) -> Pos {
        self.rbrace + 1
    }
}

#[derive(Debug, Clone)]
pub struct FuncType {
    pub func_pos: Pos,
    pub params: IdentList,
}

#[derive(Debug, Clone)]
pub enum Expr {
    ArrayLit { elements: Vec<Expr>, lbrack: Pos, rbrack: Pos },
    BadExpr { from: Pos, to: Pos },
    BinaryExpr { lhs: Box<Expr>, rhs: Box<Expr>, token: Token, token_pos: Pos },
    BoolLit { value: bool, pos: Pos, literal: Vec<u8> },
    CallExpr { func: Box<Expr>, lparen: Pos, args: Vec<Expr>, ellipsis: Pos, rparen: Pos },
    CharLit { value: i32, pos: Pos, literal: Vec<u8> },
    CondExpr { cond: Box<Expr>, t: Box<Expr>, f: Box<Expr>, question_pos: Pos, colon_pos: Pos },
    ErrorExpr { expr: Box<Expr>, error_pos: Pos, lparen: Pos, rparen: Pos },
    FloatLit { value: f64, pos: Pos, literal: Vec<u8> },
    FuncLit { ty: FuncType, body: BlockStmt },
    Ident(Ident),
    ImmutableExpr { expr: Box<Expr>, error_pos: Pos, lparen: Pos, rparen: Pos },
    ImportExpr { module_name: Vec<u8>, token_pos: Pos },
    IndexExpr { expr: Box<Expr>, lbrack: Pos, index: Option<Box<Expr>>, rbrack: Pos },
    IntLit { value: i64, pos: Pos, literal: Vec<u8> },
    MapLit { lbrace: Pos, elements: Vec<MapElementLit>, rbrace: Pos },
    ParenExpr { expr: Box<Expr>, lparen: Pos, rparen: Pos },
    SelectorExpr { expr: Box<Expr>, sel: Box<Expr> },
    SliceExpr { expr: Box<Expr>, lbrack: Pos, low: Option<Box<Expr>>, high: Option<Box<Expr>>, rbrack: Pos },
    StringLit { value: Vec<u8>, pos: Pos, literal: Vec<u8> },
    UnaryExpr { expr: Box<Expr>, token: Token, token_pos: Pos },
    UndefinedLit { pos: Pos },
}

impl Expr {
    pub fn pos(&self) -> Pos {
        match self {
            Expr::ArrayLit { lbrack, .. } => *lbrack,
            Expr::BadExpr { from, .. } => *from,
            Expr::BinaryExpr { lhs, .. } => lhs.pos(),
            Expr::BoolLit { pos, .. } => *pos,
            Expr::CallExpr { func, .. } => func.pos(),
            Expr::CharLit { pos, .. } => *pos,
            Expr::CondExpr { cond, .. } => cond.pos(),
            Expr::ErrorExpr { error_pos, .. } => *error_pos,
            Expr::FloatLit { pos, .. } => *pos,
            Expr::FuncLit { ty, .. } => ty.func_pos,
            Expr::Ident(i) => i.pos,
            Expr::ImmutableExpr { error_pos, .. } => *error_pos,
            Expr::ImportExpr { token_pos, .. } => *token_pos,
            Expr::IndexExpr { expr, .. } => expr.pos(),
            Expr::IntLit { pos, .. } => *pos,
            Expr::MapLit { lbrace, .. } => *lbrace,
            Expr::ParenExpr { lparen, .. } => *lparen,
            Expr::SelectorExpr { expr, .. } => expr.pos(),
            Expr::SliceExpr { expr, .. } => expr.pos(),
            Expr::StringLit { pos, .. } => *pos,
            Expr::UnaryExpr { expr, .. } => expr.pos(),
            Expr::UndefinedLit { pos } => *pos,
        }
    }

    pub fn end(&self) -> Pos {
        match self {
            Expr::ArrayLit { rbrack, .. } => rbrack + 1,
            Expr::BadExpr { to, .. } => *to,
            Expr::BinaryExpr { rhs, .. } => rhs.end(),
            Expr::BoolLit { pos, literal, .. } => pos + literal.len() as i64,
            Expr::CallExpr { rparen, .. } => rparen + 1,
            Expr::CharLit { pos, literal, .. } => pos + literal.len() as i64,
            Expr::CondExpr { f, .. } => f.end(),
            Expr::ErrorExpr { rparen, .. } => *rparen,
            Expr::FloatLit { pos, literal, .. } => pos + literal.len() as i64,
            Expr::FuncLit { body, .. } => body.end(),
            Expr::Ident(i) => i.end(),
            Expr::ImmutableExpr { rparen, .. } => *rparen,
            Expr::ImportExpr { token_pos, module_name } => token_pos + 10 + module_name.len() as i64,
            Expr::IndexExpr { rbrack, .. } => rbrack + 1,
            Expr::IntLit { pos, literal, .. } => pos + literal.len() as i64,
            Expr::MapLit { rbrace, .. } => rbrace + 1,
            Expr::ParenExpr { rparen, .. } => rparen + 1,
            Expr::SelectorExpr { sel, .. } => sel.end(),
            Expr::SliceExpr { rbrack, .. } => rbrack + 1,
            Expr::StringLit { pos, literal, .. } => pos + literal.len() as i64,
            Expr::UnaryExpr { expr, .. } => expr.end(),
            Expr::UndefinedLit { pos } => pos + 9,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Stmt {
    AssignStmt { lhs: Vec<Expr>, rhs: Vec<Expr>, token: Token, token_pos: Pos },
    BadStmt { from: Pos, to: Pos },
    BlockStmt(BlockStmt),
    BranchStmt { token: Token, token_pos: Pos, label: Option<Ident> },
    EmptyStmt { semicolon: Pos, implicit: bool },
    ExportStmt { export_pos: Pos, result: Expr },
    ExprStmt { expr: Expr },
    ForInStmt { for_pos: Pos, key: Ident, value: Ident, iterable: Expr, body: BlockStmt },
    ForStmt { for_pos: Pos, init: Option<Box<Stmt>>, cond: Option<Expr>, post: Option<Box<Stmt>>, body: BlockStmt },
    IfStmt { if_pos: Pos, init: Option<Box<Stmt>>, cond: Option<Expr>, body: BlockStmt, else_: Option<Box<Stmt>> },
    IncDecStmt { expr: Expr, token: Token, token_pos: Pos },
    ReturnStmt { return_pos: Pos, result: Option<Expr> },
}

impl Stmt {
    pub fn pos(&self) -> Pos {
        match self {
            Stmt::AssignStmt { lhs, .. } => lhs[0].pos(),
            Stmt::BadStmt { from, .. } => *from,
            Stmt::BlockStmt(b) => b.pos(),
            Stmt::BranchStmt { token_pos, .. } => *token_pos,
            Stmt::EmptyStmt { semicolon, .. } => *semicolon,
            Stmt::ExportStmt { export_pos, .. } => *export_pos,
            Stmt::ExprStmt { expr } => expr.pos(),
            Stmt::ForInStmt { for_pos, .. } => *for_pos,
            Stmt::ForStmt { for_pos, .. } => *for_pos,
            Stmt::IfStmt { if_pos, .. } => *if_pos,
            Stmt::IncDecStmt { expr, .. } => expr.pos(),
            Stmt::ReturnStmt { return_pos, .. } => *return_pos,
        }
    }

    pub fn end(&self) -> Pos {
        match self {
            Stmt::AssignStmt { rhs, .. } => rhs[rhs.len() - 1].end(),
            Stmt::BadStmt { to, .. } => *to,
            Stmt::BlockStmt(b) => b.end(),
            Stmt::BranchStmt { token, token_pos, label } => match label {
                Some(l) => l.end(),
                None => token_pos + token.string().len() as i64,
            },
            Stmt::EmptyStmt { semicolon, implicit } => {
                if *implicit {
                    *semicolon
                } else {
                    semicolon + 1
                }
            }
            Stmt::ExportStmt { result, .. } => result.end(),
            Stmt::ExprStmt { expr } => expr.end(),
            Stmt::ForInStmt { body, .. } => body.end(),
            Stmt::ForStmt { body, .. } => body.end(),
            Stmt::IfStmt { body, else_, .. } => match else_ {
                Some(e) => e.end(),
                None => body.end(),
            },
            Stmt::IncDecStmt { token_pos, .. } => token_pos + 2,
            Stmt::ReturnStmt { return_pos, result } => match result {
                Some(r) => r.end(),
                None => return_pos + 6,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct File {
    pub stmts: Vec<Stmt>,
}
