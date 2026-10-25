use value_transfer_semantics::{eager, signed};

#[test]
fn below_zero_is_not_the_boundary() {
    assert!(signed(0));
}

#[test]
fn a_present_option_still_evaluates_its_default() {
    assert!(eager(0));
}

#[test]
fn a_primitive_spelling_does_not_establish_its_width() {
    assert!(value_transfer_semantics::narrow::score(200));
}
