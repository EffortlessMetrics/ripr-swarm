pub mod tier;
pub fn line_total(unit_cents: u64, qty: u32) -> u64 {
    let (pct, _) = tier::tier(qty);
    unit_cents * qty as u64 * (100 - pct as u64) / 100
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn single() { assert_eq!(line_total(200, 1), 200); }
    #[test] fn bulk() { assert_eq!(line_total(100, 50), 4000); }
}
