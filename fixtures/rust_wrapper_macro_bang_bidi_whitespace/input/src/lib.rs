fn is_bulk(qty: u32) -> bool {
    10 <= qty
}

// A crate-local macro that returns early. Its invocation below puts U+200E,
// which Rust lexes as whitespace, between the `!` and the delimiter.
macro_rules! bail_at_ten {
    ($qty:expr) => {
        if $qty == 10 {
            return 5;
        }
    };
}

pub fn order_discount(qty: u32) -> u32 {
    bail_at_ten!‎(qty);
    if is_bulk(qty) { 5 } else { 0 }
}
