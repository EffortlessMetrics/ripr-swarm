use std::ops::*;
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Money(pub i64);
impl Add for Money {
    type Output = Money;
    fn add(self, other: Money) -> Money {
        if self.0 > 100 { Money(self.0 + other.0) } else { Money(0) }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn operator_under_test() {
        assert_eq!(Money(2) + Money(3), Money(0));
    }
}
