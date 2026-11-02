//! Carried over from symbol_table_test.go.

use tengo::compiler::{new_symbol_table, st_define, st_fork, st_resolve, Scope, TableRef};

fn defines(t: &TableRef, name: &str, scope: Scope, index: i64) {
    let s = st_define(t, name.as_bytes());
    let s = s.borrow();
    assert!(s.scope == scope, "{}", name);
    assert_eq!(s.index, index, "{}", name);
}

fn resolves(t: &TableRef, name: &str, scope: Scope, index: i64, depth: i64) {
    let (s, d) = st_resolve(t, name.as_bytes(), true).expect(name);
    let s = s.borrow();
    assert!(s.scope == scope, "{}", name);
    assert_eq!((s.index, d), (index, depth), "{}", name);
}

#[test]
fn symbol_table() {
    use Scope::{Free, Global, Local};
    let global = new_symbol_table();
    defines(&global, "a", Global, 0);
    defines(&global, "b", Global, 1);
    let local1 = st_fork(&global, false);
    defines(&local1, "d", Local, 0);
    let local1_block1 = st_fork(&local1, true);
    for (i, n) in ["l", "m", "n", "o", "p"].iter().enumerate() {
        defines(&local1_block1, n, Local, i as i64 + 1);
    }
    let local2 = st_fork(&local1, false);
    defines(&local2, "e", Local, 0);
    defines(&local2, "f", Local, 1);
    let local2_block1 = st_fork(&local2, true);
    defines(&local2_block1, "g", Local, 2);
    defines(&local2_block1, "h", Local, 3);
    let local2_block2 = st_fork(&local2, true);
    defines(&local2_block2, "i", Local, 2);
    defines(&local2_block2, "j", Local, 3);
    defines(&local2_block2, "k", Local, 4);
    let local3 = st_fork(&local1_block1, false);
    defines(&local3, "q", Local, 0);
    defines(&local3, "r", Local, 1);

    resolves(&global, "a", Global, 0, 0);
    resolves(&local1, "d", Local, 0, 0);
    resolves(&local1, "a", Global, 0, 1);
    resolves(&local3, "a", Global, 0, 3);
    resolves(&local3, "d", Free, 0, 2);
    resolves(&local3, "r", Local, 1, 0);
    resolves(&local2_block2, "k", Local, 4, 0);
    resolves(&local2_block2, "e", Local, 0, 1);
    resolves(&local2_block2, "b", Global, 1, 3);
}
