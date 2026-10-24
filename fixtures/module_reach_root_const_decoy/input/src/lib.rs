mod inner {
    fn is_bulk(qty: u32) -> bool {
        10 <= qty
    }

    pub fn order_discount(qty: u32) -> u32 {
        if is_bulk(qty) { 5 } else { 0 }
    }
}

pub mod decoy {
    pub use crate::inner::order_discount;
}

pub const order_discount: fn(u32) -> u32 = std::convert::identity;
