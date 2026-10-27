//! The stack VM (vm.go). The stack, globals and frames are fixed-size arrays as in Go, so an
//! out-of-range access is a Go runtime panic with the same index and length.

use std::cell::RefCell;
use std::rc::Rc;

use crate::builtins::builtin_function;
use crate::compiler::*;
use crate::goslice::GoSlice;
use crate::objects::*;
use crate::source::FileSetRef;
use crate::token::Token;

pub const GLOBALS_SIZE: usize = 1024;
pub const STACK_SIZE: usize = 2048;
pub const MAX_FRAMES: usize = 1024;

#[derive(Clone)]
struct Frame {
    func: Rc<CompiledFunction>,
    free_vars: Vec<Rc<RefCell<Object>>>,
    ip: i64,
    base_pointer: i64,
}

pub struct Vm {
    constants: Vec<Object>,
    stack: Vec<Object>,
    sp: i64,
    globals: Vec<Object>,
    fset: FileSetRef,
    frames: Vec<Option<Frame>>,
    frames_index: usize,
    cur: Frame,
    cur_insts: Rc<Vec<u8>>,
    ip: i64,
}

/// How a run ended with an error.
pub enum RunFail {
    /// A Tengo runtime error, fully formatted ("Runtime Error: ...\n\tat ...").
    Error(String),
    /// A Go panic: the message after "panic: ".
    Panic(String),
}

fn idx_panic(i: i64, len: usize) -> TErr {
    if i < 0 {
        return TErr::panic(format!("runtime error: index out of range [{}]", i));
    }
    TErr::panic(format!("runtime error: index out of range [{}] with length {}", i, len))
}

impl Vm {
    pub fn new(bc: Bytecode, fset: FileSetRef) -> Vm {
        let main = Frame { func: bc.main.clone(), free_vars: Vec::new(), ip: -1, base_pointer: 0 };
        let mut frames = vec![None; MAX_FRAMES];
        frames[0] = Some(main.clone());
        Vm {
            constants: bc.constants,
            stack: vec![Object::Nil; STACK_SIZE],
            sp: 0,
            globals: vec![Object::Nil; GLOBALS_SIZE],
            fset,
            frames,
            frames_index: 1,
            cur_insts: main.func.instructions.clone(),
            cur: main,
            ip: -1,
        }
    }

    #[inline]
    fn sget(&self, i: i64) -> Result<Object, TErr> {
        if i < 0 || i as usize >= STACK_SIZE {
            return Err(idx_panic(i, STACK_SIZE));
        }
        Ok(self.stack[i as usize].clone())
    }

    #[inline]
    fn sset(&mut self, i: i64, v: Object) -> Result<(), TErr> {
        if i < 0 || i as usize >= STACK_SIZE {
            return Err(idx_panic(i, STACK_SIZE));
        }
        self.stack[i as usize] = v;
        Ok(())
    }

    #[inline]
    fn push(&mut self, v: Object) -> Result<(), TErr> {
        self.sset(self.sp, v)?;
        self.sp += 1;
        Ok(())
    }

    #[inline]
    fn ins(&self, i: i64) -> Result<i64, TErr> {
        if i < 0 || i as usize >= self.cur_insts.len() {
            return Err(idx_panic(i, self.cur_insts.len()));
        }
        Ok(self.cur_insts[i as usize] as i64)
    }

    fn gget(&self, i: i64) -> Result<Object, TErr> {
        if i < 0 || i as usize >= GLOBALS_SIZE {
            return Err(idx_panic(i, GLOBALS_SIZE));
        }
        Ok(self.globals[i as usize].clone())
    }

    fn save_cur(&mut self) {
        self.frames[self.frames_index - 1] = Some(self.cur.clone());
    }

    /// Run: executes the main function and formats a runtime error with its call stack.
    pub fn run(&mut self) -> Result<(), RunFail> {
        match self.run_loop() {
            Ok(()) => Ok(()),
            Err(TErr::Panic(m)) => Err(RunFail::Panic(String::from_utf8_lossy(&m).to_string())),
            Err(e) => {
                self.save_cur();
                let mut fset = self.fset.borrow_mut();
                let pos = fset.position(self.cur.func.source_pos(self.ip - 1));
                let mut out = format!("Runtime Error: {}\n\tat {}", String::from_utf8_lossy(&e.message()), pos);
                let mut fi = self.frames_index;
                while fi > 1 {
                    fi -= 1;
                    let f = self.frames[fi - 1].as_ref().unwrap();
                    let pos = fset.position(f.func.source_pos(f.ip - 1));
                    out.push_str(&format!("\n\tat {}", pos));
                }
                Err(RunFail::Error(out))
            }
        }
    }

