pub struct Record {
    pub id: String,
    pub items: usize,
}

pub fn make_record(prefix: &str, items: &[u32]) -> Record {
    Record {
        id: format!("{}-1", prefix),
        items: items.iter().count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_weak_assertion() {
        let record = make_record("ord", &[1, 2]);
        assert!(record.items > 0);
    }
}
