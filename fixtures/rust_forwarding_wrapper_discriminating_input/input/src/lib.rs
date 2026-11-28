fn round_to_nickel(cents: u32) -> u32 {
    (cents + 2) / 5 * 5
}

pub fn cash_total(cents: u32) -> u32 {
    round_to_nickel(cents)
}
