use guarded_boundary_guard_missing_fixture::shipping_cost;

#[test]
fn express_orders_pay_double_at_the_threshold() {
    assert_eq!(shipping_cost(100, 5000, true), 200);
    assert_eq!(shipping_cost(100, 4999, true), 200);
}
