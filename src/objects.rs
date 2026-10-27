//! Tengo values (objects.go, iterator.go, errors.go and the conversions in tengo.go).

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use crate::ftoa::{float_string, parse_float};
use crate::goslice::{bytes_from_string, GoSlice, SliceElem};
use crate::gostr::{parse_int, quote, rune_string, runes};
use crate::gotime::GoTime;
use crate::source::Pos;
use crate::token::Token;

pub const MAX_STRING_LEN: usize = 2147483647;
pub const MAX_BYTES_LEN: usize = 2147483647;
pub const MAX_RANGE_LEN: i64 = 10000000;

/// The errors a Tengo operation can return (the sentinel errors of errors.go, formatted
/// errors, and Go runtime panics that end the process).
#[derive(Debug, Clone)]
pub enum TErr {
    StackOverflow,
    ObjectAllocLimit,
    IndexOutOfBounds,
    InvalidIndexType,
    InvalidIndexValueType,
    InvalidIndexOnError,
    InvalidOperator,
    WrongNumArguments,
    BytesLimit,
    StringLimit,
    NotIndexable,
    NotIndexAssignable,
    NotImplemented,
    InvalidRangeStep,
    RangeLimit,
    InvalidArgumentType { name: String, expected: String, found: String },
    Msg(Vec<u8>),
    /// A Go runtime panic: the process prints "panic: <msg>" and exits 2.
    Panic(Vec<u8>),
}

impl TErr {
    pub fn msg<S: AsRef<[u8]>>(s: S) -> TErr {
        TErr::Msg(s.as_ref().to_vec())
    }

    pub fn panic<S: AsRef<[u8]>>(s: S) -> TErr {
        TErr::Panic(s.as_ref().to_vec())
    }

    pub fn arg(name: &str, expected: &str, found: &Object) -> TErr {
        TErr::InvalidArgumentType { name: name.to_string(), expected: expected.to_string(), found: found.type_name() }
    }

    pub fn message(&self) -> Vec<u8> {
        match self {
            TErr::StackOverflow => b"stack overflow".to_vec(),
            TErr::ObjectAllocLimit => b"object allocation limit exceeded".to_vec(),
            TErr::IndexOutOfBounds => b"index out of bounds".to_vec(),
            TErr::InvalidIndexType => b"invalid index type".to_vec(),
            TErr::InvalidIndexValueType => b"invalid index value type".to_vec(),
            TErr::InvalidIndexOnError => b"invalid index on error".to_vec(),
            TErr::InvalidOperator => b"invalid operator".to_vec(),
            TErr::WrongNumArguments => b"wrong number of arguments".to_vec(),
            TErr::BytesLimit => b"exceeding bytes size limit".to_vec(),
            TErr::StringLimit => b"exceeding string size limit".to_vec(),
            TErr::NotIndexable => b"not indexable".to_vec(),
            TErr::NotIndexAssignable => b"not index-assignable".to_vec(),
            TErr::NotImplemented => b"not implemented".to_vec(),
            TErr::InvalidRangeStep => b"range step must be greater than 0".to_vec(),
            TErr::RangeLimit => b"exceeding range size limit".to_vec(),
            TErr::InvalidArgumentType { name, expected, found } => {
                format!("invalid type for argument '{}': expected {}, found {}", name, expected, found).into_bytes()
            }
            TErr::Msg(m) => m.clone(),
            TErr::Panic(m) => m.clone(),
        }
    }
}

pub type TResult<T> = Result<T, TErr>;

/// A native function: builtins, stdlib functions and user functions.
pub type NativeFn = Rc<dyn Fn(&[Object]) -> TResult<Object>>;

pub struct NativeFunction {
    pub name: String,
    pub func: NativeFn,
}

