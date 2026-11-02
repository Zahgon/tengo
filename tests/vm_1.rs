//! Carried over from vm_test.go: every script runs in the VM and `out` is compared with the Go
//! test's expected value. Tests that need host objects (Go callbacks, custom types, allocation
//! limits) are listed in tests/PORTING.md.

mod common;

use common::*;

#[test]
fn array() {
    run("out = [1, 2 * 2, 3 + 3]", "[1, 4, 6]");
    run("a1 := [1, 2, 3]; a2 := a1; a1[0] = 5; out = a2", "[5, 2, 3]");
    run("func () { a1 := [1, 2, 3]; a2 := a1; a1[0] = 5; out = a2 }()", "[5, 2, 3]");
    fails("a1 := [1, 2, 3]; a1[3] = 5", "index out of bounds");
    run("out = [1, 2, 3, 4, 5, 6][0]", "1");
    run("out = [1, 2, 3, 4, 5, 6][0 + 0]", "1");
    run("out = [1, 2, 3, 4, 5, 6][1 + 0 - 1]", "1");
    run("idx := 0; out = [1, 2, 3, 4, 5, 6][idx]", "1");
    run("out = [1, 2, 3, 4, 5, 6][1]", "2");
    run("out = [1, 2, 3, 4, 5, 6][0 + 1]", "2");
    run("out = [1, 2, 3, 4, 5, 6][1 + 1 - 1]", "2");
    run("idx := 1; out = [1, 2, 3, 4, 5, 6][idx]", "2");
    run("out = [1, 2, 3, 4, 5, 6][2]", "3");
    run("out = [1, 2, 3, 4, 5, 6][0 + 2]", "3");
    run("out = [1, 2, 3, 4, 5, 6][1 + 2 - 1]", "3");
    run("idx := 2; out = [1, 2, 3, 4, 5, 6][idx]", "3");
    run("out = [1, 2, 3, 4, 5, 6][3]", "4");
    run("out = [1, 2, 3, 4, 5, 6][0 + 3]", "4");
    run("out = [1, 2, 3, 4, 5, 6][1 + 3 - 1]", "4");
    run("idx := 3; out = [1, 2, 3, 4, 5, 6][idx]", "4");
    run("out = [1, 2, 3, 4, 5, 6][4]", "5");
    run("out = [1, 2, 3, 4, 5, 6][0 + 4]", "5");
    run("out = [1, 2, 3, 4, 5, 6][1 + 4 - 1]", "5");
    run("idx := 4; out = [1, 2, 3, 4, 5, 6][idx]", "5");
    run("out = [1, 2, 3, 4, 5, 6][5]", "6");
    run("out = [1, 2, 3, 4, 5, 6][0 + 5]", "6");
    run("out = [1, 2, 3, 4, 5, 6][1 + 5 - 1]", "6");
    run("idx := 5; out = [1, 2, 3, 4, 5, 6][idx]", "6");
    run("[1, 2, 3, 4, 5, 6][-1]", "undefined");
    run("[1, 2, 3, 4, 5, 6][6]", "undefined");
    run("out = [1, 2, 3, 4, 5, 6][0:0]", "[]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 0 : 0 + 0]", "[]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 0 - 1 : 1 + 0 - 1]", "[]");
    run("out = [1, 2, 3, 4, 5, 6][:0]", "[]");
    run("out = [1, 2, 3, 4, 5, 6][0:]", "[1, 2, 3, 4, 5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][0:1]", "[1]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 0 : 0 + 1]", "[1]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 0 - 1 : 1 + 1 - 1]", "[1]");
    run("out = [1, 2, 3, 4, 5, 6][:1]", "[1]");
    run("out = [1, 2, 3, 4, 5, 6][0:2]", "[1, 2]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 0 : 0 + 2]", "[1, 2]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 0 - 1 : 1 + 2 - 1]", "[1, 2]");
    run("out = [1, 2, 3, 4, 5, 6][:2]", "[1, 2]");
    run("out = [1, 2, 3, 4, 5, 6][0:3]", "[1, 2, 3]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 0 : 0 + 3]", "[1, 2, 3]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 0 - 1 : 1 + 3 - 1]", "[1, 2, 3]");
    run("out = [1, 2, 3, 4, 5, 6][:3]", "[1, 2, 3]");
    run("out = [1, 2, 3, 4, 5, 6][0:4]", "[1, 2, 3, 4]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 0 : 0 + 4]", "[1, 2, 3, 4]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 0 - 1 : 1 + 4 - 1]", "[1, 2, 3, 4]");
    run("out = [1, 2, 3, 4, 5, 6][:4]", "[1, 2, 3, 4]");
    run("out = [1, 2, 3, 4, 5, 6][0:5]", "[1, 2, 3, 4, 5]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 0 : 0 + 5]", "[1, 2, 3, 4, 5]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 0 - 1 : 1 + 5 - 1]", "[1, 2, 3, 4, 5]");
    run("out = [1, 2, 3, 4, 5, 6][:5]", "[1, 2, 3, 4, 5]");
    run("out = [1, 2, 3, 4, 5, 6][0:6]", "[1, 2, 3, 4, 5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 0 : 0 + 6]", "[1, 2, 3, 4, 5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 0 - 1 : 1 + 6 - 1]", "[1, 2, 3, 4, 5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][:6]", "[1, 2, 3, 4, 5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][1:1]", "[]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 1 : 0 + 1]", "[]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 1 - 1 : 1 + 1 - 1]", "[]");
    run("out = [1, 2, 3, 4, 5, 6][1:]", "[2, 3, 4, 5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][1:2]", "[2]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 1 : 0 + 2]", "[2]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 1 - 1 : 1 + 2 - 1]", "[2]");
    run("out = [1, 2, 3, 4, 5, 6][1:3]", "[2, 3]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 1 : 0 + 3]", "[2, 3]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 1 - 1 : 1 + 3 - 1]", "[2, 3]");
    run("out = [1, 2, 3, 4, 5, 6][1:4]", "[2, 3, 4]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 1 : 0 + 4]", "[2, 3, 4]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 1 - 1 : 1 + 4 - 1]", "[2, 3, 4]");
    run("out = [1, 2, 3, 4, 5, 6][1:5]", "[2, 3, 4, 5]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 1 : 0 + 5]", "[2, 3, 4, 5]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 1 - 1 : 1 + 5 - 1]", "[2, 3, 4, 5]");
    run("out = [1, 2, 3, 4, 5, 6][1:6]", "[2, 3, 4, 5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 1 : 0 + 6]", "[2, 3, 4, 5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 1 - 1 : 1 + 6 - 1]", "[2, 3, 4, 5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][2:2]", "[]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 2 : 0 + 2]", "[]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 2 - 1 : 1 + 2 - 1]", "[]");
    run("out = [1, 2, 3, 4, 5, 6][2:]", "[3, 4, 5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][2:3]", "[3]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 2 : 0 + 3]", "[3]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 2 - 1 : 1 + 3 - 1]", "[3]");
    run("out = [1, 2, 3, 4, 5, 6][2:4]", "[3, 4]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 2 : 0 + 4]", "[3, 4]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 2 - 1 : 1 + 4 - 1]", "[3, 4]");
    run("out = [1, 2, 3, 4, 5, 6][2:5]", "[3, 4, 5]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 2 : 0 + 5]", "[3, 4, 5]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 2 - 1 : 1 + 5 - 1]", "[3, 4, 5]");
    run("out = [1, 2, 3, 4, 5, 6][2:6]", "[3, 4, 5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 2 : 0 + 6]", "[3, 4, 5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 2 - 1 : 1 + 6 - 1]", "[3, 4, 5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][3:3]", "[]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 3 : 0 + 3]", "[]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 3 - 1 : 1 + 3 - 1]", "[]");
    run("out = [1, 2, 3, 4, 5, 6][3:]", "[4, 5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][3:4]", "[4]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 3 : 0 + 4]", "[4]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 3 - 1 : 1 + 4 - 1]", "[4]");
    run("out = [1, 2, 3, 4, 5, 6][3:5]", "[4, 5]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 3 : 0 + 5]", "[4, 5]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 3 - 1 : 1 + 5 - 1]", "[4, 5]");
    run("out = [1, 2, 3, 4, 5, 6][3:6]", "[4, 5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 3 : 0 + 6]", "[4, 5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 3 - 1 : 1 + 6 - 1]", "[4, 5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][4:4]", "[]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 4 : 0 + 4]", "[]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 4 - 1 : 1 + 4 - 1]", "[]");
    run("out = [1, 2, 3, 4, 5, 6][4:]", "[5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][4:5]", "[5]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 4 : 0 + 5]", "[5]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 4 - 1 : 1 + 5 - 1]", "[5]");
    run("out = [1, 2, 3, 4, 5, 6][4:6]", "[5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 4 : 0 + 6]", "[5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 4 - 1 : 1 + 6 - 1]", "[5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][5:5]", "[]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 5 : 0 + 5]", "[]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 5 - 1 : 1 + 5 - 1]", "[]");
    run("out = [1, 2, 3, 4, 5, 6][5:]", "[6]");
    run("out = [1, 2, 3, 4, 5, 6][5:6]", "[6]");
    run("out = [1, 2, 3, 4, 5, 6][0 + 5 : 0 + 6]", "[6]");
    run("out = [1, 2, 3, 4, 5, 6][1 + 5 - 1 : 1 + 6 - 1]", "[6]");
    run("out = [1, 2, 3, 4, 5, 6][:]", "[1, 2, 3, 4, 5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][-1:]", "[1, 2, 3, 4, 5, 6]");
    run("out = [1, 2, 3, 4, 5, 6][:7]", "[1, 2, 3, 4, 5, 6]");
    fails("[1, 2, 3, 4, 5, 6][:-1]", "invalid slice index");
    fails("[1, 2, 3, 4, 5, 6][7:]", "invalid slice index");
    fails("[1, 2, 3, 4, 5, 6][0:-1]", "invalid slice index");
    fails("[1, 2, 3, 4, 5, 6][2:1]", "invalid slice index");
}

