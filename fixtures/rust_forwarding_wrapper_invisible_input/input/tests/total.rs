use rust_forwarding_wrapper_invisible_input_fixture::cash_total;

#[test]
fn exact_nickels_stay() {
    assert_eq!(cash_total(105), 105);
    assert_eq!(cash_total(100), 100);
}
