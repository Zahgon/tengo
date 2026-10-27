//! Bytecode compiler (compiler.go, symbol_table.go, instructions.go, parser/opcodes.go). The
//! instruction stream, operand widths, dead-code elimination and source maps follow the Go
//! compiler byte for byte, because runtime error positions are looked up from them.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::*;
use crate::objects::{CompiledFunction, Object, MAX_STRING_LEN};
use crate::parser::Parser;
use crate::source::{FileSetRef, Pos, NO_POS};
use crate::stdlib::{get_module, Module};
use crate::token::Token;

// ---------------------------------------------------------------------------------------------
// opcodes

pub const OP_CONSTANT: u8 = 0;
pub const OP_BCOMPLEMENT: u8 = 1;
pub const OP_POP: u8 = 2;
pub const OP_TRUE: u8 = 3;
pub const OP_FALSE: u8 = 4;
pub const OP_EQUAL: u8 = 5;
pub const OP_NOT_EQUAL: u8 = 6;
pub const OP_MINUS: u8 = 7;
pub const OP_LNOT: u8 = 8;
pub const OP_JUMP_FALSY: u8 = 9;
pub const OP_AND_JUMP: u8 = 10;
pub const OP_OR_JUMP: u8 = 11;
pub const OP_JUMP: u8 = 12;
pub const OP_NULL: u8 = 13;
pub const OP_ARRAY: u8 = 14;
pub const OP_MAP: u8 = 15;
pub const OP_ERROR: u8 = 16;
pub const OP_IMMUTABLE: u8 = 17;
pub const OP_INDEX: u8 = 18;
pub const OP_SLICE_INDEX: u8 = 19;
pub const OP_CALL: u8 = 20;
pub const OP_RETURN: u8 = 21;
pub const OP_GET_GLOBAL: u8 = 22;
pub const OP_SET_GLOBAL: u8 = 23;
pub const OP_SET_SEL_GLOBAL: u8 = 24;
pub const OP_GET_LOCAL: u8 = 25;
pub const OP_SET_LOCAL: u8 = 26;
pub const OP_DEFINE_LOCAL: u8 = 27;
pub const OP_SET_SEL_LOCAL: u8 = 28;
pub const OP_GET_FREE_PTR: u8 = 29;
pub const OP_GET_FREE: u8 = 30;
pub const OP_SET_FREE: u8 = 31;
pub const OP_GET_LOCAL_PTR: u8 = 32;
pub const OP_SET_SEL_FREE: u8 = 33;
pub const OP_GET_BUILTIN: u8 = 34;
pub const OP_CLOSURE: u8 = 35;
pub const OP_ITERATOR_INIT: u8 = 36;
pub const OP_ITERATOR_NEXT: u8 = 37;
pub const OP_ITERATOR_KEY: u8 = 38;
pub const OP_ITERATOR_VALUE: u8 = 39;
pub const OP_BINARY_OP: u8 = 40;
pub const OP_SUSPEND: u8 = 41;

const OPERANDS: [&[usize]; 42] = [
    &[2],    // CONST
    &[],     // NEG (bcomplement)
    &[],     // POP
    &[],     // TRUE
    &[],     // FALSE
    &[],     // EQL
    &[],     // NEQ
    &[],     // NEG
    &[],     // NOT
    &[4],    // JMPF
    &[4],    // ANDJMP
    &[4],    // ORJMP
    &[4],    // JMP
    &[],     // NULL
    &[2],    // ARR
    &[2],    // MAP
    &[],     // ERROR
    &[],     // IMMUT
    &[],     // INDEX
    &[],     // SLICE
    &[1, 1], // CALL
    &[1],    // RET
    &[2],    // GETG
    &[2],    // SETG
    &[2, 1], // SETSG
    &[1],    // GETL
    &[1],    // SETL
    &[1],    // DEFL
    &[1, 1], // SETSL
    &[1],    // GETFP
    &[1],    // GETF
    &[1],    // SETF
    &[1],    // GETLP
    &[1, 1], // SETSF
    &[1],    // BUILTIN
    &[2, 1], // CLOSURE
    &[],     // ITER
    &[],     // ITNXT
    &[],     // ITKEY
    &[],     // ITVAL
    &[1],    // BINARYOP
    &[],     // SUSPEND
];

/// A compile failure: the error text, or a Go panic inside the compiler.
#[derive(Debug)]
pub enum CompileFail {
    Error(String),
    Panic(String),
}

type CResult<T> = Result<T, CompileFail>;

fn go_index_panic(i: i64, len: usize) -> CompileFail {
    if i < 0 {
        return CompileFail::Panic(format!("runtime error: index out of range [{}]", i));
    }
    CompileFail::Panic(format!("runtime error: index out of range [{}] with length {}", i, len))
}

fn operands_of(op: u8) -> CResult<&'static [usize]> {
    if (op as usize) < OPERANDS.len() {
        Ok(OPERANDS[op as usize])
    } else {
        Err(go_index_panic(op as i64, OPERANDS.len()))
    }
}

/// MakeInstruction
pub fn make_instruction(op: u8, operands: &[i64]) -> CResult<Vec<u8>> {
    let widths = operands_of(op)?;
    let total: usize = 1 + widths.iter().sum::<usize>();
    let mut inst = vec![0u8; total];
    inst[0] = op;
    let mut offset = 1;
    for (i, &o) in operands.iter().enumerate() {
        if i >= widths.len() {
            return Err(go_index_panic(i as i64, widths.len()));
        }
        let width = widths[i];
        match width {
            1 => inst[offset] = o as u8,
            2 => {
                let n = o as u16;
                inst[offset] = (n >> 8) as u8;
                inst[offset + 1] = n as u8;
            }
            4 => {
                let n = o as u32;
                inst[offset] = (n >> 24) as u8;
                inst[offset + 1] = (n >> 16) as u8;
                inst[offset + 2] = (n >> 8) as u8;
                inst[offset + 3] = n as u8;
            }
            _ => {}
        }
        offset += width;
    }
    Ok(inst)
}

/// ReadOperands
fn read_operands(widths: &[usize], ins: &[u8]) -> CResult<(Vec<i64>, usize)> {
    let mut operands = Vec::new();
    let mut offset = 0;
    let at = |i: usize| -> CResult<i64> {
        if i < ins.len() {
            Ok(ins[i] as i64)
        } else {
            Err(go_index_panic(i as i64, ins.len()))
        }
    };
    for &w in widths {
        match w {
            1 => operands.push(at(offset)?),
            2 => operands.push(at(offset + 1)? | at(offset)? << 8),
            4 => operands.push(at(offset + 3)? | at(offset + 2)? << 8 | at(offset + 1)? << 16 | at(offset)? << 24),
            _ => {}
        }
        offset += w;
    }
    Ok((operands, offset))
}

