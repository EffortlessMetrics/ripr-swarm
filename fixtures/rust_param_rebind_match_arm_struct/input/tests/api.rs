use rust_param_rebind_match_arm_struct::*;

#[test]
fn the_rebound_value_does_not_reach_the_boundary() {
    assert_eq!(score(20, None), 1);
}
