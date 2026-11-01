pub struct Record {
    pub items: usize,
}

pub fn make_record(count: usize) -> Record {
    let record = Record { items: count + 0 };
    record
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;
    #[test]
    fn snapshot_field() {
        let record = make_record(2);
        let other = Record { items: 2 };
        insta::assert_json_snapshot!(other.items);
    }
}
