use rust_param_rebind_closure_or_pattern::*;

#[test]
fn the_rebound_value_does_not_reach_the_boundary() {
    assert_eq!(score(20), 1);
}
