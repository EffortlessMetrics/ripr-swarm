use rust_wrapper_closure_shadowed_callee_fixture::order_discount;

#[test]
fn wrapper_pins_the_boundary() {
    assert_eq!(order_discount(10), 5);
    assert_eq!(order_discount(9), 0);
}
