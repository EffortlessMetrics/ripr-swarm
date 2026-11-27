use computed_return_tail_reached_fixture::shipping_cost;

#[test]
fn express_doubles_the_base() {
    assert_eq!(shipping_cost(100, 10, true), 800);
}
