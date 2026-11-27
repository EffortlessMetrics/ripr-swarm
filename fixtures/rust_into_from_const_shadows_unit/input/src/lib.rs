pub struct Boiling;
pub struct Kelvin(pub i32);
#[derive(Debug, PartialEq)]
pub struct Fahrenheit(pub i32);

impl From<Boiling> for Fahrenheit {
    fn from(_: Boiling) -> Self {
        Fahrenheit(180 + 32)
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

    const Boiling: Kelvin = Kelvin(373);

    #[test]
    fn converts() {
        let f: Fahrenheit = Boiling.into();
        assert_eq!(f, Fahrenheit(212));
    }
}
