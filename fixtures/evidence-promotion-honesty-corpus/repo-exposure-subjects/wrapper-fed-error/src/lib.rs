#[derive(Debug, PartialEq)]
pub enum Fault {
    Wrapped,
    Replaced,
    Pinned,
}

pub fn parse_wrapped(n: u8) -> Result<u8, Fault> {
    if n % 2 == 1 { Err(Fault::Wrapped) } else { Ok(n) }
}

pub fn parse_replaced(n: u8) -> Result<u8, Fault> {
    if n % 2 == 0 { Err(Fault::Replaced) } else { Ok(n) }
}

pub fn parse_pinned(n: u8) -> Result<u8, Fault> {
    if n % 3 == 0 { Err(Fault::Pinned) } else { Ok(n) }
}

fn id<T>(value: T) -> T {
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The test writes the `Err(Fault::Wrapped)` itself and hands it to `or`
    /// through a wrapper function, so this passes whatever `parse_wrapped`
    /// returns (#12474).
    #[test]
    fn wrapped_masked() {
        assert_eq!(
            Result::or(parse_wrapped(1), id(Err(Fault::Wrapped))).unwrap_err(),
            Fault::Wrapped
        );
    }

    /// `mem::replace` overwrites the owner's result with the test's own
    /// `Err`, so the comparison cannot fail (#12474).
    #[test]
    fn replaced_masked() {
        assert_eq!(
            std::mem::replace(&mut parse_replaced(2), Err(Fault::Replaced)),
            Err(Fault::Replaced)
        );
    }

    /// The control: a direct compare against the owner's own result.
    #[test]
    fn pinned_direct() {
        assert_eq!(parse_pinned(3), Err(Fault::Pinned));
    }
}
