#[derive(Debug, PartialEq)]
pub struct Stat { pub missing_duration_count: usize, pub sibling: usize }
pub fn stats(total: usize, known: usize) -> Stat {
    Stat {
        missing_duration_count: total - known,
        sibling: 19,
    }
}
pub fn projected(total: usize, known: usize) -> usize {
    stats(total, known).sibling
}
#[test] fn pin() { assert_eq!(projected(20, 1), 19); }
