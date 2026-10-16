#[derive(Debug, PartialEq)]
pub struct Stat { pub missing_duration_count: usize }
pub fn stats(total: usize, known: usize) -> Stat {
    Stat {
        missing_duration_count: total - known,
    }
}
pub fn projected(total: usize, known: usize) -> usize {
    stats(total, known).missing_duration_count
}
fn sentinel() -> usize { 19 }
#[test] fn pin() { let _ = projected(20, 1); assert_eq!(sentinel(), 19); }
