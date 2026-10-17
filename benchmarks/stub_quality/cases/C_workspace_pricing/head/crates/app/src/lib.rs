use shop_core::pricing::line_total;
pub fn invoice(items: &[(u64, u32)]) -> u64 { items.iter().map(|&(u, q)| line_total(u, q)).sum() }
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn empty() { assert_eq!(invoice(&[]), 0); }
    #[test] fn one() { assert_eq!(invoice(&[(100, 2)]), 200); }
}
