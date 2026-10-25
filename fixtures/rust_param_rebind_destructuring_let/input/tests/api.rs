use rust_param_rebind_destructuring_let::*;

#[test]
fn the_rebound_value_does_not_reach_the_boundary() {
    assert_eq!(score(20), 1);
}
