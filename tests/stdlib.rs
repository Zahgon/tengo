//! Carried over from stdlib/*_test.go: each Go call of a module function becomes the same call
//! from a script.

mod common;

use common::*;

#[test]
fn base64() {
    call("base64", r#"encode(bytes("\x06\xacv\x1b\x1dj\xfa\x9d\xb1\xa0"))"#, r#""Bqx2Gx1q+p2xoA==""#);
    call("base64", r#"decode("Bqx2Gx1q+p2xoA==")"#, "b06ac761b1d6afa9db1a0");
    call("base64", r#"url_encode(bytes("\x06\xacv\x1b\x1dj\xfa\x9d\xb1\xa0"))"#, r#""Bqx2Gx1q-p2xoA==""#);
    call("base64", r#"url_decode("Bqx2Gx1q-p2xoA==")"#, "b06ac761b1d6afa9db1a0");
    call("base64", r#"raw_encode(bytes("\x06\xacv\x1b\x1dj\xfa\x9d\xb1\xa0"))"#, r#""Bqx2Gx1q+p2xoA""#);
    call("base64", r#"raw_decode("Bqx2Gx1q+p2xoA")"#, "b06ac761b1d6afa9db1a0");
    call("base64", r#"raw_url_encode(bytes("\x06\xacv\x1b\x1dj\xfa\x9d\xb1\xa0"))"#, r#""Bqx2Gx1q-p2xoA""#);
    call("base64", r#"raw_url_decode("Bqx2Gx1q-p2xoA")"#, "b06ac761b1d6afa9db1a0");
}

#[test]
fn fmt_sprintf() {
    call("fmt", r#"sprintf("")"#, r#""""#);
    call("fmt", r#"sprintf("foo")"#, r#""foo""#);
    call("fmt", r#"sprintf("foo %d %v %s", 1, 2, "bar")"#, r#""foo 1 2 bar""#);
    call("fmt", r#"sprintf("foo %v", [1, "bar", true])"#, r#""foo [1, \"bar\", true]""#);
    call("fmt", r#"sprintf("foo %v %d", [1, "bar", true], 19)"#, r#""foo [1, \"bar\", true] 19""#);
}

#[test]
fn hex() {
    call("hex", r#"encode(bytes("\x06\xacv\x1b\x1dj\xfa\x9d\xb1\xa0"))"#, r#""06ac761b1d6afa9db1a0""#);
    call("hex", r#"decode("06ac761b1d6afa9db1a0")"#, "b06ac761b1d6afa9db1a0");
}

#[test]
fn json() {
    call("json", "encode(5)", "b35");
    call("json", r#"encode("foobar")"#, "b22666f6f62617222");
    call("json", r#"encode({"foo": 5})"#, "b7b22666f6f223a357d");
    call("json", "encode([1, 2, 3])", "b5b312c322c335d");
    call("json", r#"encode({"foo": "bar"})"#, "b7b22666f6f223a22626172227d");
    call("json", r#"encode({"foo": 1.8})"#, "b7b22666f6f223a312e387d");
    call("json", r#"encode({"foo": true})"#, "b7b22666f6f223a747275657d");
    call("json", r#"encode({"foo": '8'})"#, "b7b22666f6f223a35367d");
    call("json", r#"encode({"foo": bytes("foo")})"#, "b7b22666f6f223a225a6d3976227d");
    call("json", r#"encode({"foo": ["bar", 1, 1.8, '8', true]})"#, "b7b22666f6f223a5b22626172222c312c312e382c35362c747275655d7d");
    call("json", r#"encode({"foo": [["bar", 1], ["bar", 1]]})"#, "b7b22666f6f223a5b5b22626172222c315d2c5b22626172222c315d5d7d");
    call("json", r#"encode({"foo": {"string": "bar"}})"#, "b7b22666f6f223a7b22737472696e67223a22626172227d7d");
    call("json", r#"encode({"foo": {"map1": {"string": "bar"}}})"#, "b7b22666f6f223a7b226d617031223a7b22737472696e67223a22626172227d7d7d");
    call("json", r#"encode([["bar", 1], ["bar", 1]])"#, "b5b5b22626172222c315d2c5b22626172222c315d5d");
    call("json", r#"decode("5")"#, "5");
    call("json", r#"decode("\"foo\"")"#, r#""foo""#);
    call("json", r#"decode("[1,2,3,\"bar\"]")"#, r#"[1, 2, 3, "bar"]"#);
    call("json", r#"decode("{\"foo\":5}")"#, r#"{"foo": 5}"#);
    call("json", r#"decode("{\"foo\":2.5}")"#, r#"{"foo": f2.5}"#);
    call("json", r#"decode("{\"foo\":true}")"#, r#"{"foo": true}"#);
    call("json", r#"decode("{\"foo\":\"bar\"}")"#, r#"{"foo": "bar"}"#);
    call("json", r#"decode("{\"foo\":[1,2,3,\"bar\"]}")"#, r#"{"foo": [1, 2, 3, "bar"]}"#);
    call("json", r#"indent(bytes("{\"foo\":[\"bar\",1,1.8,56,true]}"), "", "  ")"#, "b7b0a202022666f6f223a205b0a2020202022626172222c0a20202020312c0a20202020312e382c0a2020202035362c0a20202020747275650a20205d0a7d");
    call("json", r#"html_escape(bytes("{\"M\":\"<html>foo &\u2028 \u2029</html>\"}"))"#, "b7b224d223a225c753030336368746d6c5c7530303365666f6f205c75303032365c7532303238205c75323032395c75303033632f68746d6c5c7530303365227d");
}

#[test]
fn text_re_alternation() {
    call("text", r#"re_find("([a-zA-Z])|([0-9])", "a")"#, r#"[[i{"begin": 0, "end": 1, "text": "a"}, i{"begin": 0, "end": 1, "text": "a"}]]"#);
    call("text", r#"re_find("([a-zA-Z])|([0-9])", "5")"#, r#"[[i{"begin": 0, "end": 1, "text": "5"}, i{"begin": 0, "end": 1, "text": "5"}]]"#);
    call("text", r#"re_find("([a-zA-Z])|([0-9])", "")"#, "undefined");
    call("text", r#"re_find("([a-zA-Z])|([0-9])", "!")"#, "undefined");
    call("text", r#"re_find("(?:([a-zA-Z])|([0-9]))+", "a5b")"#, r#"[[i{"begin": 0, "end": 3, "text": "a5b"}, i{"begin": 2, "end": 3, "text": "b"}, i{"begin": 1, "end": 2, "text": "5"}]]"#);
    call("text", r#"re_find("(foo)|(bar)|(baz)", "foo")"#, r#"[[i{"begin": 0, "end": 3, "text": "foo"}, i{"begin": 0, "end": 3, "text": "foo"}]]"#);
    call("text", r#"re_find("((cat)|(dog))((run)|(walk))", "catrun")"#, r#"[[i{"begin": 0, "end": 6, "text": "catrun"}, i{"begin": 0, "end": 3, "text": "cat"}, i{"begin": 0, "end": 3, "text": "cat"}, i{"begin": 3, "end": 6, "text": "run"}, i{"begin": 3, "end": 6, "text": "run"}]]"#);
}

#[test]
fn text_re() {
    call("text", r#"re_match("abc", "")"#, "false");
    call("text", r#"re_compile("abc").match("")"#, "false");
    call("text", r#"re_match("abc", "abc")"#, "true");
    call("text", r#"re_compile("abc").match("abc")"#, "true");
    call("text", r#"re_match("a", "abc")"#, "true");
    call("text", r#"re_compile("a").match("abc")"#, "true");
    call("text", r#"re_match("b", "abc")"#, "true");
    call("text", r#"re_compile("b").match("abc")"#, "true");
    call("text", r#"re_match("^a", "abc")"#, "true");
    call("text", r#"re_compile("^a").match("abc")"#, "true");
    call("text", r#"re_match("^b", "abc")"#, "false");
    call("text", r#"re_compile("^b").match("abc")"#, "false");
    call("text", r#"re_find("a(b)", "")"#, "undefined");
    call("text", r#"re_compile("a(b)").find("")"#, "undefined");
    call("text", r#"re_find("a(b)", "ab")"#, r#"[[i{"begin": 0, "end": 2, "text": "ab"}, i{"begin": 1, "end": 2, "text": "b"}]]"#);
    call("text", r#"re_compile("a(b)").find("ab")"#, r#"[[i{"begin": 0, "end": 2, "text": "ab"}, i{"begin": 1, "end": 2, "text": "b"}]]"#);
    call("text", r#"re_find("a(bc)d", "abcdefgabcd")"#, r#"[[i{"begin": 0, "end": 4, "text": "abcd"}, i{"begin": 1, "end": 3, "text": "bc"}]]"#);
    call("text", r#"re_compile("a(bc)d").find("abcdefgabcd")"#, r#"[[i{"begin": 0, "end": 4, "text": "abcd"}, i{"begin": 1, "end": 3, "text": "bc"}]]"#);
    call("text", r#"re_find("(a)b(c)d", "abcdefgabcd")"#, r#"[[i{"begin": 0, "end": 4, "text": "abcd"}, i{"begin": 0, "end": 1, "text": "a"}, i{"begin": 2, "end": 3, "text": "c"}]]"#);
    call("text", r#"re_compile("(a)b(c)d").find("abcdefgabcd")"#, r#"[[i{"begin": 0, "end": 4, "text": "abcd"}, i{"begin": 0, "end": 1, "text": "a"}, i{"begin": 2, "end": 3, "text": "c"}]]"#);
    call("text", r#"re_find("a(b)", "", -1)"#, "undefined");
    call("text", r#"re_compile("a(b)").find("", -1)"#, "undefined");
    call("text", r#"re_find("a(b)", "ab", -1)"#, r#"[[i{"begin": 0, "end": 2, "text": "ab"}, i{"begin": 1, "end": 2, "text": "b"}]]"#);
    call("text", r#"re_compile("a(b)").find("ab", -1)"#, r#"[[i{"begin": 0, "end": 2, "text": "ab"}, i{"begin": 1, "end": 2, "text": "b"}]]"#);
    call("text", r#"re_find("a(bc)d", "abcdefgabcd", -1)"#, r#"[[i{"begin": 0, "end": 4, "text": "abcd"}, i{"begin": 1, "end": 3, "text": "bc"}], [i{"begin": 7, "end": 11, "text": "abcd"}, i{"begin": 8, "end": 10, "text": "bc"}]]"#);
    call("text", r#"re_compile("a(bc)d").find("abcdefgabcd", -1)"#, r#"[[i{"begin": 0, "end": 4, "text": "abcd"}, i{"begin": 1, "end": 3, "text": "bc"}], [i{"begin": 7, "end": 11, "text": "abcd"}, i{"begin": 8, "end": 10, "text": "bc"}]]"#);
    call("text", r#"re_find("(a)b(c)d", "abcdefgabcd", -1)"#, r#"[[i{"begin": 0, "end": 4, "text": "abcd"}, i{"begin": 0, "end": 1, "text": "a"}, i{"begin": 2, "end": 3, "text": "c"}], [i{"begin": 7, "end": 11, "text": "abcd"}, i{"begin": 7, "end": 8, "text": "a"}, i{"begin": 9, "end": 10, "text": "c"}]]"#);
    call("text", r#"re_compile("(a)b(c)d").find("abcdefgabcd", -1)"#, r#"[[i{"begin": 0, "end": 4, "text": "abcd"}, i{"begin": 0, "end": 1, "text": "a"}, i{"begin": 2, "end": 3, "text": "c"}], [i{"begin": 7, "end": 11, "text": "abcd"}, i{"begin": 7, "end": 8, "text": "a"}, i{"begin": 9, "end": 10, "text": "c"}]]"#);
    call("text", r#"re_find("(a)b(c)d", "abcdefgabcd", 0)"#, "undefined");
    call("text", r#"re_compile("(a)b(c)d").find("abcdefgabcd", 0)"#, "undefined");
    call("text", r#"re_find("(a)b(c)d", "abcdefgabcd", 1)"#, r#"[[i{"begin": 0, "end": 4, "text": "abcd"}, i{"begin": 0, "end": 1, "text": "a"}, i{"begin": 2, "end": 3, "text": "c"}]]"#);
    call("text", r#"re_compile("(a)b(c)d").find("abcdefgabcd", 1)"#, r#"[[i{"begin": 0, "end": 4, "text": "abcd"}, i{"begin": 0, "end": 1, "text": "a"}, i{"begin": 2, "end": 3, "text": "c"}]]"#);
    call("text", r#"re_replace("a", "", "b")"#, r#""""#);
    call("text", r#"re_compile("a").replace("", "b")"#, r#""""#);
    call("text", r#"re_replace("a", "a", "b")"#, r#""b""#);
    call("text", r#"re_compile("a").replace("a", "b")"#, r#""b""#);
    call("text", r#"re_replace("a", "acac", "b")"#, r#""bcbc""#);
    call("text", r#"re_compile("a").replace("acac", "b")"#, r#""bcbc""#);
    call("text", r#"re_replace("b", "acac", "x")"#, r#""acac""#);
    call("text", r#"re_compile("b").replace("acac", "x")"#, r#""acac""#);
    call("text", r#"re_replace("a", "acac", "123")"#, r#""123c123c""#);
    call("text", r#"re_compile("a").replace("acac", "123")"#, r#""123c123c""#);
    call("text", r#"re_replace("ac", "acac", "99")"#, r#""9999""#);
    call("text", r#"re_compile("ac").replace("acac", "99")"#, r#""9999""#);
    call("text", r#"re_replace("ac$", "acac", "foo")"#, r#""acfoo""#);
    call("text", r#"re_compile("ac$").replace("acac", "foo")"#, r#""acfoo""#);
    call("text", r#"re_replace("a(b)", "ababab", "$1")"#, r#""bbb""#);
    call("text", r#"re_compile("a(b)").replace("ababab", "$1")"#, r#""bbb""#);
    call("text", r#"re_replace("a(b)(c)", "abcabcabc", "$2$1")"#, r#""cbcbcb""#);
    call("text", r#"re_compile("a(b)(c)").replace("abcabcabc", "$2$1")"#, r#""cbcbcb""#);
    call("text", r#"re_replace("(a(b)c)", "abcabcabc", "$1$2")"#, r#""abcbabcbabcb""#);
    call("text", r#"re_compile("(a(b)c)").replace("abcabcabc", "$1$2")"#, r#""abcbabcbabcb""#);
    call("text", r#"re_replace("(일(2)삼)", "일2삼12삼일23", "$1$2")"#, r#""일2삼212삼일23""#);
    call("text", r#"re_compile("(일(2)삼)").replace("일2삼12삼일23", "$1$2")"#, r#""일2삼212삼일23""#);
    call("text", r#"re_replace("((일)(2)3)", "일23\n일이3\n일23", "$1$2$3")"#, r#""일23일2\n일이3\n일23일2""#);
    call("text", r#"re_compile("((일)(2)3)").replace("일23\n일이3\n일23", "$1$2$3")"#, r#""일23일2\n일이3\n일23일2""#);
    call("text", r#"re_replace("(a(b)c)", "abc\nabc\nabc", "$1$2")"#, r#""abcb\nabcb\nabcb""#);
    call("text", r#"re_compile("(a(b)c)").replace("abc\nabc\nabc", "$1$2")"#, r#""abcb\nabcb\nabcb""#);
    call("text", r#"re_split("a", "")"#, r#"[""]"#);
    call("text", r#"re_compile("a").split("")"#, r#"[""]"#);
    call("text", r#"re_split("a", "abcabc")"#, r#"["", "bc", "bc"]"#);
    call("text", r#"re_compile("a").split("abcabc")"#, r#"["", "bc", "bc"]"#);
    call("text", r#"re_split("ab", "abcabc")"#, r#"["", "c", "c"]"#);
    call("text", r#"re_compile("ab").split("abcabc")"#, r#"["", "c", "c"]"#);
    call("text", r#"re_split("^a", "abcabc")"#, r#"["", "bcabc"]"#);
    call("text", r#"re_compile("^a").split("abcabc")"#, r#"["", "bcabc"]"#);
    call("text", r#"re_split("a", "", -1)"#, r#"[""]"#);
    call("text", r#"re_compile("a").split("", -1)"#, r#"[""]"#);
    call("text", r#"re_split("a", "abcabc", -1)"#, r#"["", "bc", "bc"]"#);
    call("text", r#"re_compile("a").split("abcabc", -1)"#, r#"["", "bc", "bc"]"#);
    call("text", r#"re_split("ab", "abcabc", -1)"#, r#"["", "c", "c"]"#);
    call("text", r#"re_compile("ab").split("abcabc", -1)"#, r#"["", "c", "c"]"#);
    call("text", r#"re_split("^a", "abcabc", -1)"#, r#"["", "bcabc"]"#);
    call("text", r#"re_compile("^a").split("abcabc", -1)"#, r#"["", "bcabc"]"#);
    call("text", r#"re_split("a", "abcabc", 0)"#, "[]");
    call("text", r#"re_compile("a").split("abcabc", 0)"#, "[]");
    call("text", r#"re_split("a", "abcabc", 1)"#, r#"["abcabc"]"#);
    call("text", r#"re_compile("a").split("abcabc", 1)"#, r#"["abcabc"]"#);
    call("text", r#"re_split("a", "abcabc", 2)"#, r#"["", "bcabc"]"#);
    call("text", r#"re_compile("a").split("abcabc", 2)"#, r#"["", "bcabc"]"#);
    call("text", r#"re_split("a", "abcabc", 3)"#, r#"["", "bc", "bc"]"#);
    call("text", r#"re_compile("a").split("abcabc", 3)"#, r#"["", "bc", "bc"]"#);
    call("text", r#"re_split("b", "abcabc", 1)"#, r#"["abcabc"]"#);
    call("text", r#"re_compile("b").split("abcabc", 1)"#, r#"["abcabc"]"#);
    call("text", r#"re_split("b", "abcabc", 2)"#, r#"["a", "cabc"]"#);
    call("text", r#"re_compile("b").split("abcabc", 2)"#, r#"["a", "cabc"]"#);
    call("text", r#"re_split("b", "abcabc", 3)"#, r#"["a", "ca", "c"]"#);
    call("text", r#"re_compile("b").split("abcabc", 3)"#, r#"["a", "ca", "c"]"#);
}

#[test]
fn text() {
    call("text", r#"compare("", "")"#, "0");
    call("text", r#"compare("", "a")"#, "-1");
    call("text", r#"compare("a", "")"#, "1");
    call("text", r#"compare("a", "a")"#, "0");
    call("text", r#"compare("a", "b")"#, "-1");
    call("text", r#"compare("b", "a")"#, "1");
    call("text", r#"compare("abcde", "abcde")"#, "0");
    call("text", r#"compare("abcde", "abcdf")"#, "-1");
    call("text", r#"compare("abcdf", "abcde")"#, "1");
    call("text", r#"contains("", "")"#, "true");
    call("text", r#"contains("", "a")"#, "false");
    call("text", r#"contains("a", "")"#, "true");
    call("text", r#"contains("a", "a")"#, "true");
    call("text", r#"contains("abcde", "a")"#, "true");
    call("text", r#"contains("abcde", "abcde")"#, "true");
    call("text", r#"contains("abc", "abcde")"#, "false");
    call("text", r#"contains("ab cd", "bc")"#, "false");
    call("text", r#"replace("", "", "", -1)"#, r#""""#);
    call("text", r#"replace("abcd", "a", "x", -1)"#, r#""xbcd""#);
    call("text", r#"replace("aaaa", "a", "x", -1)"#, r#""xxxx""#);
    call("text", r#"replace("aaaa", "a", "x", 0)"#, r#""aaaa""#);
    call("text", r#"replace("aaaa", "a", "x", 2)"#, r#""xxaa""#);
    call("text", r#"replace("abcd", "bc", "x", -1)"#, r#""axd""#);
    call("text", "format_bool(true)", r#""true""#);
    call("text", "format_bool(false)", r#""false""#);
    call("text", "format_float(-19.84, 'f', -1, 64)", r#""-19.84""#);
    call("text", "format_int(-1984, 10)", r#""-1984""#);
    call("text", "format_int(1984, 8)", r#""3700""#);
    call("text", r#"parse_bool("true")"#, "true");
    call("text", r#"parse_bool("0")"#, "false");
    call("text", r#"parse_float("-19.84", 64)"#, "f-19.84");
    call("text", r#"parse_int("-1984", 10, 64)"#, "-1984");
}

#[test]
fn replace_limit() {
    call("text", r#"replace("123456789012", "1", "x", -1)"#, r#""x234567890x2""#);
    call("text", r#"replace("123456789012", "12", "x", -1)"#, r#""x34567890x""#);
    call("text", r#"replace("123456789012", "012", "xyz", -1)"#, r#""123456789xyz""#);
    call("text", r#"re_replace("1", "123456789012", "x")"#, r#""x234567890x2""#);
    call("text", r#"re_replace("12", "123456789012", "x")"#, r#""x34567890x""#);
    call("text", r#"re_replace("1(2)", "123456789012", "x$1")"#, r#""x234567890x2""#);
    call("text", r#"re_replace("(1)(2)", "123456789012", "$2$1")"#, r#""213456789021""#);
}

#[test]
fn text_repeat() {
    call("text", r#"repeat("1234", "3")"#, r#""123412341234""#);
    call("text", r#"repeat("1", "12")"#, r#""111111111111""#);
}

#[test]
fn substr() {
    call("text", r#"substr("", 0, 0)"#, r#""""#);
    call("text", r#"substr("abcdef", 0, 3)"#, r#""abc""#);
    call("text", r#"substr("abcdef", 0, 6)"#, r#""abcdef""#);
    call("text", r#"substr("abcdef", 0, 10)"#, r#""abcdef""#);
    call("text", r#"substr("abcdef", -10, 10)"#, r#""abcdef""#);
    call("text", r#"substr("abcdef", 0)"#, r#""abcdef""#);
    call("text", r#"substr("abcdef", 3)"#, r#""def""#);
    fails(r#"out = import("text").substr("", 10, 0)"#, "");
    fails(r#"out = import("text").substr("", "10", 0)"#, "");
    fails(r#"out = import("text").substr("", 10, "0")"#, "");
    fails(r#"out = import("text").substr("", "10", "0")"#, "");
    call("text", "substr(0, 0, 1)", r#""0""#);
    call("text", "substr(123, 0, 1)", r#""1""#);
    call("text", "substr(123.456, 4, 7)", r#""456""#);
}

#[test]
fn pad_left() {
    call("text", r#"pad_left("ab", 7, 0)"#, r#""00000ab""#);
    call("text", r#"pad_right("ab", 7, 0)"#, r#""ab00000""#);
    call("text", r#"pad_left("ab", 7, "+-")"#, r#""-+-+-ab""#);
    call("text", r#"pad_right("ab", 7, "+-")"#, r#""ab+-+-+""#);
}

#[test]
fn times() {
    call("times", "sleep(1)", "undefined");
    call("times", r#"parse_duration("1ns")"#, "1");
    call("times", r#"parse_duration("1ms")"#, "1000000");
    call("times", r#"parse_duration("1h")"#, "3600000000000");
    call("times", "duration_hours(1800000000000)", "f0.5");
    call("times", "duration_minutes(1800000000000)", "f30");
    call("times", "duration_nanoseconds(100)", "100");
    call("times", "duration_seconds(1000000)", "f0.001");
    call("times", "duration_string(1800000000000)", r#""30m0s""#);
    call("times", "month_string(1)", r#""January""#);
    call("times", "month_string(12)", r#""December""#);
    call("times", "date(1982, 9, 28, 19, 21, 44, 999)", "t1982-09-28T19:21:44.000000999Z");
    call("times", r#"parse("2006-01-02T15:04:05Z07:00", "1982-09-28T19:21:44+07:00")"#, "t1982-09-28T12:21:44Z");
    call("times", "unix(1234325, 94493)", "t1970-01-15T06:52:05.000094493Z");
    run1(r#"times := import("times"); out = import("times").time_year(times.parse(times.format_rfc3339_nano, "1982-09-28T19:21:44.000000999Z"))"#, "1982");
    run1(r#"times := import("times"); out = import("times").time_month(times.parse(times.format_rfc3339_nano, "1982-09-28T19:21:44.000000999Z"))"#, "9");
    run1(r#"times := import("times"); out = import("times").time_day(times.parse(times.format_rfc3339_nano, "1982-09-28T19:21:44.000000999Z"))"#, "28");
    run1(r#"times := import("times"); out = import("times").time_hour(times.parse(times.format_rfc3339_nano, "1982-09-28T19:21:44.000000999Z"))"#, "19");
    run1(r#"times := import("times"); out = import("times").time_minute(times.parse(times.format_rfc3339_nano, "1982-09-28T19:21:44.000000999Z"))"#, "21");
    run1(r#"times := import("times"); out = import("times").time_second(times.parse(times.format_rfc3339_nano, "1982-09-28T19:21:44.000000999Z"))"#, "44");
    run1(r#"times := import("times"); out = import("times").time_nanosecond(times.parse(times.format_rfc3339_nano, "1982-09-28T19:21:44.000000999Z"))"#, "999");
    run1(r#"times := import("times"); out = import("times").time_unix(times.parse(times.format_rfc3339_nano, "1982-09-28T19:21:44.000000999Z"))"#, "402088904");
    run1(r#"times := import("times"); out = import("times").time_unix_nano(times.parse(times.format_rfc3339_nano, "1982-09-28T19:21:44.000000999Z"))"#, "402088904000000999");
    run1(r#"times := import("times"); out = import("times").time_format(times.parse(times.format_rfc3339_nano, "1982-09-28T19:21:44.000000999Z"), "2006-01-02T15:04:05Z07:00")"#, r#""1982-09-28T19:21:44Z""#);
    run1(r#"times := import("times"); out = import("times").is_zero(times.parse(times.format_rfc3339_nano, "1982-09-28T19:21:44.000000999Z"))"#, "false");
    run1(r#"times := import("times"); out = import("times").is_zero(times.parse(times.format_rfc3339_nano, "0001-01-01T00:00:00Z"))"#, "true");
    run1(r#"times := import("times"); out = import("times").to_local(times.parse(times.format_rfc3339_nano, "1982-09-28T19:21:44.000000999Z"))"#, "t1982-09-28T19:21:44.000000999Z");
    run1(r#"times := import("times"); out = import("times").to_utc(times.parse(times.format_rfc3339_nano, "1982-09-28T19:21:44.000000999Z"))"#, "t1982-09-28T19:21:44.000000999Z");
    run1(r#"times := import("times"); out = import("times").time_location(times.parse(times.format_rfc3339_nano, "1982-09-28T19:21:44.000000999Z"))"#, r#""UTC""#);
    run1(r#"times := import("times"); out = import("times").time_string(times.parse(times.format_rfc3339_nano, "1982-09-28T19:21:44.000000999Z"))"#, r#""1982-09-28 19:21:44.000000999 +0000 UTC""#);
}