    fn run_loop(&mut self) -> Result<(), TErr> {
        loop {
            self.ip += 1;
            let op = self.ins(self.ip)? as u8;
            match op {
                OP_CONSTANT => {
                    self.ip += 2;
                    let cidx = self.ins(self.ip)? | self.ins(self.ip - 1)? << 8;
                    let c = match self.constants.get(cidx as usize) {
                        Some(c) => c.clone(),
                        None => return Err(idx_panic(cidx, self.constants.len())),
                    };
                    self.push(c)?;
                }
                OP_NULL => self.push(Object::Undefined)?,
                OP_BINARY_OP => {
                    self.ip += 1;
                    let right = self.sget(self.sp - 1)?;
                    let left = self.sget(self.sp - 2)?;
                    let tok = Token::from_u8(self.ins(self.ip)? as u8).unwrap_or(Token::Illegal);
                    match left.binary_op(tok, &right) {
                        Ok(res) => {
                            self.sset(self.sp - 2, res)?;
                            self.sp -= 1;
                        }
                        Err(TErr::InvalidOperator) => {
                            self.sp -= 2;
                            return Err(TErr::msg(format!(
                                "invalid operation: {} {} {}",
                                left.type_name(),
                                tok.string(),
                                right.type_name()
                            )));
                        }
                        Err(e) => {
                            self.sp -= 2;
                            return Err(e);
                        }
                    }
                }
                OP_EQUAL | OP_NOT_EQUAL => {
                    let right = self.sget(self.sp - 1)?;
                    let left = self.sget(self.sp - 2)?;
                    self.sp -= 2;
                    let eq = left.equals(&right);
                    self.push(Object::Bool(if op == OP_EQUAL { eq } else { !eq }))?;
                }
                OP_POP => self.sp -= 1,
                OP_TRUE => self.push(Object::Bool(true))?,
                OP_FALSE => self.push(Object::Bool(false))?,
                OP_LNOT => {
                    let operand = self.sget(self.sp - 1)?;
                    self.sp -= 1;
                    self.push(Object::Bool(operand.is_falsy()))?;
                }
                OP_BCOMPLEMENT => {
                    let operand = self.sget(self.sp - 1)?;
                    self.sp -= 1;
                    match operand {
                        Object::Int(x) => self.push(Object::Int(!x))?,
                        other => return Err(TErr::msg(format!("invalid operation: ^{}", other.type_name()))),
                    }
                }
                OP_MINUS => {
                    let operand = self.sget(self.sp - 1)?;
                    self.sp -= 1;
                    match operand {
                        Object::Int(x) => self.push(Object::Int(x.wrapping_neg()))?,
                        Object::Float(x) => self.push(Object::Float(-x))?,
                        other => return Err(TErr::msg(format!("invalid operation: -{}", other.type_name()))),
                    }
                }
                OP_JUMP_FALSY => {
                    self.ip += 4;
                    self.sp -= 1;
                    if self.sget(self.sp)?.is_falsy() {
                        let pos = self.read4()?;
                        self.ip = pos - 1;
                    }
                }
                OP_AND_JUMP => {
                    self.ip += 4;
                    if self.sget(self.sp - 1)?.is_falsy() {
                        let pos = self.read4()?;
                        self.ip = pos - 1;
                    } else {
                        self.sp -= 1;
                    }
                }
                OP_OR_JUMP => {
                    self.ip += 4;
                    if self.sget(self.sp - 1)?.is_falsy() {
                        self.sp -= 1;
                    } else {
                        let pos = self.read4()?;
                        self.ip = pos - 1;
                    }
                }
                OP_JUMP => {
                    let pos = self.ins(self.ip + 4)?
                        | self.ins(self.ip + 3)? << 8
                        | self.ins(self.ip + 2)? << 16
                        | self.ins(self.ip + 1)? << 24;
                    self.ip = pos - 1;
                }
                OP_SET_GLOBAL => {
                    self.ip += 2;
                    self.sp -= 1;
                    let gi = self.ins(self.ip)? | self.ins(self.ip - 1)? << 8;
                    let v = self.sget(self.sp)?;
                    if gi as usize >= GLOBALS_SIZE {
                        return Err(idx_panic(gi, GLOBALS_SIZE));
                    }
                    self.globals[gi as usize] = v;
                }
                OP_SET_SEL_GLOBAL => {
                    self.ip += 3;
                    let gi = self.ins(self.ip - 1)? | self.ins(self.ip - 2)? << 8;
                    let num_sel = self.ins(self.ip)?;
                    let (val, selectors) = self.pop_selectors(num_sel)?;
                    let dst = self.gget(gi)?;
                    index_assign(dst, val, &selectors)?;
                }
                OP_GET_GLOBAL => {
                    self.ip += 2;
                    let gi = self.ins(self.ip)? | self.ins(self.ip - 1)? << 8;
                    let v = self.gget(gi)?;
                    self.push(v)?;
                }
                OP_ARRAY => {
                    self.ip += 2;
                    let n = self.ins(self.ip)? | self.ins(self.ip - 1)? << 8;
                    let mut elements = Vec::with_capacity(n as usize);
                    for i in self.sp - n..self.sp {
                        elements.push(self.sget(i)?);
                    }
                    self.sp -= n;
                    self.push(new_array(GoSlice::from_appends(elements)))?;
                }
                OP_MAP => {
                    self.ip += 2;
                    let n = self.ins(self.ip)? | self.ins(self.ip - 1)? << 8;
                    let mut kv = MapData::new();
                    let mut i = self.sp - n;
                    while i < self.sp {
                        let key = self.sget(i)?;
                        let value = self.sget(i + 1)?;
                        if let Object::String(k) = key {
                            kv.insert((*k).clone(), value);
                        }
                        i += 2;
                    }
                    self.sp -= n;
                    self.push(new_map(kv))?;
                }
                OP_ERROR => {
                    let v = self.sget(self.sp - 1)?;
                    self.sset(self.sp - 1, new_error(v))?;
                }
                OP_IMMUTABLE => {
                    let v = self.sget(self.sp - 1)?;
                    match v {
                        Object::Array(a) => {
                            let s = a.value.borrow().clone();
                            self.sset(self.sp - 1, new_imm_array(s))?;
                        }
                        Object::Map(m) => {
                            self.sset(self.sp - 1, Object::ImmutableMap(Rc::new(MapObj { data: m.data.clone() })))?;
                        }
                        _ => {}
                    }
                }
                OP_INDEX => {
                    let index = self.sget(self.sp - 1)?;
                    let left = self.sget(self.sp - 2)?;
                    self.sp -= 2;
                    match left.index_get(&index) {
                        Ok(v) => self.push(v)?,
                        Err(TErr::NotIndexable) => {
                            return Err(TErr::msg(format!("not indexable: {}", index.type_name())))
                        }
                        Err(TErr::InvalidIndexType) => {
                            return Err(TErr::msg(format!("invalid index type: {}", index.type_name())))
                        }
                        Err(e) => return Err(e),
                    }
                }
                OP_SLICE_INDEX => {
                    let high = self.sget(self.sp - 1)?;
                    let low = self.sget(self.sp - 2)?;
                    let left = self.sget(self.sp - 3)?;
                    self.sp -= 3;
                    let res = slice_index(&left, &low, &high)?;
                    self.push(res)?;
                }
                OP_CALL => self.op_call()?,
                OP_RETURN => {
                    self.ip += 1;
                    let ret = if self.ins(self.ip)? == 1 { self.sget(self.sp - 1)? } else { Object::Undefined };
                    self.frames_index -= 1;
                    let returning_bp = self.cur.base_pointer;
                    self.cur = self.frames[self.frames_index - 1].clone().unwrap();
                    self.cur_insts = self.cur.func.instructions.clone();
                    self.ip = self.cur.ip;
                    self.sp = returning_bp;
                    self.sset(self.sp - 1, ret)?;
                }
                OP_DEFINE_LOCAL => {
                    self.ip += 1;
                    let li = self.ins(self.ip)?;
                    let sp = self.cur.base_pointer + li;
                    let val = self.sget(self.sp - 1)?;
                    self.sp -= 1;
                    self.sset(sp, val)?;
                }
                OP_SET_LOCAL => {
                    let li = self.ins(self.ip + 1)?;
                    self.ip += 1;
                    let sp = self.cur.base_pointer + li;
                    let mut val = self.sget(self.sp - 1)?;
                    self.sp -= 1;
                    if let Object::ObjectPtr(p) = self.sget(sp)? {
                        *p.borrow_mut() = val;
                        val = Object::ObjectPtr(p);
                    }
                    self.sset(sp, val)?;
                }
                OP_SET_SEL_LOCAL => {
                    let li = self.ins(self.ip + 1)?;
                    let num_sel = self.ins(self.ip + 2)?;
                    self.ip += 2;
                    let (val, selectors) = self.pop_selectors(num_sel)?;
                    let mut dst = self.sget(self.cur.base_pointer + li)?;
                    if let Object::ObjectPtr(p) = &dst {
                        let inner = p.borrow().clone();
                        dst = inner;
                    }
                    index_assign(dst, val, &selectors)?;
                }
                OP_GET_LOCAL => {
                    self.ip += 1;
                    let li = self.ins(self.ip)?;
                    let mut val = self.sget(self.cur.base_pointer + li)?;
                    if let Object::ObjectPtr(p) = &val {
                        let inner = p.borrow().clone();
                        val = inner;
                    }
                    self.push(val)?;
                }
                OP_GET_BUILTIN => {
                    self.ip += 1;
                    let bi = self.ins(self.ip)?;
                    match builtin_function(bi as usize) {
                        Some(f) => self.push(f)?,
                        None => return Err(idx_panic(bi, 32)),
                    }
                }
                OP_CLOSURE => {
                    self.ip += 3;
                    let ci = self.ins(self.ip - 1)? | self.ins(self.ip - 2)? << 8;
                    let num_free = self.ins(self.ip)?;
                    let f = match self.constants.get(ci as usize) {
                        Some(Object::CompiledFunction(f)) => f.clone(),
                        Some(_) => {
                            return Err(TErr::panic(
                                "runtime error: invalid memory address or nil pointer dereference",
                            ))
                        }
                        None => return Err(idx_panic(ci, self.constants.len())),
                    };
                    let mut free = Vec::with_capacity(num_free as usize);
                    for i in 0..num_free {
                        match self.sget(self.sp - num_free + i)? {
                            Object::ObjectPtr(p) => free.push(p),
                            other => free.push(Rc::new(RefCell::new(other))),
                        }
                    }
                    self.sp -= num_free;
                    let cl = CompiledFunction {
                        instructions: f.instructions.clone(),
                        num_locals: f.num_locals,
                        num_parameters: f.num_parameters,
                        var_args: f.var_args,
                        source_map: f.source_map.clone(),
                        free,
                    };
                    self.push(Object::CompiledFunction(Rc::new(cl)))?;
                }
                OP_GET_FREE_PTR => {
                    self.ip += 1;
                    let fi = self.ins(self.ip)?;
                    let p = self.free_var(fi)?;
                    self.push(Object::ObjectPtr(p))?;
                }
                OP_GET_FREE => {
                    self.ip += 1;
                    let fi = self.ins(self.ip)?;
                    let v = self.free_var(fi)?.borrow().clone();
                    self.push(v)?;
                }
                OP_SET_FREE => {
                    self.ip += 1;
                    let fi = self.ins(self.ip)?;
                    let v = self.sget(self.sp - 1)?;
                    *self.free_var(fi)?.borrow_mut() = v;
                    self.sp -= 1;
                }
                OP_GET_LOCAL_PTR => {
                    self.ip += 1;
                    let li = self.ins(self.ip)?;
                    let sp = self.cur.base_pointer + li;
                    let val = self.sget(sp)?;
                    let fv = match val {
                        Object::ObjectPtr(p) => p,
                        other => {
                            let p = Rc::new(RefCell::new(other));
                            self.sset(sp, Object::ObjectPtr(p.clone()))?;
                            p
                        }
                    };
                    self.push(Object::ObjectPtr(fv))?;
                }
                OP_SET_SEL_FREE => {
                    self.ip += 2;
                    let fi = self.ins(self.ip - 1)?;
                    let num_sel = self.ins(self.ip)?;
                    let (val, selectors) = self.pop_selectors(num_sel)?;
                    let dst = self.free_var(fi)?.borrow().clone();
                    index_assign(dst, val, &selectors)?;
                }
                OP_ITERATOR_INIT => {
                    let dst = self.sget(self.sp - 1)?;
                    self.sp -= 1;
                    if !dst.can_iterate() {
                        return Err(TErr::msg(format!("not iterable: {}", dst.type_name())));
                    }
                    self.push(dst.iterate())?;
                }
                OP_ITERATOR_NEXT => {
                    let it = self.sget(self.sp - 1)?;
                    self.sp -= 1;
                    let more = iterator_next(&it)?;
                    self.push(Object::Bool(more))?;
                }
                OP_ITERATOR_KEY => {
                    let it = self.sget(self.sp - 1)?;
                    self.sp -= 1;
                    let k = iterator_key(&it)?;
                    self.push(k)?;
                }
                OP_ITERATOR_VALUE => {
                    let it = self.sget(self.sp - 1)?;
                    self.sp -= 1;
                    let v = iterator_value(&it)?;
                    self.push(v)?;
                }
                OP_SUSPEND => return Ok(()),
                other => return Err(TErr::msg(format!("unknown opcode: {}", other))),
            }
        }
    }