fn iterate_instructions(b: &[u8], mut f: impl FnMut(usize, u8, &[i64]) -> CResult<()>) -> CResult<()> {
    let mut i = 0;
    while i < b.len() {
        let widths = operands_of(b[i])?;
        let (operands, read) = read_operands(widths, &b[i + 1..])?;
        f(i, b[i], &operands)?;
        i += read + 1;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// symbol table

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Scope {
    Global,
    Local,
    Builtin,
    Free,
}

#[derive(Debug)]
pub struct Symbol {
    pub name: Vec<u8>,
    pub scope: Scope,
    pub index: i64,
    pub local_assigned: bool,
}

pub type SymRef = Rc<RefCell<Symbol>>;

pub struct SymbolTable {
    parent: Option<TableRef>,
    block: bool,
    store: HashMap<Vec<u8>, SymRef>,
    num_definition: i64,
    max_definition: i64,
    free_symbols: Vec<SymRef>,
    builtin_symbols: Vec<SymRef>,
}

pub type TableRef = Rc<RefCell<SymbolTable>>;

pub fn new_symbol_table() -> TableRef {
    Rc::new(RefCell::new(SymbolTable {
        parent: None,
        block: false,
        store: HashMap::new(),
        num_definition: 0,
        max_definition: 0,
        free_symbols: Vec::new(),
        builtin_symbols: Vec::new(),
    }))
}

fn st_parent(t: &TableRef, skip_block: bool) -> Option<TableRef> {
    let tb = t.borrow();
    if skip_block && tb.block {
        let p = tb.parent.clone().unwrap();
        drop(tb);
        return st_parent(&p, skip_block);
    }
    tb.parent.clone()
}

fn st_next_index(t: &TableRef) -> i64 {
    let tb = t.borrow();
    if tb.block {
        let p = tb.parent.clone().unwrap();
        let n = tb.num_definition;
        drop(tb);
        return st_next_index(&p) + n;
    }
    tb.num_definition
}

fn st_update_max_defs(t: &TableRef, n: i64) {
    let mut tb = t.borrow_mut();
    if n > tb.max_definition {
        tb.max_definition = n;
    }
    if tb.block {
        let p = tb.parent.clone().unwrap();
        drop(tb);
        st_update_max_defs(&p, n);
    }
}

pub fn st_define(t: &TableRef, name: &[u8]) -> SymRef {
    let index = st_next_index(t);
    let sym = Rc::new(RefCell::new(Symbol { name: name.to_vec(), scope: Scope::Local, index, local_assigned: false }));
    t.borrow_mut().num_definition += 1;
    if st_parent(t, true).is_none() {
        sym.borrow_mut().scope = Scope::Global;
        let parent = t.borrow().parent.clone();
        if let Some(mut p) = parent {
            loop {
                let next = p.borrow().parent.clone();
                match next {
                    Some(n) => p = n,
                    None => break,
                }
            }
            t.borrow_mut().num_definition -= 1;
            p.borrow_mut().num_definition += 1;
        }
    }
    t.borrow_mut().store.insert(name.to_vec(), sym.clone());
    st_update_max_defs(t, index + 1);
    sym
}

pub fn st_define_builtin(t: &TableRef, index: i64, name: &[u8]) -> SymRef {
    let parent = t.borrow().parent.clone();
    if let Some(p) = parent {
        return st_define_builtin(&p, index, name);
    }
    let sym = Rc::new(RefCell::new(Symbol { name: name.to_vec(), scope: Scope::Builtin, index, local_assigned: false }));
    let mut tb = t.borrow_mut();
    tb.store.insert(name.to_vec(), sym.clone());
    tb.builtin_symbols.push(sym.clone());
    sym
}

fn st_define_free(t: &TableRef, original: &SymRef) -> SymRef {
    let mut tb = t.borrow_mut();
    tb.free_symbols.push(original.clone());
    let sym = Rc::new(RefCell::new(Symbol {
        name: original.borrow().name.clone(),
        index: tb.free_symbols.len() as i64 - 1,
        scope: Scope::Free,
        local_assigned: false,
    }));
    let name = original.borrow().name.clone();
    tb.store.insert(name, sym.clone());
    sym
}

pub fn st_resolve(t: &TableRef, name: &[u8], recur: bool) -> Option<(SymRef, i64)> {
    let found = t.borrow().store.get(name).cloned();
    if let Some(sym) = found {
        let s = sym.borrow();
        if s.scope != Scope::Local || s.local_assigned || recur {
            drop(s);
            return Some((sym, 0));
        }
    }
    let parent = t.borrow().parent.clone()?;
    let (sym, depth) = st_resolve(&parent, name, true)?;
    let depth = depth + 1;
    let block = t.borrow().block;
    let scope = sym.borrow().scope;
    if !block && depth > 0 && scope != Scope::Global && scope != Scope::Builtin {
        return Some((st_define_free(t, &sym), depth));
    }
    Some((sym, depth))
}

pub fn st_fork(t: &TableRef, block: bool) -> TableRef {
    Rc::new(RefCell::new(SymbolTable {
        parent: Some(t.clone()),
        block,
        store: HashMap::new(),
        num_definition: 0,
        max_definition: 0,
        free_symbols: Vec::new(),
        builtin_symbols: Vec::new(),
    }))
}

fn st_builtin_symbols(t: &TableRef) -> Vec<SymRef> {
    let parent = t.borrow().parent.clone();
    if let Some(p) = parent {
        return st_builtin_symbols(&p);
    }
    t.borrow().builtin_symbols.clone()
}

// ---------------------------------------------------------------------------------------------
// compiler

struct CompilationScope {
    instructions: Vec<u8>,
    source_map: HashMap<usize, Pos>,
}

#[derive(Default)]
struct Loop {
    continues: Vec<usize>,
    breaks: Vec<usize>,
}

/// State shared by a compiler and the module compilers it forks (the root's constants and
/// compiled-module cache).
pub struct Shared {
    pub constants: Vec<Object>,
    compiled_modules: HashMap<String, Rc<CompiledFunction>>,
    pub fset: FileSetRef,
}

pub struct Compiler {
    shared: Rc<RefCell<Shared>>,
    /// Index of this compiler's source file in the shared file set.
    pub file: usize,
    /// modulePath of this compiler and of every parent, innermost first.
    module_chain: Vec<String>,
    has_parent: bool,
    allow_file_import: bool,
    import_dir: String,
    symbol_table: TableRef,
    scopes: Vec<CompilationScope>,
    scope_index: usize,
    loops: Vec<Loop>,
}

pub const BUILTIN_NAMES: [&str; 31] = [
    "len",
    "copy",
    "append",
    "delete",
    "splice",
    "string",
    "int",
    "bool",
    "float",
    "char",
    "bytes",
    "time",
    "is_int",
    "is_float",
    "is_string",
    "is_bool",
    "is_char",
    "is_bytes",
    "is_array",
    "is_immutable_array",
    "is_map",
    "is_immutable_map",
    "is_iterable",
    "is_time",
    "is_error",
    "is_undefined",
    "is_function",
    "is_callable",
    "type_name",
    "format",
    "range",
];

/// The main function and constants produced by Compiler.Bytecode().
pub struct Bytecode {
    pub main: Rc<CompiledFunction>,
    pub constants: Vec<Object>,
}

impl Compiler {
    pub fn new(fset: FileSetRef, file: usize, symbol_table: Option<TableRef>) -> Compiler {
        let shared = Rc::new(RefCell::new(Shared { constants: Vec::new(), compiled_modules: HashMap::new(), fset }));
        Compiler::with_shared(shared, file, symbol_table)
    }

    fn with_shared(shared: Rc<RefCell<Shared>>, file: usize, symbol_table: Option<TableRef>) -> Compiler {
        let st = symbol_table.unwrap_or_else(new_symbol_table);
        for (idx, name) in BUILTIN_NAMES.iter().enumerate() {
            st_define_builtin(&st, idx as i64, name.as_bytes());
        }
        // "freeze" is the 32nd builtin
        st_define_builtin(&st, 31, b"freeze");
        Compiler {
            shared,
            file,
            module_chain: vec![String::new()],
            has_parent: false,
            allow_file_import: false,
            import_dir: String::new(),
            symbol_table: st,
            scopes: vec![CompilationScope { instructions: Vec::new(), source_map: HashMap::new() }],
            scope_index: 0,
            loops: Vec::new(),
        }
    }

    pub fn enable_file_import(&mut self, enable: bool) {
        self.allow_file_import = enable;
    }

    fn errorf(&self, pos: Pos, msg: String) -> CompileFail {
        let p = self.shared.borrow().fset.borrow_mut().position(pos);
        CompileFail::Error(format!("Compile Error: {}\n\tat {}", msg, p))
    }

    fn cur(&mut self) -> &mut CompilationScope {
        &mut self.scopes[self.scope_index]
    }

    fn emit(&mut self, pos: Pos, op: u8, operands: &[i64]) -> CResult<usize> {
        let inst = make_instruction(op, operands)?;
        let scope = self.cur();
        let p = scope.instructions.len();
        scope.instructions.extend_from_slice(&inst);
        scope.source_map.insert(p, pos);
        Ok(p)
    }

    fn add_constant(&mut self, o: Object) -> i64 {
        let mut sh = self.shared.borrow_mut();
        sh.constants.push(o);
        sh.constants.len() as i64 - 1
    }

    fn change_operand(&mut self, op_pos: usize, operand: i64) -> CResult<()> {
        let len = self.cur().instructions.len();
        if op_pos >= len {
            return Err(go_index_panic(op_pos as i64, len));
        }
        let op = self.cur().instructions[op_pos];
        let inst = make_instruction(op, &[operand])?;
        let ins = &mut self.cur().instructions;
        // copy() writes as many bytes as fit
        for (k, b) in inst.iter().enumerate() {
            if op_pos + k < ins.len() {
                ins[op_pos + k] = *b;
            }
        }
        Ok(())
    }

    fn enter_scope(&mut self) {
        self.scopes.push(CompilationScope { instructions: Vec::new(), source_map: HashMap::new() });
        self.scope_index += 1;
        self.symbol_table = st_fork(&self.symbol_table, false);
    }

    fn leave_scope(&mut self) -> (Vec<u8>, HashMap<usize, Pos>) {
        let s = self.scopes.pop().unwrap();
        self.scope_index -= 1;
        self.symbol_table = st_parent(&self.symbol_table, true).unwrap();
        (s.instructions, s.source_map)
    }

    fn fork_block(&mut self) {
        self.symbol_table = st_fork(&self.symbol_table, true);
    }

    fn unfork_block(&mut self) {
        self.symbol_table = st_parent(&self.symbol_table, false).unwrap();
    }

    pub fn compile_file(&mut self, file: &File) -> CResult<()> {
        for s in &file.stmts {
            self.compile_stmt(s)?;
        }
        Ok(())
    }

    fn compile_stmt(&mut self, stmt: &Stmt) -> CResult<()> {
        match stmt {
            Stmt::ExprStmt { expr } => {
                self.compile_expr(expr)?;
                self.emit(stmt.pos(), OP_POP, &[])?;
            }
            Stmt::IncDecStmt { expr, token, .. } => {
                let op = if *token == Token::Dec { Token::SubAssign } else { Token::AddAssign };
                let one = Expr::IntLit { value: 1, pos: NO_POS, literal: Vec::new() };
                return self.compile_assign(stmt.pos(), std::slice::from_ref(expr), std::slice::from_ref(&one), op);
            }
            Stmt::IfStmt { init, cond, body, else_, .. } => {
                self.fork_block();
                let r = (|| -> CResult<()> {
                    if let Some(init) = init {
                        self.compile_stmt(init)?;
                    }
                    if let Some(cond) = cond {
                        self.compile_expr(cond)?;
                    }
                    let jump1 = self.emit(stmt.pos(), OP_JUMP_FALSY, &[0])?;
                    self.compile_block(body)?;
                    if let Some(e) = else_ {
                        let jump2 = self.emit(stmt.pos(), OP_JUMP, &[0])?;
                        let cur = self.cur().instructions.len() as i64;
                        self.change_operand(jump1, cur)?;
                        self.compile_stmt(e)?;
                        let cur = self.cur().instructions.len() as i64;
                        self.change_operand(jump2, cur)?;
                    } else {
                        let cur = self.cur().instructions.len() as i64;
                        self.change_operand(jump1, cur)?;
                    }
                    Ok(())
                })();
                self.unfork_block();
                r?;
            }
            Stmt::ForStmt { init, cond, post, body, .. } => return self.compile_for(stmt.pos(), init, cond, post, body),
            Stmt::ForInStmt { key, value, iterable, body, .. } => {
                return self.compile_for_in(stmt.pos(), key, value, iterable, body)
            }
            Stmt::BranchStmt { token, .. } => {
                if *token == Token::Break {
                    if self.loops.is_empty() {
                        return Err(self.errorf(stmt.pos(), "break not allowed outside loop".into()));
                    }
                    let p = self.emit(stmt.pos(), OP_JUMP, &[0])?;
                    self.loops.last_mut().unwrap().breaks.push(p);
                } else if *token == Token::Continue {
                    if self.loops.is_empty() {
                        return Err(self.errorf(stmt.pos(), "continue not allowed outside loop".into()));
                    }
                    let p = self.emit(stmt.pos(), OP_JUMP, &[0])?;
                    self.loops.last_mut().unwrap().continues.push(p);
                } else {
                    return Err(CompileFail::Panic(format!("invalid branch statement: {}", token.string())));
                }
            }
            Stmt::BlockStmt(b) => self.compile_block(b)?,
            Stmt::AssignStmt { lhs, rhs, token, .. } => self.compile_assign(stmt.pos(), lhs, rhs, *token)?,
            Stmt::ReturnStmt { result, .. } => {
                if st_parent(&self.symbol_table, true).is_none() {
                    return Err(self.errorf(stmt.pos(), "return not allowed outside function".into()));
                }
                match result {
                    None => {
                        self.emit(stmt.pos(), OP_RETURN, &[0])?;
                    }
                    Some(r) => {
                        self.compile_expr(r)?;
                        self.emit(stmt.pos(), OP_RETURN, &[1])?;
                    }
                }
            }
            Stmt::ExportStmt { result, .. } => {
                if self.scope_index != 0 {
                    return Err(self.errorf(stmt.pos(), "export not allowed inside function".into()));
                }
                if !self.has_parent {
                    return Ok(());
                }
                self.compile_expr(result)?;
                self.emit(stmt.pos(), OP_IMMUTABLE, &[])?;
                self.emit(stmt.pos(), OP_RETURN, &[1])?;
            }
            Stmt::EmptyStmt { .. } | Stmt::BadStmt { .. } => {}
        }
        Ok(())
    }

    fn compile_block(&mut self, b: &BlockStmt) -> CResult<()> {
        if b.stmts.is_empty() {
            return Ok(());
        }
        self.fork_block();
        let mut r = Ok(());
        for s in &b.stmts {
            r = self.compile_stmt(s);
            if r.is_err() {
                break;
            }
        }
        self.unfork_block();
        r
    }

    fn compile_expr(&mut self, expr: &Expr) -> CResult<()> {
        let pos = expr.pos();
        match expr {
            Expr::ParenExpr { expr: e, .. } => self.compile_expr(e)?,
            Expr::BinaryExpr { lhs, rhs, token, .. } => {
                if *token == Token::LAnd || *token == Token::LOr {
                    return self.compile_logical(pos, lhs, rhs, *token);
                }
                self.compile_expr(lhs)?;
                self.compile_expr(rhs)?;
                match token {
                    Token::Add
                    | Token::Sub
                    | Token::Mul
                    | Token::Quo
                    | Token::Rem
                    | Token::Greater
                    | Token::GreaterEq
                    | Token::Less
                    | Token::LessEq
                    | Token::And
                    | Token::Or
                    | Token::Xor
                    | Token::AndNot
                    | Token::Shl
                    | Token::Shr => {
                        self.emit(pos, OP_BINARY_OP, &[*token as i64])?;
                    }
                    Token::Equal => {
                        self.emit(pos, OP_EQUAL, &[])?;
                    }
                    Token::NotEqual => {
                        self.emit(pos, OP_NOT_EQUAL, &[])?;
                    }
                    _ => {
                        return Err(self.errorf(pos, format!("invalid binary operator: {}", token.string())));
                    }
                }
            }
            Expr::IntLit { value, .. } => {
                let c = self.add_constant(Object::Int(*value));
                self.emit(pos, OP_CONSTANT, &[c])?;
            }
            Expr::FloatLit { value, .. } => {
                let c = self.add_constant(Object::Float(*value));
                self.emit(pos, OP_CONSTANT, &[c])?;
            }
            Expr::BoolLit { value, .. } => {
                self.emit(pos, if *value { OP_TRUE } else { OP_FALSE }, &[])?;
            }
            Expr::StringLit { value, .. } => {
                if value.len() > MAX_STRING_LEN {
                    return Err(self.errorf(pos, "exceeding string size limit".into()));
                }
                let c = self.add_constant(Object::String(Rc::new(value.clone())));
                self.emit(pos, OP_CONSTANT, &[c])?;
            }
            Expr::CharLit { value, .. } => {
                let c = self.add_constant(Object::Char(*value));
                self.emit(pos, OP_CONSTANT, &[c])?;
            }
            Expr::UndefinedLit { .. } => {
                self.emit(pos, OP_NULL, &[])?;
            }
            Expr::UnaryExpr { expr: e, token, .. } => {
                self.compile_expr(e)?;
                match token {
                    Token::Not => {
                        self.emit(pos, OP_LNOT, &[])?;
                    }
                    Token::Sub => {
                        self.emit(pos, OP_MINUS, &[])?;
                    }
                    Token::Xor => {
                        self.emit(pos, OP_BCOMPLEMENT, &[])?;
                    }
                    Token::Add => {}
                    _ => return Err(self.errorf(pos, format!("invalid unary operator: {}", token.string()))),
                }
            }
            Expr::Ident(id) => {
                let r = st_resolve(&self.symbol_table, &id.name, false);
                let (sym, _) = match r {
                    Some(r) => r,
                    None => {
                        return Err(self.errorf(
                            pos,
                            format!("unresolved reference '{}'", String::from_utf8_lossy(&id.name)),
                        ))
                    }
                };
                let (scope, index) = {
                    let s = sym.borrow();
                    (s.scope, s.index)
                };
                match scope {
                    Scope::Global => self.emit(pos, OP_GET_GLOBAL, &[index])?,
                    Scope::Local => self.emit(pos, OP_GET_LOCAL, &[index])?,
                    Scope::Builtin => self.emit(pos, OP_GET_BUILTIN, &[index])?,
                    Scope::Free => self.emit(pos, OP_GET_FREE, &[index])?,
                };
            }
            Expr::ArrayLit { elements, .. } => {
                for e in elements {
                    self.compile_expr(e)?;
                }
                self.emit(pos, OP_ARRAY, &[elements.len() as i64])?;
            }
            Expr::MapLit { elements, .. } => {
                for elt in elements {
                    if elt.key.len() > MAX_STRING_LEN {
                        return Err(self.errorf(pos, "exceeding string size limit".into()));
                    }
                    let c = self.add_constant(Object::String(Rc::new(elt.key.clone())));
                    self.emit(pos, OP_CONSTANT, &[c])?;
                    self.compile_expr(&elt.value)?;
                }
                self.emit(pos, OP_MAP, &[elements.len() as i64 * 2])?;
            }
            Expr::SelectorExpr { expr: e, sel } => {
                self.compile_expr(e)?;
                self.compile_expr(sel)?;
                self.emit(pos, OP_INDEX, &[])?;
            }
            Expr::IndexExpr { expr: e, index, .. } => {
                self.compile_expr(e)?;
                match index {
                    Some(i) => self.compile_expr(i)?,
                    None => return Err(CompileFail::Panic(
                        "runtime error: invalid memory address or nil pointer dereference".into(),
                    )),
                }
                self.emit(pos, OP_INDEX, &[])?;
            }
            Expr::SliceExpr { expr: e, low, high, .. } => {
                self.compile_expr(e)?;
                match low {
                    Some(l) => self.compile_expr(l)?,
                    None => {
                        self.emit(pos, OP_NULL, &[])?;
                    }
                }
                match high {
                    Some(h) => self.compile_expr(h)?,
                    None => {
                        self.emit(pos, OP_NULL, &[])?;
                    }
                }
                self.emit(pos, OP_SLICE_INDEX, &[])?;
            }
            Expr::FuncLit { ty, body } => self.compile_func_lit(pos, ty, body)?,
            Expr::CallExpr { func, args, ellipsis, .. } => {
                self.compile_expr(func)?;
                for a in args {
                    self.compile_expr(a)?;
                }
                let ell = if *ellipsis != NO_POS { 1 } else { 0 };
                self.emit(pos, OP_CALL, &[args.len() as i64, ell])?;
            }
            Expr::ImportExpr { module_name, .. } => self.compile_import(pos, module_name)?,
            Expr::ErrorExpr { expr: e, .. } => {
                self.compile_expr(e)?;
                self.emit(pos, OP_ERROR, &[])?;
            }
            Expr::ImmutableExpr { expr: e, .. } => {
                self.compile_expr(e)?;
                self.emit(pos, OP_IMMUTABLE, &[])?;
            }
            Expr::CondExpr { cond, t, f, .. } => {
                self.compile_expr(cond)?;
                let jump1 = self.emit(pos, OP_JUMP_FALSY, &[0])?;
                self.compile_expr(t)?;
                let jump2 = self.emit(pos, OP_JUMP, &[0])?;
                let cur = self.cur().instructions.len() as i64;
                self.change_operand(jump1, cur)?;
                self.compile_expr(f)?;
                let cur = self.cur().instructions.len() as i64;
                self.change_operand(jump2, cur)?;
            }
            Expr::BadExpr { .. } => {}
        }
        Ok(())
    }

    fn compile_func_lit(&mut self, pos: Pos, ty: &FuncType, body: &BlockStmt) -> CResult<()> {
        self.enter_scope();
        for p in &ty.params.list {
            let s = st_define(&self.symbol_table, &p.name);
            s.borrow_mut().local_assigned = true;
        }
        self.compile_block(body)?;
        self.optimize_func(pos)?;
        let free_symbols = self.symbol_table.borrow().free_symbols.clone();
        let num_locals = self.symbol_table.borrow().max_definition;
        let (instructions, source_map) = self.leave_scope();
        for s in &free_symbols {
            let (scope, index, assigned) = {
                let b = s.borrow();
                (b.scope, b.index, b.local_assigned)
            };
            match scope {
                Scope::Local => {
                    if !assigned {
                        self.emit(pos, OP_NULL, &[])?;
                        self.emit(pos, OP_DEFINE_LOCAL, &[index])?;
                        s.borrow_mut().local_assigned = true;
                    }
                    self.emit(pos, OP_GET_LOCAL_PTR, &[index])?;
                }
                Scope::Free => {
                    self.emit(pos, OP_GET_FREE_PTR, &[index])?;
                }
                _ => {}
            }
        }
        let cf = CompiledFunction {
            instructions: Rc::new(instructions),
            num_locals: num_locals as usize,
            num_parameters: ty.params.list.len(),
            var_args: ty.params.var_args,
            source_map: Some(Rc::new(source_map)),
            free: Vec::new(),
        };
        let c = self.add_constant(Object::CompiledFunction(Rc::new(cf)));
        if !free_symbols.is_empty() {
            self.emit(pos, OP_CLOSURE, &[c, free_symbols.len() as i64])?;
        } else {
            self.emit(pos, OP_CONSTANT, &[c])?;
        }
        Ok(())
    }

    fn compile_import(&mut self, pos: Pos, module_name: &[u8]) -> CResult<()> {
        if module_name.is_empty() {
            return Err(self.errorf(pos, "empty module name".into()));
        }
        let name = String::from_utf8_lossy(module_name).to_string();
        if let Some(m) = get_module(&name) {
            match m {
                Module::Source(src) => {
                    let compiled = self.compile_module(pos, &name, src.as_bytes().to_vec(), false)?;
                    let c = self.add_constant(Object::CompiledFunction(compiled));
                    self.emit(pos, OP_CONSTANT, &[c])?;
                    self.emit(pos, OP_CALL, &[0, 0])?;
                }
                Module::Builtin(obj) => {
                    let c = self.add_constant(obj);
                    self.emit(pos, OP_CONSTANT, &[c])?;
                }
            }
        } else if self.allow_file_import {
            let module_path = match self.get_path_module(&name) {
                Ok(p) => p,
                Err(e) => return Err(self.errorf(pos, format!("module file path error: {}", e))),
            };
            let src = match std::fs::read(&module_path) {
                Ok(s) => s,
                Err(e) => {
                    return Err(self.errorf(pos, format!("module file read error: {}", go_read_error(&module_path, &e))))
                }
            };
            let compiled = self.compile_module(pos, &module_path, src, true)?;
            let c = self.add_constant(Object::CompiledFunction(compiled));
            self.emit(pos, OP_CONSTANT, &[c])?;
            self.emit(pos, OP_CALL, &[0, 0])?;
        } else {
            return Err(self.errorf(pos, format!("module '{}' not found", name)));
        }
        Ok(())
    }

    fn get_path_module(&self, module_name: &str) -> Result<String, String> {
        let mut path_file = String::new();
        for ext in [".tengo"] {
            let mut name_file = module_name.to_string();
            if !name_file.ends_with(ext) {
                name_file.push_str(ext);
            }
            path_file = abs_path(&join_path(&self.import_dir, &name_file));
            match std::fs::metadata(&path_file) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                _ => return Ok(path_file),
            }
        }
        Err(format!("module '{}' not found at: {}", module_name, path_file))
    }

    fn compile_module(&mut self, pos: Pos, module_path: &str, src: Vec<u8>, is_file: bool) -> CResult<Rc<CompiledFunction>> {
        if self.module_chain.iter().any(|m| m == module_path) {
            return Err(self.errorf(pos, format!("cyclic module import: {}", module_path)));
        }
        if let Some(m) = self.shared.borrow().compiled_modules.get(module_path) {
            return Ok(m.clone());
        }
        let fset = self.shared.borrow().fset.clone();
        let mod_file = fset.borrow_mut().add_file(module_path, -1, src.len() as i64);
        let p = Parser::new(fset.clone(), mod_file, src);
        let file = match p.parse_file() {
            Ok(f) => f,
            Err(el) => return Err(CompileFail::Error(el.error())),
        };
        let st = new_symbol_table();
        for sym in st_builtin_symbols(&self.symbol_table) {
            let (idx, name) = {
                let s = sym.borrow();
                (s.index, s.name.clone())
            };
            st_define_builtin(&st, idx, &name);
        }
        let st = st_fork(&st, false);
        let mut child = Compiler::with_shared(self.shared.clone(), mod_file, Some(st.clone()));
        let mut chain = vec![module_path.to_string()];
        chain.extend(self.module_chain.iter().cloned());
        child.module_chain = chain;
        child.has_parent = true;
        child.allow_file_import = self.allow_file_import;
        child.import_dir = self.import_dir.clone();
        if is_file && !self.import_dir.is_empty() {
            child.import_dir = dir_path(module_path);
        }
        child.compile_file(&file)?;
        child.optimize_func(pos)?;
        let mut instructions = child.scopes[0].instructions.clone();
        instructions.push(OP_SUSPEND);
        let source_map = child.scopes[0].source_map.clone();
        let cf = Rc::new(CompiledFunction {
            instructions: Rc::new(instructions),
            num_locals: st.borrow().max_definition as usize,
            num_parameters: 0,
            var_args: false,
            source_map: Some(Rc::new(source_map)),
            free: Vec::new(),
        });
        self.shared.borrow_mut().compiled_modules.insert(module_path.to_string(), cf.clone());
        Ok(cf)
    }

    fn compile_assign(&mut self, pos: Pos, lhs: &[Expr], rhs: &[Expr], op: Token) -> CResult<()> {
        if lhs.len() > 1 || rhs.len() > 1 {
            return Err(self.errorf(pos, "tuple assignment not allowed".into()));
        }
        let (ident, selectors) = resolve_assign_lhs(&lhs[0]);
        let num_sel = selectors.len();
        if op == Token::Define && num_sel > 0 {
            return Err(self.errorf(pos, "operator ':=' not allowed with selector".into()));
        }
        let is_func = matches!(rhs[0], Expr::FuncLit { .. });
        let resolved = st_resolve(&self.symbol_table, &ident, false);
        let mut symbol: Option<SymRef> = resolved.as_ref().map(|(s, _)| s.clone());
        let (depth, exists) = match &resolved {
            Some((_, d)) => (*d, true),
            None => (0, false),
        };
        if op == Token::Define {
            if depth == 0 && exists {
                return Err(self.errorf(
                    pos,
                    format!("'{}' redeclared in this block", String::from_utf8_lossy(&ident)),
                ));
            }
            if is_func {
                symbol = Some(st_define(&self.symbol_table, &ident));
            }
        } else if !exists {
            return Err(self.errorf(pos, format!("unresolved reference '{}'", String::from_utf8_lossy(&ident))));
        }

        if op != Token::Assign && op != Token::Define {
            self.compile_expr(&lhs[0])?;
        }
        for e in rhs {
            self.compile_expr(e)?;
        }
        if op == Token::Define && !is_func {
            symbol = Some(st_define(&self.symbol_table, &ident));
        }
        let binop = match op {
            Token::AddAssign => Some(Token::Add),
            Token::SubAssign => Some(Token::Sub),
            Token::MulAssign => Some(Token::Mul),
            Token::QuoAssign => Some(Token::Quo),
            Token::RemAssign => Some(Token::Rem),
            Token::AndAssign => Some(Token::And),
            Token::OrAssign => Some(Token::Or),
            Token::AndNotAssign => Some(Token::AndNot),
            Token::XorAssign => Some(Token::Xor),
            Token::ShlAssign => Some(Token::Shl),
            Token::ShrAssign => Some(Token::Shr),
            _ => None,
        };
        if let Some(b) = binop {
            self.emit(pos, OP_BINARY_OP, &[b as i64])?;
        }
        for i in (0..num_sel).rev() {
            self.compile_expr(&selectors[i])?;
        }
        let symbol = match symbol {
            Some(s) => s,
            None => {
                return Err(CompileFail::Panic(
                    "runtime error: invalid memory address or nil pointer dereference".into(),
                ))
            }
        };
        let (scope, index, assigned) = {
            let s = symbol.borrow();
            (s.scope, s.index, s.local_assigned)
        };
        match scope {
            Scope::Global => {
                if num_sel > 0 {
                    self.emit(pos, OP_SET_SEL_GLOBAL, &[index, num_sel as i64])?;
                } else {
                    self.emit(pos, OP_SET_GLOBAL, &[index])?;
                }
            }
            Scope::Local => {
                if num_sel > 0 {
                    self.emit(pos, OP_SET_SEL_LOCAL, &[index, num_sel as i64])?;
                } else if op == Token::Define && !assigned {
                    self.emit(pos, OP_DEFINE_LOCAL, &[index])?;
                } else {
                    self.emit(pos, OP_SET_LOCAL, &[index])?;
                }
                symbol.borrow_mut().local_assigned = true;
            }
            Scope::Free => {
                if num_sel > 0 {
                    self.emit(pos, OP_SET_SEL_FREE, &[index, num_sel as i64])?;
                } else {
                    self.emit(pos, OP_SET_FREE, &[index])?;
                }
            }
            Scope::Builtin => {
                return Err(CompileFail::Panic("invalid assignment variable scope: BUILTIN".into()));
            }
        }
        Ok(())
    }

    fn compile_logical(&mut self, pos: Pos, lhs: &Expr, rhs: &Expr, token: Token) -> CResult<()> {
        self.compile_expr(lhs)?;
        let jump = if token == Token::LAnd {
            self.emit(pos, OP_AND_JUMP, &[0])?
        } else {
            self.emit(pos, OP_OR_JUMP, &[0])?
        };
        self.compile_expr(rhs)?;
        let cur = self.cur().instructions.len() as i64;
        self.change_operand(jump, cur)
    }

    fn compile_for(
        &mut self,
        pos: Pos,
        init: &Option<Box<Stmt>>,
        cond: &Option<Expr>,
        post: &Option<Box<Stmt>>,
        body: &BlockStmt,
    ) -> CResult<()> {
        self.fork_block();
        let r = (|| -> CResult<()> {
            if let Some(init) = init {
                self.compile_stmt(init)?;
            }
            let pre_cond_pos = self.cur().instructions.len() as i64;
            let mut post_cond_pos: Option<usize> = None;
            if let Some(cond) = cond {
                self.compile_expr(cond)?;
                post_cond_pos = Some(self.emit(pos, OP_JUMP_FALSY, &[0])?);
            }
            self.loops.push(Loop::default());
            if let Err(e) = self.compile_block(body) {
                self.loops.pop();
                return Err(e);
            }
            let lp = self.loops.pop().unwrap();
            let post_body_pos = self.cur().instructions.len() as i64;
            if let Some(post) = post {
                self.compile_stmt(post)?;
            }
            self.emit(pos, OP_JUMP, &[pre_cond_pos])?;
            let post_stmt_pos = self.cur().instructions.len() as i64;
            if let Some(p) = post_cond_pos {
                self.change_operand(p, post_stmt_pos)?;
            }
            for p in lp.breaks {
                self.change_operand(p, post_stmt_pos)?;
            }
            for p in lp.continues {
                self.change_operand(p, post_body_pos)?;
            }
            Ok(())
        })();
        self.unfork_block();
        r
    }

    fn compile_for_in(&mut self, pos: Pos, key: &Ident, value: &Ident, iterable: &Expr, body: &BlockStmt) -> CResult<()> {
        self.fork_block();
        let r = (|| -> CResult<()> {
            let it = st_define(&self.symbol_table, b":it");
            let (it_scope, it_index) = {
                let s = it.borrow();
                (s.scope, s.index)
            };
            self.compile_expr(iterable)?;
            self.emit(pos, OP_ITERATOR_INIT, &[])?;
            if it_scope == Scope::Global {
                self.emit(pos, OP_SET_GLOBAL, &[it_index])?;
            } else {
                self.emit(pos, OP_DEFINE_LOCAL, &[it_index])?;
            }
            let pre_cond_pos = self.cur().instructions.len() as i64;
            let get_it = if it_scope == Scope::Global { OP_GET_GLOBAL } else { OP_GET_LOCAL };
            self.emit(pos, get_it, &[it_index])?;
            self.emit(pos, OP_ITERATOR_NEXT, &[])?;
            let post_cond_pos = self.emit(pos, OP_JUMP_FALSY, &[0])?;
            self.loops.push(Loop::default());
            for (ident, op) in [(key, OP_ITERATOR_KEY), (value, OP_ITERATOR_VALUE)] {
                if ident.name != b"_" {
                    let sym = st_define(&self.symbol_table, &ident.name);
                    self.emit(pos, get_it, &[it_index])?;
                    self.emit(pos, op, &[])?;
                    let (sc, idx) = {
                        let s = sym.borrow();
                        (s.scope, s.index)
                    };
                    if sc == Scope::Global {
                        self.emit(pos, OP_SET_GLOBAL, &[idx])?;
                    } else {
                        sym.borrow_mut().local_assigned = true;
                        self.emit(pos, OP_DEFINE_LOCAL, &[idx])?;
                    }
                }
            }
            if let Err(e) = self.compile_block(body) {
                self.loops.pop();
                return Err(e);
            }
            let lp = self.loops.pop().unwrap();
            let post_body_pos = self.cur().instructions.len() as i64;
            self.emit(pos, OP_JUMP, &[pre_cond_pos])?;
            let post_stmt_pos = self.cur().instructions.len() as i64;
            self.change_operand(post_cond_pos, post_stmt_pos)?;
            for p in lp.breaks {
                self.change_operand(p, post_stmt_pos)?;
            }
            for p in lp.continues {
                self.change_operand(p, post_body_pos)?;
            }
            Ok(())
        })();
        self.unfork_block();
        r
    }

    /// optimizeFunc: dead-code elimination, jump rewriting and the trailing return.
    fn optimize_func(&mut self, pos: Pos) -> CResult<()> {
        let insts = self.cur().instructions.clone();
        let mut dsts: HashSet<i64> = HashSet::new();
        iterate_instructions(&insts, |_, op, operands| {
            if matches!(op, OP_JUMP | OP_JUMP_FALSY | OP_AND_JUMP | OP_OR_JUMP) {
                dsts.insert(operands[0]);
            }
            Ok(())
        })?;

        let mut new_insts: Vec<u8> = Vec::new();
        let mut pos_map: HashMap<i64, i64> = HashMap::new();
        let mut dead_code = false;
        iterate_instructions(&insts, |p, op, operands| {
            if dsts.contains(&(p as i64)) {
                dead_code = false;
            } else if op == OP_RETURN {
                if dead_code {
                    return Ok(());
                }
                dead_code = true;
            } else if dead_code {
                return Ok(());
            }
            pos_map.insert(p as i64, new_insts.len() as i64);
            new_insts.extend_from_slice(&make_instruction(op, operands)?);
            Ok(())
        })?;

        let mut last_op: u8 = 0;
        let mut append_return = false;
        let end_pos = insts.len() as i64;
        let new_end_pos = new_insts.len() as i64;
        let snapshot = new_insts.clone();
        let mut patches: Vec<(usize, Vec<u8>)> = Vec::new();
        iterate_instructions(&snapshot, |p, op, operands| {
            if matches!(op, OP_JUMP | OP_JUMP_FALSY | OP_AND_JUMP | OP_OR_JUMP) {
                if let Some(nd) = pos_map.get(&operands[0]) {
                    patches.push((p, make_instruction(op, &[*nd])?));
                } else if end_pos == operands[0] {
                    patches.push((p, make_instruction(op, &[new_end_pos])?));
                    append_return = true;
                } else {
                    return Err(CompileFail::Panic("invalid jump position: 0".into()));
                }
            }
            last_op = op;
            Ok(())
        })?;
        for (p, inst) in patches {
            new_insts[p..p + inst.len()].copy_from_slice(&inst);
        }
        if last_op != OP_RETURN {
            append_return = true;
        }
        let mut new_source_map = HashMap::new();
        for (p, src) in self.cur().source_map.iter() {
            if let Some(np) = pos_map.get(&(*p as i64)) {
                new_source_map.insert(*np as usize, *src);
            }
        }
        self.cur().instructions = new_insts;
        self.cur().source_map = new_source_map;
        if append_return {
            self.emit(pos, OP_RETURN, &[0])?;
        }
        Ok(())
    }

    /// Bytecode(): the main function gets a trailing SUSPEND (no source map entry).
    pub fn bytecode(&mut self) -> Bytecode {
        let mut instructions = self.scopes[self.scope_index].instructions.clone();
        instructions.push(OP_SUSPEND);
        let main = Rc::new(CompiledFunction {
            instructions: Rc::new(instructions),
            num_locals: 0,
            num_parameters: 0,
            var_args: false,
            source_map: Some(Rc::new(self.scopes[self.scope_index].source_map.clone())),
            free: Vec::new(),
        });
        Bytecode { main, constants: self.shared.borrow().constants.clone() }
    }
}

fn resolve_assign_lhs(expr: &Expr) -> (Vec<u8>, Vec<Expr>) {
    match expr {
        Expr::SelectorExpr { expr: e, sel } => {
            let (name, mut sels) = resolve_assign_lhs(e);
            sels.push((**sel).clone());
            (name, sels)
        }
        Expr::IndexExpr { expr: e, index, .. } => {
            let (name, mut sels) = resolve_assign_lhs(e);
            if let Some(i) = index {
                sels.push((**i).clone());
            }
            (name, sels)
        }
        Expr::Ident(id) => (id.name.clone(), Vec::new()),
        _ => (Vec::new(), Vec::new()),
    }
}

// ---------------------------------------------------------------------------------------------
// path helpers (path/filepath on Unix)

/// filepath.Clean
pub fn clean_path(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let rooted = path.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if let Some(last) = out.last() {
                    if *last != ".." {
                        out.pop();
                        continue;
                    }
                }
                if !rooted {
                    out.push("..");
                }
            }
            p => out.push(p),
        }
    }
    let joined = out.join("/");
    if rooted {
        format!("/{}", joined)
    } else if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    }
}

