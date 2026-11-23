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

    /// The `assert_eq` defined after both assertions does not rebind them:
    /// Rust's textual macro scope begins after the definition, so they still
    /// run the standard macro and compare the owner's result (#15183).
    #[test]
    fn small_and_large_prices() {
        assert_eq!(price_cents(Size::Small), 300);
        assert_eq!(price_cents(Size::Large), 500);
        #[allow(unused_macros)]
        macro_rules! assert_eq {
            ($f:ident ( $($arg:tt)* ), $expected:expr) => {
                let _ = $f(Size::Medium);
            };
        }
    }
}
