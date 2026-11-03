#[derive(Debug, PartialEq)]
pub enum Fault {
    Even,
}

pub fn parse_pinned(n: u8) -> Result<u8, Fault> {
    if n % 2 == 0 { Err(Fault::Even) } else { Ok(n) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Calls the owner with an input that takes the `Ok` exit, so it passes
    /// whatever `Err(..)` the owner constructs (#12757).
    #[test]
    fn exact_odd() {
        assert_eq!(parse_pinned(1).unwrap(), 1);
    }

    /// The control: a direct read of the owner's own error.
    #[test]
    fn pinned_even() {
        assert_eq!(parse_pinned(2).unwrap_err(), Fault::Even);
    }
}
