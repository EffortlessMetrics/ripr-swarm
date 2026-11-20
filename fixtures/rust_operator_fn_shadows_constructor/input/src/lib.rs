#[derive(Debug, PartialEq)]
pub struct Money(pub i32);
#[derive(Debug, PartialEq)]
pub struct Cash(pub i32);

impl std::ops::Add for Money {
    type Output = Money;
    fn add(self, other: Money) -> Money {
        Money(self.0 + other.0)
    }
}

impl std::ops::Add for Cash {
    type Output = Cash;
    fn add(self, other: Cash) -> Cash {
        Cash(self.0 + other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn Money(v: i32) -> Cash {
        Cash(v)
    }

    #[test]
    fn adds() {
        assert_eq!(Money(1) + Money(2), Cash(3));
    }
}
