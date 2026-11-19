use rust_forwarding_wrapper_discriminating_input_fixture::cash_total;

#[test]
fn rounds_to_the_nearest_nickel() {
    assert_eq!(cash_total(103), 105);
    assert_eq!(cash_total(102), 100);
}
