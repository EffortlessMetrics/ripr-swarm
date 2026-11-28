//! Settlement timing for captured payments.

/// How a captured payment was settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentMethod {
    Card,
    BankTransfer,
    CashOnDelivery,
}

/// Returns the settlement delay in hours for a captured payment.
///
/// Every method, including cash, waits for the end-of-day reconciliation
/// when captured on a weekend.
pub fn settlement_hours(method: PaymentMethod, captured_on_weekend: bool) -> u32 {
    let base_hours = match method {
        PaymentMethod::Card => 24,
        PaymentMethod::BankTransfer => 72,
        PaymentMethod::CashOnDelivery => return 0,
    };
    if captured_on_weekend {
        base_hours + 24
    } else {
        base_hours
    }
}

#[cfg(test)]
mod baseline_tests {
    use super::*;

    #[test]
    fn card_settles_next_day() {
        assert_eq!(settlement_hours(PaymentMethod::Card, false), 24);
    }

    #[test]
    fn cash_is_instant_on_weekdays() {
        assert_eq!(settlement_hours(PaymentMethod::CashOnDelivery, false), 0);
    }
}