    fn read4(&self) -> Result<i64, TErr> {
        Ok(self.ins(self.ip)? | self.ins(self.ip - 1)? << 8 | self.ins(self.ip - 2)? << 16 | self.ins(self.ip - 3)? << 24)
    }

    fn free_var(&self, i: i64) -> Result<Rc<RefCell<Object>>, TErr> {
        match self.cur.free_vars.get(i as usize) {
            Some(p) => Ok(p.clone()),
            None => Err(idx_panic(i, self.cur.free_vars.len())),
        }
    }

    fn pop_selectors(&mut self, num_sel: i64) -> Result<(Object, Vec<Object>), TErr> {
        let mut selectors = Vec::with_capacity(num_sel as usize);
        for i in 0..num_sel {
            selectors.push(self.sget(self.sp - num_sel + i)?);
        }
        let val = self.sget(self.sp - num_sel - 1)?;
        self.sp -= num_sel + 1;
        Ok((val, selectors))
    }

    fn op_call(&mut self) -> Result<(), TErr> {
        let mut num_args = self.ins(self.ip + 1)?;
        let spread = self.ins(self.ip + 2)?;
        self.ip += 2;
        let value = self.sget(self.sp - 1 - num_args)?;
        if !value.can_call() {
            return Err(TErr::msg(format!("not callable: {}", value.type_name())));
        }
        if spread == 1 {
            self.sp -= 1;
            let arr = self.sget(self.sp)?;
            match array_elems(&arr) {
                Some(items) => {
                    let n = items.len() as i64;
                    for item in items {
                        self.push(item)?;
                    }
                    num_args += n - 1;
                }
                None => return Err(TErr::msg(format!("not an array: {}", arr.type_name()))),
            }
        }
        if let Object::CompiledFunction(callee) = &value {
            let callee = callee.clone();
            if callee.var_args {
                let real_args = callee.num_parameters as i64 - 1;
                let var_args = num_args - real_args;
                if var_args >= 0 {
                    num_args = real_args + 1;
                    let sp_start = self.sp - var_args;
                    let mut args = Vec::with_capacity(var_args as usize);
                    for i in sp_start..self.sp {
                        args.push(self.sget(i)?);
                    }
                    self.sset(sp_start, new_array(GoSlice::from_vec_exact(args)))?;
                    self.sp = sp_start + 1;
                }
            }
            if num_args != callee.num_parameters as i64 {
                if callee.var_args {
                    return Err(TErr::msg(format!(
                        "wrong number of arguments: want>={}, got={}",
                        callee.num_parameters as i64 - 1,
                        num_args
                    )));
                }
                return Err(TErr::msg(format!(
                    "wrong number of arguments: want={}, got={}",
                    callee.num_parameters, num_args
                )));
            }
            if Rc::ptr_eq(&callee, &self.cur.func) {
                let next_op = self.ins(self.ip + 1)? as u8;
                let tail = next_op == OP_RETURN || (next_op == OP_POP && self.ins(self.ip + 2)? as u8 == OP_RETURN);
                if tail {
                    for p in 0..num_args {
                        let v = self.sget(self.sp - num_args + p)?;
                        self.sset(self.cur.base_pointer + p, v)?;
                    }
                    self.sp -= num_args + 1;
                    self.ip = -1;
                    return Ok(());
                }
            }
            if self.frames_index >= MAX_FRAMES {
                return Err(TErr::StackOverflow);
            }
            self.cur.ip = self.ip;
            self.save_cur();
            let frame = Frame {
                func: callee.clone(),
                free_vars: callee.free.clone(),
                ip: -1,
                base_pointer: self.sp - num_args,
            };
            self.frames[self.frames_index] = Some(frame.clone());
            self.cur = frame;
            self.cur_insts = callee.instructions.clone();
            self.ip = -1;
            self.frames_index += 1;
            self.sp = self.sp - num_args + callee.num_locals as i64;
            return Ok(());
        }

        let mut args = Vec::with_capacity(num_args as usize);
        for i in self.sp - num_args..self.sp {
            args.push(self.sget(i)?);
        }
        let (f, type_name) = match &value {
            Object::BuiltinFunction(f) | Object::UserFunction(f) => (f.func.clone(), value.type_name()),
            _ => unreachable!(),
        };
        let r = f(&args);
        self.sp -= num_args + 1;
        match r {
            Ok(ret) => {
                let ret = if let Object::Nil = ret { Object::Undefined } else { ret };
                self.push(ret)?;
                Ok(())
            }
            Err(TErr::WrongNumArguments) => {
                Err(TErr::msg(format!("wrong number of arguments in call to '{}'", type_name)))
            }
            Err(TErr::InvalidArgumentType { name, expected, found }) => Err(TErr::msg(format!(
                "invalid type for argument '{}' in call to '{}': expected {}, found {}",
                name, type_name, expected, found
            ))),
            Err(e) => Err(e),
        }
    }
}

