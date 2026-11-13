use test_local_rival_import_alias_external_subject::discounted_total as dt;

#[test]
fn aliased_owner_boundary() {
    assert_eq!(dt(100, 100), 90);
}
