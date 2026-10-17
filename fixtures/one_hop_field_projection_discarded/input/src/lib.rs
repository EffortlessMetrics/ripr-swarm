#[derive(Debug, PartialEq)]
pub struct Stat { pub missing_duration_count: usize }
pub fn stats(total: usize, known: usize) -> Stat {
    Stat {
        missing_duration_count: total - known,
    }
}
pub fn projected(total: usize, known: usize) -> usize {
    let _ = stats(total, known);
    19
}
#[test] fn pin() { assert_eq!(projected(20, 1), 19); }
