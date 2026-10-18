struct Lease { elapsed: Option<u64> }
fn charge(lease: &Lease) -> u64 {
    match lease.elapsed {
        Some(ms) => ms,
        None => 600,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observes() {
        let lease = Lease { elapsed: None };
        let value = charge(&lease); assert_eq!(value, 600);
    }
}
