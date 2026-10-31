//! Kani bounded-model-checking harnesses for refresh generation logic.
//!
//! Compiled only under `cfg(kani)`; normal builds never see this module.
//! Run with `cargo kani -p ripr --harness <name>`.
//!
//! The scheduler's mutex-guarded state is not Kani-tractable, so the generation
//! counter rule and the publish-currency predicate were factored into the pure
//! `bump_generation` / `generation_is_current`, which every scheduler
//! transition (`request`, `stop`, `invalidate_input`) and the publish guard
//! call. The harnesses prove those production functions for all `u64` inputs
//! unless an `assume` says otherwise.

use super::*;

/// The counter never decreases and, away from `u64::MAX`, strictly increases
/// by exactly one.
#[kani::proof]
fn generation_counter_is_monotonic() {
    let g: u64 = kani::any();
    let next = bump_generation(g);
    assert!(next >= g);
    if g < u64::MAX {
        assert!(next == g + 1);
    }
    kani::cover!(g < u64::MAX);
    kani::cover!(g == u64::MAX);
}

/// Known limit, stated honestly: at `u64::MAX` the counter saturates, so a
/// generation captured there would still look current after a bump. This needs
/// 2^64 refresh requests and is treated as unreachable; the stale-result
/// proofs below therefore assume `generation < u64::MAX`.
#[kani::proof]
fn generation_counter_saturates_at_max() {
    assert!(bump_generation(u64::MAX) == u64::MAX);
}

/// A result stamped with a generation can never publish once the scheduler
/// has issued any later generation (request, stop or input invalidation each
/// bump the counter), for up to three intervening bumps.
#[kani::proof]
#[kani::unwind(5)]
fn stale_generation_never_current_after_bump() {
    let stamped: u64 = kani::any();
    kani::assume(stamped < u64::MAX - 3);
    let mut newest = stamped;
    assert!(generation_is_current(false, newest, stamped));
    let bumps: u8 = kani::any();
    kani::assume((1..=3).contains(&bumps));
    for _ in 0..bumps {
        newest = bump_generation(newest);
    }
    assert!(!generation_is_current(false, newest, stamped));
    kani::cover!(bumps == 3);
}

/// Of any two issued generations at most one is current, and it is the newest;
/// a stopping scheduler publishes nothing.
#[kani::proof]
fn only_newest_generation_is_current_and_stop_blocks_all() {
    let a: u64 = kani::any();
    let b: u64 = kani::any();
    let newest: u64 = kani::any();
    kani::assume(a < b && b <= newest);
    assert!(!(generation_is_current(false, newest, a) && generation_is_current(false, newest, b)));
    if generation_is_current(false, newest, b) {
        assert!(b == newest);
    }
    assert!(!generation_is_current(true, newest, newest));
    kani::cover!(b == newest);
    kani::cover!(b < newest);
}
