use rust_verification_ghost_only_edit_fixture::bump;

#[test]
fn bump_adds_one() {
    assert_eq!(bump(2), 3);
}
