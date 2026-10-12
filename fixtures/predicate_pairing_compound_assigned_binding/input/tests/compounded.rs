use predicate_pairing_compound_assigned_binding::bucket;

#[test]
fn compounded() {
    let mut got = bucket(10);
    got += 1;
    assert_eq!(got, 2);
}
