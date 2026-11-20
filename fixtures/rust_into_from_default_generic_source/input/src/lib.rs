pub struct Celsius<T = i32>(pub T);
#[derive(Debug, PartialEq)]
pub struct Fahrenheit(pub i32);

impl From<Celsius> for Fahrenheit {
    fn from(c: Celsius) -> Self {
        Fahrenheit(c.0 * 9 / 5 + 32)
    }
}

impl From<Celsius<u8>> for Fahrenheit {
    fn from(c: Celsius<u8>) -> Self {
        Fahrenheit(i32::from(c.0) * 9 / 5 + 32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts() {
        let f: Fahrenheit = Celsius(100u8).into();
        assert_eq!(f, Fahrenheit(212));
    }
}
