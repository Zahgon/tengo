//! The recursive-descent parser (parser/parser.go).

use crate::ast::*;
use crate::ftoa::parse_float;
use crate::gostr::{parse_int, unquote, unquote_char};
use crate::scanner::{ParseError, Scanner};
use crate::source::{FileSetRef, Pos, NO_POS};
use crate::token::{Token, LOWEST_PREC};

/// Raised when more than ten errors were found (Go's panic(bailout{})).
pub struct Bailout;

type PResult<T> = Result<T, Bailout>;

/// The parse error list as ParseFile returns it.
pub struct ErrorList(pub Vec<ParseError>);

impl ParseError {
    pub fn error(&self) -> String {
        if !self.pos.filename.is_empty() || self.pos.is_valid() {
            format!("Parse Error: {}\n\tat {}", self.msg, self.pos)
        } else {
            format!("Parse Error: {}", self.msg)
        }
    }
}

impl ErrorList {
    pub fn error(&self) -> String {
        match self.0.len() {
            0 => "no errors".to_string(),
            1 => self.0[0].error(),
            n => format!("{} (and {} more errors)", self.0[0].error(), n - 1),
        }
    }
}

fn stmt_start(t: Token) -> bool {
    matches!(t, Token::Break | Token::Continue | Token::For | Token::If | Token::Return | Token::Export)
}

pub struct Parser {
    fset: FileSetRef,
    file: usize,
    file_base: i64,
    file_size: i64,
    errors: Vec<ParseError>,
    scanner: Scanner,
    pos: Pos,
    token: Token,
    token_lit: Vec<u8>,
    expr_level: i32,
    sync_pos: Pos,
    sync_count: i32,
}

impl Parser {
    pub fn new(fset: FileSetRef, file: usize, src: Vec<u8>) -> Parser {
        let mut errors = Vec::new();
        let (base, size) = {
            let fs = fset.borrow();
            (fs.files[file].base, fs.files[file].size)
        };
        let scanner = Scanner::new(fset.clone(), file, src, &mut errors);
        let mut p = Parser {
            fset,
            file,
            file_base: base,
            file_size: size,
            errors,
            scanner,
            pos: NO_POS,
            token: Token::Illegal,
            token_lit: Vec::new(),
            expr_level: 0,
            sync_pos: NO_POS,
            sync_count: 0,
        };
        p.next();
        p
    }

    pub fn parse_file(mut self) -> Result<File, ErrorList> {
        if !self.errors.is_empty() {
            return Err(self.sorted_errors());
        }
        let result: PResult<Vec<Stmt>> = (|| {
            let stmts = self.parse_stmt_list()?;
            self.expect(Token::EOF)?;
            Ok(stmts)
        })();
        match result {
            Ok(stmts) => {
                if !self.errors.is_empty() {
                    return Err(self.sorted_errors());
                }
                Ok(File { stmts })
            }
            Err(Bailout) => Err(self.sorted_errors()),
        }
    }

    fn sorted_errors(&mut self) -> ErrorList {
        let mut errs = std::mem::take(&mut self.errors);
        errs.sort_by(|a, b| {
            a.pos
                .filename
                .cmp(&b.pos.filename)
                .then(a.pos.line.cmp(&b.pos.line))
                .then(a.pos.column.cmp(&b.pos.column))
                .then(a.msg.cmp(&b.msg))
        });
        ErrorList(errs)
    }

    fn next(&mut self) {
        let (t, l, p) = self.scanner.scan(&mut self.errors);
        self.token = t;
        self.token_lit = l;
        self.pos = p;
    }

    fn file_position(&self, pos: Pos) -> crate::source::SourceFilePos {
        if pos == NO_POS {
            return Default::default();
        }
        self.fset.borrow().files[self.file].position(pos)
    }

    fn error(&mut self, pos: Pos, msg: String) -> PResult<()> {
        let file_pos = self.file_position(pos);
        let n = self.errors.len();
        if n > 0 && self.errors[n - 1].pos.line == file_pos.line {
            return Ok(());
        }
        if n > 10 {
            return Err(Bailout);
        }
        self.errors.push(ParseError { pos: file_pos, msg });
        Ok(())
    }

