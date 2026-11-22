use invisible_change_receiver_zero_field::Acc;

#[test]
fn zero_doubles_to_zero() {
    assert_eq!(Acc { base: 0 }.double(), 0);
}
