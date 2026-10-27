//! The tengo command (cmd/tengo/main.go): `tengo [flags] {input-file}` compiles and runs a
//! .tengo file. Errors go to stderr with exit status 1; a Go runtime panic prints
//! "panic: ..." and exits with status 2.

use std::io::Write;

use crate::compiler::{abs_path, base_path, CompileFail, Compiler};
use crate::parser::Parser;
use crate::source::SourceFileSet;
use crate::vm::{RunFail, Vm};

fn eprint_line(s: &str) {
    let mut e = std::io::stderr();
    let _ = e.write_all(s.as_bytes());
    let _ = e.write_all(b"\n");
}

fn go_panic(msg: &str) -> i32 {
    crate::out::flush();
    let mut e = std::io::stderr();
    let _ = write!(e, "panic: {}\n\ngoroutine 1 [running]:\nmain.main()\n", msg);
    2
}

fn do_help() {
    let text = "Usage:\n\n\ttengo [flags] {input-file}\n\nFlags:\n\n\t-o        compile output file\n\t-version  show version\n\nExamples:\n\n\ttengo\n\n\t          Start Tengo REPL\n\n\ttengo myapp.tengo\n\n\t          Compile and run source file (myapp.tengo)\n\t          Source file must have .tengo extension\n\n\ttengo -o myapp myapp.tengo\n\n\t          Compile source file (myapp.tengo) into bytecode file (myapp)\n\n\ttengo myapp\n\n\t          Run bytecode file (myapp)\n\n\n";
    crate::out::write(text.as_bytes());
}

pub fn run(args: Vec<String>) -> i32 {
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if !rest.is_empty() || !a.starts_with('-') || a == "-" {
            rest.push(a.clone());
        } else if a == "--" {
            rest.extend(args[i + 1..].iter().cloned());
            break;
        } else {
            let name = a.trim_start_matches('-');
            match name {
                "help" | "h" => {
                    do_help();
                    return 2;
                }
                "version" => {
                    crate::out::write(b"dev\n");
                    return 0;
                }
                "resolve" => {}
                _ => {}
            }
        }
        i += 1;
    }
    let input = match rest.first() {
        None => return 0,
        Some(f) => f.clone(),
    };
    let mut data = match std::fs::read(&input) {
        Ok(d) => d,
        Err(e) => {
            eprint_line(&format!("Error reading input file: {}", crate::compiler::go_read_error(&input, &e)));
            return 1;
        }
    };
    let input_abs = abs_path(&input);
    if data.len() > 1 && &data[..2] == b"#!" {
        data[0] = b'/';
        data[1] = b'/';
    }
    let base = base_path(&input_abs);
    let ext = match base.rfind('.') {
        Some(p) => &base[p..],
        None => "",
    };
    if ext != ".tengo" {
        // RunCompiled: a gob-encoded bytecode file is not supported by this build.
        eprint_line("unexpected EOF");
        return 1;
    }
    compile_and_run(data, &input_abs)
}

fn compile_and_run(src: Vec<u8>, input_file: &str) -> i32 {
    let fset = SourceFileSet::new();
    let file = fset.borrow_mut().add_file(&base_path(input_file), -1, src.len() as i64);
    let p = Parser::new(fset.clone(), file, src);
    let ast = match p.parse_file() {
        Ok(f) => f,
        Err(el) => {
            eprint_line(&el.error());
            return 1;
        }
    };
    let mut c = Compiler::new(fset.clone(), file, None);
    c.enable_file_import(true);
    if let Err(e) = c.compile_file(&ast) {
        return match e {
            CompileFail::Error(m) => {
                eprint_line(&m);
                1
            }
            CompileFail::Panic(m) => go_panic(&m),
        };
    }
    let mut bc = c.bytecode();
    if let Err(e) = bc.remove_duplicates() {
        return match e {
            CompileFail::Error(m) => {
                eprint_line(&m);
                1
            }
            CompileFail::Panic(m) => go_panic(&m),
        };
    }
    let mut vm = Vm::new(bc, fset);
    match vm.run() {
        Ok(()) => 0,
        Err(RunFail::Error(m)) => {
            crate::out::flush();
            eprint_line(&m);
            1
        }
        Err(RunFail::Panic(m)) => go_panic(&m),
    }
}
