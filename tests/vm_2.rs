//! Carried over from vm_test.go: every script runs in the VM and `out` is compared with the Go
//! test's expected value. Tests that need host objects (Go callbacks, custom types, allocation
//! limits) are listed in tests/PORTING.md.

mod common;

use common::*;

#[test]
fn user_modules() {
    run_mods(r#"out = import("mod1")"#, &[("mod1", "fn := func() { return 5.0 }; a := 2")], "undefined");
    run_mods(r#"out = import("mod1")"#, &[("mod1", "export 5")], "5");
    run_mods(r#"out = import("mod1")"#, &[("mod1", r#"export "foo""#)], r#""foo""#);
    run_mods(r#"out = import("mod1")"#, &[("mod1", "export [1, 2, 3]")], "i[1, 2, 3]");
    run_mods(r#"out = import("mod1")"#, &[("mod1", "export {a: 1, b: 2}")], r#"i{"a": 1, "b": 2}"#);
    fails_mods(r#"m1 := import("mod1"); m1.a = 5"#, &[("mod1", "export {a: 1, b: 2}")], "not index-assignable");
    fails_mods(r#"m1 := import("mod1"); m1[1] = 5"#, &[("mod1", "export [1, 2, 3]")], "not index-assignable");
    run_mods(r#"out = import("mod1")"#, &[("mod1", "a := 10; export a; a = 20")], "10");
    run_mods(r#"out = import("mod1")"#, &[("mod1", "a := 10; export a; a = 20; export a")], "10");
    run_mods(r#"out = import("mod1")()"#, &[("mod1", "export func() { return 5.0 }")], "f5");
    run_mods(r#"out = import("mod1")()"#, &[("mod1", "a := 1.5; export func() { return a + 5.0 }")], "f6.5");
    run_mods(r#"out = import("mod1")()"#, &[("mod1", "export func() { a := 1.5; return a + 5.0 }")], "f6.5");
    run_mods(r#"out = import("mod1")()"#, &[("mod1", "export func() { a := 1.5; return func() { return a + 5.0 }() }")], "f6.5");
    run_mods(r#"out = import("mod1")"#, &[("mod1", r"a := func(x) {
return x == 0 ? 0 : x + a(x-1)
}
export a(5)")], "15");
    run_mods(r#"out = import("mod1")"#, &[("mod1", r"export func() {
a := func(x) {
return x == 0 ? 0 : x + a(x-1)
}
return a(5)
}()")], "15");
    run_mods(r#"out = import("mod1")()"#, &[("mod1", r#"export import("mod2")"#), ("mod2", "export func() { return 5.0 }")], "f5");
    run_mods(r#"import("mod1"); out = import("mod2")()"#, &[("mod1", r#"export import("mod2")"#), ("mod2", "export func() { return 5.0 }")], "f5");
    run_mods(r#"import("mod1"); out = import("mod2")()"#, &[("mod1", r#"export import("mod2")"#), ("mod2", r#"export import("mod3")"#), ("mod3", "export func() { return 5.0 }")], "f5");
    fails_mods(r#"import("mod1")"#, &[("mod1", r#"import("mod2")"#), ("mod2", r#"import("mod1")"#)], r"Compile Error: cyclic module import: mod1
	at mod2:1:1");
    fails_mods(r#"import("mod1")"#, &[("mod1", r#"import("mod2")"#), ("mod2", r#"import("mod3")"#), ("mod3", r#"import("mod1")"#)], r"Compile Error: cyclic module import: mod1
	at mod3:1:1");
    fails_mods(r#"import("mod1")"#, &[("mod1", r#"import("mod2")"#), ("mod2", r#"import("mod3")"#), ("mod3", r#"import("mod2")"#)], r"Compile Error: cyclic module import: mod2
	at mod3:1:1");
    fails_mods(r#"import("mod0")"#, &[("mod1", "a := 5")], "module 'mod0' not found");
    fails_mods(r#"import("mod1")"#, &[("mod1", r#"import("mod2")"#)], "module 'mod2' not found");
    run_mods(r#"m1 := import("mod1"); m1.a.b = 5; out = m1.a.b"#, &[("mod1", "export {a: {b: 3}}")], "5");
    run_mods(r#"out = import("mod1")"#, &[("mod1", "export func() { return type_name(0) }()")], r#""int""#);
    run1("a := 5; export func() { a = 10 }(); out = a", "5");
    fails_mods(r#"import("mod1")"#, &[("mod1", "func() { export 5 }()")], r"Compile Error: export not allowed inside function
	at mod1:1:10");
    fails_mods(r#"import("mod1")"#, &[("mod1", "func() { func() { export 5 }() }()")], r"Compile Error: export not allowed inside function
	at mod1:1:19");
    fails_mods(r#"a := 5; import("mod1")"#, &[("mod1", "export a")], r"Compile Error: unresolved reference 'a'
	at mod1:1:8");
    fails_mods(r#"
a := 1;
b := import("mod1");
b(a)"#, &[("mod1", r"
export func(a) {
   a()
}
")], r"Runtime Error: not callable: int
	at mod1:3:4
	at test:4:1");
    run_mods(r#"out = import("mod0")"#, &[("mod0", "")], "undefined");
    run_mods(r#"out = import("mod0")"#, &[("mod0", "if 1 { export true }")], "true");
    run_mods(r#"out = import("mod0")"#, &[("mod0", "if 0 { export true }")], "undefined");
    run_mods(r#"out = import("mod0")"#, &[("mod0", "if 1 { } else { export true }")], "undefined");
    run_mods(r#"out = import("mod0")"#, &[("mod0", "for v:=0;;v++ { if v == 3 { export true } }")], "true");
    run_mods(r#"out = import("mod0")"#, &[("mod0", "for v:=0;;v++ { if v == 3 { break } }")], "undefined");
    run_mods(r#"m1 := import("mod")
m2 := import("mod")
out = m1.x"#, &[("mod", r"f1 := func(a, b) {
c := a + b + 1
return a + b + 1
}
export { x: 1 }")], "1");
}

#[test]
fn bang_operator() {
    run("out = !true", "false");
    run("out = !false", "true");
    run("out = !0", "true");
    run("out = !5", "false");
    run("out = !!true", "true");
    run("out = !!false", "false");
    run("out = !!5", "true");
}

#[test]
fn objects_limit() {
    run1("5", "undefined");
    run1("5 + 5", "undefined");
    run1("a := [1, 2, 3]", "undefined");
    run1("a := 1; b := 2; c := 3; d := [a, b, c]", "undefined");
    run1("a := {foo: 1, bar: 2}", "undefined");
    run1("a := 1; b := 2; c := {foo: a, bar: b}", "undefined");
    run1(r"f := func() {
return 5 + 5
}
a := f() + 5", "undefined");
    run1(r"f := func() {
return 5 + 5
}
a := f()", "undefined");
    run1(r"a := []
f := func() {
a = append(a, 5)
}
f()
f()
f()", "undefined");
}

#[test]
fn return_stmt() {
    run("out = func() { return 10; }()", "10");
    run("out = func() { return 10; return 9; }()", "10");
    run("out = func() { return 2 * 5; return 9 }()", "10");
    run("out = func() { 9; return 2 * 5; return 9 }()", "10");
    run(r"out = func() {
if (10 > 1) {
if (10 > 1) {
return 10;
}
return 1;
}
}()", "10");
    run("f1 := func() { return 2 * 5; }; out = f1()", "10");
}

#[test]
fn vm_scopes() {
    run(r"c := 5
if a := 3; a {
c := 6
} else {
c := 7
}
out = c", "5");
    run(r"func() {
c := 5
if a := 3; a {
c := 6
} else {
c := 7
}
out = c
}()", "5");
    run(r"c := 5
if a := 3; a {
b := 8
c = b
} else {
b := 9
c = b
}
out = c", "8");
    run(r"a := 4
b := 5
for i:=0;i<3;i++ {
b := 6
for j:=0;j<2;j++ {
b := 7
a = i*j
}
}
out = a", "2");
    run(r"if a := 5; a {
a := 6
out = a
}", "6");
    run(r"a := 4
if a := 5; a {
a := 6
out = a
}", "6");
    run(r"a := 4
if a := 0; a {
a := 6
out = a
} else {
a := 7
out = a
}", "7");
    run(r"a := 4
if a := 0; a {
out = a
} else {
out = a
}", "0");
    run(r"a := 5
func() {
a := 6
a = 7
}()
out = a", "5");
    run(r"a := 5
func() {
if a := 7; true {
a = 8
}
}()
out = a", "5");
}

#[test]
fn selector() {
    run(r#"a := {k1: 5, k2: "foo"}; out = a.k1"#, "5");
    run(r#"a := {k1: 5, k2: "foo"}; out = a.k2"#, r#""foo""#);
    run(r#"a := {k1: 5, k2: "foo"}; out = a.k3"#, "undefined");
    run(r#"a := {
b: {
c: 4,
a: false
},
c: "foo bar"
}
out = a.b.c"#, "4");
    run(r#"a := {
b: {
c: 4,
a: false
},
c: "foo bar"
}
b := a.x.c"#, "undefined");
    run(r#"a := {
b: {
c: 4,
a: false
},
c: "foo bar"
}
b := a.x.y"#, "undefined");
    run(r#"a := {b: 1, c: "foo"}; a.b = 2; out = a.b"#, "2");
    run(r#"a := {b: 1, c: "foo"}; a.c = 2; out = a.c"#, "2");
    run("a := {b: {c: 1}}; a.b.c = 2; out = a.b.c", "2");
    run("a := {b: 1}; a.c = 2; out = a", r#"{"b": 1, "c": 2}"#);
    run("a := {b: {c: 1}}; a.b.d = 2; out = a", r#"{"b": {"c": 1, "d": 2}}"#);
    run(r#"func() { a := {b: 1, c: "foo"}; a.b = 2; out = a.b }()"#, "2");
    run(r#"func() { a := {b: 1, c: "foo"}; a.c = 2; out = a.c }()"#, "2");
    run("func() { a := {b: {c: 1}}; a.b.c = 2; out = a.b.c }()", "2");
    run("func() { a := {b: 1}; a.c = 2; out = a }()", r#"{"b": 1, "c": 2}"#);
    run("func() { a := {b: {c: 1}}; a.b.d = 2; out = a }()", r#"{"b": {"c": 1, "d": 2}}"#);
    run(r#"func() { a := {b: 1, c: "foo"}; func() { a.b = 2 }(); out = a.b }()"#, "2");
    run(r#"func() { a := {b: 1, c: "foo"}; func() { a.c = 2 }(); out = a.c }()"#, "2");
    run("func() { a := {b: {c: 1}}; func() { a.b.c = 2 }(); out = a.b.c }()", "2");
    run("func() { a := {b: 1}; func() { a.c = 2 }(); out = a }()", r#"{"b": 1, "c": 2}"#);
    run("func() { a := {b: {c: 1}}; func() { a.b.d = 2 }(); out = a }()", r#"{"b": {"c": 1, "d": 2}}"#);
    run(r#"a := {
b: [1, 2, 3],
c: {
d: 8,
e: "foo",
f: [9, 8]
}
}
out = [a.b[2], a.c.d, a.c.e, a.c.f[1]]"#, r#"[3, 8, "foo", 8]"#);
    run(r"func() {
a := [1, 2, 3]
b := 9
a[1] = b
b = 7     // make sure a[1] has a COPY of value of 'b'
out = a[1]
}()", "9");
    fails("a := {b: {c: 1}}; a.d.c = 2", "not index-assignable");
    fails("a := [1, 2, 3]; a.b = 2", "invalid index type");
    fails(r#"a := "foo"; a.b = 2"#, "not index-assignable");
    fails("func() { a := {b: {c: 1}}; a.d.c = 2 }()", "not index-assignable");
    fails("func() { a := [1, 2, 3]; a.b = 2 }()", "invalid index type");
    fails(r#"func() { a := "foo"; a.b = 2 }()"#, "not index-assignable");
}

#[test]
fn source_modules() {
    run(r#"enum := import("enum"); out = enum.key(0, 20)"#, "0");
    run(r#"enum := import("enum"); out = enum.key(10, 20)"#, "10");
    run(r#"enum := import("enum"); out = enum.value(0, 0)"#, "0");
    run(r#"enum := import("enum"); out = enum.value(10, 20)"#, "20");
    run(r#"enum := import("enum"); out = enum.all([], enum.value)"#, "true");
    run(r#"enum := import("enum"); out = enum.all([1], enum.value)"#, "true");
    run(r#"enum := import("enum"); out = enum.all([true, 1], enum.value)"#, "true");
    run(r#"enum := import("enum"); out = enum.all([true, 0], enum.value)"#, "false");
    run(r#"enum := import("enum"); out = enum.all([true, 0, 1], enum.value)"#, "false");
    run(r#"enum := import("enum"); out = enum.all(immutable([true, 0, 1]), enum.value)"#, "false");
    run(r#"enum := import("enum"); out = enum.all({}, enum.value)"#, "true");
    run(r#"enum := import("enum"); out = enum.all({a:1}, enum.value)"#, "true");
    run(r#"enum := import("enum"); out = enum.all({a:true, b:1}, enum.value)"#, "true");
    run(r#"enum := import("enum"); out = enum.all(immutable({a:true, b:1}), enum.value)"#, "true");
    run(r#"enum := import("enum"); out = enum.all({a:true, b:0}, enum.value)"#, "false");
    run(r#"enum := import("enum"); out = enum.all({a:true, b:0, c:1}, enum.value)"#, "false");
    run(r#"enum := import("enum"); out = enum.all(0, enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.all("123", enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.any([], enum.value)"#, "false");
    run(r#"enum := import("enum"); out = enum.any([1], enum.value)"#, "true");
    run(r#"enum := import("enum"); out = enum.any([true, 1], enum.value)"#, "true");
    run(r#"enum := import("enum"); out = enum.any([true, 0], enum.value)"#, "true");
    run(r#"enum := import("enum"); out = enum.any([true, 0, 1], enum.value)"#, "true");
    run(r#"enum := import("enum"); out = enum.any(immutable([true, 0, 1]), enum.value)"#, "true");
    run(r#"enum := import("enum"); out = enum.any([false], enum.value)"#, "false");
    run(r#"enum := import("enum"); out = enum.any([false, 0], enum.value)"#, "false");
    run(r#"enum := import("enum"); out = enum.any({}, enum.value)"#, "false");
    run(r#"enum := import("enum"); out = enum.any({a:1}, enum.value)"#, "true");
    run(r#"enum := import("enum"); out = enum.any({a:true, b:1}, enum.value)"#, "true");
    run(r#"enum := import("enum"); out = enum.any({a:true, b:0}, enum.value)"#, "true");
    run(r#"enum := import("enum"); out = enum.any({a:true, b:0, c:1}, enum.value)"#, "true");
    run(r#"enum := import("enum"); out = enum.any(immutable({a:true, b:0, c:1}), enum.value)"#, "true");
    run(r#"enum := import("enum"); out = enum.any({a:false}, enum.value)"#, "false");
    run(r#"enum := import("enum"); out = enum.any({a:false, b:0}, enum.value)"#, "false");
    run(r#"enum := import("enum"); out = enum.any(0, enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.any("123", enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.chunk([], 1)"#, "[]");
    run(r#"enum := import("enum"); out = enum.chunk([1], 1)"#, "[[1]]");
    run(r#"enum := import("enum"); out = enum.chunk([1,2,3], 1)"#, "[[1], [2], [3]]");
    run(r#"enum := import("enum"); out = enum.chunk([1,2,3], 2)"#, "[[1, 2], [3]]");
    run(r#"enum := import("enum"); out = enum.chunk([1,2,3], 3)"#, "[[1, 2, 3]]");
    run(r#"enum := import("enum"); out = enum.chunk([1,2,3], 4)"#, "[[1, 2, 3]]");
    run(r#"enum := import("enum"); out = enum.chunk([1,2,3,4], 3)"#, "[[1, 2, 3], [4]]");
    run(r#"enum := import("enum"); out = enum.chunk([], 0)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.chunk([1], 0)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.chunk([1,2,3], 0)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.chunk({a:1,b:2,c:3}, 1)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.chunk(0, 1)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.chunk("123", 1)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.at([], 0)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.at([], 1)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.at([], -1)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.at(["one"], 0)"#, r#""one""#);
    run(r#"enum := import("enum"); out = enum.at(["one"], 1)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.at(["one"], -1)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.at(["one","two","three"], 0)"#, r#""one""#);
    run(r#"enum := import("enum"); out = enum.at(["one","two","three"], 1)"#, r#""two""#);
    run(r#"enum := import("enum"); out = enum.at(["one","two","three"], 2)"#, r#""three""#);
    run(r#"enum := import("enum"); out = enum.at(["one","two","three"], -1)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.at(["one","two","three"], 3)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.at(["one","two","three"], "1")"#, "undefined");
    run(r#"enum := import("enum"); out = enum.at({}, "a")"#, "undefined");
    run(r#"enum := import("enum"); out = enum.at({a:"one"}, "a")"#, r#""one""#);
    run(r#"enum := import("enum"); out = enum.at({a:"one"}, "b")"#, "undefined");
    run(r#"enum := import("enum"); out = enum.at({a:"one",b:"two",c:"three"}, "a")"#, r#""one""#);
    run(r#"enum := import("enum"); out = enum.at({a:"one",b:"two",c:"three"}, "b")"#, r#""two""#);
    run(r#"enum := import("enum"); out = enum.at({a:"one",b:"two",c:"three"}, "c")"#, r#""three""#);
    run(r#"enum := import("enum"); out = enum.at({a:"one",b:"two",c:"three"}, "d")"#, "undefined");
    run(r#"enum := import("enum"); out = enum.at({a:"one",b:"two",c:"three"}, 'a')"#, "undefined");
    run(r#"enum := import("enum"); out = enum.at(0, 1)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.at("abc", 1)"#, "undefined");
    run(r#"enum := import("enum"); out=0; enum.each([],func(k,v){out+=v})"#, "0");
    run(r#"enum := import("enum"); out=0; enum.each([1,2,3],func(k,v){out+=v})"#, "6");
    run(r#"enum := import("enum"); out=0; enum.each([1,2,3],func(k,v){out+=k})"#, "3");
    run(r#"enum := import("enum"); out=0; enum.each({a:1,b:2,c:3},func(k,v){out+=v})"#, "6");
    run(r#"enum := import("enum"); out=""; enum.each({a:1,b:2,c:3},func(k,v){out+=k}); out=len(out)"#, "3");
    run(r#"enum := import("enum"); out=0; enum.each(5,func(k,v){out+=v})"#, "0");
    run(r#"enum := import("enum"); out=0; enum.each("123",func(k,v){out+=v})"#, "0");
    run(r#"enum := import("enum"); out = enum.filter([], enum.value)"#, "[]");
    run(r#"enum := import("enum"); out = enum.filter([false,1,2], enum.value)"#, "[1, 2]");
    run(r#"enum := import("enum"); out = enum.filter([false,1,0,2], enum.value)"#, "[1, 2]");
    run(r#"enum := import("enum"); out = enum.filter({}, enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.filter(0, enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.filter("123", enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.find([], enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.find([0], enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.find([1], enum.value)"#, "1");
    run(r#"enum := import("enum"); out = enum.find([false,0,undefined,1], enum.value)"#, "1");
    run(r#"enum := import("enum"); out = enum.find([1,2,3], enum.value)"#, "1");
    run(r#"enum := import("enum"); out = enum.find({}, enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.find({a:0}, enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.find({a:1}, enum.value)"#, "1");
    run(r#"enum := import("enum"); out = enum.find({a:false,b:0,c:undefined,d:1}, enum.value)"#, "1");
    run(r#"enum := import("enum"); out = enum.find(0, enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.find("123", enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.find_key([], enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.find_key([0], enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.find_key([1], enum.value)"#, "0");
    run(r#"enum := import("enum"); out = enum.find_key([false,0,undefined,1], enum.value)"#, "3");
    run(r#"enum := import("enum"); out = enum.find_key([1,2,3], enum.value)"#, "0");
    run(r#"enum := import("enum"); out = enum.find_key({}, enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.find_key({a:0}, enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.find_key({a:1}, enum.value)"#, r#""a""#);
    run(r#"enum := import("enum"); out = enum.find_key({a:false,b:0,c:undefined,d:1}, enum.value)"#, r#""d""#);
    run(r#"enum := import("enum"); out = enum.find_key(0, enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.find_key("123", enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.map([], enum.value)"#, "[]");
    run(r#"enum := import("enum"); out = enum.map([1,2,3], enum.value)"#, "[1, 2, 3]");
    run(r#"enum := import("enum"); out = enum.map([1,2,3], enum.key)"#, "[0, 1, 2]");
    run(r#"enum := import("enum"); out = enum.map([1,2,3], func(k,v) { return v*2 })"#, "[2, 4, 6]");
    run(r#"enum := import("enum"); out = enum.map({}, enum.value)"#, "[]");
    run(r#"enum := import("enum"); out = enum.map({a:1}, func(k,v) { return v*2 })"#, "[2]");
    run(r#"enum := import("enum"); out = enum.map(0, enum.value)"#, "undefined");
    run(r#"enum := import("enum"); out = enum.map("123", enum.value)"#, "undefined");
}

#[test]
fn src_mod_enum() {
    run(r#"x := import("enum")
out = x.all([1, 2, 3], func(_, v) { return v >= 1 })"#, "true");
    run(r#"x := import("enum")
out = x.all([1, 2, 3], func(_, v) { return v >= 2 })"#, "false");
    run(r#"x := import("enum")
out = x.any([1, 2, 3], func(_, v) { return v >= 1 })"#, "true");
    run(r#"x := import("enum")
out = x.any([1, 2, 3], func(_, v) { return v >= 2 })"#, "true");
    run(r#"x := import("enum")
out = x.chunk([1, 2, 3], 1)"#, "[[1], [2], [3]]");
    run(r#"x := import("enum")
out = x.chunk([1, 2, 3], 2)"#, "[[1, 2], [3]]");
    run(r#"x := import("enum")
out = x.chunk([1, 2, 3], 3)"#, "[[1, 2, 3]]");
    run(r#"x := import("enum")
out = x.chunk([1, 2, 3], 4)"#, "[[1, 2, 3]]");
    run(r#"x := import("enum")
out = x.chunk([1, 2, 3, 4, 5, 6], 2)"#, "[[1, 2], [3, 4], [5, 6]]");
    run(r#"x := import("enum")
out = x.at([1, 2, 3], 0)"#, "1");
}

#[test]
fn vm_stack_overflow() {
    fails("f := func() { return f() + 1 }; f()", "stack overflow");
}

#[test]
fn string() {
    run(r#"out = "Hello World!""#, r#""Hello World!""#);
    run(r#"out = "Hello" + " " + "World!""#, r#""Hello World!""#);
    run(r#"out = "Hello" == "Hello""#, "true");
    run(r#"out = "Hello" == "World""#, "false");
    run(r#"out = "Hello" != "Hello""#, "false");
    run(r#"out = "Hello" != "World""#, "true");
    run(r#"out = "Hello" > "World""#, "false");
    run(r#"out = "World" < "Hello""#, "false");
    run(r#"out = "Hello" < "World""#, "true");
    run(r#"out = "World" > "Hello""#, "true");
    run(r#"out = "Hello" >= "World""#, "false");
    run(r#"out = "Hello" <= "World""#, "true");
    run(r#"out = "Hello" >= "Hello""#, "true");
    run(r#"out = "World" <= "World""#, "true");
    run(r#"out = "abcdef"[0]"#, "c97");
    run(r#"out = "abcdef"[0 + 0]"#, "c97");
    run(r#"out = "abcdef"[1 + 0 - 1]"#, "c97");
    run(r#"idx := 0; out = "abcdef"[idx]"#, "c97");
    run(r#"out = "abcdef"[1]"#, "c98");
    run(r#"out = "abcdef"[0 + 1]"#, "c98");
    run(r#"out = "abcdef"[1 + 1 - 1]"#, "c98");
    run(r#"idx := 1; out = "abcdef"[idx]"#, "c98");
    run(r#"out = "abcdef"[2]"#, "c99");
    run(r#"out = "abcdef"[0 + 2]"#, "c99");
    run(r#"out = "abcdef"[1 + 2 - 1]"#, "c99");
    run(r#"idx := 2; out = "abcdef"[idx]"#, "c99");
    run(r#"out = "abcdef"[3]"#, "c100");
    run(r#"out = "abcdef"[0 + 3]"#, "c100");
    run(r#"out = "abcdef"[1 + 3 - 1]"#, "c100");
    run(r#"idx := 3; out = "abcdef"[idx]"#, "c100");
    run(r#"out = "abcdef"[4]"#, "c101");
    run(r#"out = "abcdef"[0 + 4]"#, "c101");
    run(r#"out = "abcdef"[1 + 4 - 1]"#, "c101");
    run(r#"idx := 4; out = "abcdef"[idx]"#, "c101");
    run(r#"out = "abcdef"[5]"#, "c102");
    run(r#"out = "abcdef"[0 + 5]"#, "c102");
    run(r#"out = "abcdef"[1 + 5 - 1]"#, "c102");
    run(r#"idx := 5; out = "abcdef"[idx]"#, "c102");
    run(r#""abcdef"[-1]"#, "undefined");
    run(r#""abcdef"[6]"#, "undefined");
    run(r#"out = "abcdef"[0:0]"#, r#""""#);
    run(r#"out = "abcdef"[0 + 0 : 0 + 0]"#, r#""""#);
    run(r#"out = "abcdef"[1 + 0 - 1 : 1 + 0 - 1]"#, r#""""#);
    run(r#"out = "abcdef"[:0]"#, r#""""#);
    run(r#"out = "abcdef"[0:]"#, r#""abcdef""#);
    run(r#"out = "abcdef"[0:1]"#, r#""a""#);
    run(r#"out = "abcdef"[0 + 0 : 0 + 1]"#, r#""a""#);
    run(r#"out = "abcdef"[1 + 0 - 1 : 1 + 1 - 1]"#, r#""a""#);
    run(r#"out = "abcdef"[:1]"#, r#""a""#);
    run(r#"out = "abcdef"[0:2]"#, r#""ab""#);
    run(r#"out = "abcdef"[0 + 0 : 0 + 2]"#, r#""ab""#);
    run(r#"out = "abcdef"[1 + 0 - 1 : 1 + 2 - 1]"#, r#""ab""#);
    run(r#"out = "abcdef"[:2]"#, r#""ab""#);
    run(r#"out = "abcdef"[0:3]"#, r#""abc""#);
    run(r#"out = "abcdef"[0 + 0 : 0 + 3]"#, r#""abc""#);
    run(r#"out = "abcdef"[1 + 0 - 1 : 1 + 3 - 1]"#, r#""abc""#);
    run(r#"out = "abcdef"[:3]"#, r#""abc""#);
    run(r#"out = "abcdef"[0:4]"#, r#""abcd""#);
    run(r#"out = "abcdef"[0 + 0 : 0 + 4]"#, r#""abcd""#);
    run(r#"out = "abcdef"[1 + 0 - 1 : 1 + 4 - 1]"#, r#""abcd""#);
    run(r#"out = "abcdef"[:4]"#, r#""abcd""#);
    run(r#"out = "abcdef"[0:5]"#, r#""abcde""#);
    run(r#"out = "abcdef"[0 + 0 : 0 + 5]"#, r#""abcde""#);
    run(r#"out = "abcdef"[1 + 0 - 1 : 1 + 5 - 1]"#, r#""abcde""#);
    run(r#"out = "abcdef"[:5]"#, r#""abcde""#);
    run(r#"out = "abcdef"[0:6]"#, r#""abcdef""#);
    run(r#"out = "abcdef"[0 + 0 : 0 + 6]"#, r#""abcdef""#);
    run(r#"out = "abcdef"[1 + 0 - 1 : 1 + 6 - 1]"#, r#""abcdef""#);
    run(r#"out = "abcdef"[:6]"#, r#""abcdef""#);
    run(r#"out = "abcdef"[1:1]"#, r#""""#);
    run(r#"out = "abcdef"[0 + 1 : 0 + 1]"#, r#""""#);
    run(r#"out = "abcdef"[1 + 1 - 1 : 1 + 1 - 1]"#, r#""""#);
    run(r#"out = "abcdef"[1:]"#, r#""bcdef""#);
    run(r#"out = "abcdef"[1:2]"#, r#""b""#);
    run(r#"out = "abcdef"[0 + 1 : 0 + 2]"#, r#""b""#);
    run(r#"out = "abcdef"[1 + 1 - 1 : 1 + 2 - 1]"#, r#""b""#);
    run(r#"out = "abcdef"[1:3]"#, r#""bc""#);
    run(r#"out = "abcdef"[0 + 1 : 0 + 3]"#, r#""bc""#);
    run(r#"out = "abcdef"[1 + 1 - 1 : 1 + 3 - 1]"#, r#""bc""#);
    run(r#"out = "abcdef"[1:4]"#, r#""bcd""#);
    run(r#"out = "abcdef"[0 + 1 : 0 + 4]"#, r#""bcd""#);
    run(r#"out = "abcdef"[1 + 1 - 1 : 1 + 4 - 1]"#, r#""bcd""#);
    run(r#"out = "abcdef"[1:5]"#, r#""bcde""#);
    run(r#"out = "abcdef"[0 + 1 : 0 + 5]"#, r#""bcde""#);
    run(r#"out = "abcdef"[1 + 1 - 1 : 1 + 5 - 1]"#, r#""bcde""#);
    run(r#"out = "abcdef"[1:6]"#, r#""bcdef""#);
    run(r#"out = "abcdef"[0 + 1 : 0 + 6]"#, r#""bcdef""#);
    run(r#"out = "abcdef"[1 + 1 - 1 : 1 + 6 - 1]"#, r#""bcdef""#);
    run(r#"out = "abcdef"[2:2]"#, r#""""#);
    run(r#"out = "abcdef"[0 + 2 : 0 + 2]"#, r#""""#);
    run(r#"out = "abcdef"[1 + 2 - 1 : 1 + 2 - 1]"#, r#""""#);
    run(r#"out = "abcdef"[2:]"#, r#""cdef""#);
    run(r#"out = "abcdef"[2:3]"#, r#""c""#);
    run(r#"out = "abcdef"[0 + 2 : 0 + 3]"#, r#""c""#);
    run(r#"out = "abcdef"[1 + 2 - 1 : 1 + 3 - 1]"#, r#""c""#);
    run(r#"out = "abcdef"[2:4]"#, r#""cd""#);
    run(r#"out = "abcdef"[0 + 2 : 0 + 4]"#, r#""cd""#);
    run(r#"out = "abcdef"[1 + 2 - 1 : 1 + 4 - 1]"#, r#""cd""#);
    run(r#"out = "abcdef"[2:5]"#, r#""cde""#);
    run(r#"out = "abcdef"[0 + 2 : 0 + 5]"#, r#""cde""#);
    run(r#"out = "abcdef"[1 + 2 - 1 : 1 + 5 - 1]"#, r#""cde""#);
    run(r#"out = "abcdef"[2:6]"#, r#""cdef""#);
    run(r#"out = "abcdef"[0 + 2 : 0 + 6]"#, r#""cdef""#);
    run(r#"out = "abcdef"[1 + 2 - 1 : 1 + 6 - 1]"#, r#""cdef""#);
    run(r#"out = "abcdef"[3:3]"#, r#""""#);
    run(r#"out = "abcdef"[0 + 3 : 0 + 3]"#, r#""""#);
    run(r#"out = "abcdef"[1 + 3 - 1 : 1 + 3 - 1]"#, r#""""#);
    run(r#"out = "abcdef"[3:]"#, r#""def""#);
    run(r#"out = "abcdef"[3:4]"#, r#""d""#);
    run(r#"out = "abcdef"[0 + 3 : 0 + 4]"#, r#""d""#);
    run(r#"out = "abcdef"[1 + 3 - 1 : 1 + 4 - 1]"#, r#""d""#);
    run(r#"out = "abcdef"[3:5]"#, r#""de""#);
    run(r#"out = "abcdef"[0 + 3 : 0 + 5]"#, r#""de""#);
    run(r#"out = "abcdef"[1 + 3 - 1 : 1 + 5 - 1]"#, r#""de""#);
    run(r#"out = "abcdef"[3:6]"#, r#""def""#);
    run(r#"out = "abcdef"[0 + 3 : 0 + 6]"#, r#""def""#);
    run(r#"out = "abcdef"[1 + 3 - 1 : 1 + 6 - 1]"#, r#""def""#);
    run(r#"out = "abcdef"[4:4]"#, r#""""#);
    run(r#"out = "abcdef"[0 + 4 : 0 + 4]"#, r#""""#);
    run(r#"out = "abcdef"[1 + 4 - 1 : 1 + 4 - 1]"#, r#""""#);
    run(r#"out = "abcdef"[4:]"#, r#""ef""#);
    run(r#"out = "abcdef"[4:5]"#, r#""e""#);
    run(r#"out = "abcdef"[0 + 4 : 0 + 5]"#, r#""e""#);
    run(r#"out = "abcdef"[1 + 4 - 1 : 1 + 5 - 1]"#, r#""e""#);
    run(r#"out = "abcdef"[4:6]"#, r#""ef""#);
    run(r#"out = "abcdef"[0 + 4 : 0 + 6]"#, r#""ef""#);
    run(r#"out = "abcdef"[1 + 4 - 1 : 1 + 6 - 1]"#, r#""ef""#);
    run(r#"out = "abcdef"[5:5]"#, r#""""#);
    run(r#"out = "abcdef"[0 + 5 : 0 + 5]"#, r#""""#);
    run(r#"out = "abcdef"[1 + 5 - 1 : 1 + 5 - 1]"#, r#""""#);
    run(r#"out = "abcdef"[5:]"#, r#""f""#);
    run(r#"out = "abcdef"[5:6]"#, r#""f""#);
    run(r#"out = "abcdef"[0 + 5 : 0 + 6]"#, r#""f""#);
    run(r#"out = "abcdef"[1 + 5 - 1 : 1 + 6 - 1]"#, r#""f""#);
    run(r#"out = "abcdef"[6:6]"#, r#""""#);
    run(r#"out = "abcdef"[0 + 6 : 0 + 6]"#, r#""""#);
    run(r#"out = "abcdef"[1 + 6 - 1 : 1 + 6 - 1]"#, r#""""#);
    run(r#"out = "abcdef"[6:]"#, r#""""#);
    run(r#"out = "abcdef"[:]"#, r#""abcdef""#);
    run(r#"out = "abcdef"[-1:]"#, r#""abcdef""#);
    run(r#"out = "abcdef"[:7]"#, r#""abcdef""#);
    fails(r#""abcdef"[:-1]"#, "invalid slice index");
    fails(r#""abcdef"[7:]"#, "invalid slice index");
    fails(r#""abcdef"[0:-1]"#, "invalid slice index");
    fails(r#""abcdef"[2:1]"#, "invalid slice index");
    run(r#"out = "foo" + 1"#, r#""foo1""#);
    run(r#"out = "foo" + 1.0"#, r#""foo1""#);
    run(r#"out = "foo" + 1.5"#, r#""foo1.5""#);
    run(r#"out = "foo" + true"#, r#""footrue""#);
    run(r#"out = "foo" + 'X'"#, r#""fooX""#);
    run(r#"out = "foo" + error(5)"#, r#""fooerror: 5""#);
    run(r#"out = "foo" + undefined"#, r#""foo<undefined>""#);
    run(r#"out = "foo" + [1,2,3]"#, r#""foo[1, 2, 3]""#);
    run(r#"out = "foo"; out += 1.5"#, r#""foo1.5""#);
    fails(r#"1 + "foo""#, "invalid operation");
    fails(r#""foo" - "bar""#, "invalid operation");
}

#[test]
fn tail_call() {
    run(r"fac := func(n, a) {
if n == 1 {
return a
}
return fac(n-1, n*a)
}
out = fac(5, 1)", "120");
    run(r"fac := func(n, a) {
if n == 1 {
return a
}
x := {foo: fac} // indirection for test
return x.foo(n-1, n*a)
}
out = fac(5, 1)", "120");
    run(r"fib := func(x, s) {
if x == 0 {
return 0 + s
} else if x == 1 {
return 1 + s
}
return fib(x-1, fib(x-2, s))
}
out = fib(15, 0)", "610");
    run(r"fib := func(n, a, b) {
if n == 0 {
return a
} else if n == 1 {
return b
}
return fib(n-1, b, a + b)
}
out = fib(15, 0, 1)", "610");
    run(r"out = 0
foo := func(a) {
if a == 0 {
return
}
out += a
foo(a-1)
}
foo(10)", "55");
    run(r"f1 := func() {
f2 := 0    // TODO: this might be fixed in the future
f2 = func(n, s) {
if n == 0 { return s }
return f2(n-1, n + s)
}
return f2(5, 0)
}
out = f1()", "15");
    run(r"iter := func(n, max) {
if n == max {
return n
}
return iter(n+1, max)
}
out = iter(0, 9999)", "9999");
    run(r"c := 0
iter := func(n, max) {
if n == max {
return
}
c++
iter(n+1, max)
}
iter(0, 9999)
out = c", "9999");
}

#[test]
fn tail_call_free_vars() {
    run(r"func() {
a := 10
f2 := 0
f2 = func(n, s) {
if n == 0 {
return s + a
}
return f2(n-1, n+s)
}
out = f2(5, 0)
}()", "25");
}

#[test]
fn spread() {
    run(r"f := func(...a) {
return append(a, 3)
}
out = f([1, 2]...)", "[1, 2, 3]");
    run(r"f := func(a, ...b) {
return append([a], append(b, 3)...)
}
out = f([1, 2]...)", "[1, 2, 3]");
    run(r"f := func(a, ...b) {
return append(append([a], b), 3)
}
out = f(1, [2]...)", "[1, [2], 3]");
    run(r"f1 := func(...a){
return append([3], a...)
}
f2 := func(a, ...b) {
return f1(append([a], b...)...)
}
out = f2([1, 2]...)", "[3, 1, 2]");
    run(r"f := func(a, ...b) {
return func(...a) {
return append([3], append(a, 4)...)
}(a, b...)
}
out = f([1, 2]...)", "[3, 1, 2, 4]");
    run(r"f := func(a, ...b) {
c := append(b, 4)
return func(){
return append(append([a], b...), c...)
}()
}
out = f(1, immutable([2, 3])...)", "[1, 2, 3, 2, 3, 4]");
    fails("func(a) {}([1, 2]...)", "Runtime Error: wrong number of arguments: want=1, got=2");
    fails("func(a, b, c) {}([1, 2]...)", "Runtime Error: wrong number of arguments: want=3, got=2");
}

#[test]
fn slice_index() {
    fails("undefined[:1]", "Runtime Error: not indexable");
    fails("123[-1:2]", "Runtime Error: not indexable");
    fails("{}[:]", "Runtime Error: not indexable");
    fails("a := 123[-1:2] ; a += 1", "Runtime Error: not indexable");
}