/// filepath.Join
pub fn join_path(a: &str, b: &str) -> String {
    let parts: Vec<&str> = [a, b].into_iter().filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return String::new();
    }
    clean_path(&parts.join("/"))
}

/// filepath.Abs
pub fn abs_path(p: &str) -> String {
    if p.starts_with('/') {
        return clean_path(p);
    }
    let cwd = std::env::current_dir().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
    clean_path(&format!("{}/{}", cwd, p))
}

/// filepath.Dir
pub fn dir_path(p: &str) -> String {
    match p.rfind('/') {
        Some(i) => clean_path(&p[..i + 1]),
        None => ".".to_string(),
    }
}

/// filepath.Base
pub fn base_path(p: &str) -> String {
    if p.is_empty() {
        return ".".to_string();
    }
    let t = p.trim_end_matches('/');
    if t.is_empty() {
        return "/".to_string();
    }
    match t.rfind('/') {
        Some(i) => t[i + 1..].to_string(),
        None => t.to_string(),
    }
}

/// The text of a *PathError from os.ReadFile.
pub fn go_read_error(path: &str, e: &std::io::Error) -> String {
    let what = match e.raw_os_error() {
        Some(21) => "is a directory".to_string(),
        Some(13) => "permission denied".to_string(),
        Some(2) => "no such file or directory".to_string(),
        _ => e.to_string(),
    };
    let op = if e.raw_os_error() == Some(21) { "read" } else { "open" };
    format!("{} {}: {}", op, path, what)
}

