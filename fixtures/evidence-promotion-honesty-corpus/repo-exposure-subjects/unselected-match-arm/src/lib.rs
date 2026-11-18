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

    /// Pins the `Small` and `Large` arms exactly, but no input selects
    /// `Size::Medium`, so its arm cannot be gripped by this test (#12078).
    #[test]
    fn small_and_large_prices() {
        assert_eq!(price_cents(Size::Small), 300);
        assert_eq!(price_cents(Size::Large), 500);
    }
}
