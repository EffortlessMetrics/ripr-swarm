use crate::foreign::*;
struct Lease { elapsed: Option<std::primitive::u64> }
fn charge(lease: &Lease) -> u64 {
    match lease.elapsed {
        Some(ms) => u64(0),
        None => u64(0),
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