pub struct CompiledFunction {
    pub instructions: Rc<Vec<u8>>,
    pub num_locals: usize,
    pub num_parameters: usize,
    pub var_args: bool,
    /// None for a function produced by Copy(), which drops the source map.
    pub source_map: Option<Rc<HashMap<usize, Pos>>>,
    pub free: Vec<Rc<RefCell<Object>>>,
}

impl CompiledFunction {
    pub fn source_pos(&self, ip: i64) -> Pos {
        let mut ip = ip;
        if let Some(sm) = &self.source_map {
            while ip >= 0 {
                if let Some(p) = sm.get(&(ip as usize)) {
                    return *p;
                }
                ip -= 1;
            }
        }
        0
    }
}

pub struct ArrayObj {
    pub value: RefCell<GoSlice<Object>>,
}

pub struct ImmArrayObj {
    pub value: GoSlice<Object>,
}

pub type MapData = BTreeMap<Vec<u8>, Object>;

pub struct MapObj {
    pub data: Rc<RefCell<MapData>>,
}

pub struct ErrorObj {
    pub value: Object,
}

pub enum IterState {
    Array { v: GoSlice<Object>, i: usize, l: usize },
    Bytes { v: GoSlice<u8>, i: usize, l: usize },
    Map { data: Rc<RefCell<MapData>>, keys: Vec<Vec<u8>>, i: usize, l: usize },
    String { v: Rc<Vec<i32>>, i: usize, l: usize },
}

#[derive(Clone)]
pub enum Object {
    /// A Go nil Object (an interface with no value); only reachable through Go quirks.
    Nil,
    Undefined,
    Bool(bool),
    Int(i64),
    Float(f64),
    Char(i32),
    String(Rc<Vec<u8>>),
    Bytes(GoSlice<u8>),
    Array(Rc<ArrayObj>),
    ImmutableArray(Rc<ImmArrayObj>),
    Map(Rc<MapObj>),
    ImmutableMap(Rc<MapObj>),
    Error(Rc<ErrorObj>),
    CompiledFunction(Rc<CompiledFunction>),
    BuiltinFunction(Rc<NativeFunction>),
    UserFunction(Rc<NativeFunction>),
    Time(Rc<GoTime>),
    ObjectPtr(Rc<RefCell<Object>>),
    Iterator(Rc<RefCell<IterState>>),
}

impl SliceElem for Object {
    const SIZE: usize = 16;
    const NOSCAN: bool = false;
    fn zero() -> Self {
        Object::Nil
    }
}

pub fn new_str<S: AsRef<[u8]>>(s: S) -> Object {
    Object::String(Rc::new(s.as_ref().to_vec()))
}

pub fn new_array(v: GoSlice<Object>) -> Object {
    Object::Array(Rc::new(ArrayObj { value: RefCell::new(v) }))
}

pub fn new_imm_array(v: GoSlice<Object>) -> Object {
    Object::ImmutableArray(Rc::new(ImmArrayObj { value: v }))
}

pub fn new_map(m: MapData) -> Object {
    Object::Map(Rc::new(MapObj { data: Rc::new(RefCell::new(m)) }))
}

pub fn new_imm_map(m: MapData) -> Object {
    Object::ImmutableMap(Rc::new(MapObj { data: Rc::new(RefCell::new(m)) }))
}

pub fn new_error(v: Object) -> Object {
    Object::Error(Rc::new(ErrorObj { value: v }))
}

/// &tengo.Error{Value: &tengo.String{Value: msg}}
pub fn error_str<S: AsRef<[u8]>>(msg: S) -> Object {
    new_error(new_str(msg))
}

pub fn user_fn(name: &str, f: impl Fn(&[Object]) -> TResult<Object> + 'static) -> Object {
    Object::UserFunction(Rc::new(NativeFunction { name: name.to_string(), func: Rc::new(f) }))
}

pub fn bool_obj(b: bool) -> Object {
    Object::Bool(b)
}

