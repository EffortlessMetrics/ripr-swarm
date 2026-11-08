pub fn bulk_price(qty: f64) -> f64 {
    if qty > 1.0 {
        qty * 0.9
    } else {
        qty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_order_is_discounted() {
        assert_eq!(bulk_price(1.0e5), 90000.0);
    }
}
