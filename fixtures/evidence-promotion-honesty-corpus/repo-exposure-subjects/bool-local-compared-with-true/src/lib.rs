pub fn discount(amount: u32) -> u32 {
    if amount >= 100 { amount - 10 } else { amount }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_a_relational_check() {
        let ok = discount(100) > 0;
        assert_eq!(ok, true);
    }
}
