use predicate_pairing_mutably_borrowed_binding::gate;

#[test]
fn borrowed() {
    let mut got = gate(10);
    let r = &mut got;
    *r = true;
    assert_eq!(got, true);
}
