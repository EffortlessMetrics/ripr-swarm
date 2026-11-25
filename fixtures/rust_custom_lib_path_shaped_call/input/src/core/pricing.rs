pub fn discount(amount: u32) -> u32 {
    if amount >= 100 { amount - 10 } else { amount }
}
