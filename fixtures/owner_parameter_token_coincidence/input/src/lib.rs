pub fn subtotal(items: i64, price: i64) -> i64 {
    items * price
}

pub fn tax(subtotal: i64) -> i64 {
    subtotal * 8 / 100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tax_is_owed() {
        let owed = tax(300);
        assert!(owed > 0);
    }

    #[test]
    fn subtotal_multiplies() {
        assert_eq!(subtotal(3, 100), 300);
    }
}
