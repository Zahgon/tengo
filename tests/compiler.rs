//! Carried over from compiler_test.go: each source compiles to the instructions and constants the
//! Go test expects (hex of the main function, then the constant pool).

mod common;

use common::{canon, hex};
use tengo::compiler::{CompileFail, Compiler};
use tengo::objects::Object;
use tengo::parser::Parser;
use tengo::source::SourceFileSet;

fn compile(src: &str) -> Result<(String, String), String> {
    let fset = SourceFileSet::new();
    let file = fset.borrow_mut().add_file("test", -1, src.len() as i64);
    let ast = Parser::new(fset.clone(), file, src.as_bytes().to_vec()).parse_file().map_err(|e| e.error())?;
    let mut compiler = Compiler::new(fset, file, None);
    compiler.compile_file(&ast).map_err(|e| match e {
        CompileFail::Error(m) | CompileFail::Panic(m) => m,
    })?;
    let mut bytecode = compiler.bytecode();
    bytecode.remove_duplicates().map_err(|e| match e {
        CompileFail::Error(m) | CompileFail::Panic(m) => m,
    })?;
    // Go's equalBytecode compares only the instructions of a compiled function.
    let constant = |o: &Object| match o {
        Object::CompiledFunction(f) => format!("fn({})", hex(&f.instructions)),
        other => canon(other),
    };
    let constants = bytecode.constants.iter().map(constant).collect::<Vec<_>>().join(", ");
    Ok((hex(&bytecode.main.instructions), constants))
}

#[track_caller]
fn compiles(src: &str, main: &str, constants: &str) {
    let (got_main, got_constants) = compile(src).unwrap();
    assert_eq!(got_main, main, "instructions of: {}", src);
    assert_eq!(got_constants, constants, "constants of: {}", src);
}

#[track_caller]
fn compile_fails(src: &str, expected: &str) {
    let e = compile(src).expect_err(src);
    assert!(e.contains(expected), "source: {}\nwanted: {}\ngot: {}", src, expected, e);
}

