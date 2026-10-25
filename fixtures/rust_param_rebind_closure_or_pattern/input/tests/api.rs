use rust_param_rebind_closure_or_pattern::*;

#[test]
fn the_closure_binding_not_the_caller_input_feeds_the_comparison() {
    assert_eq!(score(20), 1);
}
