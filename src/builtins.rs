//! Builtin functions (builtins.go).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::formatter::format;
use crate::goslice::GoSlice;
use crate::gotime::GoTime;
use crate::objects::*;

type Bf = fn(&[Object]) -> TResult<Object>;

const BUILTINS: [(&str, Bf); 32] = [
    ("len", builtin_len),
    ("copy", builtin_copy),
    ("append", builtin_append),
    ("delete", builtin_delete),
    ("splice", builtin_splice),
    ("string", builtin_string),
    ("int", builtin_int),
    ("bool", builtin_bool),
    ("float", builtin_float),
    ("char", builtin_char),
    ("bytes", builtin_bytes),
    ("time", builtin_time),
    ("is_int", |a| is_kind(a, |o| matches!(o, Object::Int(_)))),
    ("is_float", |a| is_kind(a, |o| matches!(o, Object::Float(_)))),
    ("is_string", |a| is_kind(a, |o| matches!(o, Object::String(_)))),
    ("is_bool", |a| is_kind(a, |o| matches!(o, Object::Bool(_)))),
    ("is_char", |a| is_kind(a, |o| matches!(o, Object::Char(_)))),
    ("is_bytes", |a| is_kind(a, |o| matches!(o, Object::Bytes(_)))),
    ("is_array", |a| is_kind(a, |o| matches!(o, Object::Array(_)))),
    ("is_immutable_array", |a| is_kind(a, |o| matches!(o, Object::ImmutableArray(_)))),
    ("is_map", |a| is_kind(a, |o| matches!(o, Object::Map(_)))),
    ("is_immutable_map", |a| is_kind(a, |o| matches!(o, Object::ImmutableMap(_)))),
    ("is_iterable", |a| is_kind(a, |o| o.can_iterate())),
    ("is_time", |a| is_kind(a, |o| matches!(o, Object::Time(_)))),
    ("is_error", |a| is_kind(a, |o| matches!(o, Object::Error(_)))),
    ("is_undefined", |a| is_kind(a, |o| matches!(o, Object::Undefined))),
    ("is_function", |a| is_kind(a, |o| matches!(o, Object::CompiledFunction(_)))),
    ("is_callable", |a| is_kind(a, |o| o.can_call())),
    ("type_name", builtin_type_name),
    ("format", builtin_format),
    ("range", builtin_range),
    ("freeze", builtin_freeze),
];

thread_local! {
    static BUILTIN_OBJECTS: Vec<Object> = BUILTINS
        .iter()
        .map(|(name, f)| {
            let f = *f;
            Object::BuiltinFunction(Rc::new(NativeFunction { name: name.to_string(), func: Rc::new(f) }))
        })
        .collect();
}

/// The builtin function object at an index (the same object on every lookup).
pub fn builtin_function(i: usize) -> Option<Object> {
    BUILTIN_OBJECTS.with(|b| b.get(i).cloned())
}

fn is_kind(args: &[Object], pred: impl Fn(&Object) -> bool) -> TResult<Object> {
    if args.len() != 1 {
        return Err(TErr::WrongNumArguments);
    }
    Ok(Object::Bool(pred(&args[0])))
}

fn builtin_type_name(args: &[Object]) -> TResult<Object> {
    if args.len() != 1 {
        return Err(TErr::WrongNumArguments);
    }
    Ok(new_str(args[0].type_name()))
}

fn builtin_len(args: &[Object]) -> TResult<Object> {
    if args.len() != 1 {
        return Err(TErr::WrongNumArguments);
    }
    let n = match &args[0] {
        Object::Array(a) => a.value.borrow().len(),
        Object::ImmutableArray(a) => a.value.len(),
        Object::String(s) => s.len(),
        Object::Bytes(b) => b.len(),
        Object::Map(m) | Object::ImmutableMap(m) => m.data.borrow().len(),
        other => return Err(TErr::arg("first", "array/string/bytes/map", other)),
    };
    Ok(Object::Int(n as i64))
}

fn range_len(start: i64, stop: i64, step: i64) -> i64 {
    let span: u64 = if start <= stop {
        (stop as u64).wrapping_sub(start as u64)
    } else {
        (start as u64).wrapping_sub(stop as u64)
    };
    let ustep = step as u64;
    let mut n = span / ustep;
    if span % ustep != 0 {
        n += 1;
    }
    if n > i64::MAX as u64 {
        return i64::MAX;
    }
    n as i64
}

fn builtin_range(args: &[Object]) -> TResult<Object> {
    if args.len() < 2 || args.len() > 3 {
        return Err(TErr::WrongNumArguments);
    }
    let start = match &args[0] {
        Object::Int(i) => *i,
        o => return Err(TErr::arg("start", "int", o)),
    };
    let stop = match &args[1] {
        Object::Int(i) => *i,
        o => return Err(TErr::arg("stop", "int", o)),
    };
    let mut step = 1i64;
    if args.len() == 3 {
        match &args[2] {
            Object::Int(i) => {
                if *i <= 0 {
                    return Err(TErr::InvalidRangeStep);
                }
                step = *i;
            }
            o => return Err(TErr::arg("step", "int", o)),
        }
    }
    let n = range_len(start, stop, step);
    if n > MAX_RANGE_LEN {
        return Err(TErr::RangeLimit);
    }
    if n == 0 {
        return Ok(new_array(GoSlice::nil()));
    }
    if start > stop {
        step = -step;
    }
    let mut v = start;
    let mut elems = Vec::with_capacity(n as usize);
    for _ in 0..n {
        elems.push(Object::Int(v));
        v = v.wrapping_add(step);
    }
    Ok(new_array(GoSlice::from_vec_exact(elems)))
}

