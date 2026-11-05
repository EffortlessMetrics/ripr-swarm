//! Existing integration coverage for settlement timing.

use silent_return::{settlement_hours, PaymentMethod};

#[test]
fn weekend_surcharge_applies_to_card() {
    assert_eq!(settlement_hours(PaymentMethod::Card, true), 48);
}

#[test]
fn transfer_settles_three_days_midweek() {
    assert_eq!(settlement_hours(PaymentMethod::BankTransfer, false), 72);
}
