use rust_wrapper_boolean_return_discarded_fixture::order_discount;

#[test]
fn wrapper_recomputes_the_discount() {
    assert_eq!(order_discount(10), 5);
    assert_eq!(order_discount(9), 0);
}
