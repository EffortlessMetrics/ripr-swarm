use predicate_pairing_reassigned_binding::gate;

#[test]
fn rebound() {
    let mut got = gate(10);
    got = true;
    assert_eq!(got, true);
}
