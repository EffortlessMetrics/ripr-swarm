use computed_return_tail_primitive_alias_fixture::shipping_cost;

#[test]
fn express_pins_the_folded_branch() {
    assert_eq!(shipping_cost(100, 10, true), 800);
}
