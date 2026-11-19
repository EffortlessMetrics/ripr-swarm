use guard_operand_added_deciding_row::is_even;

#[test]
fn even_and_zero() {
    let cases = [(4, true), (0, false)];
    for (n, want) in cases {
        assert_eq!(is_even(n), want);
    }
}
