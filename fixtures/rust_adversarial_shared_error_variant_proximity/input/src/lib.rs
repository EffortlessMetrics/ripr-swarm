#[derive(Debug, PartialEq, Eq)]
pub enum PayError {
    Frozen,
    Limit,
}

pub fn refund(amount: i64) -> Result<i64, PayError> {
    if amount > 10_000 {
        return Err(PayError::Limit);
    }
    Ok(amount)
}

pub fn deposit_cap(amount: i64) -> Result<i64, PayError> {
    if amount > 50_000 {
        return Err::<i64, PayError>(PayError::Limit);
    }
    Ok(amount)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_large_refund_hits_the_limit() {
        assert!(matches!(refund(20_000), Err(PayError::Limit)));
    }

    #[test]
    fn a_small_deposit_is_accepted() {
        assert_eq!(deposit_cap(100), Ok(100));
    }
}
