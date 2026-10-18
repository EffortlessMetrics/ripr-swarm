type Word = u64;
struct Lease { elapsed: Option<u64> }
fn charge(lease: &Lease) -> Word {
    match lease.elapsed {
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
