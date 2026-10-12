use predicate_pairing_newline_split_reassigned_binding::gate;

#[test]
fn rebound() {
    let mut got = gate(10);
    got
    = true;
    assert_eq!(got, true);
}
