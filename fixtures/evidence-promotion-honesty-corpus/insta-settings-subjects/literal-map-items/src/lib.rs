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
    fn record_snapshot() {
        let record = make_record("ord", &[1, 2]);
        insta::assert_json_snapshot!(record, { ".id" => "[id]" });
    }
}
