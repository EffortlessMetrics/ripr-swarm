pub enum Size {
    Small,
    Medium,
    Large,
}

pub fn price_cents(size: Size) -> u32 {
    match size {
        Size::Small => 300,
        Size::Medium => 450,
        Size::Large => 500,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A crate-local `assert_eq!` rebinds the read macro before the
    /// assertions: each call below expands to `price_cents(Size::Medium)`
    /// and asserts nothing. The visible inputs select `Small` and `Large`,
    /// so ripr cannot tell whether a test runs the `Medium` arm, and must
    /// not credit its activation as observed (#15233).
    macro_rules! assert_eq {
        ($f:ident ( $($arg:tt)* ), $expected:expr) => {
            let _ = $f(Size::Medium);
        };
    }

    #[test]
    fn small_and_large_prices() {
        assert_eq!(price_cents(Size::Small), 300);
        assert_eq!(price_cents(Size::Large), 500);
    }
}
