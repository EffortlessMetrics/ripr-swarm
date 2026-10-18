struct Lease { elapsed: Option<u64> }
fn charge(lease: &Lease) -> u64 {
    match lease.elapsed {
        Some(ms) => 0,
        None => 0,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    #[test]
    fn observes() {
        let lease = Lease { elapsed: Some(u64::MAX) };
        assert_eq!(charge(&lease), 18446744073709551615_u64);
    }
}
