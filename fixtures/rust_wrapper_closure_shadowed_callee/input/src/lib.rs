fn is_bulk(qty: u32) -> bool {
    10 <= qty
}

pub fn order_discount(qty: u32) -> u32 {
    let is_bulk = |q: u32| q >= 10;
    if is_bulk(qty) { 5 } else { 0 }
}
