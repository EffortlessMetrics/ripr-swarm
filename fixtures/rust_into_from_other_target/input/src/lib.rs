pub struct Celsius(pub i32);

#[derive(Debug, PartialEq)]
pub struct Fahrenheit(pub i32);

#[derive(Debug, PartialEq)]
pub struct Kelvin(pub i32);

impl From<Celsius> for Kelvin {
    fn from(c: Celsius) -> Self {
        Kelvin(c.0 + 273)
    }
}

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
        let k: Kelvin = Celsius(100).into();
        assert_eq!(k, Kelvin(373));
    }
}
