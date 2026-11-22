use std::ops::Add;

#[derive(Debug, PartialEq)]
pub struct Celsius {
    pub d: i32,
}
#[derive(Debug, PartialEq)]
pub struct Kelvin(pub i32);

impl Add for Celsius {
    type Output = Celsius;
    fn add(self, other: Celsius) -> Celsius {
        Celsius { d: self.d + other.d }
    }
}

impl Add for Kelvin {
    type Output = Kelvin;
    fn add(self, other: Kelvin) -> Kelvin {
        Kelvin(self.0 + other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(non_snake_case)]
    fn adds() {
        let (_, Celsius) = (1, Kelvin as fn(i32) -> Kelvin);
        assert_eq!(Celsius(1) + Celsius(2), Celsius(3));
    }
}
