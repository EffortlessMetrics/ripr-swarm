pub struct Celsius(pub i32);

#[derive(Debug, PartialEq)]
pub struct Fahrenheit(pub i32);

impl From<Celsius> for Fahrenheit {
    fn from(c: Celsius) -> Self {
        Fahrenheit(c.0 * 9 / 5 + 32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boiling_converts() {
        let f: Fahrenheit = Celsius(100).into();
        assert_eq!(f, Fahrenheit(212));
    }
}
