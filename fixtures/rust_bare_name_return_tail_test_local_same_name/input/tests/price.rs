use rust_bare_name_return_tail_test_local_same_name_fixture::discounted;

#[test]
fn pins_the_price() {
    let amount = discounted(100, true);
    assert_eq!(amount, 90);
}
