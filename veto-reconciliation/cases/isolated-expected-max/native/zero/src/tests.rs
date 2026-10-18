use crate::proof::{charge, Lease};
use crate::*;
#[test]
fn observes() {
    let lease = Lease { elapsed: Some(u64::MAX) };
    assert_eq!(charge(&lease), u64::MAX);
}
