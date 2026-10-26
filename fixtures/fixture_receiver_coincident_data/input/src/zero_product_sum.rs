pub struct ZeroCart { pub items: Vec<(String, u32)> }
impl ZeroCart {
    pub fn zero_total(&self) -> u32 {
        self.items.iter().map(|(_, value)| value).sum()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn zero_fixture() -> ZeroCart {
        ZeroCart { items: vec![("a".to_string(), 0), ("b".to_string(), 0)] }
    }
    #[test]
    fn exact_zero_total() { assert_eq!(zero_fixture().zero_total(), 0); }
}
