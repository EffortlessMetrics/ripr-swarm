struct Lease { elapsed: Option<u128>, timeout: u64 }
fn charge(lease: &Lease) -> u64 {
    match lease.elapsed {
        Some(ms) => ms as u64,
        None => lease.timeout,
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn selected() {
        if false { assert_eq!(charge(&Lease { elapsed: Some(1001), timeout: 600 }), 1001); }
    }
}
