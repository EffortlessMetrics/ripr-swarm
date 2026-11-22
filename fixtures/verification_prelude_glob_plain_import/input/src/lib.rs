extern crate shim;

// Edition 2015: `prusti_contracts::*` below is a crate-relative path, so it
// globs `shim::prusti_contracts`, whose `assert_eq!` ripr cannot read.
use shim::prusti_contracts;
use prusti_contracts::*;

pub fn weight(input: i32) -> i32 {
    input * 3
}

#[test]
fn weighs() {
    assert_eq!(weight(4), 12);
}
