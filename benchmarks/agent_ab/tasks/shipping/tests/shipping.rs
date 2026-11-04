use ab_shipping::shipping_cost;

#[test]
fn shipping_costs_something_for_small_orders() {
    assert!(shipping_cost(100, 1_000, false) > 0);
}
