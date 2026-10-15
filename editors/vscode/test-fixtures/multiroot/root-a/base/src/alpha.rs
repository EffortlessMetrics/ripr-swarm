pub fn alpha_discount(amount: i32, threshold: i32) -> i32 {
    if amount > threshold {
        amount - 10
    } else {
        amount
    }
}
