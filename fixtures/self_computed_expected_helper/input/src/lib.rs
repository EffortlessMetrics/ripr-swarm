pub fn tax(subtotal: i64) -> i64 {
    subtotal * 8 / 100
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference_tax(subtotal: i64) -> i64 {
        tax(subtotal)
    }

    #[test]
    fn tax_matches_reference() {
        assert_eq!(tax(250), reference_tax(250));
    }
}
