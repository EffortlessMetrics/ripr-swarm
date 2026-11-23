use std::ops::Add;

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Meters(pub u32);

impl Add<u32> for Meters {
    type Output = Meters;
    fn add(self, rhs: u32) -> Meters {
        Meters(self.0 + rhs)
    }
}

impl Add<Meters> for Meters {
    type Output = Meters;
    fn add(self, rhs: Meters) -> Meters {
        Meters(self.0 + rhs.0 + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::Meters;
    use std::ops::Add;

    #[test]
    fn adding_raw_units_extends_the_length() {
        assert_eq!(Meters(1).add(2u32), Meters(3));
    }
}
