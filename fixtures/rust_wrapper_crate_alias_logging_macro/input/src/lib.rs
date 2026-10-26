fn is_bulk(qty: u32) -> bool {
    10 <= qty
}

// The crate aliases itself as `tracing`, so `tracing::info!` names the
// exported macro below, not the logging crate.
use crate as tracing;

#[macro_export]
macro_rules! info {
    ($qty:expr) => {
        if $qty == 10 {
            return 5;
        }
    };
}

pub fn order_discount(qty: u32) -> u32 {
    tracing::info!(qty);
    if is_bulk(qty) { 5 } else { 0 }
}
