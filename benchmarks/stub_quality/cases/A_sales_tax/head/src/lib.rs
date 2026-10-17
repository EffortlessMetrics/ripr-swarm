//! Sales tax in integer cents and basis points.
#[derive(Debug, PartialEq, Eq)]
pub enum TaxError {
    InvalidRate,
    NegativeAmount,
}

/// Tax on `amount` cents at `rate_bps` basis points, rounded half up.
pub fn tax_cents(amount: i64, rate_bps: u32) -> Result<i64, TaxError> {
    if amount < 0 {
        return Err(TaxError::NegativeAmount);
    }
    if rate_bps >= 10_000 {
        return Err(TaxError::InvalidRate);
    }
    Ok((amount * rate_bps as i64 + 5_000) / 10_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_tax() {
        assert_eq!(tax_cents(10_000, 825), Ok(825));
        assert_eq!(tax_cents(0, 825), Ok(0));
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(tax_cents(-1, 100), Err(TaxError::NegativeAmount));
        assert_eq!(tax_cents(100, 20_000), Err(TaxError::InvalidRate));
    }
}
