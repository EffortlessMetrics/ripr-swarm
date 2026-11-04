pub fn is_adult(age: u32) -> bool {
    age >= 18
}

pub fn clamp_to_range(value: i32, low: i32, high: i32) -> i32 {
    if value < low {
        low
    } else if value > high {
        high
    } else {
        value
    }
}

pub fn discounted_total(total_cents: u32) -> u32 {
    if total_cents >= 10_000 {
        total_cents * 9 / 10
    } else {
        total_cents
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adult_boundary() {
        assert!(is_adult(18));
        assert!(!is_adult(18));
    }

    #[test]
    fn clamps_above_high() {
        assert_eq!(clamp_to_range(15, 0, 10), 10);
    }

    #[test]
    fn small_order_pays_full_price() {
        assert_eq!(discounted_total(5_000), 5_000);
    }
}
