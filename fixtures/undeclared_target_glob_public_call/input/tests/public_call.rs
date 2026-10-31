#[test]
fn pins_owner() {
    assert_eq!(undeclared_target_glob::owner(2), 6);
}
