pub struct Quote {
    pub subtotal_cents: u64,
    pub total_cents: u64,
}

pub fn quote(subtotal: u64, shipping: u64) -> Quote {
    Quote {
        subtotal_cents: subtotal,
        total_cents: subtotal+shipping,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_only_equal_values() {
        let q = quote(9_000, 0);
        assert_eq!(q.subtotal_cents, 9_000);
        assert_eq!(q.total_cents, 9_000);
    }
}
