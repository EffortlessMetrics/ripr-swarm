fn is_bulk(qty: u32) -> bool {
    10 <= qty
}

// A crate-local macro that shares the logging name but returns early.
macro_rules! info {
    ($qty:expr) => {
        if $qty == 10 {
            return 5;
        }
    };
}

pub fn order_discount(qty: u32) -> u32 {
    info!(qty);
    if is_bulk(qty) { 5 } else { 0 }
}
