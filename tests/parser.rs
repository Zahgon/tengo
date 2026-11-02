//! Carried over from parser/parser_test.go and parser/scanner_test.go: each source parses to the
//! tree the Go test builds, compared through the String form of Go's nodes, or fails to parse.
//! The scanner table is compared token by token with line and column.

use tengo::ast::{BlockStmt, Expr, File, IdentList, Stmt};
use tengo::parser::Parser;
use tengo::scanner::Scanner;
use tengo::source::SourceFileSet;
use tengo::token::Token;

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn exprs(v: &[Expr]) -> Vec<String> {
    v.iter().map(expr).collect()
}

fn idents(l: &IdentList) -> String {
    let last = l.list.len().wrapping_sub(1);
    let names: Vec<String> = l
        .list
        .iter()
        .enumerate()
        .map(|(i, n)| format!("{}{}", if l.var_args && i == last { "..." } else { "" }, text(&n.name)))
        .collect();
    format!("({})", names.join(", "))
}

fn block(b: &BlockStmt) -> String {
    format!("{{{}}}", b.stmts.iter().map(stmt).collect::<Vec<_>>().join("; "))
}

fn opt(e: &Option<Box<Expr>>) -> String {
    e.as_ref().map(|x| expr(x)).unwrap_or_default()
}

fn expr(e: &Expr) -> String {
    match e {
        Expr::ArrayLit { elements, .. } => format!("[{}]", exprs(elements).join(", ")),
        Expr::BadExpr { .. } => "<bad expression>".into(),
        Expr::BinaryExpr { lhs, rhs, token, .. } => format!("({} {} {})", expr(lhs), token.string(), expr(rhs)),
        Expr::BoolLit { literal, .. }
        | Expr::CharLit { literal, .. }
        | Expr::FloatLit { literal, .. }
        | Expr::IntLit { literal, .. }
        | Expr::StringLit { literal, .. } => text(literal),
        Expr::CallExpr { func, args, ellipsis, .. } => {
            let mut a = exprs(args);
            if *ellipsis != tengo::source::NO_POS {
                if let Some(l) = a.last_mut() {
                    l.push_str("...");
                }
            }
            format!("{}({})", expr(func), a.join(", "))
        }
        Expr::CondExpr { cond, t, f, .. } => format!("({} ? {} : {})", expr(cond), expr(t), expr(f)),
        Expr::ErrorExpr { expr: x, .. } => format!("error({})", expr(x)),
        Expr::FuncLit { ty, body } => format!("func{} {}", idents(&ty.params), block(body)),
        Expr::Ident(i) => text(&i.name),
        Expr::ImmutableExpr { expr: x, .. } => format!("immutable({})", expr(x)),
        Expr::ImportExpr { module_name, .. } => format!("import(\"{}\")", text(module_name)),
        Expr::IndexExpr { expr: x, index, .. } => format!("{}[{}]", expr(x), opt(index)),
        Expr::MapLit { elements, .. } => {
            let e: Vec<String> = elements.iter().map(|m| format!("{}: {}", text(&m.key), expr(&m.value))).collect();
            format!("{{{}}}", e.join(", "))
        }
        Expr::ParenExpr { expr: x, .. } => format!("({})", expr(x)),
        Expr::SelectorExpr { expr: x, sel } => format!("{}.{}", expr(x), expr(sel)),
        Expr::SliceExpr { expr: x, low, high, .. } => format!("{}[{}:{}]", expr(x), opt(low), opt(high)),
        Expr::UnaryExpr { expr: x, token, .. } => format!("({}{})", token.string(), expr(x)),
        Expr::UndefinedLit { .. } => "undefined".into(),
    }
}