impl Object {
    pub fn type_name(&self) -> String {
        match self {
            Object::Nil => "<nil>".to_string(),
            Object::Undefined => "undefined".to_string(),
            Object::Bool(_) => "bool".to_string(),
            Object::Int(_) => "int".to_string(),
            Object::Float(_) => "float".to_string(),
            Object::Char(_) => "char".to_string(),
            Object::String(_) => "string".to_string(),
            Object::Bytes(_) => "bytes".to_string(),
            Object::Array(_) => "array".to_string(),
            Object::ImmutableArray(_) => "immutable-array".to_string(),
            Object::Map(_) => "map".to_string(),
            Object::ImmutableMap(_) => "immutable-map".to_string(),
            Object::Error(_) => "error".to_string(),
            Object::CompiledFunction(_) => "compiled-function".to_string(),
            Object::BuiltinFunction(f) => format!("builtin-function:{}", f.name),
            Object::UserFunction(f) => format!("user-function:{}", f.name),
            Object::Time(_) => "time".to_string(),
            Object::ObjectPtr(_) => "<free-var>".to_string(),
            Object::Iterator(it) => match &*it.borrow() {
                IterState::Array { .. } => "array-iterator".to_string(),
                IterState::Bytes { .. } => "bytes-iterator".to_string(),
                IterState::Map { .. } => "map-iterator".to_string(),
                IterState::String { .. } => "string-iterator".to_string(),
            },
        }
    }

    /// Object.String()
    pub fn string(&self) -> TResult<Vec<u8>> {
        Ok(match self {
            Object::Nil => return Err(TErr::panic("runtime error: invalid memory address or nil pointer dereference")),
            Object::Undefined => b"<undefined>".to_vec(),
            Object::Bool(b) => if *b { b"true".to_vec() } else { b"false".to_vec() },
            Object::Int(i) => i.to_string().into_bytes(),
            Object::Float(f) => float_string(*f),
            Object::Char(c) => rune_string(*c),
            Object::String(s) => quote(s),
            Object::Bytes(b) => b.to_vec(),
            Object::Array(a) => elements_string(&a.value.borrow().to_vec())?,
            Object::ImmutableArray(a) => elements_string(&a.value.to_vec())?,
            Object::Map(m) | Object::ImmutableMap(m) => {
                let mut out = b"{".to_vec();
                let data = m.data.borrow();
                let mut first = true;
                for (k, v) in data.iter() {
                    if !first {
                        out.extend_from_slice(b", ");
                    }
                    first = false;
                    out.extend_from_slice(k);
                    out.extend_from_slice(b": ");
                    out.extend_from_slice(&v.string()?);
                }
                out.push(b'}');
                out
            }
            Object::Error(e) => {
                if let Object::Nil = e.value {
                    b"error".to_vec()
                } else {
                    let mut out = b"error: ".to_vec();
                    out.extend_from_slice(&e.value.string()?);
                    out
                }
            }
            Object::CompiledFunction(_) => b"<compiled-function>".to_vec(),
            Object::BuiltinFunction(_) => b"<builtin-function>".to_vec(),
            Object::UserFunction(_) => b"<user-function>".to_vec(),
            Object::Time(t) => t.string(),
            Object::ObjectPtr(_) => b"free-var".to_vec(),
            Object::Iterator(_) => {
                let mut s = b"<".to_vec();
                s.extend_from_slice(self.type_name().as_bytes());
                s.push(b'>');
                s
            }
        })
    }

    pub fn is_falsy(&self) -> bool {
        match self {
            Object::Nil => false,
            Object::Undefined => true,
            Object::Bool(b) => !b,
            Object::Int(i) => *i == 0,
            Object::Float(f) => f.is_nan(),
            Object::Char(c) => *c == 0,
            Object::String(s) => s.is_empty(),
            Object::Bytes(b) => b.len() == 0,
            Object::Array(a) => a.value.borrow().len() == 0,
            Object::ImmutableArray(a) => a.value.len() == 0,
            Object::Map(m) | Object::ImmutableMap(m) => m.data.borrow().is_empty(),
            Object::Error(_) => true,
            Object::Time(t) => t.is_zero(),
            Object::ObjectPtr(_) => false,
            Object::Iterator(it) => !matches!(&*it.borrow(), IterState::Bytes { .. }),
            _ => false,
        }
    }

