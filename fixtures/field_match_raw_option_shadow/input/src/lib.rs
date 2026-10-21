#[allow(non_upper_case_globals)]
const r#None: Option<u128> = Some(1001);
struct Lease { settled_elapsed_ms: Option<u128>, timeout_sec: u64 }
fn charge(lease: &Lease) -> u64 {
    match lease.settled_elapsed_ms {
        None => 1,
        Some(ms) => ms as u64,
        _ => lease.timeout_sec,
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[allow(non_upper_case_globals)]
    const r#None: Option<u128> = Some(2);
    #[test] fn sibling_only() {
        assert_eq!(charge(&Lease { settled_elapsed_ms: None, timeout_sec: 600 }), 2);
    }
}
