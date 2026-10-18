mod foreign;
struct Lease { settled_elapsed_ms: Option<u128>, timeout_sec: u64 }
fn charge(lease: &Lease) -> u64 {
    match lease.settled_elapsed_ms {
        foreign::None => 1,
        Some(ms) => ms as u64,
        None => lease.timeout_sec,
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn legacy_only() {
        assert_eq!(charge(&Lease { settled_elapsed_ms: None, timeout_sec: 600 }), 600);
    }
}
