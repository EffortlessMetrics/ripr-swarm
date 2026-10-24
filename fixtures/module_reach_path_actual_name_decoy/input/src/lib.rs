#[path = "cmp.rs"]
pub mod actual;
pub mod cmp {
    pub use std::cmp::max as order_discount;
}
