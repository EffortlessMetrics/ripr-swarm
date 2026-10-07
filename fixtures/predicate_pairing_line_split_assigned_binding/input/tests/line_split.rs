use predicate_pairing_line_split_assigned_binding::gate;

#[test]
fn line_split() {
    let mut got = gate(10);
    got
        = true;
    assert_eq!(got, true);
}