#[test]
fn compiler_compile() {
    compiles("1 + 2", "000000000001280b0229", "1, 2");
    compiles("1; 2", "000000020000010229", "1, 2");
    compiles("1 - 2", "000000000001280c0229", "1, 2");
    compiles("1 * 2", "000000000001280d0229", "1, 2");
    compiles("2 / 1", "000000000001280e0229", "2, 1");
    compiles("true", "030229", "");
    compiles("false", "040229", "");
    compiles("1 > 2", "00000000000128270229", "1, 2");
    compiles("1 < 2", "00000000000128260229", "1, 2");
    compiles("1 >= 2", "000000000001282c0229", "1, 2");
    compiles("1 <= 2", "000000000001282b0229", "1, 2");
    compiles("1 == 2", "000000000001050229", "1, 2");
    compiles("1 != 2", "000000000001060229", "1, 2");
    compiles("true == false", "0304050229", "");
    compiles("true != false", "0304060229", "");
    compiles("-1", "000000070229", "1");
    compiles("!true", "03080229", "");
    compiles("if true { 10 }; 3333", "03090000000a000000020000010229", "10, 3333");
    compiles("if (true) { 10 } else { 20 }; 3333;", "03090000000f000000020c00000013000001020000020229", "10, 20, 3333");
    compiles(r#""kami""#, "0000000229", r#""kami""#);
    compiles(r#""ka" + "mi""#, "000000000001280b0229", r#""ka", "mi""#);
    compiles("a := 1; b := 2; a += b", "000000170000000001170001160000160001280b17000029", "1, 2");
    compiles("a := 1; b := 2; a /= b", "000000170000000001170001160000160001280e17000029", "1, 2");
    compiles("[]", "0e00000229", "");
    compiles("[1, 2, 3]", "0000000000010000020e00030229", "1, 2, 3");
    compiles("[1 + 2, 3 - 4, 5 * 6]", "000000000001280b000002000003280c000004000005280d0e00030229", "1, 2, 3, 4, 5, 6");
    compiles("{}", "0f00000229", "");
    compiles("{a: 2, b: 4, c: 6}", "0000000000010000020000030000040000050f00060229", r#""a", 2, "b", 4, "c", 6"#);
    compiles("{a: 2 + 3, b: 5 * 6}", "000000000001000002280b000003000004000005280d0f00040229", r#""a", 2, 3, "b", 5, 6"#);
    compiles("[1, 2, 3][1 + 1]", "0000000000010000020e0003000000000000280b120229", "1, 2, 3");
    compiles("{a: 2}[2 - 1]", "0000000000010f0002000001000002280c120229", r#""a", 2, 1"#);
    compiles("[1, 2, 3][:]", "0000000000010000020e00030d0d130229", "1, 2, 3");
    compiles("[1, 2, 3][0 : 2]", "0000000000010000020e0003000003000001130229", "1, 2, 3, 0");
    compiles("[1, 2, 3][:2]", "0000000000010000020e00030d000001130229", "1, 2, 3");
    compiles("[1, 2, 3][0:]", "0000000000010000020e00030000030d130229", "1, 2, 3, 0");
    compiles("f1 := func(a) { return a }; f1([1, 2]...);", "0000001700001600000000010000020e00021401010229", "fn(19001501), 1, 2");
    compiles("func() { return 5 + 10 }", "0000020229", "5, 10, fn(000000000001280b1501)");
    compiles("func() { 5 + 10 }", "0000020229", "5, 10, fn(000000000001280b021500)");
    compiles("func() { 1; 2 }", "0000020229", "1, 2, fn(00000002000001021500)");
    compiles("func() { 1; return 2 }", "0000020229", "1, 2, fn(000000020000011501)");
    compiles("func() { if(true) { return 1 } else { return 2 } }", "0000020229", "1, 2, fn(03090000000b00000015010000011501)");
    compiles("func() { 1; if(true) { 2 } else { 3 }; 4 }", "0000040229", "1, 2, 3, 4, fn(00000002030900000013000001020c0000001700000202000003021500)");
    compiles("func() { }", "0000000229", "fn(1500)");
    compiles("func() { 24 }()", "0000011400000229", "24, fn(000000021500)");
    compiles("func() { return 24 }()", "0000011400000229", "24, fn(0000001501)");
    compiles("noArg := func() { 24 }; noArg();", "0000011700001600001400000229", "24, fn(000000021500)");
    compiles("noArg := func() { return 24 }; noArg();", "0000011700001600001400000229", "24, fn(0000001501)");
    compiles("n := 55; func() { n };", "0000001700000000010229", "55, fn(160000021500)");
    compiles("func() { n := 55; return n }", "0000010229", "55, fn(0000001b0019001501)");
    compiles("func() { a := 55; b := 77; return a + b }", "0000020229", "55, 77, fn(0000001b000000011b0119001901280b1501)");
    compiles("f1 := func(a) { return a }; f1(24);", "0000001700001600000000011401000229", "fn(19001501), 24");
    compiles("varTest := func(...a) { return a }; varTest(1,2,3);", "0000001700001600000000010000020000031403000229", "fn(19001501), 1, 2, 3");
    compiles("f1 := func(a, b, c) { a; b; return c; }; f1(24, 25, 26);", "0000001700001600000000010000020000031403000229", "fn(19000219010219021501), 24, 25, 26");
    compiles("func() { n := 55; n = 23; return n }", "0000020229", "55, 23, fn(0000001b000000011a0019001501)");
    compiles("len([]);", "22000e00001401000229", "");
    compiles("func() { return len([]) }", "0000000229", "fn(22000e00001401001501)");
    compiles("func(a) { func(b) { return a + b } }", "0000010229", "fn(1e001900280b1501), fn(200023000001021500)");
    compiles(r"func(a) {
return func(b) {
return func(c) {
return a + b + c
}
}
}", "0000020229", "fn(1e001e01280b1900280b1501), fn(1d002000230000021501), fn(2000230001011501)");
    compiles(r"g := 55;
func() {
a := 66;
return func() {
b := 77;
return func() {
c := 88;
return g + a + b + c;
}
}
}", "0000001700000000060229", "55, 66, 77, 88, fn(0000031b001600001e00280b1e01280b1900280b1501), fn(0000021b001d002000230004021501), fn(0000011b002000230005011501)");
    compiles("for i:=0; i<10; i++ {}", "00000017000016000000000128260900000023160000000002280b1700000c0000000629", "0, 10, 1");
    compiles("m := {}; for k, v in m {}", "0f00001700001600002417000116000125090000002916000126170002160001271700030c0000000d29", "");
    compiles("a := 0; a == 0 && a != 1 || a < 1", "000000170000160000000000050a00000019160000000001060b0000002616000000000128260229", "0, 1");
    compile_fails(r#"import("user1")"#, "module 'user1' not found");
    compile_fails(r#"
r["x"] = {
    @a:1,
    @b:1,
    @c:1,
    @d:1,
    @e:1,
    @f:1,
    @g:1,
    @h:1,
    @i:1,
    @j:1,
    @k:1
}
"#, r"Parse Error: illegal character U+0040 '@'
	at test:3:5 (and 10 more errors)");
    compile_fails(r#"import("")"#, "empty module name");
    compile_fails(r"
(func() {
	fn := fn()
})()
", "unresolved reference 'fn");
}

#[test]
fn compiler_error_report() {
    compile_fails(r#"import("user1")"#, r"Compile Error: module 'user1' not found
	at test:1:1");
    compile_fails("a = 1", r"Compile Error: unresolved reference 'a'
	at test:1:1");
    compile_fails("a := a", r"Compile Error: unresolved reference 'a'
	at test:1:6");
    compile_fails("a, b := 1, 2", r"Compile Error: tuple assignment not allowed
	at test:1:1");
    compile_fails("a.b := 1", "not allowed with selector");
    compile_fails("a:=1; a:=3", r"Compile Error: 'a' redeclared in this block
	at test:1:7");
    compile_fails("return 5", r"Compile Error: return not allowed outside function
	at test:1:1");
    compile_fails("func() { break }", r"Compile Error: break not allowed outside loop
	at test:1:10");
    compile_fails("func() { continue }", r"Compile Error: continue not allowed outside loop
	at test:1:10");
    compile_fails("func() { export 5 }", r"Compile Error: export not allowed inside function
	at test:1:10");
}

#[test]
fn compiler_dead_code() {
    compiles(r"func() {
a := 4
return a
b := 5 // dead code from here
c := a
return b
}", "0000020229", "4, 5, fn(0000001b0019001501)");
    compiles(r"func() {
if true {
return 5
a := 4  // dead code from here
b := a
return b
} else {
return 4
c := 5  // dead code from here
d := c
return d
}
}", "0000020229", "5, 4, fn(03090000000b00000015010000011501)");
    compiles(r"func() {
a := 1
for {
if a == 5 {
return 10
}
5 + 5
return 20
b := a
return b
}
}", "0000040229", "1, 5, 10, 20, fn(0000001b0019000000010509000000150000021501000001000001280b020000031501)");
    compiles(r"func() {
if true {
return
}
return
return 123
}", "0000010229", "123, fn(030900000008150015000000001501)");
}

#[test]
fn compiler_scopes() {
    compiles(r"if a := 1; a {
a = 2
b := a
} else {
a = 3
b := a
}", "000000170000160000090000001f0000011700001600001700010c0000002b00000217000016000017000229", "1, 2, 3");
    compiles(r"func() {
if a := 1; a {
a = 2
b := a
} else {
a = 3
b := a
}
}", "0000030229", "1, 2, 3, fn(0000001b001900090000001a0000011a0019001b010c000000230000021a0019001b011500)");
}
