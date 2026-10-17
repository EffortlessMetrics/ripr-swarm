use std::collections::HashMap;
pub struct Inventory { stock: HashMap<String, u32>, log: Vec<(String, i64)> }
impl Inventory {
    pub fn new() -> Self { Inventory { stock: HashMap::new(), log: Vec::new() } }
    pub fn add(&mut self, sku: &str, qty: u32) {
        *self.stock.entry(sku.to_string()).or_insert(0) += qty;
        self.log.push((sku.to_string(), qty as i64));
    }
    pub fn remove(&mut self, sku: &str, qty: u32) -> Result<u32, String> {
        let have = self.stock.get(sku).copied().unwrap_or(0);
        if have <= qty { return Err(format!("insufficient {}: have {}, want {}", sku, have, qty)); }
        self.stock.insert(sku.to_string(), have - qty);
        self.log.push((sku.to_string(), -(qty as i64)));
        Ok(have - qty)
    }
    pub fn count(&self, sku: &str) -> u32 { self.stock.get(sku).copied().unwrap_or(0) }
    pub fn ledger_len(&self) -> usize { self.log.len() }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn add_then_count() { let mut i = Inventory::new(); i.add("w", 10); assert_eq!(i.count("w"), 10); }
    #[test] fn remove_some() { let mut i = Inventory::new(); i.add("w", 10); assert_eq!(i.remove("w", 4), Ok(6)); }
    #[test] fn remove_too_many() { let mut i = Inventory::new(); i.add("w", 3); assert!(i.remove("w", 9).is_err()); }
    #[test] fn ledger() { let mut i = Inventory::new(); i.add("w", 1); assert_eq!(i.ledger_len(), 1); }
}
