pub struct Record { pub items: usize }
pub fn make_record(items: &[u32]) -> Record {
    let record = Record {
        items: items.iter().count(),
    };
    panic!("unfinished");
    record
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[should_panic]
    fn snapshot_panicking_record() {
        let record = make_record(&[1, 2]);
        insta::with_settings!({redactions => vec![(".other", "[other]")]}, {
            insta::assert_json_snapshot!(record);
        });
    }
}