fn slice_index(left: &Object, low: &Object, high: &Object) -> Result<Object, TErr> {
    let mut low_idx: i64 = 0;
    if !matches!(low, Object::Undefined) {
        match low {
            Object::Int(i) => low_idx = *i,
            _ => return Err(TErr::msg(format!("invalid slice index type: {}", low.type_name()))),
        }
    }
    let num_elements: i64 = match left {
        Object::Array(a) => a.value.borrow().len() as i64,
        Object::ImmutableArray(a) => a.value.len() as i64,
        Object::String(s) => s.len() as i64,
        Object::Bytes(b) => b.len() as i64,
        _ => return Err(TErr::msg(format!("not indexable: {}", left.type_name()))),
    };
    let mut high_idx = match high {
        Object::Undefined => num_elements,
        Object::Int(i) => *i,
        _ => return Err(TErr::msg(format!("invalid slice index type: {}", high.type_name()))),
    };
    if low_idx > high_idx {
        return Err(TErr::msg(format!("invalid slice index: {} > {}", low_idx, high_idx)));
    }
    low_idx = low_idx.clamp(0, num_elements);
    high_idx = high_idx.clamp(0, num_elements);
    let (lo, hi) = (low_idx as usize, high_idx as usize);
    Ok(match left {
        Object::Array(a) => new_array(a.value.borrow().slice(lo, hi)),
        Object::ImmutableArray(a) => new_array(a.value.slice(lo, hi)),
        Object::String(s) => new_str(&s[lo..hi]),
        Object::Bytes(b) => Object::Bytes(b.slice(lo, hi)),
        _ => unreachable!(),
    })
}

