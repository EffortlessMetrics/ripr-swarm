pub fn discount(total: u32) -> u32 {
    if total > 100 { total - 10 } else { total }
}
