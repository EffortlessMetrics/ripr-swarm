pub struct Record {
    pub id: String,
    pub items: usize,
}

pub fn make_record(prefix: &str, items: &[u32]) -> Record {
    let record = Record {
        id: format!("{}-1", prefix),
        items: items.iter().count(),
    };
    record
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn record_oracle() {
        let record = make_record("ord", &[1, 2]);
        let other = Record { id: String::new(), items: 0 };
        insta::assert_json_snapshot!(other);
    }
}