    fn error_expected(&mut self, pos: Pos, msg: &str) -> PResult<()> {
        let mut msg = format!("expected {}", msg);
        if pos == self.pos {
            if self.token == Token::Semicolon && self.token_lit == b"\n" {
                msg.push_str(", found newline");
            } else if self.token.is_literal() {
                msg.push_str(", found ");
                msg.push_str(&String::from_utf8_lossy(&self.token_lit));
            } else {
                msg.push_str(&format!(", found '{}'", self.token.string()));
            }
        }
        self.error(pos, msg)
    }

    fn expect(&mut self, tok: Token) -> PResult<Pos> {
        let pos = self.pos;
        if self.token != tok {
            self.error_expected(pos, &format!("'{}'", tok.string()))?;
        }
        self.next();
        Ok(pos)
    }

    fn expect_semi(&mut self) -> PResult<()> {
        match self.token {
            Token::RParen | Token::RBrace => {}
            Token::Comma => {
                self.error_expected(self.pos, "';'")?;
                self.next();
            }
            Token::Semicolon => self.next(),
            _ => {
                self.error_expected(self.pos, "';'")?;
                self.advance();
            }
        }
        Ok(())
    }

    fn advance(&mut self) {
        while self.token != Token::EOF {
            if stmt_start(self.token) {
                if self.pos == self.sync_pos && self.sync_count < 10 {
                    self.sync_count += 1;
                    return;
                }
                if self.pos > self.sync_pos {
                    self.sync_pos = self.pos;
                    self.sync_count = 0;
                    return;
                }
            }
            self.next();
        }
    }

    fn safe_pos(&self, pos: Pos) -> Pos {
        if pos < self.file_base || pos > self.file_base + self.file_size {
            return self.file_base + self.file_size;
        }
        pos
    }

    fn parse_expr(&mut self) -> PResult<Expr> {
        let expr = self.parse_binary_expr(LOWEST_PREC + 1)?;
        if self.token == Token::Question {
            return self.parse_cond_expr(expr);
        }
        Ok(expr)
    }

    fn parse_binary_expr(&mut self, prec1: i32) -> PResult<Expr> {
        let mut x = self.parse_unary_expr()?;
        loop {
            let op = self.token;
            let prec = op.precedence();
            if prec < prec1 {
                return Ok(x);
            }
            let pos = self.expect(op)?;
            let y = self.parse_binary_expr(prec + 1)?;
            x = Expr::BinaryExpr { lhs: Box::new(x), rhs: Box::new(y), token: op, token_pos: pos };
        }
    }

    fn parse_cond_expr(&mut self, cond: Expr) -> PResult<Expr> {
        let question_pos = self.expect(Token::Question)?;
        let t = self.parse_expr()?;
        let colon_pos = self.expect(Token::Colon)?;
        let f = self.parse_expr()?;
        Ok(Expr::CondExpr { cond: Box::new(cond), t: Box::new(t), f: Box::new(f), question_pos, colon_pos })
    }

    fn parse_unary_expr(&mut self) -> PResult<Expr> {
        match self.token {
            Token::Add | Token::Sub | Token::Not | Token::Xor => {
                let pos = self.pos;
                let op = self.token;
                self.next();
                let x = self.parse_unary_expr()?;
                Ok(Expr::UnaryExpr { expr: Box::new(x), token: op, token_pos: pos })
            }
            _ => self.parse_primary_expr(),
        }
    }

    fn parse_primary_expr(&mut self) -> PResult<Expr> {
        let mut x = self.parse_operand()?;
        loop {
            match self.token {
                Token::Period => {
                    self.next();
                    match self.token {
                        Token::Ident => x = self.parse_selector(x)?,
                        _ => {
                            let pos = self.pos;
                            self.error_expected(pos, "selector")?;
                            self.advance();
                            return Ok(Expr::BadExpr { from: pos, to: self.pos });
                        }
                    }
                }
                Token::LBrack => x = self.parse_index_or_slice(x)?,
                Token::LParen => x = self.parse_call(x)?,
                _ => break,
            }
        }
        Ok(x)
    }

