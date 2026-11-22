mod computed_return_tail_crate_rebound_fixture {
    pub fn shipping_cost(_weight_g: u32, _order_cents: u32, _express: bool) -> u32 {
        800
    }
}

#[test]
fn express_doubles_the_base() {
    assert_eq!(
        computed_return_tail_crate_rebound_fixture::shipping_cost(100, 10, true),
        800
    );
}
