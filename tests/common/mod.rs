//! Helpers for the tests carried over from the Go test suite (`vm_test.go`, `compiler_test.go`,
//! `parser_test.go` and `stdlib/*_test.go`). Expected values are the Go tests' own.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use tengo::compiler::{new_symbol_table, st_define, CompileFail, Compiler};
use tengo::objects::Object;
use tengo::parser::Parser;
use tengo::source::SourceFileSet;
use tengo::vm::{RunFail, Vm};

/// A value in the form the expected strings use: `f1.5` is a float, `c97` a char, `b6162` bytes,
/// `i[...]` and `i{...}` the immutable containers.
pub fn canon(o: &Object) -> String {
    let list = |v: Vec<Object>| v.iter().map(canon).collect::<Vec<_>>().join(", ");
    let map = |o: &std::collections::BTreeMap<Vec<u8>, Object>| {
        let quoted = |k: &[u8]| String::from_utf8_lossy(&tengo::gostr::quote(k)).into_owned();
        o.iter().map(|(k, v)| format!("{}: {}", quoted(k), canon(v))).collect::<Vec<_>>().join(", ")
    };
    match o {
        Object::Nil => "nil".into(),
        Object::Undefined => "undefined".into(),
        Object::Bool(b) => b.to_string(),
        Object::Int(i) => i.to_string(),
        Object::Float(_) => format!("f{}", String::from_utf8_lossy(&o.string().unwrap())),
        Object::Char(c) => format!("c{}", c),
        Object::String(s) => String::from_utf8_lossy(&tengo::gostr::quote(s)).into_owned(),
        Object::Bytes(b) => format!("b{}", hex(&b.to_vec())),
        Object::Array(a) => format!("[{}]", list(a.value.borrow().to_vec())),
        Object::ImmutableArray(a) => format!("i[{}]", list(a.value.to_vec())),
        Object::Map(m) => format!("{{{}}}", map(&m.data.borrow())),
        Object::ImmutableMap(m) => format!("i{{{}}}", map(&m.data.borrow())),
        Object::Error(e) => format!("error({})", canon(&e.value)),
        Object::Time(t) => {
            let utc = t.with_loc(tengo::gotime::Loc::Utc);
            format!("t{}", String::from_utf8_lossy(&utc.format(b"2006-01-02T15:04:05.999999999Z07:00")))
        }
        Object::CompiledFunction(f) => {
            format!("fn({},{},{},{})", hex(&f.instructions), f.num_locals, f.num_parameters, f.var_args)
        }
        Object::ObjectPtr(p) => canon(&p.borrow()),
        other => format!("?{}", other.type_name()),
    }
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

fn failed(e: CompileFail) -> String {
    match e {
        CompileFail::Error(m) => m,
        CompileFail::Panic(m) => format!("panic: {}", m),
    }
}

/// Compiles and runs `src` with a predefined global `out`, and returns `out`, as Go's
/// traceCompileRun does. File imports resolve against `dir`.
pub fn eval(src: &str, dir: Option<&PathBuf>) -> Result<Object, String> {
    // The Go tests' expected times were recorded under TZ=UTC.
    static UTC: std::sync::Once = std::sync::Once::new();
    UTC.call_once(|| std::env::set_var("TZ", "UTC"));
    let fset = SourceFileSet::new();
    let file = fset.borrow_mut().add_file("test", -1, src.len() as i64);
    let parsed = Parser::new(fset.clone(), file, src.as_bytes().to_vec()).parse_file();
    let ast = parsed.map_err(|e| e.error())?;
    let table = new_symbol_table();
    let out = st_define(&table, b"out").borrow().index as usize;
    let mut compiler = Compiler::new(fset.clone(), file, Some(table));
    if let Some(d) = dir {
        compiler.enable_file_import(true);
        compiler.set_import_dir(&d.to_string_lossy());
    }
    compiler.compile_file(&ast).map_err(failed)?;
    let mut bytecode = compiler.bytecode();
    bytecode.remove_duplicates().map_err(failed)?;
    let mut vm = Vm::new(bytecode, fset);
    match vm.run() {
        // Go seeds `out` with the zero value of the expected type; a script that never assigns
        // it is checked against undefined.
        Ok(()) => Ok(match vm.global(out) {
            Object::Nil => Object::Undefined,
            o => o,
        }),
        Err(RunFail::Error(m)) => Err(m),
        Err(RunFail::Panic(m)) => Err(format!("panic: {}", m)),
    }
}

fn scratch(mods: &[(&str, &str)]) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tengo-go-tests-{}-{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, body) in mods {
        std::fs::write(dir.join(format!("{}.tengo", name)), body).unwrap();
    }
    dir
}

fn first_pass(src: &str, mods: &[(&str, &str)], expected: &str) {
    let dir = scratch(mods);
    let got = eval(src, Some(&dir));
    let _ = std::fs::remove_dir_all(&dir);
    match got {
        Ok(o) => assert_eq!(canon(&o), expected, "script: {}", src),
        Err(e) => panic!("script: {}\nfailed: {}", src, e),
    }
}

/// Go's second pass: the same code as an imported module, whose export is frozen.
fn second_pass(src: &str, mods: &[(&str, &str)], expected: &str) {
    let code = format!("out := undefined; {}; export out", src);
    let mut all = mods.to_vec();
    all.push(("__code__", &code));
    let dir = scratch(&all);
    let got = eval("out = import(\"__code__\")", Some(&dir));
    let _ = std::fs::remove_dir_all(&dir);
    let frozen = if expected.starts_with('[') || expected.starts_with('{') {
        format!("i{}", expected)
    } else {
        expected.to_string()
    };
    match got {
        Ok(o) => assert_eq!(canon(&o), frozen, "module: {}", src),
        Err(e) => panic!("module: {}\nfailed: {}", src, e),
    }
}

/// expectRun: both passes.
#[track_caller]
pub fn run(src: &str, expected: &str) {
    first_pass(src, &[], expected);
    second_pass(src, &[], expected);
}

/// expectRun with Skip2ndPass.
#[track_caller]
pub fn run1(src: &str, expected: &str) {
    first_pass(src, &[], expected);
}

/// expectRun with source modules.
#[track_caller]
pub fn run_mods(src: &str, mods: &[(&str, &str)], expected: &str) {
    first_pass(src, mods, expected);
    second_pass(src, mods, expected);
}

/// expectRun with source modules and Skip2ndPass.
#[track_caller]
pub fn run1_mods(src: &str, mods: &[(&str, &str)], expected: &str) {
    first_pass(src, mods, expected);
}

/// expectError with source modules: the error text contains `expected`.
#[track_caller]
pub fn fails_mods(src: &str, mods: &[(&str, &str)], expected: &str) {
    let dir = scratch(mods);
    let got = eval(src, Some(&dir));
    let _ = std::fs::remove_dir_all(&dir);
    match got {
        Ok(o) => panic!("script: {}\nexpected an error, got {}", src, canon(&o)),
        Err(e) => {
            // Go's tests register modules by name; files on disk report their path.
            let prefix = format!("{}/", dir.display());
            let e = e.replace(&prefix, "").replace(".tengo", "");
            assert!(e.contains(expected), "script: {}\nwanted: {}\ngot: {}", src, expected, e);
        }
    }
}

/// expectError.
#[track_caller]
pub fn fails(src: &str, expected: &str) {
    fails_mods(src, &[], expected);
}

/// A Go `module(t, name).call(...).expect(...)`: one module function called from a script.
#[track_caller]
pub fn call(module: &str, expr: &str, expected: &str) {
    first_pass(&format!("out = import(\"{}\").{}", module, expr), &[], expected);
}
