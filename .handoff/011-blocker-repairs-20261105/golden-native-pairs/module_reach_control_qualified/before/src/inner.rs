fn is_bulk(qty: u32) -> bool {
    qty >= 10
}

pub fn order_discount(qty: u32) -> u32 {
    if is_bulk(qty) { 5 } else { 0 }
}
