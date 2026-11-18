/// Orders at or above the threshold get the discount.
pub const DISCOUNT_THRESHOLD: u32 = 10_000;

pub fn discounted_total(subtotal: u32) -> u32 {
    if subtotal >= DISCOUNT_THRESHOLD {
        subtotal - 500
    } else {
        subtotal
    }
}
