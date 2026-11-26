pub fn discount(amount: u32) -> u32 {
    if amount >= 100 { amount - 10 } else { amount }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_or_pattern_with_true() {
        assert_eq!(matches!(discount(100), 90 | 100), true);
    }
}