    fn parse_call(&mut self, x: Expr) -> PResult<Expr> {
        let lparen = self.expect(Token::LParen)?;
        self.expr_level += 1;
        let mut list = Vec::new();
        let mut ellipsis = NO_POS;
        while self.token != Token::RParen && self.token != Token::EOF && ellipsis == NO_POS {
            list.push(self.parse_expr()?);
            if self.token == Token::Ellipsis {
                ellipsis = self.pos;
                self.next();
            }
            if !self.expect_comma(Token::RParen, "call argument")? {
                break;
            }
        }
        self.expr_level -= 1;
        let rparen = self.expect(Token::RParen)?;
        Ok(Expr::CallExpr { func: Box::new(x), lparen, args: list, ellipsis, rparen })
    }

    fn expect_comma(&mut self, closing: Token, want: &str) -> PResult<bool> {
        if self.token == Token::Comma {
            self.next();
            if self.token == closing {
                self.error_expected(self.pos, want)?;
                return Ok(false);
            }
            return Ok(true);
        }
        if self.token == Token::Semicolon && self.token_lit == b"\n" {
            self.next();
        }
        Ok(false)
    }

    fn parse_index_or_slice(&mut self, x: Expr) -> PResult<Expr> {
        let lbrack = self.expect(Token::LBrack)?;
        self.expr_level += 1;
        let mut index0: Option<Box<Expr>> = None;
        let mut index1: Option<Box<Expr>> = None;
        if self.token != Token::Colon {
            index0 = Some(Box::new(self.parse_expr()?));
        }
        let mut num_colons = 0;
        if self.token == Token::Colon {
            num_colons += 1;
            self.next();
            if self.token != Token::RBrack && self.token != Token::EOF {
                index1 = Some(Box::new(self.parse_expr()?));
            }
        }
        self.expr_level -= 1;
        let rbrack = self.expect(Token::RBrack)?;
        if num_colons > 0 {
            return Ok(Expr::SliceExpr { expr: Box::new(x), lbrack, low: index0, high: index1, rbrack });
        }
        Ok(Expr::IndexExpr { expr: Box::new(x), lbrack, index: index0, rbrack })
    }

    fn parse_selector(&mut self, x: Expr) -> PResult<Expr> {
        let sel = self.parse_ident()?;
        Ok(Expr::SelectorExpr {
            expr: Box::new(x),
            sel: Box::new(Expr::StringLit { value: sel.name.clone(), pos: sel.pos, literal: sel.name }),
        })
    }

    fn parse_operand(&mut self) -> PResult<Expr> {
        match self.token {
            Token::Ident => return Ok(Expr::Ident(self.parse_ident()?)),
            Token::Int => {
                let v = match parse_int(&self.token_lit, 0, 64) {
                    Ok(v) => v,
                    // Go compares the *NumError against strconv.ErrRange, which never
                    // matches, so every failure reports "invalid integer".
                    Err((v, _)) => {
                        self.error(self.pos, "invalid integer".to_string())?;
                        v
                    }
                };
                let x = Expr::IntLit { value: v, pos: self.pos, literal: self.token_lit.clone() };
                self.next();
                return Ok(x);
            }
            Token::Float => {
                let v = match parse_float(&self.token_lit, 64) {
                    Ok(v) => v,
                    Err((v, _)) => {
                        self.error(self.pos, "invalid float".to_string())?;
                        v
                    }
                };
                let x = Expr::FloatLit { value: v, pos: self.pos, literal: self.token_lit.clone() };
                self.next();
                return Ok(x);
            }
            Token::Char => return self.parse_char_lit(),
            Token::String => {
                let v = unquote(&self.token_lit).unwrap_or_default();
                let x = Expr::StringLit { value: v, pos: self.pos, literal: self.token_lit.clone() };
                self.next();
                return Ok(x);
            }
            Token::True => {
                let x = Expr::BoolLit { value: true, pos: self.pos, literal: self.token_lit.clone() };
                self.next();
                return Ok(x);
            }
            Token::False => {
                let x = Expr::BoolLit { value: false, pos: self.pos, literal: self.token_lit.clone() };
                self.next();
                return Ok(x);
            }
            Token::Undefined => {
                let x = Expr::UndefinedLit { pos: self.pos };
                self.next();
                return Ok(x);
            }
            Token::Import => return self.parse_import_expr(),
            Token::LParen => {
                let lparen = self.pos;
                self.next();
                self.expr_level += 1;
                let x = self.parse_expr()?;
                self.expr_level -= 1;
                let rparen = self.expect(Token::RParen)?;
                return Ok(Expr::ParenExpr { expr: Box::new(x), lparen, rparen });
            }
            Token::LBrack => return self.parse_array_lit(),
            Token::LBrace => return self.parse_map_lit(),
            Token::Func => return self.parse_func_lit(),
            Token::Error => return self.parse_error_expr(),
            Token::Immutable => return self.parse_immutable_expr(),
            _ => {
                self.error_expected(self.pos, "operand")?;
            }
        }
        let pos = self.pos;
        self.advance();
        Ok(Expr::BadExpr { from: pos, to: self.pos })
    }

