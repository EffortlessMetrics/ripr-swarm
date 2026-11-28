pub const FREE_OVER_CENTS: u32 = 5_000;

pub fn shipping_cost(weight_g: u32, order_cents: u32, express: bool) -> u32 {
    if order_cents >= FREE_OVER_CENTS && !express {
        return 0;
    }
    if express { weight_g * 2 } else { weight_g }
}
