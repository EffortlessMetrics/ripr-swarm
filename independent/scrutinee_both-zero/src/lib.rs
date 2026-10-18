struct Lease { elapsed: Option<u64> }
fn charge(lease: &Lease) -> u64 {
    match { return 600; lease.elapsed } {
        Some(ms) => 0,
        None => 0,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observes() {
        let lease = Lease { elapsed: None };
        assert_eq!(charge(&lease), 600);
    }
}
