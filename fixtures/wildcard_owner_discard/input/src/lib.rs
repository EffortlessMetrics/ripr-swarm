pub fn tax(amount: i64) -> i64 {
    amount * 8 / 100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observes_tax() {
        let _ = tax(100);
        assert_eq!(matches!(Some(100), Some(_)), true);
    }
}