// ---------------------------------------------------------------------------------------------
// Bytecode.RemoveDuplicates

/// Rewrites the CONST/CLOSURE operands of one instruction stream through the index map.
fn update_const_indexes(insts: &mut [u8], index_map: &HashMap<i64, i64>) -> CResult<()> {
    let mut i = 0usize;
    while i < insts.len() {
        let op = insts[i];
        let widths = operands_of(op)?;
        let (_, read) = read_operands(widths, &insts[i + 1..])?;
        if op == OP_CONSTANT || op == OP_CLOSURE {
            let at = |k: usize| -> CResult<i64> {
                if k < insts.len() {
                    Ok(insts[k] as i64)
                } else {
                    Err(go_index_panic(k as i64, insts.len()))
                }
            };
            let cur = at(i + 2)? | at(i + 1)? << 8;
            let new = match index_map.get(&cur) {
                Some(n) => *n,
                None => return Err(CompileFail::Panic(format!("constant index not found: {}", cur))),
            };
            let inst = if op == OP_CONSTANT {
                make_instruction(op, &[new])?
            } else {
                let num_free = at(i + 3)?;
                make_instruction(op, &[new, num_free])?
            };
            for (k, b) in inst.iter().enumerate() {
                if i + k < insts.len() {
                    insts[i + k] = *b;
                }
            }
        }
        i += 1 + read;
    }
    Ok(())
}

