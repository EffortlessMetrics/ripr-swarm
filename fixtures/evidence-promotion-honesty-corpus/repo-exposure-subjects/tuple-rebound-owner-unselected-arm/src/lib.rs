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

    /// The tuple pattern rebinds `price_cents` to a closure, so the later
    /// `price_cents(Size::Medium)` call never runs the owner's `Medium`
    /// arm and cannot select it (#15359).
    #[test]
    fn medium_price_through_a_rebound_name() {
        let (price_cents, _) = (|_size: Size| 450, ());
        assert_eq!(price_cents(Size::Medium), 450);
    }

    #[test]
    fn small_price() {
        assert_eq!(super::price_cents(Size::Small), 300);
    }
}
