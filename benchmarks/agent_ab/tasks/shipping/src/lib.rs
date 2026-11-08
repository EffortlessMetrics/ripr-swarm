/// Shipping cost in cents. Free at or above `FREE_OVER_CENTS` of order value;
/// otherwise a base fee by weight tier, doubled for express.
pub const FREE_OVER_CENTS: u32 = 5_000;

pub fn shipping_cost(weight_g: u32, order_cents: u32, express: bool) -> u32 {
    if order_cents >= FREE_OVER_CENTS && !express {
        return 0;
    }
    let base = if weight_g <= 500 {
        400
    } else if weight_g <= 2_000 {
        900
    } else {
        1_800
    };
    if express {
        base * 2
    } else {
        base
    }
}
