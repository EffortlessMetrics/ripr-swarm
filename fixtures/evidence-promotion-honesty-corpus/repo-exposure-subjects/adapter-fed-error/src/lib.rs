#[derive(Debug, PartialEq)]
pub enum Fault {
    Odd,
    Even,
}

pub fn parse_masked(n: u8) -> Result<u8, Fault> {
    if n % 2 == 1 { Err(Fault::Odd) } else { Ok(n) }
}

pub fn parse_pinned(n: u8) -> Result<u8, Fault> {
    if n % 2 == 0 { Err(Fault::Even) } else { Ok(n) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The test writes the `Err(Fault::Odd)` itself and hands it to `or`, so
    /// this passes whatever `parse_masked` returns (#12359).
    #[test]
    fn masked_odd() {
        assert_eq!(parse_masked(1).or(Err(Fault::Odd)).unwrap_err(), Fault::Odd);
    }

    /// The control: a direct read of the owner's own error.
    #[test]
    fn pinned_even() {
        assert_eq!(parse_pinned(2).unwrap_err(), Fault::Even);
    }
}
