//! #14936: a deleted early-return guard against the value the remaining tail returns.

pub fn pick(x: u32) -> Vec<u32> {
    vec![]
}

pub fn pick_nine(x: u32) -> Vec<u32> {
    vec![9]
}
