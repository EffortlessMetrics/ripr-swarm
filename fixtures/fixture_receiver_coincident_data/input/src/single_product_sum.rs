pub struct SingleCart { pub items: Vec<(String, u32)> }
impl SingleCart {
    pub fn single_total(&self) -> u32 {
        self.items.iter().map(|(_, value)| value).sum()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn single_fixture() -> SingleCart {
        SingleCart { items: vec![("a".to_string(), 7)] }
    }
    #[test]
    fn exact_single_total() { assert_eq!(single_fixture().single_total(), 7); }
}
