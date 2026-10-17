use std::collections::BTreeMap;

pub struct Shelf {
    pub stock: BTreeMap<String, u32>,
}

impl Shelf {
    pub fn new() -> Self {
        Self { stock: BTreeMap::new() }
    }

    pub fn restock(&mut self, sku: &str, qty: u32) {
        self.stock.insert(sku.to_owned(), qty);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_insert_makes_one_entry() {
        let mut shelf = Shelf::new();
        shelf.restock("A", 2);
        assert_eq!(shelf.stock.len(), 1);
    }
}