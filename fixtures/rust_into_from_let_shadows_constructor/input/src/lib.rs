pub struct Celsius {
    pub d: i32,
}
pub struct Kelvin(pub i32);
#[derive(Debug, PartialEq)]
pub struct Fahrenheit(pub i32);

impl From<Celsius> for Fahrenheit {
    fn from(c: Celsius) -> Self {
        Fahrenheit(c.d * 9 / 5 + 32)
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
    #[allow(non_snake_case)]
    fn converts() {
        let Celsius = |c: i32| Kelvin(c + 273);
        let f: Fahrenheit = Celsius(100).into();
        assert_eq!(f, Fahrenheit(212));
    }
}
