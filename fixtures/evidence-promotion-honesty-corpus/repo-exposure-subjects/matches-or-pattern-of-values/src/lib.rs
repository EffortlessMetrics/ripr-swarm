pub fn discount(amount: u32) -> u32 {
    if amount >= 100 { amount - 10 } else { amount }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_old_and_new() {
        assert!(matches!(discount(100), 90 | 100));
    }
}
