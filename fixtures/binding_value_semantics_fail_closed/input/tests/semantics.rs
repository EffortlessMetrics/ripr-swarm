use value_transfer_semantics::{eager, signed};

#[test]
fn below_zero_is_not_the_boundary() {
    assert!(signed(0));
}

#[test]
fn a_present_option_still_evaluates_its_default() {
    assert!(eager(0));
}
