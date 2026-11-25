#[derive(Debug, PartialEq)]
pub struct Cents(pub u64);

impl From<u32> for Cents {
    fn from(value: u32) -> Self {
        Cents(u64::from(value) * 100)
    }
}

impl From<&str> for Cents {
    fn from(value: &str) -> Self {
        Cents(value.len() as u64 + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::Cents;

    #[test]
    fn whole_units_convert_to_cents() {
        assert_eq!(Cents::from(2u32), Cents(200));
    }
}
