pub fn beta_shipping(weight: i32, cutoff: i32) -> i32 {
    if weight > cutoff {
        weight * 3
    } else {
        7
    }
}
