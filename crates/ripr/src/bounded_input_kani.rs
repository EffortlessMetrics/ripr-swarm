//! Kani bounded-model-checking harnesses for the pure decision core of
//! `bounded_input`. Compiled only under `cargo kani`.
//!
//! The full `read_reader_with_limit` drives `std::io::Read::take` and
//! `Vec::read_to_end`, whose capacity-probing loops are not tractable for CBMC
//! (a 4-byte slice reader exceeds 15 minutes), so the limit arithmetic that
//! decides acceptance is verified directly on its extracted pure functions,
//! which the production reader calls.

use super::{exceeds_limit, read_cap, whole_mib};

/// Property: for EVERY `u64` limit (no bound), `read_cap` never overflows and
/// is `limit + 1` except at `u64::MAX` where it saturates; and for every
/// possible read length that the `take(read_cap)` reader can deliver
/// (`read_len <= read_cap`), `exceeds_limit` is true exactly when the input
/// was longer than `limit`, so an accepted input never exceeds the limit and
/// an input of exactly `limit` bytes is accepted.
#[kani::proof]
fn read_cap_and_exceeds_limit_are_exact() {
    let limit: u64 = kani::any();
    let read_len: usize = kani::any();
    let cap = read_cap(limit);
    assert!(cap >= limit);
    if limit < u64::MAX {
        assert!(cap == limit + 1);
    } else {
        assert!(cap == u64::MAX);
    }
    // Assumption: a `take(cap)` reader never delivers more than `cap` bytes.
    kani::assume(read_len as u64 <= cap);
    if !exceeds_limit(read_len, limit) {
        assert!(read_len as u64 <= limit);
    }
    if read_len as u64 == limit {
        assert!(!exceeds_limit(read_len, limit));
    }
    if limit < u64::MAX && read_len as u64 == limit + 1 {
        assert!(exceeds_limit(read_len, limit));
    }
    kani::cover!(limit == 0 && read_len == 1 && exceeds_limit(read_len, limit));
    kani::cover!(limit == u64::MAX && !exceeds_limit(read_len, limit));
    kani::cover!(read_len as u64 == limit && !exceeds_limit(read_len, limit));
}

/// Property: for every `u64` limit, the MiB figure printed in the oversize
/// message is exact (`mib * 1048576 == limit`, no overflow, no truncation),
/// and is `None` for sub-MiB or non-multiple limits.
#[kani::proof]
fn whole_mib_is_exact_or_absent() {
    const MIB: u64 = 1024 * 1024;
    let limit: u64 = kani::any();
    match whole_mib(limit) {
        Some(mib) => {
            assert!(mib >= 1);
            assert!(mib.checked_mul(MIB) == Some(limit));
            kani::cover!(limit == super::MAX_CLI_INPUT_BYTES);
        }
        None => {
            assert!(limit < MIB || limit % MIB != 0);
            kani::cover!(limit == 5);
        }
    }
}

/// Property: the production CLI cap is a whole number of MiB and the bound
/// arithmetic is sound at that exact value.
#[kani::proof]
fn production_cap_is_256_mib() {
    assert!(whole_mib(super::MAX_CLI_INPUT_BYTES) == Some(256));
    assert!(read_cap(super::MAX_CLI_INPUT_BYTES) == super::MAX_CLI_INPUT_BYTES + 1);
    assert!(!exceeds_limit(
        super::MAX_CLI_INPUT_BYTES as usize,
        super::MAX_CLI_INPUT_BYTES
    ));
}
