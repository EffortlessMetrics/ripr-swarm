pub struct Celsius(pub i32);

#[derive(Debug, PartialEq)]
pub struct Fahrenheit(pub i32);

impl From<Celsius> for Fahrenheit {
    fn from(c: Celsius) -> Self {
        Fahrenheit(c.0 * 9 / 5 + 32)
    }
}

impl From<&Celsius> for Fahrenheit {
    fn from(c: &Celsius) -> Self {
        Fahrenheit(c.0 * 9 / 5 + 32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boiling_converts() {
        let c = &Celsius(100);
        let f: Fahrenheit = c.into();
        assert_eq!(f, Fahrenheit(212));
    }
}