    fn parse_import_expr(&mut self) -> PResult<Expr> {
        let pos = self.pos;
        self.next();
        self.expect(Token::LParen)?;
        if self.token != Token::String {
            self.error_expected(self.pos, "module name")?;
            self.advance();
            return Ok(Expr::BadExpr { from: pos, to: self.pos });
        }
        let module_name = unquote(&self.token_lit).unwrap_or_default();
        self.next();
        self.expect(Token::RParen)?;
        Ok(Expr::ImportExpr { module_name, token_pos: pos })
    }

    fn parse_char_lit(&mut self) -> PResult<Expr> {
        let n = self.token_lit.len();
        if n >= 3 {
            let inner = self.token_lit[1..n - 1].to_vec();
            if let Ok((code, _, _)) = unquote_char(&inner, b'\'') {
                let x = Expr::CharLit { value: code, pos: self.pos, literal: self.token_lit.clone() };
                self.next();
                return Ok(x);
            }
        }
        let pos = self.pos;
        self.error(pos, "illegal char literal".to_string())?;
        self.next();
        Ok(Expr::BadExpr { from: pos, to: self.pos })
    }

    fn parse_func_lit(&mut self) -> PResult<Expr> {
        let ty = self.parse_func_type()?;
        self.expr_level += 1;
        let body = self.parse_body()?;
        self.expr_level -= 1;
        Ok(Expr::FuncLit { ty, body })
    }

    fn parse_array_lit(&mut self) -> PResult<Expr> {
        let lbrack = self.expect(Token::LBrack)?;
        self.expr_level += 1;
        let mut elements = Vec::new();
        while self.token != Token::RBrack && self.token != Token::EOF {
            elements.push(self.parse_expr()?);
            if !self.expect_comma(Token::RBrack, "array element")? {
                break;
            }
        }
        self.expr_level -= 1;
        let rbrack = self.expect(Token::RBrack)?;
        Ok(Expr::ArrayLit { elements, lbrack, rbrack })
    }

    fn parse_error_expr(&mut self) -> PResult<Expr> {
        let pos = self.pos;
        self.next();
        let lparen = self.expect(Token::LParen)?;
        let value = self.parse_expr()?;
        let rparen = self.expect(Token::RParen)?;
        Ok(Expr::ErrorExpr { expr: Box::new(value), error_pos: pos, lparen, rparen })
    }

    fn parse_immutable_expr(&mut self) -> PResult<Expr> {
        let pos = self.pos;
        self.next();
        let lparen = self.expect(Token::LParen)?;
        let value = self.parse_expr()?;
        let rparen = self.expect(Token::RParen)?;
        Ok(Expr::ImmutableExpr { expr: Box::new(value), error_pos: pos, lparen, rparen })
    }

    fn parse_func_type(&mut self) -> PResult<FuncType> {
        let pos = self.expect(Token::Func)?;
        let params = self.parse_ident_list()?;
        Ok(FuncType { func_pos: pos, params })
    }

    fn parse_body(&mut self) -> PResult<BlockStmt> {
        let lbrace = self.expect(Token::LBrace)?;
        let list = self.parse_stmt_list()?;
        let rbrace = self.expect(Token::RBrace)?;
        Ok(BlockStmt { stmts: list, lbrace, rbrace })
    }

    fn parse_stmt_list(&mut self) -> PResult<Vec<Stmt>> {
        let mut list = Vec::new();
        while self.token != Token::RBrace && self.token != Token::EOF {
            list.push(self.parse_stmt()?);
        }
        Ok(list)
    }

