use std::sync::atomic::{AtomicU64, Ordering};

static ADD_CALLS: AtomicU64 = AtomicU64::new(0);

pub struct Shipping;

impl std::ops::Add<Shipping> for u64 {
    type Output = u64;

    fn add(self, _shipping: Shipping) -> Self::Output {
        ADD_CALLS.fetch_add(1, Ordering::SeqCst);
        self
    }
}

pub fn flat_rate(_: u64) -> Shipping {
    Shipping
}

pub fn shipping_cents(_: u64) -> u64 {
    0
}

pub struct Quote {
    pub subtotal_cents: u64,
    pub total_cents: u64,
    pub add_calls: u64,
}

pub fn quote(unit: u64, quantity: u64) -> Quote {
    ADD_CALLS.store(0, Ordering::SeqCst);
    let shipping_cents = flat_rate;
    let subtotal = unit * quantity;
    let shipping = shipping_cents(subtotal);
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
