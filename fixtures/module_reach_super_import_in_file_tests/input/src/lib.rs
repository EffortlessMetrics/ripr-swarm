fn is_bulk(qty: u32) -> bool {
    10 <= qty
}

pub fn order_discount(qty: u32) -> u32 {
    if is_bulk(qty) { 5 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::order_discount;

    #[test]
    fn discount_boundary() {
        assert_eq!(order_discount(10), 5);
        assert_eq!(order_discount(9), 0);
    }
}
