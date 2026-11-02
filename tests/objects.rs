//! Carried over from objects_test.go: type names, truthiness, string forms and binary operators.

mod common;

use common::canon;
use tengo::goslice::GoSlice;
use tengo::objects::{new_array, new_error, new_map, new_str, MapData, Object};
use tengo::token::Token;

fn arr(v: &[i64]) -> Object {
    new_array(GoSlice::from_vec_exact(v.iter().map(|i| Object::Int(*i)).collect()))
}

fn bytes(b: &[u8]) -> Object {
    Object::Bytes(GoSlice::from_vec_exact(b.to_vec()))
}

fn empty_map() -> Object {
    new_map(MapData::new())
}

#[track_caller]
fn op(lhs: Object, token: Token, rhs: Object, expected: Object) {
    let got = lhs.binary_op(token, &rhs).unwrap();
    assert_eq!(canon(&got), canon(&expected), "{} {:?} {}", canon(&lhs), token, canon(&rhs));
}

#[test]
fn type_name() {
    for (o, name) in [
        (Object::Int(0), "int"),
        (Object::Float(0.0), "float"),
        (Object::Char(0), "char"),
        (new_str(""), "string"),
        (Object::Bool(false), "bool"),
        (arr(&[]), "array"),
        (empty_map(), "map"),
        (Object::Undefined, "undefined"),
        (new_error(new_str("e")), "error"),
        (bytes(b""), "bytes"),
        (arr(&[]).iterate(), "array-iterator"),
        (new_str("").iterate(), "string-iterator"),
        (empty_map().iterate(), "map-iterator"),
    ] {
        assert_eq!(o.type_name(), name);
    }
}

#[test]
fn is_falsy() {
    for (o, falsy) in [
        (Object::Int(0), true),
        (Object::Int(1), false),
        (Object::Float(0.0), false),
        (Object::Float(1.0), false),
        (Object::Char(' ' as i32), false),
        (Object::Char('T' as i32), false),
        (new_str(""), true),
        (new_str(" "), false),
        (arr(&[]), true),
        (arr(&[0]), false),
        (empty_map(), true),
        (new_str("").iterate(), true),
        (arr(&[]).iterate(), true),
        (empty_map().iterate(), true),
        (Object::Undefined, true),
        (new_error(Object::Nil), true),
        (bytes(b""), true),
        (bytes(&[1, 2]), false),
    ] {
        assert_eq!(o.is_falsy(), falsy, "{}", canon(&o));
    }
}

#[test]
fn string_form() {
    for (o, s) in [
        (Object::Int(0), "0"),
        (Object::Int(1), "1"),
        (Object::Float(0.0), "0"),
        (Object::Float(1.0), "1"),
        (Object::Char(' ' as i32), " "),
        (Object::Char('T' as i32), "T"),
        (new_str(""), "\"\""),
        (new_str(" "), "\" \""),
        (arr(&[]), "[]"),
        (empty_map(), "{}"),
        (new_error(new_str("error 1")), "error: \"error 1\""),
        (new_str("").iterate(), "<string-iterator>"),
        (arr(&[]).iterate(), "<array-iterator>"),
        (empty_map().iterate(), "<map-iterator>"),
        (Object::Undefined, "<undefined>"),
        (bytes(b""), ""),
        (bytes(b"foo"), "foo"),
    ] {
        assert_eq!(String::from_utf8_lossy(&o.string().unwrap()), s);
    }
}

#[test]
fn binary_op_rejected() {
    for o in [
        Object::Char(0),
        Object::Bool(false),
        empty_map(),
        arr(&[]).iterate(),
        new_str("").iterate(),
        empty_map().iterate(),
        Object::Undefined,
        new_error(new_str("e")),
    ] {
        assert!(o.binary_op(Token::Add, &Object::Undefined).is_err(), "{}", canon(&o));
    }
}

#[test]
fn array_add() {
    op(arr(&[]), Token::Add, arr(&[]), arr(&[]));
    op(arr(&[]), Token::Add, arr(&[1]), arr(&[1]));
    op(arr(&[]), Token::Add, arr(&[1, 2, 3]), arr(&[1, 2, 3]));
    op(arr(&[1, 2, 3]), Token::Add, arr(&[]), arr(&[1, 2, 3]));
    op(arr(&[1, 2, 3]), Token::Add, arr(&[4, 5, 6]), arr(&[1, 2, 3, 4, 5, 6]));
}

#[test]
fn error_equals() {
    let e = new_error(new_str("some error"));
    assert!(e.equals(&e.clone()));
    assert!(!e.equals(&new_error(new_str("some error"))));
}

fn steps(step: f64) -> Vec<f64> {
    let mut v = Vec::new();
    let mut x = -2.0;
    while x <= 2.1 {
        v.push(x);
        x += step;
    }
    v
}

