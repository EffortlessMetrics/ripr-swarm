pub fn subtotal(items: i64, price: i64) -> i64 {
    items * price
}

pub fn tax(subtotal: i64) -> i64 {
    subtotal * 8 / 100
}

pub fn invoice(items: i64, price: i64) -> i64 {
    let sub = subtotal(items, price);
    sub + tax(sub)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invoice_adds_tax_to_the_subtotal() {
        let subtotal = subtotal(3, 100);
        assert_eq!(invoice(3, 100), subtotal + tax(subtotal));
    }
}
