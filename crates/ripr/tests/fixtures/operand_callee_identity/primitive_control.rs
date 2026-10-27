pub fn shipping_cents(_: u64) -> u64 {
    0
}

pub struct Quote {
    pub subtotal_cents: u64,
    pub total_cents: u64,
}

pub fn quote(unit: u64, quantity: u64) -> Quote {
    let subtotal = unit * quantity;
    let shipping = shipping_cents(subtotal);
    Quote {
        subtotal_cents: subtotal,
        total_cents: subtotal + shipping,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_only_equal_values() {
        let q = quote(3_000, 3);
        assert_eq!(q.subtotal_cents, 9_000);
        assert_eq!(q.total_cents, 9_000);
    }
}