fn builtin_format(args: &[Object]) -> TResult<Object> {
    if args.is_empty() {
        return Err(TErr::WrongNumArguments);
    }
    let f = match &args[0] {
        Object::String(s) => s.clone(),
        o => return Err(TErr::arg("format", "string", o)),
    };
    if args.len() == 1 {
        return Ok(args[0].clone());
    }
    let s = format(&f, &args[1..])?;
    Ok(new_str(s))
}

fn builtin_copy(args: &[Object]) -> TResult<Object> {
    if args.len() != 1 {
        return Err(TErr::WrongNumArguments);
    }
    if let Object::Nil = args[0] {
        return Err(TErr::panic("runtime error: invalid memory address or nil pointer dereference"));
    }
    Ok(args[0].copy())
}

fn builtin_string(args: &[Object]) -> TResult<Object> {
    if !(args.len() == 1 || args.len() == 2) {
        return Err(TErr::WrongNumArguments);
    }
    if let Object::String(_) = args[0] {
        return Ok(args[0].clone());
    }
    let (v, ok) = to_string(&args[0])?;
    if ok {
        return Ok(new_str(v));
    }
    if args.len() == 2 {
        return Ok(args[1].clone());
    }
    Ok(Object::Undefined)
}

fn builtin_int(args: &[Object]) -> TResult<Object> {
    if !(args.len() == 1 || args.len() == 2) {
        return Err(TErr::WrongNumArguments);
    }
    if let Object::Int(_) = args[0] {
        return Ok(args[0].clone());
    }
    if let Some(v) = to_int(&args[0]) {
        return Ok(Object::Int(v));
    }
    if args.len() == 2 {
        return Ok(args[1].clone());
    }
    Ok(Object::Undefined)
}

fn builtin_float(args: &[Object]) -> TResult<Object> {
    if !(args.len() == 1 || args.len() == 2) {
        return Err(TErr::WrongNumArguments);
    }
    if let Object::Float(_) = args[0] {
        return Ok(args[0].clone());
    }
    if let Some(v) = to_float(&args[0]) {
        return Ok(Object::Float(v));
    }
    if args.len() == 2 {
        return Ok(args[1].clone());
    }
    Ok(Object::Undefined)
}

fn builtin_bool(args: &[Object]) -> TResult<Object> {
    if args.len() != 1 {
        return Err(TErr::WrongNumArguments);
    }
    if let Object::Bool(_) = args[0] {
        return Ok(args[0].clone());
    }
    Ok(Object::Bool(!args[0].is_falsy()))
}

fn builtin_char(args: &[Object]) -> TResult<Object> {
    if !(args.len() == 1 || args.len() == 2) {
        return Err(TErr::WrongNumArguments);
    }
    if let Object::Char(_) = args[0] {
        return Ok(args[0].clone());
    }
    if let Some(v) = to_rune(&args[0]) {
        return Ok(Object::Char(v));
    }
    if args.len() == 2 {
        return Ok(args[1].clone());
    }
    Ok(Object::Undefined)
}

fn builtin_bytes(args: &[Object]) -> TResult<Object> {
    if !(args.len() == 1 || args.len() == 2) {
        return Err(TErr::WrongNumArguments);
    }
    if let Object::Int(n) = args[0] {
        if n > MAX_BYTES_LEN as i64 {
            return Err(TErr::BytesLimit);
        }
        if n < 0 {
            return Err(TErr::panic("runtime error: makeslice: len out of range"));
        }
        return Ok(Object::Bytes(GoSlice::make(n as usize, n as usize)));
    }
    if let Some(v) = to_byte_slice(&args[0]) {
        return Ok(Object::Bytes(v));
    }
    if args.len() == 2 {
        return Ok(args[1].clone());
    }
    Ok(Object::Undefined)
}

fn builtin_time(args: &[Object]) -> TResult<Object> {
    if !(args.len() == 1 || args.len() == 2) {
        return Err(TErr::WrongNumArguments);
    }
    match &args[0] {
        Object::Time(_) => return Ok(args[0].clone()),
        Object::Int(i) => return Ok(Object::Time(Rc::new(GoTime::unix(*i, 0)))),
        _ => {}
    }
    if args.len() == 2 {
        return Ok(args[1].clone());
    }
    Ok(Object::Undefined)
}

fn builtin_append(args: &[Object]) -> TResult<Object> {
    if args.len() < 2 {
        return Err(TErr::WrongNumArguments);
    }
    match &args[0] {
        Object::Array(a) => {
            let s = a.value.borrow().clone();
            Ok(new_array(s.append(&args[1..])))
        }
        Object::ImmutableArray(a) => Ok(new_array(a.value.append(&args[1..]))),
        o => Err(TErr::arg("first", "array", o)),
    }
}

