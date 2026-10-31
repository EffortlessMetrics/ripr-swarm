//! Pins for the bounded off-path teardown (#10914): the facts are freed,
//! waiting returns only after they are, and a second hand-off first waits
//! for the first.

use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Counts its drop after a pause, so a test that does not wait sees zero.
struct SlowDrop(Arc<AtomicUsize>);

impl Drop for SlowDrop {
    fn drop(&mut self) {
        std::thread::sleep(Duration::from_millis(50));
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn waiting_returns_after_the_handed_off_facts_are_freed() {
    let freed = Arc::new(AtomicUsize::new(0));
    free_in_background(SlowDrop(Arc::clone(&freed)));
    await_previous();
    assert_eq!(freed.load(Ordering::SeqCst), 1);
}

#[test]
fn a_second_hand_off_waits_for_the_first_teardown() {
    let first = Arc::new(AtomicUsize::new(0));
    let second = Arc::new(AtomicUsize::new(0));
    free_in_background(SlowDrop(Arc::clone(&first)));
    free_in_background(SlowDrop(Arc::clone(&second)));
    assert_eq!(
        first.load(Ordering::SeqCst),
        1,
        "at most one teardown is outstanding"
    );
    await_previous();
    assert_eq!(second.load(Ordering::SeqCst), 1);
}
