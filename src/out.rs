//! Buffered standard output. Go writes to os.Stdout unbuffered, so everything written here is
//! flushed before the process exits, including on a Go-style panic.

use std::cell::RefCell;
use std::io::{ErrorKind, Write};

thread_local! {
    static OUT: RefCell<std::io::BufWriter<std::io::Stdout>> =
        RefCell::new(std::io::BufWriter::with_capacity(1 << 16, std::io::stdout()));
}

/// A Go program writing to a closed stdout dies of SIGPIPE, which a shell reports as 141. Any
/// other write failure ends the run with status 1, because there is no way to continue the
/// output the script asked for.
fn write_failed(e: std::io::Error) -> ! {
    if e.kind() == ErrorKind::BrokenPipe {
        std::process::exit(141);
    }
    eprintln!("write error: {}", e);
    std::process::exit(1);
}

pub fn write(b: &[u8]) {
    OUT.with(|o| {
        if let Err(e) = o.borrow_mut().write_all(b) {
            write_failed(e);
        }
    });
}

pub fn flush() {
    OUT.with(|o| {
        if let Err(e) = o.borrow_mut().flush() {
            write_failed(e);
        }
    });
}