    fn parse_ident(&mut self) -> PResult<Ident> {
        let pos = self.pos;
        let mut name = b"_".to_vec();
        if self.token == Token::Ident {
            name = self.token_lit.clone();
            self.next();
        } else {
            self.expect(Token::Ident)?;
        }
        Ok(Ident { name, pos })
    }

    fn parse_ident_list(&mut self) -> PResult<IdentList> {
        let mut params = Vec::new();
        let lparen = self.expect(Token::LParen)?;
        let mut is_var_args = false;
        if self.token != Token::RParen {
            if self.token == Token::Ellipsis {
                is_var_args = true;
                self.next();
            }
            params.push(self.parse_ident()?);
            while !is_var_args && self.token == Token::Comma {
                self.next();
                if self.token == Token::Ellipsis {
                    is_var_args = true;
                    self.next();
                }
                params.push(self.parse_ident()?);
            }
        }
        let rparen = self.expect(Token::RParen)?;
        Ok(IdentList { lparen, var_args: is_var_args, list: params, rparen })
    }

    fn parse_stmt(&mut self) -> PResult<Stmt> {
        match self.token {
            Token::Func
            | Token::Error
            | Token::Immutable
            | Token::Ident
            | Token::Int
            | Token::Float
            | Token::Char
            | Token::String
            | Token::True
            | Token::False
            | Token::Undefined
            | Token::Import
            | Token::LParen
            | Token::LBrace
            | Token::LBrack
            | Token::Add
            | Token::Sub
            | Token::Mul
            | Token::And
            | Token::Xor
            | Token::Not => {
                let s = self.parse_simple_stmt(false)?;
                self.expect_semi()?;
                Ok(s)
            }
            Token::Return => self.parse_return_stmt(),
            Token::Export => self.parse_export_stmt(),
            Token::If => self.parse_if_stmt(),
            Token::For => self.parse_for_stmt(),
            Token::Break | Token::Continue => self.parse_branch_stmt(self.token),
            Token::Semicolon => {
                let s = Stmt::EmptyStmt { semicolon: self.pos, implicit: self.token_lit == b"\n" };
                self.next();
                Ok(s)
            }
            Token::RBrace => Ok(Stmt::EmptyStmt { semicolon: self.pos, implicit: true }),
            _ => {
                let pos = self.pos;
                self.error_expected(pos, "statement")?;
                self.advance();
                Ok(Stmt::BadStmt { from: pos, to: self.pos })
            }
        }
    }

    fn parse_for_stmt(&mut self) -> PResult<Stmt> {
        let pos = self.expect(Token::For)?;
        if self.token == Token::LBrace {
            let body = self.parse_block_stmt()?;
            self.expect_semi()?;
            return Ok(Stmt::ForStmt { for_pos: pos, init: None, cond: None, post: None, body });
        }
        let prev_level = self.expr_level;
        self.expr_level = -1;
        let mut s1: Option<Stmt> = None;
        if self.token != Token::Semicolon {
            s1 = Some(self.parse_simple_stmt(true)?);
        }
        if let Some(Stmt::ForInStmt { key, value, iterable, .. }) = s1.clone() {
            self.expr_level = prev_level;
            let body = self.parse_block_stmt()?;
            self.expect_semi()?;
            return Ok(Stmt::ForInStmt { for_pos: pos, key, value, iterable, body });
        }
        let mut s2: Option<Stmt> = None;
        let mut s3: Option<Stmt> = None;
        if self.token == Token::Semicolon {
            self.next();
            if self.token != Token::Semicolon {
                s2 = Some(self.parse_simple_stmt(false)?);
            }
            self.expect(Token::Semicolon)?;
            if self.token != Token::LBrace {
                s3 = Some(self.parse_simple_stmt(false)?);
            }
        } else {
            s2 = s1.take();
        }
        self.expr_level = prev_level;
        let body = self.parse_block_stmt()?;
        self.expect_semi()?;
        let cond = self.make_expr(s2, "condition expression")?;
        Ok(Stmt::ForStmt { for_pos: pos, init: s1.map(Box::new), cond, post: s3.map(Box::new), body })
    }

    fn parse_branch_stmt(&mut self, tok: Token) -> PResult<Stmt> {
        let pos = self.expect(tok)?;
        let mut label = None;
        if self.token == Token::Ident {
            label = Some(self.parse_ident()?);
        }
        self.expect_semi()?;
        Ok(Stmt::BranchStmt { token: tok, token_pos: pos, label })
    }

