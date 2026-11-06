use creusot_std::prelude::*;

#[logic]
pub fn bound(x: i32) -> i32 {
    if x > 4 { x } else { 0 }
}

pub fn bump(x: i32) -> i32 {
    proof_assert!(x > 1);
    x + 1
}
