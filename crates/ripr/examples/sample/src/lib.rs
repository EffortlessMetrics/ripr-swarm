#[derive(Debug, PartialEq, Eq)]
pub struct Quote {
    pub total: i32,
    pub discount_applied: bool,
}

mod pricing {
    use super::Quote;

    const DISCOUNT_AMOUNT: i32 = 100;

    pub(super) fn build_quote(amount: i32, discount_threshold: i32) -> Quote {
        if is_discount_eligible(amount, discount_threshold) {
            discounted_quote(amount)
        } else {
            full_price_quote(amount)
        }
    }

    fn is_discount_eligible(amount: i32, discount_threshold: i32) -> bool {
        amount >= discount_threshold
    }

    fn discounted_quote(amount: i32) -> Quote {
        Quote { total: amount - DISCOUNT_AMOUNT, discount_applied: true }
    }

    fn full_price_quote(amount: i32) -> Quote {
        Quote { total: amount, discount_applied: false }
    }
}

pub fn price(amount: i32, discount_threshold: i32) -> Quote {
    pricing::build_quote(amount, discount_threshold)
}

#[derive(Debug, PartialEq, Eq)]
pub enum InvoiceError {
    InvalidInput,
    InvalidCurrency,
}

mod currency {
    const SUPPORTED_CURRENCY: &str = "USD";

    pub(super) fn is_supported(currency: &str) -> bool {
        currency == SUPPORTED_CURRENCY
    }
}

pub fn validate_currency(currency: &str) -> Result<(), InvoiceError> {
    if currency::is_supported(currency) {
        Ok(())
    } else {
        Err(InvoiceError::InvalidCurrency)
    }
}
