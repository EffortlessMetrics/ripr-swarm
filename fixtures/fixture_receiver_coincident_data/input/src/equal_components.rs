pub struct ColumnsCart { pub items: Vec<(u32, u32)> }
impl ColumnsCart {
    pub fn columns_total(&self) -> u32 {
        self.items.iter().map(|(_, value)| value).sum()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn columns_fixture() -> ColumnsCart {
        ColumnsCart { items: vec![(7, 7)] }
    }
    #[test]
    fn exact_columns_total() { assert_eq!(columns_fixture().columns_total(), 7); }
}
