use test_local_rival_import_control_subject::discounted_total;

#[test]
fn real_owner_boundary() {
    assert_eq!(discounted_total(100, 100), 90);
}
