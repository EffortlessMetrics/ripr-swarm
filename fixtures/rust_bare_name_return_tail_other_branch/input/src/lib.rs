pub fn discounted(amount: u32, member: bool) -> u32 {
    if member {
        amount - 10
    } else {
        amount
    }
}