fn stmt(s: &Stmt) -> String {
    match s {
        Stmt::AssignStmt { lhs, rhs, token, .. } => {
            format!("{} {} {}", exprs(lhs).join(", "), token.string(), exprs(rhs).join(", "))
        }
        Stmt::BadStmt { .. } => "<bad statement>".into(),
        Stmt::BlockStmt(b) => block(b),
        Stmt::BranchStmt { token, label, .. } => match label {
            Some(l) => format!("{} {}", token.string(), text(&l.name)),
            None => token.string(),
        },
        Stmt::EmptyStmt { .. } => ";".into(),
        Stmt::ExportStmt { result, .. } => format!("export {}", expr(result)),
        Stmt::ExprStmt { expr: x } => expr(x),
        Stmt::ForInStmt { key, value, iterable, body, .. } => {
            let v = text(&value.name);
            let names = if v.is_empty() { text(&key.name) } else { format!("{}, {}", text(&key.name), v) };
            format!("for {} in {} {}", names, expr(iterable), block(body))
        }
        Stmt::ForStmt { init, cond, post, body, .. } => {
            let i = init.as_ref().map(|x| stmt(x)).unwrap_or_default();
            let c = cond.as_ref().map(|x| format!("{} ", expr(x))).unwrap_or_default();
            let p = post.as_ref().map(|x| stmt(x)).unwrap_or_default();
            if !i.is_empty() || !p.is_empty() {
                format!("for {} ; {} ; {}{}", i, c, p, block(body))
            } else {
                format!("for {}{}", c, block(body))
            }
        }
        Stmt::IfStmt { init, cond, body, else_, .. } => {
            let i = init.as_ref().map(|x| format!("{}; ", stmt(x))).unwrap_or_default();
            let c = cond.as_ref().map(expr).unwrap_or_default();
            let e = else_.as_ref().map(|x| format!(" else {}", stmt(x))).unwrap_or_default();
            format!("if {}{} {}{}", i, c, block(body), e)
        }
        Stmt::IncDecStmt { expr: x, token, .. } => format!("{}{}", expr(x), token.string()),
        Stmt::ReturnStmt { result, .. } => match result {
            Some(r) => format!("return {}", expr(r)),
            None => "return".into(),
        },
    }
}

fn parse(src: &str) -> Result<File, String> {
    let fset = SourceFileSet::new();
    let file = fset.borrow_mut().add_file("test", -1, src.len() as i64);
    Parser::new(fset, file, src.as_bytes().to_vec()).parse_file().map_err(|e| e.error())
}

#[track_caller]
fn parses(src: &str, expected: &str) {
    let file = parse(src).unwrap_or_else(|e| panic!("source: {}\nfailed: {}", src, e));
    assert_eq!(file.stmts.iter().map(stmt).collect::<Vec<_>>().join("; "), expected, "source: {}", src);
}

#[track_caller]
fn parse_fails(src: &str) {
    assert!(parse(src).is_err(), "source: {}", src);
}

/// scanExpect with DontInsertSemis: this port's scanner always inserts semicolons at line ends
/// and skips comments, so the inserted semicolons are dropped before comparing.
#[track_caller]
fn scans(src: &str, expected: &[(&str, &str, i64, i64)]) {
    let fset = SourceFileSet::new();
    let file = fset.borrow_mut().add_file("test", -1, src.len() as i64);
    let mut errs = Vec::new();
    let mut scanner = Scanner::new(fset.clone(), file, src.as_bytes().to_vec(), &mut errs);
    let mut got = Vec::new();
    loop {
        let (tok, lit, pos) = scanner.scan(&mut errs);
        if tok == Token::EOF {
            break;
        }
        if tok == Token::Semicolon && lit == b"\n" {
            continue;
        }
        let p = fset.borrow_mut().position(pos);
        got.push((tok.string(), text(&lit), p.line as i64, p.column as i64));
    }
    assert!(errs.is_empty());
    assert_eq!(got.len(), expected.len());
    for (g, e) in got.iter().zip(expected) {
        assert_eq!((g.0.as_str(), g.1.as_str(), g.2, g.3), *e);
    }
}