fn index_assign(dst: Object, src: Object, selectors: &[Object]) -> Result<(), TErr> {
    let mut dst = dst;
    let num_sel = selectors.len();
    let mut sidx = num_sel as i64 - 1;
    while sidx > 0 {
        let sel = &selectors[sidx as usize];
        match dst.index_get(sel) {
            Ok(next) => dst = next,
            Err(TErr::NotIndexable) => return Err(TErr::msg(format!("not indexable: {}", dst.type_name()))),
            Err(TErr::InvalidIndexType) => return Err(TErr::msg(format!("invalid index type: {}", sel.type_name()))),
            Err(e) => return Err(e),
        }
        sidx -= 1;
    }
    match dst.index_set(&selectors[0], src.clone()) {
        Ok(()) => Ok(()),
        Err(TErr::NotIndexAssignable) => Err(TErr::msg(format!("not index-assignable: {}", dst.type_name()))),
        Err(TErr::InvalidIndexValueType) => Err(TErr::msg(format!("invaid index value type: {}", src.type_name()))),
        Err(e) => Err(e),
    }
}

fn as_iter(it: &Object) -> Result<Rc<RefCell<IterState>>, TErr> {
    match it {
        Object::Iterator(s) => Ok(s.clone()),
        Object::Nil => Err(TErr::panic("runtime error: invalid memory address or nil pointer dereference")),
        other => Err(TErr::panic(format!(
            "interface conversion: tengo.Object is {}, not tengo.Iterator (missing method Key)",
            go_type_of(other)
        ))),
    }
}