#[test]
fn float_binary_op() {
    let f = Object::Float;
    for l in steps(0.4) {
        for r in steps(0.4) {
            op(f(l), Token::Add, f(r), f(l + r));
            op(f(l), Token::Sub, f(r), f(l - r));
            op(f(l), Token::Mul, f(r), f(l * r));
            if r != 0.0 {
                op(f(l), Token::Quo, f(r), f(l / r));
            }
            op(f(l), Token::Less, f(r), Object::Bool(l < r));
            op(f(l), Token::Greater, f(r), Object::Bool(l > r));
            op(f(l), Token::LessEq, f(r), Object::Bool(l <= r));
            op(f(l), Token::GreaterEq, f(r), Object::Bool(l >= r));
        }
        for r in -2..=2i64 {
            let x = r as f64;
            op(f(l), Token::Add, Object::Int(r), f(l + x));
            op(f(l), Token::Sub, Object::Int(r), f(l - x));
            op(f(l), Token::Mul, Object::Int(r), f(l * x));
            if r != 0 {
                op(f(l), Token::Quo, Object::Int(r), f(l / x));
            }
            op(f(l), Token::Less, Object::Int(r), Object::Bool(l < x));
            op(f(l), Token::GreaterEq, Object::Int(r), Object::Bool(l >= x));
        }
    }
}

#[test]
fn float_keeps_its_receiver_when_the_result_compares_equal() {
    let negative_one = Object::Int(-1);
    let got = Object::Float(0.0).binary_op(Token::Mul, &negative_one).unwrap();
    assert_eq!(canon(&got), "f0");
}

#[test]
fn int_binary_op() {
    let i = Object::Int;
    for l in -2..=2i64 {
        for r in -2..=2i64 {
            op(i(l), Token::Add, i(r), i(l + r));
            op(i(l), Token::Sub, i(r), i(l - r));
            op(i(l), Token::Mul, i(r), i(l * r));
            if r != 0 {
                op(i(l), Token::Quo, i(r), i(l / r));
                op(i(l), Token::Rem, i(r), i(l % r));
            }
            op(i(l), Token::Less, i(r), Object::Bool(l < r));
            op(i(l), Token::Greater, i(r), Object::Bool(l > r));
            op(i(l), Token::LessEq, i(r), Object::Bool(l <= r));
            op(i(l), Token::GreaterEq, i(r), Object::Bool(l >= r));
        }
        for r in steps(0.4) {
            let x = l as f64;
            op(i(l), Token::Add, Object::Float(r), Object::Float(x + r));
            op(i(l), Token::Sub, Object::Float(r), Object::Float(x - r));
            op(i(l), Token::Mul, Object::Float(r), Object::Float(x * r));
            op(i(l), Token::Less, Object::Float(r), Object::Bool(x < r));
            op(i(l), Token::Greater, Object::Float(r), Object::Bool(x > r));
        }
    }
    let m = 0xffffffffi64;
    for (a, b) in [(0, 0), (1, 0), (0, 1), (1, 1), (0, m), (1, m), (m, 0), (m, m)] {
        op(i(a), Token::And, i(b), i(a & b));
        op(i(a), Token::Or, i(b), i(a | b));
        op(i(a), Token::Xor, i(b), i(a ^ b));
        op(i(a), Token::AndNot, i(b), i(a & !b));
    }
    for s in 0..64i64 {
        for v in [1i64, 0xff, -1] {
            op(i(v), Token::Shl, i(s), i(v << s));
            op(i(v), Token::Shr, i(s), i(v >> s));
        }
    }
}

#[test]
fn int_float_equals() {
    let (i, f) = (Object::Int, Object::Float);
    assert!(i(1).equals(&i(1)));
    assert!(!i(1).equals(&i(2)));
    assert!(i(1).equals(&f(1.0)));
    assert!(!i(1).equals(&f(1.5)));
    assert!(i(0).equals(&f(0.0)));
    assert!(!i(1).equals(&f(-1.0)));
    assert!(f(1.5).equals(&f(1.5)));
    assert!(!f(1.5).equals(&f(2.5)));
    assert!(f(1.0).equals(&i(1)));
    assert!(!f(1.5).equals(&i(1)));
    assert!(f(0.0).equals(&i(0)));
    // Neither (a < b) nor (a > b) implies a == b.
    for l in -2..=2i64 {
        for r in steps(0.5) {
            let lt = i(l).binary_op(Token::Less, &f(r)).unwrap().is_falsy();
            let gt = i(l).binary_op(Token::Greater, &f(r)).unwrap().is_falsy();
            if lt && gt {
                assert!(i(l).equals(&f(r)) && f(r).equals(&i(l)), "{} == {}", l, r);
            }
        }
    }
}

#[test]
fn map_index() {
    let m = empty_map();
    m.index_set(&new_str("1"), new_str("abcdef")).unwrap();
    assert_eq!(canon(&m.index_get(&new_str("1")).unwrap()), "\"abcdef\"");
}

#[test]
fn string_binary_op() {
    let (lstr, rstr) = ("abcde", "01234");
    for l in 0..lstr.len() {
        for r in 0..rstr.len() {
            let (ls, rs) = (&lstr[l..], &rstr[r..]);
            op(new_str(ls), Token::Add, new_str(rs), new_str(format!("{}{}", ls, rs)));
            let rc = rstr.as_bytes()[r] as char;
            op(new_str(ls), Token::Add, Object::Char(rc as i32), new_str(format!("{}{}", ls, rc)));
        }
    }
}
