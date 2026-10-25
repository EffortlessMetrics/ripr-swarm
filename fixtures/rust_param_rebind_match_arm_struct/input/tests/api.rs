use rust_param_rebind_match_arm_struct::*;

#[test]
fn the_arm_binding_not_the_caller_input_feeds_the_comparison() {
    assert_eq!(score(20, Some(Foo { n: 7 })), 1);
}
