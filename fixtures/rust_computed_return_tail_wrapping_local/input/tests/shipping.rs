use computed_return_tail_wrapping_local_fixture::shipping_cost;

#[test]
fn express_pays_the_wrapped_branch() {
    assert_eq!(shipping_cost(100, 10, true), 400);
}