    /// Object.Equals(another)
    pub fn equals(&self, x: &Object) -> bool {
        match self {
            Object::Nil => false,
            Object::Undefined => matches!(x, Object::Undefined),
            Object::Bool(a) => matches!(x, Object::Bool(b) if a == b),
            Object::Int(a) => match x {
                Object::Int(b) => a == b,
                Object::Float(b) => (*a as f64) == *b,
                _ => false,
            },
            Object::Float(a) => match x {
                Object::Float(b) => a == b,
                Object::Int(b) => *a == (*b as f64),
                _ => false,
            },
            Object::Char(a) => matches!(x, Object::Char(b) if a == b),
            Object::String(a) => matches!(x, Object::String(b) if a == b),
            Object::Bytes(a) => matches!(x, Object::Bytes(b) if a.to_vec() == b.to_vec()),
            Object::Array(_) | Object::ImmutableArray(_) => {
                let mine = array_elems(self).unwrap();
                let theirs = match array_elems(x) {
                    Some(v) => v,
                    None => return false,
                };
                if mine.len() != theirs.len() {
                    return false;
                }
                mine.iter().zip(theirs.iter()).all(|(a, b)| a.equals(b))
            }
            Object::Map(m) | Object::ImmutableMap(m) => {
                let other = match x {
                    Object::Map(o) | Object::ImmutableMap(o) => o,
                    _ => return false,
                };
                let a = m.data.borrow().clone();
                let b = other.data.borrow().clone();
                if a.len() != b.len() {
                    return false;
                }
                for (k, v) in a.iter() {
                    match b.get(k) {
                        Some(tv) => {
                            if !v.equals(tv) {
                                return false;
                            }
                        }
                        None => {
                            if !v.equals(&Object::Nil) {
                                return false;
                            }
                        }
                    }
                }
                true
            }
            Object::Error(a) => matches!(x, Object::Error(b) if Rc::ptr_eq(a, b)),
            Object::Time(a) => matches!(x, Object::Time(b) if a.equal(b)),
            Object::ObjectPtr(a) => matches!(x, Object::ObjectPtr(b) if Rc::ptr_eq(a, b)),
            _ => false,
        }
    }

    /// Object.Copy()
    pub fn copy(&self) -> Object {
        match self {
            Object::Array(a) => {
                let items: Vec<Object> = a.value.borrow().to_vec().iter().map(|e| e.copy()).collect();
                new_array(GoSlice::from_appends(items))
            }
            Object::ImmutableArray(a) => {
                let items: Vec<Object> = a.value.to_vec().iter().map(|e| e.copy()).collect();
                new_array(GoSlice::from_appends(items))
            }
            Object::Map(m) | Object::ImmutableMap(m) => {
                let mut c = MapData::new();
                for (k, v) in m.data.borrow().iter() {
                    c.insert(k.clone(), v.copy());
                }
                new_map(c)
            }
            Object::Bytes(b) => {
                // append([]byte{}, v...)
                Object::Bytes(GoSlice::from_vec_exact(Vec::new()).append(&b.to_vec()))
            }
            Object::String(s) => Object::String(Rc::new((**s).clone())),
            Object::Error(e) => new_error(e.value.copy()),
            Object::BuiltinFunction(f) => {
                Object::BuiltinFunction(Rc::new(NativeFunction { name: String::new(), func: f.func.clone() }))
            }
            Object::UserFunction(f) => {
                Object::UserFunction(Rc::new(NativeFunction { name: f.name.clone(), func: f.func.clone() }))
            }
            Object::CompiledFunction(f) => Object::CompiledFunction(Rc::new(CompiledFunction {
                instructions: Rc::new((*f.instructions).clone()),
                num_locals: f.num_locals,
                num_parameters: f.num_parameters,
                var_args: f.var_args,
                source_map: None,
                free: f.free.clone(),
            })),
            Object::Time(t) => Object::Time(Rc::new((**t).clone())),
            Object::Iterator(it) => {
                let st = match &*it.borrow() {
                    IterState::Array { v, i, l } => IterState::Array { v: v.clone(), i: *i, l: *l },
                    IterState::Bytes { v, i, l } => IterState::Bytes { v: v.clone(), i: *i, l: *l },
                    IterState::Map { data, keys, i, l } => {
                        IterState::Map { data: data.clone(), keys: keys.clone(), i: *i, l: *l }
                    }
                    IterState::String { v, i, l } => IterState::String { v: v.clone(), i: *i, l: *l },
                };
                Object::Iterator(Rc::new(RefCell::new(st)))
            }
            // Bool, Undefined and ObjectPtr return themselves; scalars are values.
            other => other.clone(),
        }
    }

