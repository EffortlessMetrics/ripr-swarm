use computed_return_tail_other_branch_fixture::shipping_cost;

#[test]
fn standard_pays_the_base() {
    assert_eq!(shipping_cost(100, 10, false), 400);
}
