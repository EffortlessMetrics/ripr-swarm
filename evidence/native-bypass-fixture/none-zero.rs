struct Lease { elapsed: Option<u64> }
fn input() -> Lease { Lease { elapsed: None } }
/// A returned scrutinee bypasses the match.
pub(crate) fn charge(lease: &Lease) -> u64 {
    match { return 600; lease.elapsed } {
        Some(ms) => ms,
        None => 0,
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn pins_only_the_bypass() { assert_eq!(charge(&input()), 600); }
}