    pub fn can_call(&self) -> bool {
        matches!(self, Object::CompiledFunction(_) | Object::BuiltinFunction(_) | Object::UserFunction(_))
    }

    pub fn can_iterate(&self) -> bool {
        matches!(
            self,
            Object::Array(_)
                | Object::ImmutableArray(_)
                | Object::Bytes(_)
                | Object::Map(_)
                | Object::ImmutableMap(_)
                | Object::String(_)
                | Object::Undefined
        )
    }

    /// Object.Iterate(): an iterator object (Undefined iterates as itself).
    pub fn iterate(&self) -> Object {
        let st = match self {
            Object::Array(a) => {
                let v = a.value.borrow().clone();
                let l = v.len();
                IterState::Array { v, i: 0, l }
            }
            Object::ImmutableArray(a) => {
                let v = a.value.clone();
                let l = v.len();
                IterState::Array { v, i: 0, l }
            }
            Object::Bytes(b) => IterState::Bytes { v: b.clone(), i: 0, l: b.len() },
            Object::Map(m) | Object::ImmutableMap(m) => {
                let keys: Vec<Vec<u8>> = m.data.borrow().keys().cloned().collect();
                let l = keys.len();
                IterState::Map { data: m.data.clone(), keys, i: 0, l }
            }
            Object::String(s) => {
                let r = runes(s);
                let l = r.len();
                IterState::String { v: Rc::new(r), i: 0, l }
            }
            _ => return Object::Undefined,
        };
        Object::Iterator(Rc::new(RefCell::new(st)))
    }

    pub fn index_get(&self, index: &Object) -> TResult<Object> {
        match self {
            Object::Array(_) | Object::ImmutableArray(_) => {
                let i = match index {
                    Object::Int(i) => *i,
                    _ => return Err(TErr::InvalidIndexType),
                };
                let v = match self {
                    Object::Array(a) => a.value.borrow().clone(),
                    Object::ImmutableArray(a) => a.value.clone(),
                    _ => unreachable!(),
                };
                if i < 0 || i as usize >= v.len() {
                    return Ok(Object::Undefined);
                }
                Ok(v.get(i as usize))
            }
            Object::Bytes(b) => {
                let i = match index {
                    Object::Int(i) => *i,
                    _ => return Err(TErr::InvalidIndexType),
                };
                if i < 0 || i as usize >= b.len() {
                    return Ok(Object::Undefined);
                }
                Ok(Object::Int(b.get(i as usize) as i64))
            }
            Object::Error(e) => {
                let (s, _) = to_string(index)?;
                if s != b"value" {
                    return Err(TErr::InvalidIndexOnError);
                }
                Ok(e.value.clone())
            }
            Object::Map(m) | Object::ImmutableMap(m) => {
                let (k, ok) = to_string(index)?;
                if !ok {
                    return Err(TErr::InvalidIndexType);
                }
                Ok(m.data.borrow().get(&k).cloned().unwrap_or(Object::Undefined))
            }
            Object::String(s) => {
                let i = match index {
                    Object::Int(i) => *i,
                    _ => return Err(TErr::InvalidIndexType),
                };
                let r = runes(s);
                if i < 0 || i as usize >= r.len() {
                    return Ok(Object::Undefined);
                }
                Ok(Object::Char(r[i as usize]))
            }
            Object::Undefined => Ok(Object::Undefined),
            Object::Nil => Err(TErr::panic("runtime error: invalid memory address or nil pointer dereference")),
            _ => Err(TErr::NotIndexable),
        }
    }