fn module_name_of(o: &Object) -> Vec<u8> {
    if let Object::ImmutableMap(m) = o {
        if let Some(Object::String(s)) = m.data.borrow().get(&b"__module_name__"[..]) {
            return (**s).clone();
        }
    }
    Vec::new()
}

impl Bytecode {
    /// RemoveDuplicates: shares equal constants and rewrites every constant operand.
    pub fn remove_duplicates(&mut self) -> CResult<()> {
        let mut deduped: Vec<Object> = Vec::new();
        let mut index_map: HashMap<i64, i64> = HashMap::new();
        let mut fns: HashMap<usize, i64> = HashMap::new();
        let mut ints: HashMap<i64, i64> = HashMap::new();
        let mut strings: HashMap<Vec<u8>, i64> = HashMap::new();
        let mut floats: Vec<(f64, i64)> = Vec::new();
        let mut chars: HashMap<i32, i64> = HashMap::new();
        let mut imaps: HashMap<Vec<u8>, i64> = HashMap::new();
        for (cur, c) in self.constants.iter().enumerate() {
            let cur = cur as i64;
            let next = deduped.len() as i64;
            let found = match c {
                Object::CompiledFunction(f) => fns.get(&(Rc::as_ptr(f) as usize)).copied(),
                Object::ImmutableMap(_) => {
                    let name = module_name_of(c);
                    match imaps.get(&name) {
                        Some(i) if !name.is_empty() => Some(*i),
                        _ => {
                            imaps.insert(name, next);
                            index_map.insert(cur, next);
                            deduped.push(c.clone());
                            continue;
                        }
                    }
                }
                Object::Int(v) => ints.get(v).copied(),
                Object::String(s) => strings.get(&**s).copied(),
                Object::Float(v) => floats.iter().find(|(f, _)| f == v).map(|(_, i)| *i),
                Object::Char(v) => chars.get(v).copied(),
                other => {
                    return Err(CompileFail::Panic(format!(
                        "unsupported top-level constant type: {}",
                        other.type_name()
                    )))
                }
            };
            match found {
                Some(i) => {
                    index_map.insert(cur, i);
                }
                None => {
                    match c {
                        Object::CompiledFunction(f) => {
                            fns.insert(Rc::as_ptr(f) as usize, next);
                        }
                        Object::Int(v) => {
                            ints.insert(*v, next);
                        }
                        Object::String(s) => {
                            strings.insert((**s).clone(), next);
                        }
                        Object::Float(v) => floats.push((*v, next)),
                        Object::Char(v) => {
                            chars.insert(*v, next);
                        }
                        _ => {}
                    }
                    index_map.insert(cur, next);
                    deduped.push(c.clone());
                }
            }
        }
        let mut main_insts = (*self.main.instructions).clone();
        update_const_indexes(&mut main_insts, &index_map)?;
        self.main = Rc::new(CompiledFunction {
            instructions: Rc::new(main_insts),
            num_locals: self.main.num_locals,
            num_parameters: self.main.num_parameters,
            var_args: self.main.var_args,
            source_map: self.main.source_map.clone(),
            free: Vec::new(),
        });
        for c in deduped.iter_mut() {
            if let Object::CompiledFunction(f) = c {
                let mut insts = (*f.instructions).clone();
                update_const_indexes(&mut insts, &index_map)?;
                *c = Object::CompiledFunction(Rc::new(CompiledFunction {
                    instructions: Rc::new(insts),
                    num_locals: f.num_locals,
                    num_parameters: f.num_parameters,
                    var_args: f.var_args,
                    source_map: f.source_map.clone(),
                    free: Vec::new(),
                }));
            }
        }
        self.constants = deduped;
        Ok(())
    }
}
