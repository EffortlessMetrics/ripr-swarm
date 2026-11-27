use unsafe_obligation_carry::{assume, gate, pick};

#[test]
fn checks_unsafe_contracts() {
    assert_eq!(unsafe { pick(true) }, 4);
    assert!(unsafe { gate(8) });
    assert_eq!(assume(4), 7);
}