    pub fn index_set(&self, index: &Object, value: Object) -> TResult<()> {
        match self {
            Object::Array(a) => {
                let i = match to_int(index) {
                    Some(i) => i,
                    None => return Err(TErr::InvalidIndexType),
                };
                let v = a.value.borrow();
                if i < 0 || i as usize >= v.len() {
                    return Err(TErr::IndexOutOfBounds);
                }
                v.set(i as usize, value);
                Ok(())
            }
            Object::Map(m) => {
                let (k, ok) = to_string(index)?;
                if !ok {
                    return Err(TErr::InvalidIndexType);
                }
                m.data.borrow_mut().insert(k, value);
                Ok(())
            }
            Object::Nil => Err(TErr::panic("runtime error: invalid memory address or nil pointer dereference")),
            _ => Err(TErr::NotIndexAssignable),
        }
    }

    pub fn binary_op(&self, op: Token, rhs: &Object) -> TResult<Object> {
        match self {
            Object::Int(a) => int_binary_op(*a, op, rhs),
            Object::Float(a) => float_binary_op(*a, op, rhs),
            Object::Char(a) => char_binary_op(*a, op, rhs),
            Object::String(a) => string_binary_op(a, op, rhs),
            Object::Bytes(a) => {
                if op == Token::Add {
                    if let Object::Bytes(b) = rhs {
                        if a.len() + b.len() > MAX_BYTES_LEN {
                            return Err(TErr::BytesLimit);
                        }
                        return Ok(Object::Bytes(a.append_slice(b)));
                    }
                }
                Err(TErr::InvalidOperator)
            }
            Object::Array(a) => {
                if let Object::Array(b) = rhs {
                    if op == Token::Add {
                        let bv = b.value.borrow().clone();
                        if bv.len() == 0 {
                            return Ok(self.clone());
                        }
                        let av = a.value.borrow().clone();
                        return Ok(new_array(av.append_slice(&bv)));
                    }
                }
                Err(TErr::InvalidOperator)
            }
            Object::ImmutableArray(a) => {
                if let Object::ImmutableArray(b) = rhs {
                    if op == Token::Add {
                        return Ok(new_array(a.value.append_slice(&b.value)));
                    }
                }
                Err(TErr::InvalidOperator)
            }
            Object::Time(t) => crate::gotime::time_binary_op(self, t, op, rhs),
            Object::Nil => Err(TErr::panic("runtime error: invalid memory address or nil pointer dereference")),
            _ => Err(TErr::InvalidOperator),
        }
    }
}

fn elements_string(items: &[Object]) -> TResult<Vec<u8>> {
    let mut out = b"[".to_vec();
    for (i, e) in items.iter().enumerate() {
        if i > 0 {
            out.extend_from_slice(b", ");
        }
        out.extend_from_slice(&e.string()?);
    }
    out.push(b']');
    Ok(out)
}

/// The element slice of an Array or ImmutableArray.
pub fn array_elems(o: &Object) -> Option<Vec<Object>> {
    match o {
        Object::Array(a) => Some(a.value.borrow().to_vec()),
        Object::ImmutableArray(a) => Some(a.value.to_vec()),
        _ => None,
    }
}