    fn parse_if_stmt(&mut self) -> PResult<Stmt> {
        let pos = self.expect(Token::If)?;
        let (init, cond) = self.parse_if_header()?;
        let body = self.parse_block_stmt()?;
        let mut else_stmt = None;
        if self.token == Token::Else {
            self.next();
            match self.token {
                Token::If => else_stmt = Some(Box::new(self.parse_if_stmt()?)),
                Token::LBrace => {
                    let b = self.parse_block_stmt()?;
                    else_stmt = Some(Box::new(Stmt::BlockStmt(b)));
                    self.expect_semi()?;
                }
                _ => {
                    self.error_expected(self.pos, "if or {")?;
                    else_stmt = Some(Box::new(Stmt::BadStmt { from: self.pos, to: self.pos }));
                }
            }
        } else {
            self.expect_semi()?;
        }
        Ok(Stmt::IfStmt { if_pos: pos, init: init.map(Box::new), cond, body, else_: else_stmt })
    }

    fn parse_block_stmt(&mut self) -> PResult<BlockStmt> {
        let lbrace = self.expect(Token::LBrace)?;
        let list = self.parse_stmt_list()?;
        let rbrace = self.expect(Token::RBrace)?;
        Ok(BlockStmt { stmts: list, lbrace, rbrace })
    }

    fn parse_if_header(&mut self) -> PResult<(Option<Stmt>, Option<Expr>)> {
        if self.token == Token::LBrace {
            self.error(self.pos, "missing condition in if statement".to_string())?;
            return Ok((None, Some(Expr::BadExpr { from: self.pos, to: self.pos })));
        }
        let outer = self.expr_level;
        self.expr_level = -1;
        if self.token == Token::Semicolon {
            self.error(self.pos, "missing init in if statement".to_string())?;
            return Ok((None, None));
        }
        let mut init = Some(self.parse_simple_stmt(false)?);
        let mut cond_stmt = None;
        if self.token == Token::LBrace {
            cond_stmt = init.take();
        } else if self.token == Token::Semicolon {
            self.next();
            cond_stmt = Some(self.parse_simple_stmt(false)?);
        } else {
            self.error(self.pos, "missing condition in if statement".to_string())?;
        }
        let mut cond = None;
        if cond_stmt.is_some() {
            cond = self.make_expr(cond_stmt, "boolean expression")?;
        }
        if cond.is_none() {
            cond = Some(Expr::BadExpr { from: self.pos, to: self.pos });
        }
        self.expr_level = outer;
        Ok((init, cond))
    }

    fn make_expr(&mut self, s: Option<Stmt>, want: &str) -> PResult<Option<Expr>> {
        let s = match s {
            None => return Ok(None),
            Some(s) => s,
        };
        if let Stmt::ExprStmt { expr } = s {
            return Ok(Some(expr));
        }
        let found = if matches!(s, Stmt::AssignStmt { .. }) { "assignment" } else { "simple statement" };
        self.error(s.pos(), format!("expected {}, found {}", want, found))?;
        let to = self.safe_pos(s.end());
        Ok(Some(Expr::BadExpr { from: s.pos(), to }))
    }

    fn parse_return_stmt(&mut self) -> PResult<Stmt> {
        let pos = self.pos;
        self.expect(Token::Return)?;
        let mut x = None;
        if self.token != Token::Semicolon && self.token != Token::RBrace {
            x = Some(self.parse_expr()?);
        }
        self.expect_semi()?;
        Ok(Stmt::ReturnStmt { return_pos: pos, result: x })
    }

    fn parse_export_stmt(&mut self) -> PResult<Stmt> {
        let pos = self.pos;
        self.expect(Token::Export)?;
        let x = self.parse_expr()?;
        self.expect_semi()?;
        Ok(Stmt::ExportStmt { export_pos: pos, result: x })
    }

