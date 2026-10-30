//! Frees a finished diff run's facts off the result path (#10914). Freeing
//! pandas' ~23k tests, owners and sources takes ~0.3 s on one thread after
//! the findings are ready, and nothing in the result borrows them.
//!
//! At most one teardown is outstanding: a run waits for the previous run's
//! teardown before it reads sources, and hands off its own under the same
//! lock it waits under.
//! A long-lived caller (LSP, MCP) therefore holds no finished run's facts
//! while it builds the next run's, as before, and a backlog cannot grow.
//! The teardown runs on its own thread, so it never queues behind (or
//! blocks) work on the shared rayon pool.

use std::sync::Mutex;
use std::thread::JoinHandle;

/// The thread freeing the most recent run's facts, if it may still run.
static PENDING: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// Waits until the previous run's facts are freed.
pub(super) fn await_previous() {
    if let Ok(mut slot) = PENDING.lock() {
        join(&mut slot);
    }
}

/// Frees `facts` on a dedicated thread once any earlier teardown finished.
/// The lock is held from the wait through the hand-off, so concurrent runs
/// never have two teardowns outstanding. When no thread can be spawned (or
/// the lock is poisoned) the facts are freed here instead.
pub(super) fn free_in_background<T: Send + 'static>(facts: T) {
    let Ok(mut slot) = PENDING.lock() else {
        return;
    };
    join(&mut slot);
    if let Ok(handle) = std::thread::Builder::new()
        .name("ripr-python-teardown".to_owned())
        .spawn(move || drop(facts))
    {
        *slot = Some(handle);
    }
}

/// Joins the pending teardown, if any. The teardown thread never takes the
/// lock, so joining under it cannot deadlock.
fn join(slot: &mut Option<JoinHandle<()>>) {
    if let Some(handle) = slot.take() {
        // A panicking drop has already released what it could.
        let _ = handle.join();
    }
}

#[cfg(test)]
mod tests;
