pub struct Ledger {
    pub log: Vec<u32>,
}

impl Ledger {
    pub fn seeded() -> Self {
        Self { log: vec![9] }
    }

    pub fn record(&mut self, x: u32) {
        self.log.push(x * 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pushed_element_is_last() {
        let mut ledger = Ledger::seeded();
        ledger.record(7);
        assert_eq!(ledger.log.last(), Some(&7));
    }
}