/// The Go dynamic type name of an object (for interface-conversion panics).
pub fn go_type_of(o: &Object) -> &'static str {
    match o {
        Object::Nil => "nil",
        Object::Undefined => "*tengo.Undefined",
        Object::Bool(_) => "*tengo.Bool",
        Object::Int(_) => "*tengo.Int",
        Object::Float(_) => "*tengo.Float",
        Object::Char(_) => "*tengo.Char",
        Object::String(_) => "*tengo.String",
        Object::Bytes(_) => "*tengo.Bytes",
        Object::Array(_) => "*tengo.Array",
        Object::ImmutableArray(_) => "*tengo.ImmutableArray",
        Object::Map(_) => "*tengo.Map",
        Object::ImmutableMap(_) => "*tengo.ImmutableMap",
        Object::Error(_) => "*tengo.Error",
        Object::CompiledFunction(_) => "*tengo.CompiledFunction",
        Object::BuiltinFunction(_) => "*tengo.BuiltinFunction",
        Object::UserFunction(_) => "*tengo.UserFunction",
        Object::Time(_) => "*tengo.Time",
        Object::ObjectPtr(_) => "*tengo.ObjectPtr",
        Object::Iterator(_) => "*tengo.ArrayIterator",
    }
}

fn iterator_next(it: &Object) -> Result<bool, TErr> {
    // Undefined is its own (empty) iterator.
    if let Object::Undefined = it {
        return Ok(false);
    }
    let s = as_iter(it)?;
    let mut st = s.borrow_mut();
    Ok(match &mut *st {
        IterState::Array { i, l, .. } | IterState::Bytes { i, l, .. } | IterState::Map { i, l, .. } | IterState::String { i, l, .. } => {
            *i += 1;
            *i <= *l
        }
    })
}

