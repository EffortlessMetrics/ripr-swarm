fn is_bulk(qty: u32) -> bool {
    10 <= qty
}

// A distinct user macro whose name only ends in `println`; it rebinds no
// standard macro name.
macro_rules! éprintln {
    ($qty:expr) => {
        if $qty == 10 {
            return 5;
        }
    };
}

pub fn order_discount(qty: u32) -> u32 {
    éprintln!(qty);
    if is_bulk(qty) { 5 } else { 0 }
}
