//! Buffered standard output. Go writes to os.Stdout unbuffered, so everything written here is
//! flushed before the process exits, including on a Go-style panic.

use std::cell::RefCell;
use std::io::Write;

thread_local! {
    static OUT: RefCell<std::io::BufWriter<std::io::Stdout>> =
        RefCell::new(std::io::BufWriter::with_capacity(1 << 16, std::io::stdout()));
}

pub fn write(b: &[u8]) {
    OUT.with(|o| {
        let _ = o.borrow_mut().write_all(b);
    });
}

pub fn flush() {
    OUT.with(|o| {
        let _ = o.borrow_mut().flush();
    });
}
