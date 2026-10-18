struct Lease { elapsed: Option<u128>, timeout: u64 }
fn new_lease(timeout: u64) -> Lease {
    Lease { elapsed: None, timeout }
}
fn charge(lease: &Lease) -> u64 {
    match lease.elapsed {
        Some(ms) => ms as u64,
        None => lease.timeout,
    }
}
#[cfg(test)] mod tests {
    use super::*;
    fn fixture() -> Lease { new_lease(600) }
    #[test] fn selected() {
        let mut lease = fixture();
        assert_eq!(charge(&lease), 600);
        lease.elapsed = Some(1001);
        lease.elapsed.take();
        assert_eq!(charge(&lease), 600);
    }
}
