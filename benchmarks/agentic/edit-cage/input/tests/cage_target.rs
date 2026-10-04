//! B6 edit-cage bench fixture: the selected test target inside the allowed
//! surface. Only edits here (under `tests/`) can satisfy target movement.

#[test]
fn add_saturates_at_the_top() {
    assert_eq!(fixture_lib::add(u64::MAX, 1), u64::MAX);
}