fn cmp_result(b: bool) -> TResult<Object> {
    Ok(Object::Bool(b))
}

fn shl(a: i64, s: i64) -> i64 {
    let s = s as u64;
    if s >= 64 {
        0
    } else {
        a.wrapping_shl(s as u32)
    }
}

fn shr(a: i64, s: i64) -> i64 {
    let s = s as u64;
    if s >= 64 {
        if a < 0 {
            -1
        } else {
            0
        }
    } else {
        a >> s
    }
}

fn int_binary_op(a: i64, op: Token, rhs: &Object) -> TResult<Object> {
    match rhs {
        Object::Int(b) => {
            let b = *b;
            let r = match op {
                Token::Add => a.wrapping_add(b),
                Token::Sub => a.wrapping_sub(b),
                Token::Mul => a.wrapping_mul(b),
                Token::Quo => {
                    if b == 0 {
                        return Err(TErr::panic("runtime error: integer divide by zero"));
                    }
                    a.wrapping_div(b)
                }
                Token::Rem => {
                    if b == 0 {
                        return Err(TErr::panic("runtime error: integer divide by zero"));
                    }
                    a.wrapping_rem(b)
                }
                Token::And => a & b,
                Token::Or => a | b,
                Token::Xor => a ^ b,
                Token::AndNot => a & !b,
                Token::Shl => shl(a, b),
                Token::Shr => shr(a, b),
                Token::Less => return cmp_result(a < b),
                Token::Greater => return cmp_result(a > b),
                Token::LessEq => return cmp_result(a <= b),
                Token::GreaterEq => return cmp_result(a >= b),
                _ => return Err(TErr::InvalidOperator),
            };
            Ok(Object::Int(r))
        }
        Object::Float(b) => {
            let a = a as f64;
            match op {
                Token::Add => Ok(Object::Float(a + b)),
                Token::Sub => Ok(Object::Float(a - b)),
                Token::Mul => Ok(Object::Float(a * b)),
                Token::Quo => Ok(Object::Float(a / b)),
                Token::Less => cmp_result(a < *b),
                Token::Greater => cmp_result(a > *b),
                Token::LessEq => cmp_result(a <= *b),
                Token::GreaterEq => cmp_result(a >= *b),
                _ => Err(TErr::InvalidOperator),
            }
        }
        Object::Char(b) => match op {
            Token::Add => Ok(Object::Char((a as i32).wrapping_add(*b))),
            Token::Sub => Ok(Object::Char((a as i32).wrapping_sub(*b))),
            Token::Less => cmp_result(a < *b as i64),
            Token::Greater => cmp_result(a > *b as i64),
            Token::LessEq => cmp_result(a <= *b as i64),
            Token::GreaterEq => cmp_result(a >= *b as i64),
            _ => Err(TErr::InvalidOperator),
        },
        _ => Err(TErr::InvalidOperator),
    }
}

fn float_binary_op(a: f64, op: Token, rhs: &Object) -> TResult<Object> {
    let b = match rhs {
        Object::Float(b) => *b,
        Object::Int(b) => *b as f64,
        _ => return Err(TErr::InvalidOperator),
    };
    match op {
        Token::Add => Ok(Object::Float(a + b)),
        Token::Sub => Ok(Object::Float(a - b)),
        Token::Mul => Ok(Object::Float(a * b)),
        Token::Quo => Ok(Object::Float(a / b)),
        Token::Less => cmp_result(a < b),
        Token::Greater => cmp_result(a > b),
        Token::LessEq => cmp_result(a <= b),
        Token::GreaterEq => cmp_result(a >= b),
        _ => Err(TErr::InvalidOperator),
    }
}

