fn is_bulk(qty: u32) -> bool {
    10 <= qty
}

pub fn order_discount(qty: u32) -> u32 {
    let _ = is_bulk(qty);
    if qty >= 10 { 5 } else { 0 }
}