    fn parse_simple_stmt(&mut self, for_in: bool) -> PResult<Stmt> {
        let x = self.parse_expr_list()?;
        match self.token {
            Token::Assign | Token::Define => {
                let pos = self.pos;
                let tok = self.token;
                self.next();
                let y = self.parse_expr_list()?;
                return Ok(Stmt::AssignStmt { lhs: x, rhs: y, token: tok, token_pos: pos });
            }
            Token::In => {
                if for_in {
                    self.next();
                    let y = self.parse_expr()?;
                    let (key, value);
                    match x.len() {
                        1 => {
                            key = Ident { name: b"_".to_vec(), pos: x[0].pos() };
                            value = match &x[0] {
                                Expr::Ident(i) => i.clone(),
                                _ => {
                                    self.error_expected(x[0].pos(), "identifier")?;
                                    Ident { name: b"_".to_vec(), pos: x[0].pos() }
                                }
                            };
                        }
                        2 => {
                            key = match &x[0] {
                                Expr::Ident(i) => i.clone(),
                                _ => {
                                    self.error_expected(x[0].pos(), "identifier")?;
                                    Ident { name: b"_".to_vec(), pos: x[0].pos() }
                                }
                            };
                            value = match &x[1] {
                                Expr::Ident(i) => i.clone(),
                                _ => {
                                    self.error_expected(x[1].pos(), "identifier")?;
                                    Ident { name: b"_".to_vec(), pos: x[1].pos() }
                                }
                            };
                        }
                        _ => {
                            // Go leaves both nil; the statement is reported elsewhere.
                            key = Ident { name: b"_".to_vec(), pos: x[0].pos() };
                            value = Ident { name: b"_".to_vec(), pos: x[0].pos() };
                        }
                    }
                    return Ok(Stmt::ForInStmt {
                        for_pos: NO_POS,
                        key,
                        value,
                        iterable: y,
                        body: BlockStmt { stmts: vec![], lbrace: NO_POS, rbrace: NO_POS },
                    });
                }
            }
            _ => {}
        }
        if x.len() > 1 {
            self.error_expected(x[0].pos(), "1 expression")?;
        }
        match self.token {
            Token::Define
            | Token::AddAssign
            | Token::SubAssign
            | Token::MulAssign
            | Token::QuoAssign
            | Token::RemAssign
            | Token::AndAssign
            | Token::OrAssign
            | Token::XorAssign
            | Token::ShlAssign
            | Token::ShrAssign
            | Token::AndNotAssign => {
                let pos = self.pos;
                let tok = self.token;
                self.next();
                let y = self.parse_expr()?;
                let first = x.into_iter().next().unwrap();
                return Ok(Stmt::AssignStmt { lhs: vec![first], rhs: vec![y], token: tok, token_pos: pos });
            }
            Token::Inc | Token::Dec => {
                let first = x.into_iter().next().unwrap();
                let s = Stmt::IncDecStmt { expr: first, token: self.token, token_pos: self.pos };
                self.next();
                return Ok(s);
            }
            _ => {}
        }
        let first = x.into_iter().next().unwrap();
        Ok(Stmt::ExprStmt { expr: first })
    }

    fn parse_expr_list(&mut self) -> PResult<Vec<Expr>> {
        let mut list = vec![self.parse_expr()?];
        while self.token == Token::Comma {
            self.next();
            list.push(self.parse_expr()?);
        }
        Ok(list)
    }

    fn parse_map_element_lit(&mut self) -> PResult<MapElementLit> {
        let pos = self.pos;
        let mut name = b"_".to_vec();
        if self.token == Token::Ident {
            name = self.token_lit.clone();
        } else if self.token == Token::String {
            name = unquote(&self.token_lit).unwrap_or_default();
        } else {
            self.error_expected(pos, "map key")?;
        }
        self.next();
        let colon_pos = self.expect(Token::Colon)?;
        let value = self.parse_expr()?;
        Ok(MapElementLit { key: name, key_pos: pos, colon_pos, value })
    }

    fn parse_map_lit(&mut self) -> PResult<Expr> {
        let lbrace = self.expect(Token::LBrace)?;
        self.expr_level += 1;
        let mut elements = Vec::new();
        while self.token != Token::RBrace && self.token != Token::EOF {
            elements.push(self.parse_map_element_lit()?);
            if !self.expect_comma(Token::RBrace, "map element")? {
                break;
            }
        }
        self.expr_level -= 1;
        let rbrace = self.expect(Token::RBrace)?;
        Ok(Expr::MapLit { lbrace, elements, rbrace })
    }
}