#[test]
fn assignment() {
    run("a := 1; a = 2; out = a", "2");
    run("a := 1; a = a + 4; out = a", "5");
    run("a := 1; f1 := func() { a = 2; return a }; out = f1()", "2");
    run("a := 1; f1 := func() { a := 3; a = 2; return a }; out = f1()", "2");
    run("a := 1; out = a", "1");
    run("a := 1; func() { a = 2 }(); out = a", "2");
    run("a := 1; func() { a := 2 }(); out = a", "1");
    run("a := 1; func() { b := 2; out = b }()", "2");
    run(r"out = func() {
a := 2
func() {
a = 3 // captured from outer scope
}()
return a
}()", "3");
    run(r"func() {
a := 5
out = func() {
a := 4
return a
}()
}()", "4");
    fails("a := 1; a := 2", "redeclared");
    fails("func() { a := 1; a := 2 }()", "redeclared");
    run("a := 1; a += 2; out = a", "3");
    run("a := 1; a += 4 - 2;; out = a", "3");
    run("a := 3; a -= 1;; out = a", "2");
    run("a := 3; a -= 5 - 4;; out = a", "2");
    run("a := 2; a *= 4;; out = a", "8");
    run("a := 2; a *= 1 + 3;; out = a", "8");
    run("a := 10; a /= 2;; out = a", "5");
    run("a := 10; a /= 5 - 3;; out = a", "5");
    fails("a += 4", "unresolved reference");
    fails("a -= 4", "unresolved reference");
    fails("a *= 4", "unresolved reference");
    fails("a /= 4", "unresolved reference");
    run(r"f1 := func() {
f2 := func() {
a := 1
a += 2    // it's a statement, not an expression
return a
};
return f2();
};
out = f1();", "3");
    run("f1 := func() { f2 := func() { a := 1; a += 4 - 2; return a }; return f2(); }; out = f1()", "3");
    run("f1 := func() { f2 := func() { a := 3; a -= 1; return a }; return f2(); }; out = f1()", "2");
    run("f1 := func() { f2 := func() { a := 3; a -= 5 - 4; return a }; return f2(); }; out = f1()", "2");
    run("f1 := func() { f2 := func() { a := 2; a *= 4; return a }; return f2(); }; out = f1()", "8");
    run("f1 := func() { f2 := func() { a := 2; a *= 1 + 3; return a }; return f2(); }; out = f1()", "8");
    run("f1 := func() { f2 := func() { a := 10; a /= 2; return a }; return f2(); }; out = f1()", "5");
    run("f1 := func() { f2 := func() { a := 10; a /= 5 - 3; return a }; return f2(); }; out = f1()", "5");
    run("a := 1; f1 := func() { f2 := func() { a += 2; return a }; return f2(); }; out = f1()", "3");
    run(r"f1 := func(a) {
return func(b) {
c := a
c += b * 2
return c
}
}
out = f1(3)(4)", "11");
    run(r"out = func() {
a := 1
func() {
a = 2
func() {
a = 3
func() {
a := 4 // declared new
}()
}()
}()
return a
}()", "3");
    run(r"f1 := func() {
a := 5
return func() {
a += 3
return a
}()
}
out = f1()", "8");
    run(r"out = func() {
f1 := func() {
a := 5
add1 := func() { a += 1 }
add2 := func() { a += 2 }
a += 3
return func() { a += 4; add1(); add2(); a += 5; return a }
}
return f1()
}()()", "20");
    run(r"it := func(seq, fn) {
fn(seq[0])
fn(seq[1])
fn(seq[2])
}
foo := func(a) {
b := 0
it([1, 2, 3], func(x) {
b = x + a
})
return b
}
out = foo(2)", "5");
    run(r"it := func(seq, fn) {
fn(seq[0])
fn(seq[1])
fn(seq[2])
}
foo := func(a) {
b := 0
it([1, 2, 3], func(x) {
b += x + a
})
return b
}
out = foo(2)", "12");
    run(r"out = func() {
a := 1
func() {
a = 2
}()
return a
}()", "2");
    run(r"f := func() {
a := 1
return {
b: func() { a += 3 },
c: func() { a += 2 },
d: func() { return a }
}
}
m := f()
m.b()
m.c()
out = m.d()", "6");
    run(r"each := func(s, x) { for i:=0; i<len(s); i++ { x(s[i]) } }
out = func() {
a := 100
each([1, 2, 3], func(x) {
a += x
})
a += 10
return func(b) {
return a + b
}
}()(20)", "136");
    run(r#"a := 1; a = "foo"; out = a"#, r#""foo""#);
    run(r#"func() { a := 1; a = "foo"; out = a }()"#, r#""foo""#);
    run(r#"out = func() {
a := 5
return func() {
a = "foo"
return a
}()
}()"#, r#""foo""#);
    run(r#"for a:=0; a<5; a++ {}; a := "foo"; out = a"#, r#""foo""#);
    run(r#"func() { for a:=0; a<5; a++ {}; a := "foo"; out = a }()"#, r#""foo""#);
    run("a:=[1,2,3]; a[1] = 5; out = a[1]", "5");
    run("a:=[1,2,3]; a[1] += 5; out = a[1]", "7");
    run("a:={b:1,c:2}; a.b = 5; out = a.b", "5");
    run("a:={b:1,c:2}; a.b += 5; out = a.b", "6");
    run("a:={b:1,c:2}; a.b += a.c; out = a.b", "3");
    run("a:={b:1,c:2}; a.b += a.c; out = a.c", "2");
    run(r#"a := {
b: [1, 2, 3],
c: {
d: 8,
e: "foo",
f: [9, 8]
}
}
a.c.f[1] += 2
out = a["c"]["f"][1]"#, "10");
    run(r#"a := {
b: [1, 2, 3],
c: {
d: 8,
e: "foo",
f: [9, 8]
}
}
a.c.h = "bar"
out = a.c.h"#, r#""bar""#);
    fails(r#"
a := {
	b: [1, 2, 3],
	c: {
		d: 8,
		e: "foo",
		f: [9, 8]
	}
}
a.x.e = "bar""#, "not index-assignable");
}

#[test]
fn bitwise() {
    run("out = 1 & 1", "1");
    run("out = 1 & 0", "0");
    run("out = 0 & 1", "0");
    run("out = 0 & 0", "0");
    run("out = 1 | 1", "1");
    run("out = 1 | 0", "1");
    run("out = 0 | 1", "1");
    run("out = 0 | 0", "0");
    run("out = 1 ^ 1", "0");
    run("out = 1 ^ 0", "1");
    run("out = 0 ^ 1", "1");
    run("out = 0 ^ 0", "0");
    run("out = 1 &^ 1", "0");
    run("out = 1 &^ 0", "1");
    run("out = 0 &^ 1", "0");
    run("out = 0 &^ 0", "0");
    run("out = 1 << 2", "4");
    run("out = 16 >> 2", "4");
    run("out = 1; out &= 1", "1");
    run("out = 1; out |= 0", "1");
    run("out = 1; out ^= 0", "1");
    run("out = 1; out &^= 0", "1");
    run("out = 1; out <<= 2", "4");
    run("out = 16; out >>= 2", "4");
    run("out = ^0", "-1");
    run("out = ^1", "-2");
    run("out = ^55", "-56");
    run("out = ^-55", "54");
}

#[test]
fn boolean() {
    run("out = true", "true");
    run("out = false", "false");
    run("out = 1 < 2", "true");
    run("out = 1 > 2", "false");
    run("out = 1 < 1", "false");
    run("out = 1 == 1", "true");
    run("out = 1 != 1", "false");
    run("out = 1 == 2", "false");
    run("out = 1 != 2", "true");
    run("out = 1 <= 2", "true");
    run("out = 1 >= 2", "false");
    run("out = 1 <= 1", "true");
    run("out = true == true", "true");
    run("out = false == false", "true");
    run("out = true == false", "false");
    run("out = true != false", "true");
    run("out = false != true", "true");
    run("out = (1 < 2) == true", "true");
    run("out = (1 < 2) == false", "false");
    run("out = (1 > 2) == true", "false");
    run("out = (1 > 2) == false", "true");
    fails("5 + true", "invalid operation");
    fails("5 + true; 5", "invalid operation");
    fails("-true", "invalid operation");
    fails("true + false", "invalid operation");
    fails("5; true + false; 5", "invalid operation");
    fails("if (10 > 1) { true + false; }", "invalid operation");
    fails(r"
func() {
	if (10 > 1) {
		if (10 > 1) {
			return true + false;
		}

		return 1;
	}
}()
", "invalid operation");
    fails("if (true + false) { 10 }", "invalid operation");
    fails("10 + (true + false)", "invalid operation");
    fails("(true + false) + 20", "invalid operation");
    fails("!(true + false)", "invalid operation");
}

#[test]
fn undefined() {
    run("out = undefined", "undefined");
    run("out = undefined.a", "undefined");
    run("out = undefined[1]", "undefined");
    run("out = undefined.a.b", "undefined");
    run("out = undefined[1][2]", "undefined");
    run("out = undefined ? 1 : 2", "2");
    run("out = undefined == undefined", "true");
    run("out = undefined == 1", "false");
    run("out = 1 == undefined", "false");
    run("out = undefined == float([])", "true");
    run("out = float([]) == undefined", "true");
}

#[test]
fn builtin_function() {
    run(r#"out = len("")"#, "0");
    run(r#"out = len("four")"#, "4");
    run(r#"out = len("hello world")"#, "11");
    run("out = len([])", "0");
    run("out = len([1, 2, 3])", "3");
    run("out = len({})", "0");
    run("out = len({a:1, b:2})", "2");
    run("out = len(immutable([]))", "0");
    run("out = len(immutable([1, 2, 3]))", "3");
    run("out = len(immutable({}))", "0");
    run("out = len(immutable({a:1, b:2}))", "2");
    fails("len(1)", "invalid type for argument");
    fails(r#"len("one", "two")"#, "wrong number of arguments");
    run("out = copy(1)", "1");
    fails("copy(1, 2)", "wrong number of arguments");
    run("out = append([1, 2, 3], 4)", "[1, 2, 3, 4]");
    run("out = append([1, 2, 3], 4, 5, 6)", "[1, 2, 3, 4, 5, 6]");
    run(r#"out = append([1, 2, 3], "foo", false)"#, r#"[1, 2, 3, "foo", false]"#);
    run("out = int(1)", "1");
    run("out = int(1.8)", "1");
    run(r#"out = int("-522")"#, "-522");
    run("out = int(true)", "1");
    run("out = int(false)", "0");
    run("out = int('8')", "56");
    run("out = int([1])", "undefined");
    run("out = int({a: 1})", "undefined");
    run("out = int(undefined)", "undefined");
    run(r#"out = int("-522", 1)"#, "-522");
    run("out = int(undefined, 1)", "1");
    run("out = int(undefined, 1.8)", "f1.8");
    run("out = int(undefined, string(1))", r#""1""#);
    run("out = int(undefined, undefined)", "undefined");
    run("out = string(1)", r#""1""#);
    run("out = string(1.8)", r#""1.8""#);
    run(r#"out = string("-522")"#, r#""-522""#);
    run("out = string(true)", r#""true""#);
    run("out = string(false)", r#""false""#);
    run("out = string('8')", r#""8""#);
    run("out = string([1,8.1,true,3])", r#""[1, 8.1, true, 3]""#);
    run(r#"out = string({b: "foo"})"#, r#""{b: \"foo\"}""#);
    run("out = string(undefined)", "undefined");
    run(r#"out = string(1, "-522")"#, r#""1""#);
    run(r#"out = string(undefined, "-522")"#, r#""-522""#);
    run("out = float(1)", "f1");
    run("out = float(1.8)", "f1.8");
    run(r#"out = float("-52.2")"#, "f-52.2");
    run("out = float(true)", "undefined");
    run("out = float(false)", "undefined");
    run("out = float('8')", "undefined");
    run("out = float([1,8.1,true,3])", "undefined");
    run(r#"out = float({a: 1, b: "foo"})"#, "undefined");
    run("out = float(undefined)", "undefined");
    run(r#"out = float("-52.2", 1.8)"#, "f-52.2");
    run("out = float(undefined, 1)", "1");
    run("out = float(undefined, 1.8)", "f1.8");
    run(r#"out = float(undefined, "-52.2")"#, r#""-52.2""#);
    run("out = float(undefined, char(56))", "c56");
    run("out = float(undefined, undefined)", "undefined");
    run("out = char(56)", "c56");
    run("out = char(1.8)", "undefined");
    run(r#"out = char("-52.2")"#, "undefined");
    run("out = char(true)", "undefined");
    run("out = char(false)", "undefined");
    run("out = char('8')", "c56");
    run("out = char([1,8.1,true,3])", "undefined");
    run(r#"out = char({a: 1, b: "foo"})"#, "undefined");
    run("out = char(undefined)", "undefined");
    run("out = char(56, 'a')", "c56");
    run("out = char(undefined, '8')", "c56");
    run("out = char(undefined, 56)", "56");
    run(r#"out = char(undefined, "-52.2")"#, r#""-52.2""#);
    run("out = char(undefined, undefined)", "undefined");
    run("out = bool(1)", "true");
    run("out = bool(0)", "false");
    run("out = bool(1.8)", "true");
    run("out = bool(0.0)", "true");
    run(r#"out = bool("false")"#, "true");
    run(r#"out = bool("")"#, "false");
    run("out = bool(true)", "true");
    run("out = bool(false)", "false");
    run("out = bool('8')", "true");
    run("out = bool(char(0))", "false");
    run("out = bool([1])", "true");
    run("out = bool([])", "false");
    run("out = bool({a: 1})", "true");
    run("out = bool({})", "false");
    run("out = bool(undefined)", "false");
    run("out = bytes(1)", "b00");
    run("out = bytes(1.8)", "undefined");
    run(r#"out = bytes("-522")"#, "b2d353232");
    run("out = bytes(true)", "undefined");
    run("out = bytes(false)", "undefined");
    run("out = bytes('8')", "undefined");
    run("out = bytes([1])", "undefined");
    run("out = bytes({a: 1})", "undefined");
    run("out = bytes(undefined)", "undefined");
    run(r#"out = bytes("-522", ['8'])"#, "b2d353232");
    run(r#"out = bytes(undefined, "-522")"#, r#""-522""#);
    run("out = bytes(undefined, 1)", "1");
    run("out = bytes(undefined, 1.8)", "f1.8");
    run(r#"out = bytes(undefined, int("-522"))"#, "-522");
    run("out = bytes(undefined, undefined)", "undefined");
    run("out = is_error(error(1))", "true");
    run("out = is_error(1)", "false");
    run("out = is_undefined(undefined)", "true");
    run("out = is_undefined(error(1))", "false");
    run("out = type_name(1)", r#""int""#);
    run("out = type_name(1.1)", r#""float""#);
    run(r#"out = type_name("a")"#, r#""string""#);
    run("out = type_name([1,2,3])", r#""array""#);
    run("out = type_name({k:1})", r#""map""#);
    run("out = type_name('a')", r#""char""#);
    run("out = type_name(true)", r#""bool""#);
    run("out = type_name(false)", r#""bool""#);
    run("out = type_name(bytes( 1))", r#""bytes""#);
    run("out = type_name(undefined)", r#""undefined""#);
    run(r#"out = type_name(error("err"))"#, r#""error""#);
    run("out = type_name(func() {})", r#""compiled-function""#);
    run("a := func(x) { return func() { return x } }; out = type_name(a(5))", r#""compiled-function""#);
    run("out = is_function(1)", "false");
    run("out = is_function(func() {})", "true");
    run("out = is_function(func(x) { return x })", "true");
    run("out = is_function(len)", "false");
    run("a := func(x) { return func() { return x } }; out = is_function(a)", "true");
    run("a := func(x) { return func() { return x } }; out = is_function(a(5))", "true");
    run("out = is_callable(1)", "false");
    run("out = is_callable(func() {})", "true");
    run("out = is_callable(func(x) { return x })", "true");
    run("out = is_callable(len)", "true");
    run("a := func(x) { return func() { return x } }; out = is_callable(a)", "true");
    run("a := func(x) { return func() { return x } }; out = is_callable(a(5))", "true");
    run(r#"out = format("")"#, r#""""#);
    run(r#"out = format("foo")"#, r#""foo""#);
    run(r#"out = format("foo %d %v %s", 1, 2, "bar")"#, r#""foo 1 2 bar""#);
    run(r#"out = format("foo %v", [1, "bar", true])"#, r#""foo [1, \"bar\", true]""#);
    run(r#"out = format("foo %v %d", [1, "bar", true], 19)"#, r#""foo [1, \"bar\", true] 19""#);
    run(r#"out = format("foo %v", {"a": {"b": {"c": [1, 2, 3]}}})"#, r#""foo {a: {b: {c: [1, 2, 3]}}}""#);
    run(r#"out = format("%v", [1, [2, [3, 4]]])"#, r#""[1, [2, [3, 4]]]""#);
    fails("delete()", "wrong number of arguments");
    fails("delete(1)", "wrong number of arguments");
    fails("delete(1, 2, 3)", "wrong number of arguments");
    fails(r#"delete({}, "", 3)"#, "wrong number of arguments");
    fails("delete(1, 1)", "invalid type for argument 'first'");
    fails("delete(1.0, 1)", "invalid type for argument 'first'");
    fails(r#"delete("str", 1)"#, "invalid type for argument 'first'");
    fails(r#"delete(bytes("str"), 1)"#, "invalid type for argument 'first'");
    fails(r#"delete(error("err"), 1)"#, "invalid type for argument 'first'");
    fails("delete(true, 1)", "invalid type for argument 'first'");
    fails("delete(char('c'), 1)", "invalid type for argument 'first'");
    fails("delete(undefined, 1)", "invalid type for argument 'first'");
    fails("delete(time(1257894000), 1)", "invalid type for argument 'first'");
    fails(r#"delete(immutable({}), "key")"#, "invalid type for argument 'first'");
    fails(r#"delete(immutable([]), "")"#, "invalid type for argument 'first'");
    fails(r#"delete([], "")"#, "invalid type for argument 'first'");
    fails("delete({}, 1)", "invalid type for argument 'second'");
    fails("delete({}, 1.0)", "invalid type for argument 'second'");
    fails("delete({}, undefined)", "invalid type for argument 'second'");
    fails("delete({}, [])", "invalid type for argument 'second'");
    fails("delete({}, {})", "invalid type for argument 'second'");
    fails(r#"delete({}, error("err"))"#, "invalid type for argument 'second'");
    fails(r#"delete({}, bytes("str"))"#, "invalid type for argument 'second'");
    fails("delete({}, char(35))", "invalid type for argument 'second'");
    fails("delete({}, time(1257894000))", "invalid type for argument 'second'");
    fails("delete({}, immutable({}))", "invalid type for argument 'second'");
    fails("delete({}, immutable([]))", "invalid type for argument 'second'");
    run(r#"out = delete({}, "")"#, "undefined");
    run(r#"out = {key1: 1}; delete(out, "key1")"#, "{}");
    run(r#"out = {key1: 1, key2: "2"}; delete(out, "key1")"#, r#"{"key2": "2"}"#);
    run(r#"out = [1, "2", {a: "b", c: 10}]; delete(out[2], "c")"#, r#"[1, "2", {"a": "b"}]"#);
    fails("splice()", "wrong number of arguments");
    fails("splice(1)", "invalid type for argument 'first'");
    fails("splice(1.0)", "invalid type for argument 'first'");
    fails(r#"splice("str")"#, "invalid type for argument 'first'");
    fails(r#"splice(bytes("str"))"#, "invalid type for argument 'first'");
    fails(r#"splice(error("err"))"#, "invalid type for argument 'first'");
    fails("splice(true)", "invalid type for argument 'first'");
    fails("splice(char('c'))", "invalid type for argument 'first'");
    fails("splice(undefined)", "invalid type for argument 'first'");
    fails("splice(time(1257894000))", "invalid type for argument 'first'");
    fails("splice(immutable({}))", "invalid type for argument 'first'");
    fails("splice(immutable([]))", "invalid type for argument 'first'");
    fails("splice({})", "invalid type for argument 'first'");
    fails("splice([], 1.0)", "invalid type for argument 'second'");
    fails(r#"splice([], "str")"#, "invalid type for argument 'second'");
    fails(r#"splice([], bytes("str"))"#, "invalid type for argument 'second'");
    fails(r#"splice([], error("error"))"#, "invalid type for argument 'second'");
    fails("splice([], false)", "invalid type for argument 'second'");
    fails("splice([], char('d'))", "invalid type for argument 'second'");
    fails("splice([], undefined)", "invalid type for argument 'second'");
    fails("splice([], time(0))", "invalid type for argument 'second'");
    fails("splice([], [])", "invalid type for argument 'second'");
    fails("splice([], {})", "invalid type for argument 'second'");
    fails("splice([], immutable([]))", "invalid type for argument 'second'");
    fails("splice([], immutable({}))", "invalid type for argument 'second'");
    fails("splice([], 0, 1.0)", "invalid type for argument 'third'");
    fails(r#"splice([], 0, "string")"#, "invalid type for argument 'third'");
    fails(r#"splice([], 0, bytes("string"))"#, "invalid type for argument 'third'");
    fails(r#"splice([], 0, error("string"))"#, "invalid type for argument 'third'");
    fails("splice([], 0, true)", "invalid type for argument 'third'");
    fails("splice([], 0, char('f'))", "invalid type for argument 'third'");
    fails("splice([], 0, undefined)", "invalid type for argument 'third'");
    fails("splice([], 0, time(0))", "invalid type for argument 'third'");
    fails("splice([], 0, [])", "invalid type for argument 'third'");
    fails("splice([], 0, {})", "invalid type for argument 'third'");
    fails("splice([], 0, immutable([]))", "invalid type for argument 'third'");
    fails("splice([], 0, immutable({}))", "invalid type for argument 'third'");
    fails("splice([], 1)", "index out of bounds");
    fails("splice([1, 2, 3], 0, -1)", "index out of bounds");
    fails(r#"splice([1, 2, 3], 99, 0, "a", "b")"#, "index out of bounds");
    run("out = []; splice(out)", "[]");
    run(r#"out = ["a"]; splice(out, 1)"#, r#"["a"]"#);
    run(r#"out = ["a"]; out = splice(out, 1)"#, "[]");
    run("out = [1, 2, 3]; splice(out, 0, 1)", "[2, 3]");
    run("out = [1, 2, 3]; out = splice(out, 0, 1)", "[1]");
    run(r#"out = [1, 2, 3]; splice(out, 0, 0, "a", "b")"#, r#"["a", "b", 1, 2, 3]"#);
    run(r#"out = [1, 2, 3]; out = splice(out, 0, 0, "a", "b")"#, "[]");
    run(r#"out = [1, 2, 3]; splice(out, 1, 0, "a", "b")"#, r#"[1, "a", "b", 2, 3]"#);
    run(r#"out = [1, 2, 3]; out = splice(out, 1, 0, "a", "b")"#, "[]");
    run(r#"out = [1, 2, 3]; splice(out, 2, 0, "a", "b")"#, r#"[1, 2, "a", "b", 3]"#);
    run(r#"out = [1, 2, 3]; splice(out, 3, 0, "a", "b")"#, r#"[1, 2, 3, "a", "b"]"#);
    run(r#"array := [1, 2, 3]; deleted := splice(array, 1, 1, "a", "b");
out = [deleted, array]"#, r#"[[2], [1, "a", "b", 3]]"#);
    run(r"array := [1, 2, 3]; deleted := splice(array, 1);
out = [deleted, array]", "[[2, 3], [1]]");
    run(r#"out = []; splice(out, 0, 0, "a", "b")"#, r#"["a", "b"]"#);
    run(r#"out = []; splice(out, 0, 1, "a", "b")"#, r#"["a", "b"]"#);
    run(r#"out = []; out = splice(out, 0, 0, "a", "b")"#, "[]");
    run("out = splice(splice([1, 2, 3], 0, 3), 1, 3)", "[2, 3]");
    run(r"v := [1, 2, 3]; deleted := splice(v, 0);
out = [deleted, v]", "[[1, 2, 3], []]");
    run(r"v := [1, 2, 3]; deleted := splice(v, 1);
out = [deleted, v]", "[[2, 3], [1]]");
    run(r"v := [1, 2, 3]; deleted := splice(v, 0, 1);
out = [deleted, v]", "[[1], [2, 3]]");
    run(r#"v := ["a", "b", "c"]; deleted := splice(v, 1, 2);
out = [deleted, v]"#, r#"[["b", "c"], ["a"]]"#);
    run(r#"v := ["a", "b", "c"]; deleted := splice(v, 2, 1, "d");
out = [deleted, v]"#, r#"[["c"], ["a", "b", "d"]]"#);
    run(r#"v := ["a", "b", "c"]; deleted := splice(v, 0, 0, "d", "e");
out = [deleted, v]"#, r#"[[], ["d", "e", "a", "b", "c"]]"#);
    run(r#"v := ["a", "b", "c"]; deleted := splice(v, 1, 1, "d", "e");
out = [deleted, v]"#, r#"[["b"], ["a", "d", "e", "c"]]"#);
}

#[test]
fn range_n() {
    run("out = len(range(0, 10))", "10");
    run("out = range(0, 10, 9223372036854775806)", "[0]");
}

#[test]
fn bytes_n() {
    run("out = bytes(0)", "b");
    run("out = bytes(10)", "b00000000000000000000");
    run("out = bytes(1000)", "b00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000");
}

#[test]
fn bytes() {
    run(r#"out = bytes("Hello World!")"#, "b48656c6c6f20576f726c6421");
    run(r#"out = bytes("Hello") + bytes(" ") + bytes("World!")"#, "b48656c6c6f20576f726c6421");
    run(r#"out = bytes("abcde")[0]"#, "97");
    run(r#"out = bytes("abcde")[1]"#, "98");
    run(r#"out = bytes("abcde")[4]"#, "101");
    run(r#"out = bytes("abcde")[10]"#, "undefined");
}

#[test]
fn call() {
    run("a := { b: func(x) { return x + 2 } }; out = a.b(5)", "7");
    run("a := { b: { c: func(x) { return x + 2 } } }; out = a.b.c(5)", "7");
    run(r#"a := { b: { c: func(x) { return x + 2 } } }; out = a["b"].c(5)"#, "7");
    fails(r"a := 1
b := func(a, c) {
   c(a)
}

c := func(a) {
   a()
}
b(a, c)
", r"Runtime Error: not callable: int
	at test:7:4
	at test:3:4
	at test:9:1");
}

#[test]
fn char() {
    run("out = 'a'", "c97");
    run("out = '九'", "c20061");
    run("out = 'Æ'", "c198");
    run("out = '0' + '9'", "c105");
    run("out = '0' + 9", "c57");
    run("out = '9' - 4", "c53");
    run("out = '0' == '0'", "true");
    run("out = '0' != '0'", "false");
    run("out = '2' < '4'", "true");
    run("out = '2' > '4'", "false");
    run("out = '2' <= '4'", "true");
    run("out = '2' >= '4'", "false");
    run("out = '4' < '4'", "false");
    run("out = '4' > '4'", "false");
    run("out = '4' <= '4'", "true");
    run("out = '4' >= '4'", "true");
}

#[test]
fn cond_expr() {
    run("out = true ? 5 : 10", "5");
    run("out = false ? 5 : 10", "10");
    run("out = (1 == 1) ? 2 + 3 : 12 - 2", "5");
    run("out = (1 != 1) ? 2 + 3 : 12 - 2", "10");
    run("out = (1 == 1) ? true ? 10 - 8 : 1 + 3 : 12 - 2", "2");
    run("out = (1 == 1) ? false ? 10 - 8 : 1 + 3 : 12 - 2", "4");
    run(r"out = 0
f1 := func() { out += 10 }
f2 := func() { out = -out }
true ? f1() : f2()", "10");
    run(r"out = 5
f1 := func() { out += 10 }
f2 := func() { out = -out }
false ? f1() : f2()", "-5");
    run(r"f1 := func(a) { return a + 2 }
f2 := func(a) { return a - 2 }
f3 := func(a) { return a + 10 }
f4 := func(a) { return -a }
f := func(c) {
return c == 0 ? f1(c) : f2(c) ? f3(c) : f4(c)
}
out = [f(0), f(1), f(2)]", "[2, 11, -2]");
    run("f := func(a) { return -a }; out = f(true ? 5 : 3)", "-5");
    run("out = [false?5:10, true?1:2]", "[10, 1]");
    run(r"out = 1 > 2 ?
1 + 2 + 3 :
10 - 5", "5");
}

#[test]
fn equality() {
    run("out = 2 == 1", "false");
    run("out = 2 != 1", "true");
    run("out = 1.0 == 1.0", "true");
    run("out = 1.0 != 1.0", "false");
    run("out = 1.0 == 1.1", "false");
    run("out = 1.1 == 1.0", "false");
    run("out = 1.0 != 1.1", "true");
    run("out = 1.1 != 1.0", "true");
    run("out = true != true", "false");
    run("out = false == true", "false");
    run(r#"out = "foo" == "foo""#, "true");
    run(r#"out = "foo" != "foo""#, "false");
    run(r#"out = "foo" == "bar""#, "false");
    run(r#"out = "bar" == "foo""#, "false");
    run(r#"out = "foo" != "bar""#, "true");
    run(r#"out = "bar" != "foo""#, "true");
    run("out = 'f' == 'f'", "true");
    run("out = 'f' != 'f'", "false");
    run("out = 'f' == 'b'", "false");
    run("out = 'b' == 'f'", "false");
    run("out = 'f' != 'b'", "true");
    run("out = 'b' != 'f'", "true");
    run("out = [] == []", "true");
    run("out = [] != []", "false");
    run("out = [1] == [1]", "true");
    run("out = [1] != [1]", "false");
    run("out = [1] == [1, 2]", "false");
    run("out = [1, 2] == [1]", "false");
    run("out = [1] != [1, 2]", "true");
    run("out = [1, 2] != [1]", "true");
    run(r#"out = ["foo", "bar"] == ["foo", "bar"]"#, "true");
    run(r#"out = ["foo", "bar"] != ["foo", "bar"]"#, "false");
    run(r#"out = ["foo", "bar"] == ["bar", "foo"]"#, "false");
    run(r#"out = ["bar", "foo"] == ["foo", "bar"]"#, "false");
    run(r#"out = ["foo", "bar"] != ["bar", "foo"]"#, "true");
    run(r#"out = ["bar", "foo"] != ["foo", "bar"]"#, "true");
    run("out = {} == {}", "true");
    run("out = {} != {}", "false");
    run("out = {a: 1, b: 2} == {b: 2, a: 1}", "true");
    run("out = {b: 2, a: 1} == {a: 1, b: 2}", "true");
    run("out = {a: 1, b: 2} != {b: 2, a: 1}", "false");
    run("out = {b: 2, a: 1} != {a: 1, b: 2}", "false");
    run("out = {a: 1, b: 2} == {b: 2}", "false");
    run("out = {b: 2} == {a: 1, b: 2}", "false");
    run("out = {a: 1, b: 2} != {b: 2}", "true");
    run("out = {b: 2} != {a: 1, b: 2}", "true");
    run("out = {a: 1, b: {}} == {b: {}, a: 1}", "true");
    run("out = {b: {}, a: 1} == {a: 1, b: {}}", "true");
    run("out = {a: 1, b: {}} != {b: {}, a: 1}", "false");
    run("out = {b: {}, a: 1} != {a: 1, b: {}}", "false");
    run("out = 1 == 1.0", "true");
    run("out = 1.0 == 1", "true");
    run("out = 1 != 1.0", "false");
    run("out = 1.0 != 1", "false");
    run("out = 0 == 0.0", "true");
    run("out = 0.0 == 0", "true");
    run("out = 0 != 0.0", "false");
    run("out = 0.0 != 0", "false");
    run("out = 2 == 2.0", "true");
    run("out = 2.0 == 2", "true");
    run("out = 2 != 2.0", "false");
    run("out = 2.0 != 2", "false");
    run("out = 1 == 1.5", "false");
    run("out = 1.5 == 1", "false");
    run("out = 1 != 1.5", "true");
    run("out = 1.5 != 1", "true");
    run("out = 1 == 0.9", "false");
    run("out = 0.9 == 1", "false");
    run("out = 1 != 0.9", "true");
    run("out = 0.9 != 1", "true");
    run(r#"out = 1 == "foo""#, "false");
    run(r#"out = "foo" == 1"#, "false");
    run(r#"out = 1 != "foo""#, "true");
    run(r#"out = "foo" != 1"#, "true");
    run("out = 1 == true", "false");
    run("out = true == 1", "false");
    run("out = 1 != true", "true");
    run("out = true != 1", "true");
    run(r#"out = [1] == ["1"]"#, "false");
    run(r#"out = ["1"] == [1]"#, "false");
    run(r#"out = [1] != ["1"]"#, "true");
    run(r#"out = ["1"] != [1]"#, "true");
    run(r#"out = [1, [2]] == [1, ["2"]]"#, "false");
    run(r#"out = [1, ["2"]] == [1, [2]]"#, "false");
    run(r#"out = [1, [2]] != [1, ["2"]]"#, "true");
    run(r#"out = [1, ["2"]] != [1, [2]]"#, "true");
    run(r#"out = {a: 1} == {a: "1"}"#, "false");
    run(r#"out = {a: "1"} == {a: 1}"#, "false");
    run(r#"out = {a: 1} != {a: "1"}"#, "true");
    run(r#"out = {a: "1"} != {a: 1}"#, "true");
    run(r#"out = {a: 1, b: {c: 2}} == {a: 1, b: {c: "2"}}"#, "false");
    run(r#"out = {a: 1, b: {c: "2"}} == {a: 1, b: {c: 2}}"#, "false");
    run(r#"out = {a: 1, b: {c: 2}} != {a: 1, b: {c: "2"}}"#, "true");
    run(r#"out = {a: 1, b: {c: "2"}} != {a: 1, b: {c: 2}}"#, "true");
}

#[test]
fn vm_error_info() {
    fails(r#"a := 5
a + "boo""#, r"Runtime Error: invalid operation: int + string
	at test:2:1");
    fails(r"a := 5
b := a(5)", r"Runtime Error: not callable: int
	at test:2:6");
    fails(r"a := 5
b := {}
b.x.y = 10", r"Runtime Error: not index-assignable: undefined
	at test:3:1");
    fails(r#"
a := func() {
	b := 5
	b += "foo"
}
a()"#, r"Runtime Error: invalid operation: int + string
	at test:4:2");
    fails_mods(r#"a := 5
a + import("mod1")"#, &[("mod1", r#"export "foo""#)], r": invalid operation: int + string
	at test:2:1");
    fails_mods(r#"a := import("mod1")()"#, &[("mod1", r#"
export func() {
	b := 5
	return b + "foo"
}"#)], r"Runtime Error: invalid operation: int + string
	at mod1:4:9");
    fails_mods(r#"a := import("mod1")()"#, &[("mod1", r#"export import("mod2")()"#), ("mod2", r#"
export func() {
	b := 5
	return b + "foo"
}"#)], r"Runtime Error: invalid operation: int + string
	at mod2:4:9");
    fails(r#"a := [1, 2, 3]; b := a[:"invalid"];"#, "Runtime Error: invalid slice index type: string");
    fails("a := immutable([4, 5, 6]); b := a[:false];", "Runtime Error: invalid slice index type: bool");
    fails(r#"a := "hello"; b := a[:1.23];"#, "Runtime Error: invalid slice index type: float");
    fails(r#"a := bytes("world"); b := a[:time(1)];"#, "Runtime Error: invalid slice index type: time");
}

#[test]
fn error() {
    run("out = error(1)", "error(1)");
    run("out = error(1).value", "1");
    run(r#"out = error("some error")"#, r#"error("some error")"#);
    run(r#"out = error("some" + " error")"#, r#"error("some error")"#);
    run("out = func() { return error(5) }()", "error(5)");
    run(r#"out = error(error("foo"))"#, r#"error(error("foo"))"#);
    run(r#"out = error("some error").value"#, r#""some error""#);
    run(r#"out = error("some error")["value"]"#, r#""some error""#);
    fails(r#"error("error").err"#, "invalid index on error");
    fails(r#"error("error").value_"#, "invalid index on error");
    fails("error([1,2,3])[1]", "invalid index on error");
}

#[test]
fn float() {
    run("out = 0.0", "f0");
    run("out = -10.3", "f-10.3");
    run("out = 3.2 + 2.0 * -4.0", "f-4.8");
    run("out = 4 + 2.3", "f6.3");
    run("out = 2.3 + 4", "f6.3");
    run("out = +5.0", "f5");
    run("out = -5.0 + +5.0", "f0");
}

#[test]
fn for_in() {
    run("out = 0; for x in [1, 2, 3] { out += x }", "6");
    run("out = 0; for i, x in [1, 2, 3] { out += i + x }", "9");
    run("out = 0; func() { for i, x in [1, 2, 3] { out += i + x } }()", "9");
    run("out = 0; for i, _ in [1, 2, 3] { out += i }", "3");
    run("out = 0; func() { for i, _ in [1, 2, 3] { out += i  } }()", "3");
    run("out = 0; for v in {a:2,b:3,c:4} { out += v }", "9");
    run(r#"out = ""; for k, v in {a:2,b:3,c:4} { out = k; if v==3 { break } }"#, r#""b""#);
    run(r#"out = ""; for k, _ in {a:2} { out += k }"#, r#""a""#);
    run("out = 0; for _, v in {a:2,b:3,c:4} { out += v }", "9");
    run(r#"out = ""; func() { for k, v in {a:2,b:3,c:4} { out = k; if v==3 { break } } }()"#, r#""b""#);
    run(r#"out = ""; for c in "abcde" { out += c }"#, r#""abcde""#);
    run(r#"out = ""; for i, c in "abcde" { if i == 2 { continue }; out += c }"#, r#""abde""#);
}

#[test]
fn for_stmt() {
    run(r"out = 0
for {
out++
if out == 5 {
break
}
}", "5");
    run(r"out = 0
a := 0
for {
a++
if a == 3 { continue }
if a == 5 { break }
out += a
}", "7");
    run(r"out = 0
a := 0
for {
a++
if a == 3 { continue }
out += a
if a == 5 { break }
}", "12");
    run(r"out = 0
for true {
out++
if out == 5 {
break
}
}", "5");
    run(r"a := 0
for true {
a++
if a == 5 {
break
}
}
out = a", "5");
    run(r"out = 0
a := 0
for true {
a++
if a == 3 { continue }
if a == 5 { break }
out += a
}", "7");
    run(r"out = 0
a := 0
for true {
a++
if a == 3 { continue }
out += a
if a == 5 { break }
}", "12");
    run(r"out = 0
func() {
for true {
out++
if out == 5 {
return
}
}
}()", "5");
    run(r"out = 0
for a:=1; a<=10; a++ {
out += a
}", "55");
    run(r"out = 0
for a:=1; a<=3; a++ {
for b:=3; b<=6; b++ {
out += b
}
}", "54");
    run(r"out = 0
func() {
for {
out++
if out == 5 {
break
}
}
}()", "5");
    run(r"out = 0
func() {
for true {
out++
if out == 5 {
break
}
}
}()", "5");
    run(r"out = func() {
a := 0
for {
a++
if a == 5 {
break
}
}
return a
}()", "5");
    run(r"out = func() {
a := 0
for true {
a++
if a== 5 {
break
}
}
return a
}()", "5");
    run(r"out = func() {
a := 0
func() {
for {
a++
if a == 5 {
break
}
}
}()
return a
}()", "5");
    run(r"out = func() {
a := 0
func() {
for true {
a++
if a == 5 {
break
}
}
}()
return a
}()", "5");
    run(r"out = func() {
sum := 0
for a:=1; a<=10; a++ {
sum += a
}
return sum
}()", "55");
    run(r"out = func() {
sum := 0
for a:=1; a<=4; a++ {
for b:=3; b<=5; b++ {
sum += b
}
}
return sum
}()", "48");
    run(r"a := 1
for ; a<=10; a++ {
if a == 5 {
break
}
}
out = a", "5");
    run(r"out = 0
for a:=1; a<=10; a++ {
if a == 3 {
continue
}
out += a
if a == 5 {
break
}
}", "12");
    run(r"out = 0
for a:=1; a<=10; {
if a == 3 {
a++
continue
}
out += a
if a == 5 {
break
}
a++
}", "12");
}

#[test]
fn function() {
    run("f1 := func() {}; out = f1();", "undefined");
    run("f1 := func() {}; f2 := func() { return f1(); }; f1(); out = f2();", "undefined");
    run("f := func(x) { x; }; out = f(5);", "undefined");
    run("f := func(...x) { return x; }; out = f(1,2,3);", "[1, 2, 3]");
    run("f := func(a, b, ...x) { return [a, b, x]; }; out = f(8,9,1,2,3);", "[8, 9, [1, 2, 3]]");
    run(r#"f := func(v) { x := 2; return func(a, ...b){ return [a, b, v+x]}; }; out = f(5)("a", "b");"#, r#"["a", ["b"], 7]"#);
    run("f := func(...x) { return x; }; out = f();", "[]");
    run("f := func(a, b, ...x) { return [a, b, x]; }; out = f(8, 9);", "[8, 9, []]");
    run(r#"f := func(v) { x := 2; return func(a, ...b){ return [a, b, v+x]}; }; out = f(5)("a");"#, r#"["a", [], 7]"#);
    fails("f := func(a, b, ...x) { return [a, b, x]; }; f();", r"Runtime Error: wrong number of arguments: want>=2, got=0
	at test:1:46");
    fails("f := func(a, b, ...x) { return [a, b, x]; }; f(1);", r"Runtime Error: wrong number of arguments: want>=2, got=1
	at test:1:46");
    run("f := func(x) { return x; }; out = f(5);", "5");
    run("f := func(x) { return x * 2; }; out = f(5);", "10");
    run("f := func(x, y) { return x + y; }; out = f(5, 5);", "10");
    run("f := func(x, y) { return x + y; }; out = f(5 + 5, f(5, 5));", "20");
    run("out = func(x) { return x; }(5)", "5");
    run("x := 10; f := func(x) { return x; }; f(5); out = x;", "10");
    run(r"f2 := func(a) {
f1 := func(a) {
return a * 2;
};
return f1(a) * 3;
};
out = f2(10);", "60");
    run(r"f1 := func(f) {
a := [undefined]
a[0] = func() { return f(a) }
return a[0]()
}
out = f1(func(a) { return 2 })", "2");
    run(r"newAdder := func(x) {
return func(y) { return x + y };
};
add2 := newAdder(2);
out = add2(5);", "7");
    run(r"m := {a: 1}
for k,v in m {
func(){
out = k
}()
}", r#""a""#);
    run(r"m := {a: 1}
for k,v in m {
func(){
out = v
}()
}", "1");
    run(r"add := func(a, b) { return a + b };
sub := func(a, b) { return a - b };
applyFunc := func(a, b, f) { return f(a, b) };
out = applyFunc(applyFunc(2, 2, add), 3, sub);", "1");
    run("f1 := func() { return 5 + 10; }; out = f1();", "15");
    run("f1 := func() { return 1 }; f2 := func() { return 2 }; out = f1() + f2()", "3");
    run("f1 := func() { return 1 }; f2 := func() { return f1() + 2 }; f3 := func() { return f2() + 3 }; out = f3()", "6");
    run("f1 := func() { return 99; 100 }; out = f1();", "99");
    run("f1 := func() { return 99; return 100 }; out = f1();", "99");
    run("f1 := func() { return 33; }; f2 := func() { return f1 }; out = f2()();", "33");
    run("one := func() { one = 1; return one }; out = one()", "1");
    run("three := func() { one := 1; two := 2; return one + two }; out = three()", "3");
    run("three := func() { one := 1; two := 2; return one + two }; seven := func() { three := 3; four := 4; return three + four }; out = three() + seven()", "10");
    run(r"foo1 := func() {
foo := 50
return foo
}
foo2 := func() {
foo := 100
return foo
}
out = foo1() + foo2()", "150");
    run(r"g := 50;
minusOne := func() {
n := 1;
return g - n;
};
minusTwo := func() {
n := 2;
return g - n;
};
out = minusOne() + minusTwo()", "97");
    run(r"f1 := func() {
f2 := func() { return 1; }
return f2
};
out = f1()()", "1");
    run(r"f1 := func(a) { return a; };
out = f1(4)", "4");
    run(r"f1 := func(a, b) { return a + b; };
out = f1(1, 2)", "3");
    run(r"sum := func(a, b) {
c := a + b;
return c;
};
out = sum(1, 2);", "3");
    run(r"sum := func(a, b) {
c := a + b;
return c;
};
out = sum(1, 2) + sum(3, 4);", "10");
    run(r"sum := func(a, b) {
c := a + b
return c
};
outer := func() {
return sum(1, 2) + sum(3, 4)
};
out = outer();", "10");
    run(r"g := 10;
sum := func(a, b) {
c := a + b;
return c + g;
}
outer := func() {
return sum(1, 2) + sum(3, 4) + g;
}
out = outer() + g", "50");
    fails("func() { return 1; }(1)", "wrong number of arguments");
    fails("func(a) { return a; }()", "wrong number of arguments");
    fails("func(a, b) { return a + b; }(1)", "wrong number of arguments");
    run(r"f1 := func(a) {
return func() { return a; };
};
f2 := f1(99);
out = f2()", "99");
    run(r"f1 := func(a, b) {
return func(c) { return a + b + c };
};
f2 := f1(1, 2);
out = f2(8);", "11");
    run(r"f1 := func(a, b) {
c := a + b;
return func(d) { return c + d };
};
f2 := f1(1, 2);
out = f2(8);", "11");
    run(r"f1 := func(a, b) {
c := a + b;
return func(d) {
e := d + c;
return func(f) { return e + f };
}
};
f2 := f1(1, 2);
f3 := f2(3);
out = f3(8);", "14");
    run(r"a := 1;
f1 := func(b) {
return func(c) {
return func(d) { return a + b + c + d }
};
};
f2 := f1(2);
f3 := f2(3);
out = f3(8);", "14");
    run(r"f1 := func(a, b) {
one := func() { return a; };
two := func() { return b; };
return func() { return one() + two(); }
};
f2 := f1(9, 90);
out = f2();", "99");
    run(r"fib := func(x) {
if x == 0 {
return 0
} else if x == 1 {
return 1
} else {
return fib(x-1) + fib(x-2)
}
}
out = fib(15)", "610");
    run(r"out = func() {
sum := func(x) {
return x == 0 ? 0 : x + sum(x-1)
}
return sum(5)
}()", "15");
    fails("return 5", "return not allowed outside function");
    run(r"func() {
a := 10
func() {
b := 5
if true {
out = a + 5
}
}()
}()", "15");
    run(r"func() {
a := 10
b := func() { return 5 }
func() {
if b() {
out = a + b()
}
}()
}()", "15");
    run(r"func() {
a := 10
func() {
b := func() { return 5 }
func() {
if true {
out = a + b()
}
}()
}()
}()", "15");
    run("out = func() {}()", "undefined");
    run("out = func(v) { if v { return true } }(1)", "true");
    run("out = func(v) { if v { return true } }(0)", "undefined");
    run("out = func(v) { if v { } else { return true } }(1)", "undefined");
    run("out = func(v) { if v { return } }(1)", "undefined");
    run("out = func(v) { if v { return } }(0)", "undefined");
    run("out = func(v) { if v { } else { return } }(1)", "undefined");
    run("out = func(v) { for ;;v++ { if v == 3 { return true } } }(1)", "true");
    run("out = func(v) { for ;;v++ { if v == 3 { break } } }(1)", "undefined");
    run(r"f := func() { return 2 }
out = (func() {
f := f()
return f
})()", "2");
}

#[test]
fn blocks_in_global_scope() {
    run(r"f := undefined
if true {
a := 1
f = func() {
a = 2
}
}
b := 3
f()
out = b", "3");
    run(r"func() {
f := undefined
if true {
a := 10
f = func() {
a = 20
}
}
b := 5
f()
out = b
}()", "5");
    run(r"f := undefined
if true {
a := 1
b := 2
f = func() {
a = 3
b = 4
}
}
c := 5
d := 6
f()
out = c + d", "11");
    run(r"fn := undefined
if true {
a := 1
b := 2
if true {
c := 3
d := 4
fn = func() {
a = 5
b = 6
c = 7
d = 8
}
}
}
e := 9
f := 10
fn()
out = e + f", "19");
    run(r"out = 0
func() {
for x in [1, 2, 3] {
out += x
}
}()", "6");
    run(r"out = 0
for x in [1, 2, 3] {
out += x
}", "6");
}

#[test]
fn if_stmt() {
    run("if (true) { out = 10 }", "10");
    run("if (false) { out = 10 }", "undefined");
    run("if (false) { out = 10 } else { out = 20 }", "20");
    run("if (1) { out = 10 }", "10");
    run("if (0) { out = 10 } else { out = 20 }", "20");
    run("if (1 < 2) { out = 10 }", "10");
    run("if (1 > 2) { out = 10 }", "undefined");
    run("if (1 < 2) { out = 10 } else { out = 20 }", "10");
    run("if (1 > 2) { out = 10 } else { out = 20 }", "20");
    run("if (1 < 2) { out = 10 } else if (1 > 2) { out = 20 } else { out = 30 }", "10");
    run("if (1 > 2) { out = 10 } else if (1 < 2) { out = 20 } else { out = 30 }", "20");
    run("if (1 > 2) { out = 10 } else if (1 == 2) { out = 20 } else { out = 30 }", "30");
    run("if (1 > 2) { out = 10 } else if (1 == 2) { out = 20 } else if (1 < 2) { out = 30 } else { out = 40 }", "30");
    run("if (1 > 2) { out = 10 } else if (1 < 2) { out = 20; out = 21; out = 22 } else { out = 30 }", "22");
    run("if (1 > 2) { out = 10 } else if (1 == 2) { out = 20 } else { out = 30; out = 31; out = 32}", "32");
    run("if (1 > 2) { out = 10 } else if (1 < 2) { if (1 == 2) { out = 21 } else { out = 22 } } else { out = 30 }", "22");
    run("if (1 > 2) { out = 10 } else if (1 < 2) { if (1 == 2) { out = 21 } else if (2 == 3) { out = 22 } else { out = 23 } } else { out = 30 }", "23");
    run("if (1 > 2) { out = 10 } else if (1 == 2) { if (1 == 2) { out = 21 } else if (2 == 3) { out = 22 } else { out = 23 } } else { out = 30 }", "30");
    run("if (1 > 2) { out = 10 } else if (1 == 2) { out = 20 } else { if (1 == 2) { out = 31 } else if (2 == 3) { out = 32 } else { out = 33 } }", "33");
    run("if a:=0; a<1 { out = 10 }", "10");
    run("a:=0; if a++; a==1 { out = 10 }", "10");
    run(r"func() {
a := 1
if a++; a > 1 {
out = a
}
}()", "2");
    run(r"func() {
a := 1
if a++; a == 1 {
out = 10
} else {
out = 20
}
}()", "20");
    run(r"func() {
a := 1
func() {
if a++; a > 1 {
a++
}
}()
out = a
}()", "3");
    run("a := 1; if a; a { out = a }", "1");
    run("a := 1; if a + 4; a { out = a }", "1");
    run(r"out = func() {
if false { return 1 }
a := undefined
a = 2
if !a {
b := func() {
return is_callable(a) ? a(8) : a
}()
if is_error(b) {
return b
} else if !is_undefined(b) {
return immutable(b)
}
}
a = 3
if a {
b := func() {
return is_callable(a) ? a(9) : a
}()
if is_error(b) {
return b
} else if !is_undefined(b) {
return immutable(b)
}
}
return a
}()", "3");
}

#[test]
fn immutable() {
    run("a := immutable(1); out = a", "1");
    run("a := 5; b := immutable(a); out = b", "5");
    run("a := immutable(1); a = 5; out = a", "5");
    fails("a := immutable([1, 2, 3]); a[1] = 5", "not index-assignable");
    fails(r#"a := immutable(["foo", [1,2,3]]); a[1] = "bar""#, "not index-assignable");
    run(r#"a := immutable(["foo", [1,2,3]]); a[1][1] = "bar"; out = a"#, r#"i["foo", [1, "bar", 3]]"#);
    fails(r#"a := immutable(["foo", immutable([1,2,3])]); a[1][1] = "bar""#, "not index-assignable");
    fails(r#"a := ["foo", immutable([1,2,3])]; a[1][1] = "bar""#, "not index-assignable");
    run("a := immutable([1,2,3]); b := copy(a); b[1] = 5; out = b", "[1, 5, 3]");
    run("a := immutable([1,2,3]); b := copy(a); b[1] = 5; out = a", "i[1, 2, 3]");
    run("out = immutable([1,2,3]) == [1,2,3]", "true");
    run("out = immutable([1,2,3]) == immutable([1,2,3])", "true");
    run("out = [1,2,3] == immutable([1,2,3])", "true");
    run("out = immutable([1,2,3]) == [1,2]", "false");
    run("out = immutable([1,2,3]) == immutable([1,2])", "false");
    run("out = [1,2,3] == immutable([1,2])", "false");
    run("out = immutable([1, 2, 3, 4])[1]", "2");
    run("out = immutable([1, 2, 3, 4])[1:3]", "[2, 3]");
    run("a := immutable([1,2,3]); a = 5; out = a", "5");
    run("a := immutable([1, 2, 3]); out = a[5]", "undefined");
    fails("a := immutable({b: 1, c: 2}); a.b = 5", "not index-assignable");
    fails(r#"a := immutable({b: 1, c: 2}); a["b"] = "bar""#, "not index-assignable");
    run(r#"a := immutable({b: 1, c: [1,2,3]}); a.c[1] = "bar"; out = a"#, r#"i{"b": 1, "c": [1, "bar", 3]}"#);
    fails(r#"a := immutable({b: 1, c: immutable([1,2,3])}); a.c[1] = "bar""#, "not index-assignable");
    fails(r#"a := {b: 1, c: immutable([1,2,3])}; a.c[1] = "bar""#, "not index-assignable");
    run("out = immutable({a:1,b:2}) == {a:1,b:2}", "true");
    run("out = immutable({a:1,b:2}) == immutable({a:1,b:2})", "true");
    run("out = {a:1,b:2} == immutable({a:1,b:2})", "true");
    run("out = immutable({a:1,b:2}) == {a:1,b:3}", "false");
    run("out = immutable({a:1,b:2}) == immutable({a:1,b:3})", "false");
    run("out = {a:1,b:2} == immutable({a:1,b:3})", "false");
    run("out = immutable({a:1,b:2}).b", "2");
    run(r#"out = immutable({a:1,b:2})["b"]"#, "2");
    run("a := immutable({a:1,b:2}); a = 5; out = 5", "5");
    run("a := immutable({a:1,b:2}); out = a.c", "undefined");
    run(r#"a := immutable({b: 5, c: "foo"}); out = a.b"#, "5");
    fails(r#"a := immutable({b: 5, c: "foo"}); a.b = 10"#, "not index-assignable");
}

#[test]
fn inc_dec() {
    run("out = 0; out++", "1");
    run("out = 0; out--", "-1");
    run("a := 0; a++; out = a", "1");
    run("a := 0; a++; a--; out = a", "0");
    run(r#"a := "foo"; a++; out = a"#, r#""foo1""#);
    fails(r#"a := "foo"; a--"#, "invalid operation");
    fails("a++", "unresolved reference");
    fails("a--", "unresolved reference");
    fails("4++", "unresolved reference");
}

#[test]
fn integer() {
    run("out = 5", "5");
    run("out = 10", "10");
    run("out = -5", "-5");
    run("out = -10", "-10");
    run("out = 5 + 5 + 5 + 5 - 10", "10");
    run("out = 2 * 2 * 2 * 2 * 2", "32");
    run("out = -50 + 100 + -50", "0");
    run("out = 5 * 2 + 10", "20");
    run("out = 5 + 2 * 10", "25");
    run("out = 20 + 2 * -10", "0");
    run("out = 50 / 2 * 2 + 10", "60");
    run("out = 2 * (5 + 10)", "30");
    run("out = 3 * 3 * 3 + 10", "37");
    run("out = 3 * (3 * 3) + 10", "37");
    run("out = (5 + 10 * 2 + 15 /3) * 2 + -10", "50");
    run("out = 5 % 3", "2");
    run("out = 5 % 3 + 4", "6");
    run("out = +5", "5");
    run("out = +5 + -5", "0");
    run("out = 9 + '0'", "c57");
    run("out = '9' - 5", "c52");
}

#[test]
fn logical() {
    run("out = true && true", "true");
    run("out = true && false", "false");
    run("out = false && true", "false");
    run("out = false && false", "false");
    run("out = !true && true", "false");
    run("out = !true && false", "false");
    run("out = !false && true", "true");
    run("out = !false && false", "false");
    run("out = true || true", "true");
    run("out = true || false", "true");
    run("out = false || true", "true");
    run("out = false || false", "false");
    run("out = !true || true", "true");
    run("out = !true || false", "false");
    run("out = !false || true", "true");
    run("out = !false || false", "true");
    run("out = 1 && 2", "2");
    run("out = 1 || 2", "1");
    run("out = 1 && 0", "0");
    run("out = 1 || 0", "1");
    run("out = 1 && (0 || 2)", "2");
    run("out = 0 || (0 || 2)", "2");
    run("out = 0 || (0 && 2)", "0");
    run("out = 0 || (2 && 0)", "0");
    run("t:=func() {out = 3; return true}; f:=func() {out = 7; return false}; t() && f()", "7");
    run("t:=func() {out = 3; return true}; f:=func() {out = 7; return false}; f() && t()", "7");
    run("t:=func() {out = 3; return true}; f:=func() {out = 7; return false}; f() || t()", "3");
    run("t:=func() {out = 3; return true}; f:=func() {out = 7; return false}; t() || f()", "3");
    run("t:=func() {out = 3; return true}; f:=func() {out = 7; return false}; !t() && f()", "3");
    run("t:=func() {out = 3; return true}; f:=func() {out = 7; return false}; !f() && t()", "3");
    run("t:=func() {out = 3; return true}; f:=func() {out = 7; return false}; !f() || t()", "7");
    run("t:=func() {out = 3; return true}; f:=func() {out = 7; return false}; !t() || f()", "7");
}

#[test]
fn map_stmt() {
    run(r"out = {
one: 10 - 9,
two: 1 + 1,
three: 6 / 2
}", r#"{"one": 1, "three": 3, "two": 2}"#);
    run(r#"out = {
"one": 10 - 9,
"two": 1 + 1,
"three": 6 / 2
}"#, r#"{"one": 1, "three": 3, "two": 2}"#);
    run(r#"out = {foo: 5}["foo"]"#, "5");
    run(r#"out = {foo: 5}["bar"]"#, "undefined");
    run(r#"key := "foo"; out = {foo: 5}[key]"#, "5");
    run(r#"out = {}["foo"]"#, "undefined");
    run(r#"m := {
foo: func(x) {
return x * 2
}
}
out = m["foo"](2) + m["foo"](3)"#, "10");
    run(r#"m1 := {k1: 1, k2: "foo"}; m2 := m1; m1.k1 = 5; out = m2.k1"#, "5");
    run(r#"m1 := {k1: 1, k2: "foo"}; m2 := m1; m2.k1 = 3; out = m1.k1"#, "3");
    run(r#"func() { m1 := {k1: 1, k2: "foo"}; m2 := m1; m1.k1 = 5; out = m2.k1 }()"#, "5");
    run(r#"func() { m1 := {k1: 1, k2: "foo"}; m2 := m1; m2.k1 = 3; out = m1.k1 }()"#, "3");
}
