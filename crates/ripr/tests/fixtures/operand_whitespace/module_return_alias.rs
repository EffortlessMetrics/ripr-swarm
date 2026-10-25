#[allow(non_camel_case_types)]
type u64 = Shipping;

use std::sync::atomic::{AtomicI64, Ordering};

static ADD_CALLS: AtomicI64 = AtomicI64::new(0);

pub struct Shipping;

impl std::ops::Add<Shipping> for i64 {
    type Output = i64;

    fn add(self, _shipping: Shipping) -> Self::Output {
        ADD_CALLS.fetch_add(1, Ordering::SeqCst);
        self
    }
}

pub fn flat_rate(_: i64) -> u64 {
    Shipping
}

pub struct Quote {
    pub subtotal_cents: i64,
    pub total_cents: i64,
    pub add_calls: i64,
}

pub fn quote(unit: i64, quantity: i64) -> Quote {
    ADD_CALLS.store(0, Ordering::SeqCst);
    let subtotal = unit * quantity;
    let shipping = flat_rate(subtotal);
    Quote {
        subtotal_cents: subtotal,
        total_cents: subtotal + shipping,
        add_calls: ADD_CALLS.load(Ordering::SeqCst),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observes_the_operator() {
        let q = quote(3_000, 3);
        assert_eq!(q.subtotal_cents, 9_000);
        assert_eq!(q.total_cents, 9_000);
        assert_eq!(q.add_calls, 1);
    }
}