fn char_binary_op(a: i32, op: Token, rhs: &Object) -> TResult<Object> {
    match rhs {
        Object::Char(b) => match op {
            Token::Add => Ok(Object::Char(a.wrapping_add(*b))),
            Token::Sub => Ok(Object::Char(a.wrapping_sub(*b))),
            Token::Less => cmp_result(a < *b),
            Token::Greater => cmp_result(a > *b),
            Token::LessEq => cmp_result(a <= *b),
            Token::GreaterEq => cmp_result(a >= *b),
            _ => Err(TErr::InvalidOperator),
        },
        Object::Int(b) => match op {
            Token::Add => Ok(Object::Char(a.wrapping_add(*b as i32))),
            Token::Sub => Ok(Object::Char(a.wrapping_sub(*b as i32))),
            Token::Less => cmp_result((a as i64) < *b),
            Token::Greater => cmp_result((a as i64) > *b),
            Token::LessEq => cmp_result((a as i64) <= *b),
            Token::GreaterEq => cmp_result((a as i64) >= *b),
            _ => Err(TErr::InvalidOperator),
        },
        _ => Err(TErr::InvalidOperator),
    }
}

fn string_binary_op(a: &Rc<Vec<u8>>, op: Token, rhs: &Object) -> TResult<Object> {
    match op {
        Token::Add => {
            let r = match rhs {
                Object::String(b) => (**b).clone(),
                other => other.string()?,
            };
            if a.len() + r.len() > MAX_STRING_LEN {
                return Err(TErr::StringLimit);
            }
            let mut v = (**a).clone();
            v.extend_from_slice(&r);
            Ok(Object::String(Rc::new(v)))
        }
        Token::Less | Token::LessEq | Token::Greater | Token::GreaterEq => {
            if let Object::String(b) = rhs {
                let r = match op {
                    Token::Less => a.as_slice() < b.as_slice(),
                    Token::LessEq => a.as_slice() <= b.as_slice(),
                    Token::Greater => a.as_slice() > b.as_slice(),
                    _ => a.as_slice() >= b.as_slice(),
                };
                return Ok(Object::Bool(r));
            }
            Err(TErr::InvalidOperator)
        }
        _ => Err(TErr::InvalidOperator),
    }
}

// ---------------------------------------------------------------------------------------------
// conversions (tengo.go)

/// tengo.ToString: the raw string value, String() for other objects, and not ok for
/// undefined.
pub fn to_string(o: &Object) -> TResult<(Vec<u8>, bool)> {
    match o {
        Object::Undefined => Ok((Vec::new(), false)),
        Object::String(s) => Ok(((**s).clone(), true)),
        other => Ok((other.string()?, true)),
    }
}

/// tengo.ToInt / ToInt64 (identical on 64-bit).
pub fn to_int(o: &Object) -> Option<i64> {
    match o {
        Object::Int(i) => Some(*i),
        Object::Float(f) => Some(*f as i64),
        Object::Char(c) => Some(*c as i64),
        Object::Bool(b) => Some(if *b { 1 } else { 0 }),
        Object::String(s) => parse_int(s, 10, 64).ok(),
        _ => None,
    }
}

pub fn to_float(o: &Object) -> Option<f64> {
    match o {
        Object::Int(i) => Some(*i as f64),
        Object::Float(f) => Some(*f),
        Object::String(s) => parse_float(s, 64).ok(),
        _ => None,
    }
}

pub fn to_rune(o: &Object) -> Option<i32> {
    match o {
        Object::Int(i) => Some(*i as i32),
        Object::Char(c) => Some(*c),
        _ => None,
    }
}

pub fn to_byte_slice(o: &Object) -> Option<GoSlice<u8>> {
    match o {
        Object::Bytes(b) => Some(b.clone()),
        Object::String(s) => Some(bytes_from_string(s)),
        _ => None,
    }
}

/// The raw bytes of ToByteSlice.
pub fn to_bytes_vec(o: &Object) -> Option<Vec<u8>> {
    match o {
        Object::Bytes(b) => Some(b.to_vec()),
        Object::String(s) => Some((**s).clone()),
        _ => None,
    }
}
