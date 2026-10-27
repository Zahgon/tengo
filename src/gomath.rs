//! The math module (stdlib/math.go) over the pure-Go math ports in gomathfn.

use crate::gomathfn as m;
use crate::objects::*;
use crate::stdlib::{arg_float, arg_int, check_args};

/// FuncAFRF
fn afrf(name: &str, f: fn(f64) -> f64) -> Object {
    user_fn(name, move |args| {
        check_args(args, 1)?;
        Ok(Object::Float(f(arg_float(args, 0, "first")?)))
    })
}

/// FuncAFFRF
fn affrf(name: &str, f: fn(f64, f64) -> f64) -> Object {
    user_fn(name, move |args| {
        check_args(args, 2)?;
        let a = arg_float(args, 0, "first")?;
        let b = arg_float(args, 1, "second")?;
        Ok(Object::Float(f(a, b)))
    })
}

/// FuncAFRB
fn afrb(name: &str, f: fn(f64) -> bool) -> Object {
    user_fn(name, move |args| {
        check_args(args, 1)?;
        Ok(Object::Bool(f(arg_float(args, 0, "first")?)))
    })
}

/// FuncAIFRF
fn aifrf(name: &str, f: fn(i64, f64) -> f64) -> Object {
    user_fn(name, move |args| {
        check_args(args, 2)?;
        let a = arg_int(args, 0, "first")?;
        let b = arg_float(args, 1, "second")?;
        Ok(Object::Float(f(a, b)))
    })
}

/// FuncAIRF
fn airf(name: &str, f: fn(i64) -> f64) -> Object {
    user_fn(name, move |args| {
        check_args(args, 1)?;
        Ok(Object::Float(f(arg_int(args, 0, "first")?)))
    })
}

pub fn math_module() -> Vec<(&'static str, Object)> {
    vec![
        ("e", Object::Float(std::f64::consts::E)),
        ("pi", Object::Float(std::f64::consts::PI)),
        ("phi", Object::Float(1.618033988749895)),
        ("sqrt2", Object::Float(std::f64::consts::SQRT_2)),
        ("sqrtE", Object::Float(1.6487212707001282)),
        ("sqrtPi", Object::Float(1.772453850905516)),
        ("sqrtPhi", Object::Float(1.272019649514069)),
        ("ln2", Object::Float(std::f64::consts::LN_2)),
        ("log2E", Object::Float(std::f64::consts::LOG2_E)),
        ("ln10", Object::Float(std::f64::consts::LN_10)),
        ("log10E", Object::Float(std::f64::consts::LOG10_E)),
        ("maxFloat32", Object::Float(f32::MAX as f64)),
        ("smallestNonzeroFloat32", Object::Float(1.401298464324817070923729583289916131280e-45)),
        ("maxFloat64", Object::Float(f64::MAX)),
        ("smallestNonzeroFloat64", Object::Float(4.9406564584124654417656879286822137236505980e-324)),
        ("maxInt", Object::Int(i64::MAX)),
        ("minInt", Object::Int(i64::MIN)),
        ("maxInt8", Object::Int(i8::MAX as i64)),
        ("minInt8", Object::Int(i8::MIN as i64)),
        ("maxInt16", Object::Int(i16::MAX as i64)),
        ("minInt16", Object::Int(i16::MIN as i64)),
        ("maxInt32", Object::Int(i32::MAX as i64)),
        ("minInt32", Object::Int(i32::MIN as i64)),
        ("maxInt64", Object::Int(i64::MAX)),
        ("minInt64", Object::Int(i64::MIN)),
        ("abs", afrf("abs", m::abs)),
        ("acos", afrf("acos", m::acos)),
        ("acosh", afrf("acosh", m::acosh)),
        ("asin", afrf("asin", m::asin)),
        ("asinh", afrf("asinh", m::asinh)),
        ("atan", afrf("atan", m::atan)),
        ("atan2", affrf("atan2", m::atan2)),
        ("atanh", afrf("atanh", m::atanh)),
        ("cbrt", afrf("cbrt", m::cbrt)),
        ("ceil", afrf("ceil", m::ceil)),
        ("copysign", affrf("copysign", m::copysign)),
        ("cos", afrf("cos", m::cos)),
        ("cosh", afrf("cosh", m::cosh)),
        ("dim", affrf("dim", m::dim)),
        ("erf", afrf("erf", m::erf)),
        ("erfc", afrf("erfc", m::erfc)),
        ("exp", afrf("exp", m::exp)),
        ("exp2", afrf("exp2", m::exp2)),
        ("expm1", afrf("expm1", m::expm1)),
        ("floor", afrf("floor", m::floor)),
        ("gamma", afrf("gamma", m::gamma)),
        ("hypot", affrf("hypot", m::hypot)),
        (
            "ilogb",
            user_fn("ilogb", |args| {
                check_args(args, 1)?;
                Ok(Object::Int(m::ilogb(arg_float(args, 0, "first")?)))
            }),
        ),
        ("inf", airf("inf", m::inf)),
        (
            "is_inf",
            user_fn("is_inf", |args| {
                check_args(args, 2)?;
                let f = arg_float(args, 0, "first")?;
                let i = arg_int(args, 1, "second")?;
                Ok(Object::Bool(m::is_inf(f, i)))
            }),
        ),
        ("is_nan", afrb("is_nan", m::is_nan)),
        ("j0", afrf("j0", m::j0)),
        ("j1", afrf("j1", m::j1)),
        ("jn", aifrf("jn", m::jn)),
        (
            "ldexp",
            user_fn("ldexp", |args| {
                check_args(args, 2)?;
                let f = arg_float(args, 0, "first")?;
                let i = arg_int(args, 1, "second")?;
                Ok(Object::Float(m::ldexp(f, i)))
            }),
        ),
        ("log", afrf("log", m::log)),
        ("log10", afrf("log10", m::log10)),
        ("log1p", afrf("log1p", m::log1p)),
        ("log2", afrf("log2", m::log2)),
        ("logb", afrf("logb", m::logb)),
        ("max", affrf("max", m::max)),
        ("min", affrf("min", m::min)),
        ("mod", affrf("mod", m::fmod)),
        (
            "nan",
            user_fn("nan", |args| {
                check_args(args, 0)?;
                Ok(Object::Float(m::nan()))
            }),
        ),
        ("nextafter", affrf("nextafter", m::nextafter)),
        ("pow", affrf("pow", m::pow)),
        ("pow10", airf("pow10", m::pow10)),
        ("remainder", affrf("remainder", m::remainder)),
        ("signbit", afrb("signbit", m::signbit)),
        ("sin", afrf("sin", m::sin)),
        ("sinh", afrf("sinh", m::sinh)),
        ("sqrt", afrf("sqrt", m::sqrt)),
        ("tan", afrf("tan", m::tan)),
        ("tanh", afrf("tanh", m::tanh)),
        ("trunc", afrf("trunc", m::trunc)),
        ("y0", afrf("y0", m::y0)),
        ("y1", afrf("y1", m::y1)),
        ("yn", aifrf("yn", m::yn)),
    ]
}
