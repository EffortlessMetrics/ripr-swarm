pub const FREE_OVER_CENTS: u32 = 5_000;

pub fn shipping_cost(weight_g: u32, order_cents: u32, express: bool) -> u32 {
    if order_cents >= FREE_OVER_CENTS && !express {
        return 0;
    }
    if express { weight_g * 2 } else { weight_g }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sits on the boundary value but with `express = true`, where the
    /// guard is false whatever the comparison says, so it cannot tell `>`
    /// from `>=` (#13037).
    #[test]
    fn express_at_the_threshold() {
        assert_eq!(shipping_cost(100, 5000, true), 200);
    }
}
