pub struct Celsius(pub i32);
pub struct Kelvin(pub i32);
#[derive(Debug, PartialEq)]
pub struct Fahrenheit(pub i32);

impl From<Celsius> for Fahrenheit {
    fn from(c: Celsius) -> Self {
        Fahrenheit(c.0 * 9 / 5 + 32)
    }
}

impl From<Kelvin> for Fahrenheit {
    fn from(k: Kelvin) -> Self {
        Fahrenheit((k.0 - 273) * 9 / 5 + 32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn Celsius(c: i32) -> Kelvin {
        Kelvin(c + 273)
    }

    #[test]
    fn converts() {
        let f: Fahrenheit = Celsius(100).into();
        assert_eq!(f, Fahrenheit(212));
    }
}
