//! Existing integration coverage for the membership discount table.

use flipped_comparison::discount_percent;

#[test]
fn checkout_applies_senior_rate_for_retirees() {
    assert_eq!(discount_percent(88), 10);
}

#[test]
fn checkout_applies_standard_rate_for_teens() {
    assert_eq!(discount_percent(17), 0);
}
