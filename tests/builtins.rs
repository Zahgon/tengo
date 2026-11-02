//! Carried over from builtins_test.go: the delete, splice and range tables, called from a script.

mod common;

use common::*;

#[test]
fn builtin_delete() {
    fails(r#"out = delete("", "")"#, "invalid type for argument 'first' in call to 'builtin-function:delete': expected map, found string");
    fails("out = delete()", "wrong number of arguments");
    run1(r#"out = delete({}, "")"#, "undefined");
    fails("out = delete({}, 0)", "invalid type for argument 'second' in call to 'builtin-function:delete': expected string, found int");
    fails("out = delete({})", "wrong number of arguments");
    run1(r#"out = delete({"key": "value"}, "key1")"#, "undefined");
    run1(r#"t := {"key": "value"}; delete(t, "key1"); out = t"#, r#"{"key": "value"}"#);
    run1(r#"out = delete({"key": "value"}, "key")"#, "undefined");
    run1(r#"t := {"key": "value"}; delete(t, "key"); out = t"#, "{}");
    run1(r#"out = delete({"key1": "value1", "key2": 10}, "key1")"#, "undefined");
    run1(r#"t := {"key1": "value1", "key2": 10}; delete(t, "key1"); out = t"#, r#"{"key2": 10}"#);
}

#[test]
fn builtin_splice() {
    fails("out = splice()", "wrong number of arguments");
    fails("out = splice({})", "invalid type for argument 'first' in call to 'builtin-function:splice': expected array, found map");
    fails(r#"out = splice([], "")"#, "invalid type for argument 'second' in call to 'builtin-function:splice': expected int, found string");
    fails("out = splice([], -1)", "index out of bounds");
    fails(r#"out = splice([], 0, "")"#, "invalid type for argument 'third' in call to 'builtin-function:splice': expected int, found string");
    fails("out = splice([0, 1, 2], 0, -1)", "index out of bounds");
    run1(r#"out = splice([0, 1, 2], 0, 0, "b")"#, "[]");
    run1(r#"t := [0, 1, 2]; splice(t, 0, 0, "b"); out = t"#, r#"["b", 0, 1, 2]"#);
    run1(r#"out = splice([0, 1, 2], 1, 0, "c", "d")"#, "[]");
    run1(r#"t := [0, 1, 2]; splice(t, 1, 0, "c", "d"); out = t"#, r#"[0, "c", "d", 1, 2]"#);
    run1(r#"out = splice([0, 1, 2], 1, 1, "c", "d")"#, "[1]");
    run1(r#"t := [0, 1, 2]; splice(t, 1, 1, "c", "d"); out = t"#, r#"[0, "c", "d", 2]"#);
    run1(r#"out = splice([0, 1, 2], 1, 2, "c", "d")"#, "[1, 2]");
    run1(r#"t := [0, 1, 2]; splice(t, 1, 2, "c", "d"); out = t"#, r#"[0, "c", "d"]"#);
    run1("out = splice([0, 1, 2], 0, 3)", "[0, 1, 2]");
    run1("t := [0, 1, 2]; splice(t, 0, 3); out = t", "[]");
    run1("out = splice([0, 1, 2], 0, 5)", "[0, 1, 2]");
    run1("t := [0, 1, 2]; splice(t, 0, 5); out = t", "[]");
    run1("out = splice([0, 1, 2])", "[0, 1, 2]");
    run1("t := [0, 1, 2]; splice(t); out = t", "[]");
    run1("out = splice([0, 1, 2], 2)", "[2]");
    run1("t := [0, 1, 2]; splice(t, 2); out = t", "[0, 1]");
}

#[test]
fn builtin_range() {
    fails("out = range()", "wrong number of arguments");
    fails("out = range({})", "wrong number of arguments");
    fails(r#"out = range({}, "", "", "")"#, "wrong number of arguments");
    fails(r#"out = range("", "")"#, "invalid type for argument 'start' in call to 'builtin-function:range': expected int, found string");
    fails(r#"out = range(0, "")"#, "invalid type for argument 'stop' in call to 'builtin-function:range': expected int, found string");
    fails(r#"out = range(0, 0, "")"#, "invalid type for argument 'step' in call to 'builtin-function:range': expected int, found string");
    fails("out = range(0, 0, 0)", "range step must be greater than 0");
    fails("out = range(0, 0, -2)", "range step must be greater than 0");
    run1("out = range(0, 0)", "[]");
    run1("out = range(0, 5)", "[0, 1, 2, 3, 4]");
    run1("out = range(0, -5)", "[0, -1, -2, -3, -4]");
    run1("out = range(0, 5, 2)", "[0, 2, 4]");
    run1("out = range(0, -10, 2)", "[0, -2, -4, -6, -8]");
    run1("out = range(-10, 10, 3)", "[-10, -7, -4, -1, 2, 5, 8]");
    run1("out = range(0, 10, 9223372036854775806)", "[0]");
    fails("out = range((-9223372036854775807 - 1), 9223372036854775807)", "exceeding range size limit");
    fails("out = range(0, 10000001)", "exceeding range size limit");
}

#[test]
fn builtin_freeze() {
    run1("out = freeze([1, 2])", "i[1, 2]");
    run1("out = freeze({x: 42})", r#"i{"x": 42}"#);
    run1("a := freeze({x: [1], y: {z: 2}}); out = [is_immutable_array(a.x), is_immutable_map(a.y)]", "[true, true]");
    run1("a := immutable([1, 2]); out = freeze(a)", "i[1, 2]");
    run1("a := immutable({x: 1}); out = freeze(a)", r#"i{"x": 1}"#);
    run1("a := freeze(immutable([[1]])); out = is_immutable_array(a[0])", "true");
}
