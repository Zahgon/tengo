//! End-to-end checks of the tengo binary: each script runs from a temporary directory as
//! `tengo main.tengo`, and stdout, stderr and the exit status are compared.

use std::path::PathBuf;
use std::process::Command;

fn run(src: &str) -> (i32, String, String) {
    run_with(src, &[])
}

fn run_with(src: &str, files: &[(&str, &str)]) -> (i32, String, String) {
    let dir = scratch_dir();
    std::fs::write(dir.join("main.tengo"), src).unwrap();
    for (name, body) in files {
        std::fs::write(dir.join(name), body).unwrap();
    }
    let out = Command::new(env!("CARGO_BIN_EXE_tengo"))
        .arg("main.tengo")
        .current_dir(&dir)
        .env("TZ", "UTC")
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn scratch_dir() -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let d = std::env::temp_dir().join(format!("tengo-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn stdout_of(src: &str) -> String {
    let (code, out, err) = run(src);
    assert_eq!(code, 0, "stderr: {}", err);
    out
}

#[test]
fn arithmetic_and_float_rendering() {
    let out = stdout_of(
        "fmt := import(\"fmt\")\nfmt.println(1 + 2 * 3, \" \", 7 / 2, \" \", -7 % 3, \" \", 1 << 63, \" \", 0.1 + 0.2, \" \", 1e21, \" \", 1/3.0)\n",
    );
    assert_eq!(out, "7 3 -1 -9223372036854775808 0.30000000000000004 1000000000000000000000 0.3333333333333333\n");
}

#[test]
fn strings_iterate_by_rune_and_slice_by_byte() {
    let out = stdout_of("fmt := import(\"fmt\")\ns := \"héllo\"\nfmt.println(len(s), \" \", s[1], \" \", s[1:3])\n");
    assert_eq!(out, "6 é é\n");
}

#[test]
fn append_shares_storage_like_go() {
    let out = stdout_of(
        "fmt := import(\"fmt\")\na := []\nfor i := 0; i < 3; i++ { a = append(a, i) }\nb := append(a, 100)\nc := append(a, 200)\nfmt.println(b, c)\n",
    );
    assert_eq!(out, "[0, 1, 2, 200][0, 1, 2, 200]\n");
}

#[test]
fn closures_capture_variables() {
    let out = stdout_of(
        "fmt := import(\"fmt\")\nc := func() { n := 0; return func() { n++; return n } }()\nc(); c()\nfmt.println(c())\n",
    );
    assert_eq!(out, "3\n");
}

#[test]
fn parse_error_reports_position_and_count() {
    let (code, out, err) = run("a := (1 + 2\nb := 0x\n");
    assert_eq!(code, 1);
    assert_eq!(out, "");
    assert!(err.starts_with("Parse Error: "), "{}", err);
    assert!(err.contains("\tat main.tengo:"), "{}", err);
}

#[test]
fn compile_error_message() {
    let (code, _, err) = run("y := zzz + 1\n");
    assert_eq!(code, 1);
    assert_eq!(err, "Compile Error: unresolved reference 'zzz'\n\tat main.tengo:1:6\n");
}

#[test]
fn runtime_error_lists_every_frame() {
    let (code, out, err) = run("fmt := import(\"fmt\")\nfmt.println(\"x\")\nf := func(a) {\n  return a + \"s\"\n}\ng := func(a) { return f(a) }\ng(1)\n");
    assert_eq!(code, 1);
    assert_eq!(out, "x\n");
    assert_eq!(err, "Runtime Error: invalid operation: int + string\n\tat main.tengo:4:10\n\tat main.tengo:6:23\n\tat main.tengo:7:1\n");
}

#[test]
fn integer_division_by_zero_panics() {
    let (code, _, err) = run("x := 1 / 0\n");
    assert_eq!(code, 2);
    assert_eq!(err.lines().next().unwrap(), "panic: runtime error: integer divide by zero");
}

#[test]
fn format_verbs() {
    let out = stdout_of("fmt := import(\"fmt\")\nfmt.println(format(\"%5d|%-4s|%.2f|%x|%q|%v|%d\", 42, \"a\", 3.14159, 255, \"hi\", [1, \"a\"], \"s\"))\n");
    assert_eq!(out, "   42|a   |3.14|ff|\"hi\"|[1, \"a\"]|%!d(\"s\"=\"s\")\n");
}

#[test]
fn text_and_regexp() {
    let out = stdout_of(
        "fmt := import(\"fmt\")\ntext := import(\"text\")\nfmt.println(text.split(\"a,b\", \",\"), text.to_upper(\"straße\"), text.re_replace(\"(\\\\w+)@\", \"bob@x\", \"<$1>\"))\n",
    );
    assert_eq!(out, "[\"a\", \"b\"]STRAßE<bob>x\n");
}

#[test]
fn json_round_trip() {
    let out = stdout_of("fmt := import(\"fmt\")\njson := import(\"json\")\nfmt.println(string(json.encode(json.decode(`[1, \"a\", 2.5, null, true]`))))\n");
    assert_eq!(out, "[1,\"a\",2.5,null,true]\n");
}

#[test]
fn times_format_and_parse() {
    let out = stdout_of(
        "fmt := import(\"fmt\")\ntimes := import(\"times\")\nt := times.date(2020, 2, 29, 13, 4, 5, 0)\nfmt.println(times.time_format(t, times.format_rfc1123), \" \", times.duration_string(times.hour + 90 * times.second))\n",
    );
    assert_eq!(out, "Sat, 29 Feb 2020 13:04:05 UTC 1h1m30s\n");
}

#[test]
fn math_module() {
    let out = stdout_of("fmt := import(\"fmt\")\nmath := import(\"math\")\nfmt.println(math.sqrt(2), \" \", math.floor(-2.5), \" \", math.inf(-1))\n");
    assert_eq!(out, "1.4142135623730951 -3 -Inf\n");
}

#[test]
fn file_module_import() {
    let (code, out, err) = run_with(
        "fmt := import(\"fmt\")\nm := import(\"mod\")\nfmt.println(m.double(21))\n",
        &[("mod.tengo", "export { double: func(x) { return x * 2 } }\n")],
    );
    assert_eq!(code, 0, "{}", err);
    assert_eq!(out, "42\n");
}

#[test]
fn enum_module() {
    let out = stdout_of("fmt := import(\"fmt\")\nenum := import(\"enum\")\nfmt.println(enum.map([1, 2], func(k, v) { return v * 10 }))\n");
    assert_eq!(out, "[10, 20]\n");
}
