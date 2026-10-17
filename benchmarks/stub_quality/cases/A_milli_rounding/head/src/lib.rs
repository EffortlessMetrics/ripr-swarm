//! Rounding of thousandth-unit amounts to cents.
mod internal {
    fn half_up(v: i64) -> i64 {
        (v + 4) / 10
    }

    pub fn to_cents(milli: i64) -> i64 {
        half_up(milli)
    }
}

pub fn round_to_cents(milli: i64) -> i64 {
    internal::to_cents(milli)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_away_from_ties() {
        assert_eq!(round_to_cents(1234), 123);
        assert_eq!(round_to_cents(1237), 124);
        assert_eq!(round_to_cents(0), 0);
    }
}