fn builtin_delete(args: &[Object]) -> TResult<Object> {
    if args.len() != 2 {
        return Err(TErr::WrongNumArguments);
    }
    match &args[0] {
        Object::Map(m) => {
            if let Object::String(k) = &args[1] {
                m.data.borrow_mut().remove(&**k);
                return Ok(Object::Undefined);
            }
            Err(TErr::arg("second", "string", &args[1]))
        }
        o => Err(TErr::arg("first", "map", o)),
    }
}

fn builtin_splice(args: &[Object]) -> TResult<Object> {
    let n = args.len();
    if n == 0 {
        return Err(TErr::WrongNumArguments);
    }
    let array = match &args[0] {
        Object::Array(a) => a.clone(),
        o => return Err(TErr::arg("first", "array", o)),
    };
    let value = array.value.borrow().clone();
    let array_len = value.len() as i64;
    let mut start: i64 = 0;
    if n > 1 {
        match &args[1] {
            Object::Int(i) => {
                start = *i;
                if start < 0 || start > array_len {
                    return Err(TErr::IndexOutOfBounds);
                }
            }
            o => return Err(TErr::arg("second", "int", o)),
        }
    }
    let mut del_count = array_len;
    if n > 2 {
        match &args[2] {
            Object::Int(i) => {
                del_count = *i;
                if del_count < 0 {
                    return Err(TErr::IndexOutOfBounds);
                }
            }
            o => return Err(TErr::arg("third", "int", o)),
        }
    }
    if start.wrapping_add(del_count) > array_len {
        del_count = array_len - start;
    }
    let end = start.wrapping_add(del_count);
    if end < start || end > value.cap as i64 {
        return Err(TErr::panic(format!("runtime error: slice bounds out of range [{}:{}]", start, end)));
    }
    let (s, e) = (start as usize, end as usize);
    let deleted_items = value.slice(s, e).to_vec();
    let deleted = GoSlice::from_vec_exact(Vec::new()).append(&deleted_items);
    let head = value.slice(0, s);
    let mut items: Vec<Object> = Vec::new();
    if n > 3 {
        items.extend_from_slice(&args[3..]);
    }
    items.extend(value.slice(e, value.len()).to_vec());
    let newv = head.append(&items);
    *array.value.borrow_mut() = newv;
    Ok(new_array(deleted))
}

fn builtin_freeze(args: &[Object]) -> TResult<Object> {
    if args.len() != 1 {
        return Err(TErr::WrongNumArguments);
    }
    let mut memo: HashMap<usize, Object> = HashMap::new();
    Ok(freeze_object(&args[0], &mut memo).0)
}

/// freezeObject: returns the frozen object and whether it is a different object.
fn freeze_object(o: &Object, memo: &mut HashMap<usize, Object>) -> (Object, bool) {
    match o {
        Object::Array(a) => {
            let key = Rc::as_ptr(a) as usize;
            if let Some(c) = memo.get(&key) {
                return (c.clone(), true);
            }
            let elems = a.value.borrow().to_vec();
            let frozen_slice: GoSlice<Object> = GoSlice::make(elems.len(), elems.len());
            let frozen = new_imm_array(frozen_slice.clone());
            memo.insert(key, frozen.clone());
            for (i, e) in elems.iter().enumerate() {
                frozen_slice.set(i, freeze_object(e, memo).0);
            }
            (frozen, true)
        }
        Object::Map(m) => {
            let key = Rc::as_ptr(m) as usize;
            if let Some(c) = memo.get(&key) {
                return (c.clone(), true);
            }
            let data: Rc<RefCell<MapData>> = Rc::new(RefCell::new(MapData::new()));
            let frozen = Object::ImmutableMap(Rc::new(MapObj { data: data.clone() }));
            memo.insert(key, frozen.clone());
            let entries: Vec<(Vec<u8>, Object)> = m.data.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            for (k, v) in entries {
                let f = freeze_object(&v, memo).0;
                data.borrow_mut().insert(k, f);
            }
            (frozen, true)
        }
        Object::ImmutableArray(a) => {
            let elems = a.value.to_vec();
            let mut changed = false;
            let mut new_elems = Vec::with_capacity(elems.len());
            for e in elems.iter() {
                let (f, c) = freeze_object(e, memo);
                changed |= c;
                new_elems.push(f);
            }
            if !changed {
                return (o.clone(), false);
            }
            (new_imm_array(GoSlice::from_vec_exact(new_elems)), true)
        }
        Object::ImmutableMap(m) => {
            let entries: Vec<(Vec<u8>, Object)> = m.data.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            let mut changed = false;
            let mut nm = MapData::new();
            for (k, v) in entries {
                let (f, c) = freeze_object(&v, memo);
                changed |= c;
                nm.insert(k, f);
            }
            if !changed {
                return (o.clone(), false);
            }
            (new_imm_map(nm), true)
        }
        other => (other.clone(), false),
    }
}
