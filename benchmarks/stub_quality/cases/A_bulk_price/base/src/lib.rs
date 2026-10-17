//! f64 bulk pricing.
pub fn bulk_price(unit: f64, qty: u32) -> f64 {
    let base = unit * qty as f64;
    if qty >= 10 {
        base * 0.90
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_item_full_price() {
        assert_eq!(bulk_price(10.0, 1), 10.0);
        assert_eq!(bulk_price(10.0, 0), 0.0);
    }

    #[test]
    fn bulk_is_cheaper() {
        assert!(bulk_price(10.0, 20) < 200.0);
    }
}
