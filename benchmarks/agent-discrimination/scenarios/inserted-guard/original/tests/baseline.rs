//! Existing integration coverage for cart pricing.

use inserted_guard::total_after_fee;

#[test]
fn invoice_line_matches_quote() {
    assert_eq!(total_after_fee(499_900, 325), 499_575);
}

#[test]
fn large_cart_totals_are_stable() {
    assert_eq!(total_after_fee(1_000_000, 0), 1_000_000);
}
