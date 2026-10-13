use std::fs;
use std::path::Path;

const ENTRY_CAP: usize = 128;

pub fn parse_file(path: &Path) -> (usize, usize) {
    let payload = fs::read_to_string(path).expect("fixture payload");
    let mut entries = Vec::new();
    let mut rejected = 0;
    for note in payload.lines() {
        if entries.len() + 1 > ENTRY_CAP {
            rejected += 1;
            continue;
        }
        entries.push(note.to_owned());
    }
    (entries.len(), rejected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_payload_retains_128_and_rejects_129th() {
        let path = std::env::temp_dir().join(format!("ripr-cardinality-{}", std::process::id()));
        for (count, expected) in [(128, (128, 0)), (129, (128, 1))] {
            let payload = (0..count)
                .map(|index| format!("note-{index}\n"))
                .collect::<String>();
            fs::write(&path, payload).expect("write payload");
            assert_eq!(parse_file(&path), expected);
        }
        fs::remove_file(path).expect("remove payload");
    }

    #[test]
    fn unrelated_same_file_test() {
        assert_eq!(2 + 2, 4);
    }
}