fn iterator_key(it: &Object) -> Result<Object, TErr> {
    if let Object::Undefined = it {
        return Ok(Object::Undefined);
    }
    let s = as_iter(it)?;
    let st = s.borrow();
    Ok(match &*st {
        IterState::Array { i, .. } | IterState::Bytes { i, .. } | IterState::String { i, .. } => {
            Object::Int(*i as i64 - 1)
        }
        IterState::Map { keys, i, .. } => {
            if *i == 0 || *i > keys.len() {
                return Err(idx_panic(*i as i64 - 1, keys.len()));
            }
            new_str(&keys[*i - 1])
        }
    })
}

fn iterator_value(it: &Object) -> Result<Object, TErr> {
    if let Object::Undefined = it {
        return Ok(Object::Undefined);
    }
    let s = as_iter(it)?;
    let st = s.borrow();
    Ok(match &*st {
        IterState::Array { v, i, .. } => {
            if *i == 0 || *i > v.len() {
                return Err(idx_panic(*i as i64 - 1, v.len()));
            }
            v.get(*i - 1)
        }
        IterState::Bytes { v, i, .. } => {
            if *i == 0 || *i > v.len() {
                return Err(idx_panic(*i as i64 - 1, v.len()));
            }
            Object::Int(v.get(*i - 1) as i64)
        }
        IterState::Map { data, keys, i, .. } => {
            if *i == 0 || *i > keys.len() {
                return Err(idx_panic(*i as i64 - 1, keys.len()));
            }
            data.borrow().get(&keys[*i - 1]).cloned().unwrap_or(Object::Nil)
        }
        IterState::String { v, i, .. } => {
            if *i == 0 || *i > v.len() {
                return Err(idx_panic(*i as i64 - 1, v.len()));
            }
            Object::Char(v[*i - 1])
        }
    })
}
