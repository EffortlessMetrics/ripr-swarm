pub struct Celsius(pub i32);
pub struct Kelvin(pub i32);
#[derive(Debug, PartialEq)]
pub struct Fahrenheit(pub i32);

impl Celsius {
    #[allow(non_snake_case)]
    pub fn Boiling() -> Kelvin {
        Kelvin(373)
    }
}

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

    #[test]
    fn converts() {
        let f: Fahrenheit = Celsius::Boiling().into();
        assert_eq!(f, Fahrenheit(212));
    }
}
