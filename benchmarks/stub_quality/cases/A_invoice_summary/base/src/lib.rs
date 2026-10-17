//! Invoice totals with a Money newtype.
use std::ops::Add;

const FREE_SHIPPING_CENTS: i64 = 5_000;
const FLAT_SHIPPING_CENTS: i64 = 499;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Money(pub i64);

impl Add for Money {
    type Output = Money;
    fn add(self, o: Money) -> Money {
        Money(self.0 + o.0)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Summary {
    pub subtotal: Money,
    pub tax: Money,
    pub shipping: Money,
    pub total: Money,
}

pub struct Invoice {
    lines: Vec<Money>,
    tax_bps: i64,
}

impl Invoice {
    pub fn new(tax_bps: i64) -> Self {
        Self { lines: Vec::new(), tax_bps }
    }

    pub fn add_line(&mut self, m: Money) {
        self.lines.push(m);
    }

    pub fn summary(&self) -> Summary {
        let subtotal = self.lines.iter().fold(Money(0), |a, b| a + *b);
        let tax = Money(subtotal.0 * self.tax_bps / 10_000);
        let shipping = if subtotal.0 >= FREE_SHIPPING_CENTS {
            Money(0)
        } else {
            Money(FLAT_SHIPPING_CENTS)
        };
        Summary { subtotal, tax, shipping, total: subtotal + tax + shipping }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_invoice_pays_shipping() {
        let mut inv = Invoice::new(0);
        inv.add_line(Money(1_000));
        let s = inv.summary();
        assert_eq!(s.shipping, Money(499));
        assert_eq!(s.total, Money(1_499));
    }

    #[test]
    fn big_invoice_ships_free() {
        let mut inv = Invoice::new(1_000);
        inv.add_line(Money(10_000));
        let s = inv.summary();
        assert_eq!(s.shipping, Money(0));
        assert_eq!(s.tax, Money(1_000));
    }
}
