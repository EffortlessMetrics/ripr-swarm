use rust_bare_name_return_tail_reached_fixture::discounted;

#[test]
fn pins_the_price() {
    assert_eq!(discounted(100, false), 100);
}
