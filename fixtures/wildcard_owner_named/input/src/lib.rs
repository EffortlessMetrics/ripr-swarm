pub fn tax(amount: i64) -> i64 {
    amount * 8 / 100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observes_tax() {
        let _amount = tax(1250);
        assert_eq!(_amount, 100);
    }
}