#[test]
fn parse_array() {
    parses("[1, 2, 3]", "[1, 2, 3]");
    parses(r"[
1,
2,
3
]", "[1, 2, 3]");
    parses(r#"[1, "foo", 12.34]"#, r#"[1, "foo", 12.34]"#);
    parses("a = [1, 2, 3]", "a = [1, 2, 3]");
    parses("a = [1 + 2, b * 4, [4, c]]", "a = [(1 + 2), (b * 4), [4, c]]");
    parse_fails("[1, 2, 3,]");
    parse_fails(r"
[
	1,
	2,
	3,
]");
    parse_fails(r"
[
	1,
	2,
	3,

]");
    parse_fails("[1, 2, 3, ,]");
}

#[test]
fn parse_assignment() {
    parses("a = 5", "a = 5");
    parses("a := 5", "a := 5");
    parses("a, b = 5, 10", "a, b = 5, 10");
    parses("a, b := 5, 10", "a, b := 5, 10");
    parses("a, b = a + 2, b - 8", "a, b = (a + 2), (b - 8)");
    parses("a += 5", "a += 5");
    parses("a *= 5 + 10", "a *= (5 + 10)");
}

#[test]
fn parse_boolean() {
    parses("true", "true");
    parses("false", "false");
    parses("true != false", "(true != false)");
    parses("!false", "(!false)");
}

#[test]
fn parse_call() {
    parses("add(1, 2, 3)", "add(1, 2, 3)");
    parses("add(1, 2, v...)", "add(1, 2, v...)");
    parses("a = add(1, 2, 3)", "a = add(1, 2, 3)");
    parses("a, b = add(1, 2, 3)", "a, b = add(1, 2, 3)");
    parses("add(a + 1, 2 * 1, (b + c))", "add((a + 1), (2 * 1), ((b + c)))");
    parses("a + add(b * c) + d", "((a + add((b * c))) + d)");
    parses("add(a, b, 1, 2 * 3, 4 + 5, add(6, 7 * 8))", "add(a, b, 1, (2 * 3), (4 + 5), add(6, (7 * 8)))");
    parses("f1(a) + f2(b) * f3(c)", "(f1(a) + (f2(b) * f3(c)))");
    parses("(f1(a) + f2(b)) * f3(c)", "(((f1(a) + f2(b))) * f3(c))");
    parses("func(a, b) { a + b }(1, 2)", "func(a, b) {(a + b)}(1, 2)");
    parses("a.b()", "a.b()");
    parses("a.b.c()", "a.b.c()");
    parses(r#"a["b"].c()"#, r#"a["b"].c()"#);
    parse_fails("add(...a, 1)");
    parse_fails("add(a..., 1)");
    parse_fails("add(a..., b...)");
    parse_fails("add(1, a..., b...)");
    parse_fails("add(...)");
    parse_fails("add(1, ...)");
    parse_fails("add(1, ..., )");
    parse_fails("add(...a)");
}

#[test]
fn parse_char() {
    parses("'A'", "'A'");
    parses("'九'", "'九'");
    parse_fails("''");
    parse_fails("'AB'");
    parse_fails("'A九'");
}

#[test]
fn parse_cond_expr() {
    parses("a ? b : c", "(a ? b : c)");
    parses(r"a ?
b :
c", "(a ? b : c)");
    parses("a + b ? c - d : e * f", "((a + b) ? (c - d) : (e * f))");
    parses("a == b ? c + (d / e) : f ? g : h + i", "((a == b) ? (c + ((d / e))) : (f ? g : (h + i)))");
    parses("(a + b) ? (c - d) : (e * f)", "(((a + b)) ? ((c - d)) : ((e * f)))");
    parses("a + (b ? c : d) - e", "((a + ((b ? c : d))) - e)");
    parses("a ? b ? c : d : e", "(a ? (b ? c : d) : e)");
    parses("a := b ? c : d", "a := (b ? c : d)");
    parses("x := a ? b ? c : d : e", "x := (a ? (b ? c : d) : e)");
    parse_fails(r"a
? b
: c");
    parse_fails("a ? (b : e)");
    parse_fails("(a ? b) : e");
}

#[test]
fn parse_error() {
    parses("error(1234)", "error(1234)");
    parses(r#"err1 := error("some error")"#, r#"err1 := error("some error")"#);
    parses(r#"return error("some error")"#, r#"return error("some error")"#);
    parses(r#"return error("some" + "error")"#, r#"return error(("some" + "error"))"#);
    parse_fails("error()");
}

#[test]
fn parse_for_in() {
    parses("for x in y {}", "for _, x in y {}");
    parses("for _ in y {}", "for _, _ in y {}");
    parses("for x in [1, 2, 3] {}", "for _, x in [1, 2, 3] {}");
    parses("for x, y in z {}", "for x, y in z {}");
    parses("for x, y in {k1: 1, k2: 2} {}", "for x, y in {k1: 1, k2: 2} {}");
}

#[test]
fn parse_for() {
    parses("for {}", "for {}");
    parses("for a == 5 {}", "for (a == 5) {}");
    parses("for a := 0; a == 5;  {}", "for a := 0 ; (a == 5)  ; {}");
    parses("for a := 0; a < 5; a++ {}", "for a := 0 ; (a < 5)  ; a++{}");
    parses("for ; a < 5; a++ {}", "for  ; (a < 5)  ; a++{}");
    parses("for a := 0; ; a++ {}", "for a := 0 ;  ; a++{}");
    parses("for a == 5 && b != 4 {}", "for ((a == 5) && (b != 4)) {}");
}

#[test]
fn parse_function() {
    parses("a = func(b, c, d) { return d }", "a = func(b, c, d) {return d}");
}

#[test]
fn parse_variadic_function() {
    parses("a = func(...args) { return args }", "a = func(...args) {return args}");
}

#[test]
fn parse_variadic_function_with_args() {
    parses("a = func(x, y, ...z) { return z }", "a = func(x, y, ...z) {return z}");
    parse_fails("a = func(x, y, ...z, invalid) { return z }");
    parse_fails("a = func(...args, invalid) { return args }");
}

#[test]
fn parse_if() {
    parses("if a == 5 {}", "if (a == 5) {}");
    parses("if a == 5 && b != 3 {}", "if ((a == 5) && (b != 3)) {}");
    parses("if a == 5 { a = 3; a = 1 }", "if (a == 5) {a = 3; a = 1}");
    parses("if a == 5 { a = 3; a = 1 } else { a = 2; a = 4 }", "if (a == 5) {a = 3; a = 1} else {a = 2; a = 4}");
    parses(r"if a == 5 {
b = 3
c = 1
} else if d == 3 {
e = 8
f = 3
} else {
g = 2
h = 4
}", "if (a == 5) {b = 3; c = 1} else if (d == 3) {e = 8; f = 3} else {g = 2; h = 4}");
    parses("if a := 3; a < b {}", "if a := 3; (a < b) {}");
    parses("if a++; a < b {}", "if a++; (a < b) {}");
    parse_fails("if {}");
    parse_fails("if a == b { } else a != b { }");
    parse_fails("if a == b { } else if { }");
    parse_fails("else { }");
    parse_fails("if ; {}");
    parse_fails("if a := 3; {}");
    parse_fails("if ; a < 3 {}");
}

#[test]
fn parse_import() {
    parses(r#"a := import("mod1")"#, r#"a := import("mod1")"#);
    parses(r#"import("mod1").var1"#, r#"import("mod1").var1"#);
    parses(r#"import("mod1").func1()"#, r#"import("mod1").func1()"#);
    parses(r#"for x, y in import("mod1") {}"#, r#"for x, y in import("mod1") {}"#);
}

#[test]
fn parse_index() {
    parses("[1, 2, 3][1]", "[1, 2, 3][1]");
    parses("[1, 2, 3][5 - a]", "[1, 2, 3][(5 - a)]");
    parses("[1, 2, 3][5 : a]", "[1, 2, 3][5:a]");
    parses("[1, 2, 3][a + 3 : b - 8]", "[1, 2, 3][(a + 3):(b - 8)]");
    parses(r#"{a: 1, b: 2}["b"]"#, r#"{a: 1, b: 2}["b"]"#);
    parses("{a: 1, b: 2}[a + b]", "{a: 1, b: 2}[(a + b)]");
}

#[test]
fn parse_logical() {
    parses("a && 5 || true", "((a && 5) || true)");
    parses("a || 5 && true", "(a || (5 && true))");
    parses("a && (5 || true)", "(a && ((5 || true)))");
}

#[test]
fn parse_map() {
    parses(r#"{ key1: 1, key2: "2", key3: true }"#, r#"{key1: 1, key2: "2", key3: true}"#);
    parses(r#"{ "key1": 1 }"#, "{key1: 1}");
    parses(r#"a = { key1: 1, key2: "2", key3: true }"#, r#"a = {key1: 1, key2: "2", key3: true}"#);
    parses(r#"a = { key1: 1, key2: "2", key3: { k1: `bar`, k2: 4 } }"#, r#"a = {key1: 1, key2: "2", key3: {k1: `bar`, k2: 4}}"#);
    parses(r#"{
key1: 1,
key2: "2",
key3: true
}"#, r#"{key1: 1, key2: "2", key3: true}"#);
    parse_fails(r#"
{
	key1: 1,
	key2: "2",
	key3: true,
}"#);
    parse_fails("{ key1: 1, }");
    parse_fails(r"{
key1: 1,
key2: 2,
}");
}

#[test]
fn parse_precedence() {
    parses("a + b + c", "((a + b) + c)");
    parses("a + b * c", "(a + (b * c))");
    parses("x = 2 * 1 + 3 / 4", "x = ((2 * 1) + (3 / 4))");
}

#[test]
fn parse_selector() {
    parses("a.b", "a.b");
    parses("a.b.c", "a.b.c");
    parses("{k1:1}.k1", "{k1: 1}.k1");
    parses("{k1:{v1:1}}.k1.v1", "{k1: {v1: 1}}.k1.v1");
    parses("a.b = 4", "a.b = 4");
    parses("a.b.c = 4", "a.b.c = 4");
    parses("a.b.c = 4 + 5", "a.b.c = (4 + 5)");
    parses("a[0].c = 4", "a[0].c = 4");
    parses("a.b[0].c = 4", "a.b[0].c = 4");
    parses("a.b[0][2].c = 4", "a.b[0][2].c = 4");
    parses(r#"a.b["key1"][2].c = 4"#, r#"a.b["key1"][2].c = 4"#);
    parses("a[0].b[2].c = 4", "a[0].b[2].c = 4");
    parse_fails("a.(b.c)");
}

#[test]
fn parse_semicolon() {
    parses("1", "1");
    parses("1;", "1");
    parses("1;;", "1; ;");
    parses(r"1
;", "1; ;");
    parses(r"1;
;", "1; ;");
}

#[test]
fn parse_string() {
    parses(r#"a = "foo\nbar""#, r#"a = "foo\nbar""#);
    parses("a = `raw string`", "a = `raw string`");
}

#[test]
fn parse_int() {
    parses("1234567890", "1234567890");
    parses("0b10", "0b10");
    parses("0o12345670", "0o12345670");
    parses("0x123456789abcdef0", "0x123456789abcdef0");
    parses("0x123456789ABCDEF0", "0x123456789ABCDEF0");
    parses("010", "010");
    parses("0B10", "0B10");
    parses("0O10", "0O10");
    parses("0X10", "0X10");
    parse_fails("0b2");
    parse_fails("08");
    parse_fails("0o8");
    parse_fails("1a");
    parse_fails("0xg");
    parses("9223372036854775807", "9223372036854775807");
    parse_fails("9223372036854775808");
    parses("42", "42");
    parses("4_2", "4_2");
    parses("0600", "0600");
    parses("0_600", "0_600");
    parses("0o600", "0o600");
    parses("0O600", "0O600");
    parses("0xBadFace", "0xBadFace");
    parses("0xBad_Face", "0xBad_Face");
    parses("0x_67_7a_2f_cc_40_c6", "0x_67_7a_2f_cc_40_c6");
    parse_fails("170141183460469231731687303715884105727");
    parse_fails("170_141183_460469_231731_687303_715884_105727");
    parse_fails("42_");
    parse_fails("4__2");
    parse_fails("0_xBadFace");
}

#[test]
fn parse_float() {
    parses(".0", ".0");
    parses("0.", "0.");
    parses("0.0", "0.0");
    parses("00.0", "00.0");
    parses("00.00", "00.00");
    parse_fails("0.0.0");
    parse_fails("0..0");
    parses("010.0", "010.0");
    parses("00010.0", "00010.0");
    parses("08.0", "08.0");
    parse_fails("0a.0");
    parses("1e1", "1e1");
    parses("1E1", "1E1");
    parse_fails("1e1.1");
    parses("1e+1", "1e+1");
    parses("1e-1", "1e-1");
    parse_fails("1e+-1");
    parses("0x1p1", "0x1p1");
    parses("0x10p1", "0x10p1");
    parses("72.40", "72.40");
    parses("072.40", "072.40");
    parses("2.71828", "2.71828");
    parses("1.e+0", "1.e+0");
    parses("6.67428e-11", "6.67428e-11");
    parses("1E6", "1E6");
    parses(".25", ".25");
    parses(".12345E+5", ".12345E+5");
    parses("1_5.", "1_5.");
    parses("0.15e+0_2", "0.15e+0_2");
    parses("0x1p-2", "0x1p-2");
    parses("0x2.p10", "0x2.p10");
    parses("0x1.Fp+0", "0x1.Fp+0");
    parses("0X.8p-0", "0X.8p-0");
    parses("0X_1FFFP-16", "0X_1FFFP-16");
    parse_fails("0x.p1");
    parse_fails("1p-2");
    parse_fails("0x1.5e-2");
    parse_fails("1_.5");
    parse_fails("1._5");
    parse_fails("1.5_e1");
    parse_fails("1.5e_1");
    parse_fails("1.5e1_");
}

#[test]
fn mismatch_brace() {
    parse_fails(r#"
fmt := import("fmt")
out := 0
if 3 == 1 {
	out = 1
}
} else {
	out = 2
}
fmt.println(out)
	"#);
}

#[test]
fn parse_number_expressions() {
    parses("0x15e+2", "(0x15e + 2)");
    parses("0-_42", "(0 - _42)");
}

#[test]
fn scanner_scan() {
    let src = "         /* a comment */       \n \n    // a comment \n        \n   \n     \n       /*\r*/\n       \n         /**\r/*/   \n   \n \n    /**\r\r/*/      \n   //\r\n     \n     foobar      \n        a۰۱۸     \n         \n \n foo६४ \n  \n       \n      bar９８７６\n       \n     ŝ\n        \n        \n   ŝfoo\n       \n0         \n     \n    \n        1   \n        \n      \n      123456789012345678890\n     01234567       \n\n      0xcafebabe\n   \n  \n0.  \n  \n     \n    .0 \n    \n3.14159265       \n        \n1e0  \n\n   \n 1e+100  \n     1e-100       \n     \n    \n     2.71828e-1000    \n      \n\n     'a'     \n \n   '\\000'  \n         \n        \n  '\\xFF' \n  '\\uff16' \n       \n\n        '\\U0000ff16'         \n        `foobar`   \n   `foo\n\t                        bar`       \n       `\r`    \n   \n`foo\r\nbar`       \n        \n  \n     +     \n\n\n         -\n     *\n  \n  \n  /    \n  %     \n        \n       \n&         \n \n         |\n   \n       \n    ^       \n   \n        <<\n      \n    >>    \n        \n  &^        \n+=       \n  \n  -=   \n*=        \n   \n      /=     \n    \n \n        %=  \n         \n         \n&=\n   \n    \n   |=         \n \n     \n         ^=\n     \n         <<=  \n         >>=    \n       \n       \n &^=         \n         &&      \n        \n||   \n         ++        \n      \n--    \n     ==        \n    <         \n   >\n       \n     =\n       !        \n         \n   !=      \n      <=    \n     \n \n>=        \n    \n    \n:=\n    \n         ...        \n     \n \n  (   \n      [       \n {      \n     \n    , \n   .   \n    \n       )    \n ]       \n    }      \n     ;         \n      \n    \n :     \n   break         \n    continue       \n      else  \n     \n \n     for    \n     func        \n      if      \n return  \n         \n \n export     ";
    let expected: &[(&str, &str, i64, i64)] = &[
        ("IDENT", "foobar", 15, 6),
        ("IDENT", "a۰۱۸", 16, 9),
        ("IDENT", "foo६४", 19, 2),
        ("IDENT", "bar９８７６", 22, 7),
        ("IDENT", "ŝ", 24, 6),
        ("IDENT", "ŝfoo", 27, 4),
        ("INT", "0", 29, 1),
        ("INT", "1", 32, 9),
        ("INT", "123456789012345678890", 35, 7),
        ("INT", "01234567", 36, 6),
        ("INT", "0xcafebabe", 38, 7),
        ("FLOAT", "0.", 41, 1),
        ("FLOAT", ".0", 44, 5),
        ("FLOAT", "3.14159265", 46, 1),
        ("FLOAT", "1e0", 48, 1),
        ("FLOAT", "1e+100", 51, 2),
        ("FLOAT", "1e-100", 52, 6),
        ("FLOAT", "2.71828e-1000", 55, 6),
        ("CHAR", "'a'", 58, 6),
        ("CHAR", r"'\000'", 60, 4),
        ("CHAR", r"'\xFF'", 63, 3),
        ("CHAR", r"'\uff16'", 64, 3),
        ("CHAR", r"'\U0000ff16'", 67, 9),
        ("STRING", "`foobar`", 68, 9),
        ("STRING", r"`foo
	                        bar`", 69, 4),
        ("STRING", "``", 71, 8),
        ("STRING", r"`foo
bar`", 73, 1),
        ("+", "", 77, 6),
        ("-", "", 80, 10),
        ("*", "", 81, 6),
        ("/", "", 84, 3),
        ("%", "", 85, 3),
        ("&", "", 88, 1),
        ("|", "", 90, 10),
        ("^", "", 93, 5),
        ("<<", "", 95, 9),
        (">>", "", 97, 5),
        ("&^", "", 99, 3),
        ("+=", "", 100, 1),
        ("-=", "", 102, 3),
        ("*=", "", 103, 1),
        ("/=", "", 105, 7),
        ("%=", "", 108, 9),
        ("&=", "", 111, 1),
        ("|=", "", 114, 4),
        ("^=", "", 117, 10),
        ("<<=", "", 119, 10),
        (">>=", "", 120, 10),
        ("&^=", "", 123, 2),
        ("&&", "", 124, 10),
        ("||", "", 126, 1),
        ("++", "", 127, 10),
        ("--", "", 129, 1),
        ("==", "", 130, 6),
        ("<", "", 131, 5),
        (">", "", 132, 4),
        ("=", "", 134, 6),
        ("!", "", 135, 8),
        ("!=", "", 137, 4),
        ("<=", "", 138, 7),
        (">=", "", 141, 1),
        (":=", "", 144, 1),
        ("...", "", 146, 10),
        ("(", "", 149, 3),
        ("[", "", 150, 7),
        ("{", "", 151, 2),
        (",", "", 153, 5),
        (".", "", 154, 4),
        (")", "", 156, 8),
        ("]", "", 157, 2),
        ("}", "", 158, 5),
        (";", ";", 159, 6),
        (":", "", 162, 2),
        ("break", "break", 163, 4),
        ("continue", "continue", 164, 5),
        ("else", "else", 165, 7),
        ("for", "for", 168, 6),
        ("func", "func", 169, 6),
        ("if", "if", 170, 7),
        ("return", "return", 171, 2),
        ("export", "export", 174, 2),
    ];
    scans(src, expected);
}
