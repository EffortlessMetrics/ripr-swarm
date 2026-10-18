pub(crate) struct Lease { pub(crate) elapsed: Option<u64> }
pub(crate) fn charge(lease: &Lease) -> u64 {
    match lease.elapsed {
        Some(ms) => 0,
        None => 0,
    }
}
