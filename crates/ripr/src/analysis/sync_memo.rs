//! A thread-safe drop-in for the `RefCell` memos a classification run shares
//! across its probes (#8756).
//!
//! Per-probe classification reads one immutable index and fills lazy memos
//! that are pure in their keys. Those memos were `RefCell`s, which pinned the
//! whole run to one thread. [`SyncMemo`] keeps the `borrow()`/`borrow_mut()`
//! call shape over a `Mutex`, so the probes can be classified on the rayon
//! pool while every worker still hits the same warm entries.
//!
//! A `RefCell` lets a second shared borrow start while one is held; a mutex
//! does not, so a nested borrow of the same memo would deadlock. Under
//! `cfg(test)` the memo records the thread that holds it and panics on
//! re-entry, so such a site fails the lib-test run instead of hanging.

use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::{Mutex, MutexGuard, PoisonError};

#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(test)]
static NEXT_THREAD_TAG: AtomicU64 = AtomicU64::new(1);

#[cfg(test)]
thread_local! {
    static THREAD_TAG: u64 = NEXT_THREAD_TAG.fetch_add(1, Ordering::Relaxed);
}

#[cfg(test)]
fn current_thread_tag() -> u64 {
    THREAD_TAG.with(|tag| *tag)
}

pub(in crate::analysis) struct SyncMemo<T> {
    inner: Mutex<T>,
    /// Tag of the thread currently holding `inner`, or 0 (test builds only).
    #[cfg(test)]
    holder: AtomicU64,
}

impl<T> SyncMemo<T> {
    pub(in crate::analysis) fn new(value: T) -> Self {
        Self {
            inner: Mutex::new(value),
            #[cfg(test)]
            holder: AtomicU64::new(0),
        }
    }

    /// Locks the memo. A poisoned lock is recovered: the entries are pure in
    /// their keys, so a worker that panicked mid-insert leaves at worst a
    /// missing entry that the next reader recomputes.
    pub(in crate::analysis) fn borrow(&self) -> SyncMemoGuard<'_, T> {
        #[cfg(test)]
        {
            let tag = current_thread_tag();
            assert!(
                self.holder.load(Ordering::Acquire) != tag,
                "re-entrant SyncMemo borrow on one thread would deadlock"
            );
            let guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            self.holder.store(tag, Ordering::Release);
            SyncMemoGuard {
                guard,
                holder: &self.holder,
            }
        }
        #[cfg(not(test))]
        {
            SyncMemoGuard {
                guard: self.inner.lock().unwrap_or_else(PoisonError::into_inner),
            }
        }
    }

    /// Same lock as [`Self::borrow`], named for the mutating call sites.
    pub(in crate::analysis) fn borrow_mut(&self) -> SyncMemoGuard<'_, T> {
        self.borrow()
    }
}

pub(in crate::analysis) struct SyncMemoGuard<'a, T> {
    guard: MutexGuard<'a, T>,
    #[cfg(test)]
    holder: &'a AtomicU64,
}

impl<T> Deref for SyncMemoGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.guard
    }
}

impl<T> DerefMut for SyncMemoGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.guard
    }
}

#[cfg(test)]
impl<T> Drop for SyncMemoGuard<'_, T> {
    fn drop(&mut self) {
        self.holder.store(0, Ordering::Release);
    }
}

impl<T: Default> Default for SyncMemo<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: Clone> Clone for SyncMemo<T> {
    fn clone(&self) -> Self {
        Self::new(self.borrow().clone())
    }
}

impl<T: fmt::Debug> fmt::Debug for SyncMemo<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.inner.try_lock() {
            Ok(value) => f.debug_tuple("SyncMemo").field(&*value).finish(),
            Err(_) => f.write_str("SyncMemo(<locked>)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SyncMemo;
    use std::collections::BTreeMap;

    #[test]
    fn memo_is_shared_across_threads_and_keeps_entries() {
        let memo: SyncMemo<BTreeMap<u32, u32>> = SyncMemo::default();
        std::thread::scope(|scope| {
            for key in 0..4u32 {
                let memo = &memo;
                scope.spawn(move || {
                    memo.borrow_mut().insert(key, key * 10);
                });
            }
        });
        let seen: Vec<(u32, u32)> = memo.borrow().iter().map(|(k, v)| (*k, *v)).collect();
        assert_eq!(seen, vec![(0, 0), (1, 10), (2, 20), (3, 30)]);
    }

    #[test]
    fn a_nested_borrow_on_one_thread_panics_instead_of_deadlocking() {
        let memo: SyncMemo<u32> = SyncMemo::default();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _outer = memo.borrow();
            let _inner = memo.borrow();
        }));
        assert!(outcome.is_err(), "re-entry must fail loudly in tests");
    }

    #[test]
    fn the_holder_is_released_so_a_later_borrow_succeeds() {
        let memo: SyncMemo<u32> = SyncMemo::new(7);
        {
            let mut guard = memo.borrow_mut();
            *guard += 1;
        }
        assert_eq!(*memo.borrow(), 8);
    }

    #[test]
    fn clone_copies_the_current_entries() {
        let memo: SyncMemo<Vec<u8>> = SyncMemo::new(vec![1, 2]);
        let copy = memo.clone();
        memo.borrow_mut().push(3);
        assert_eq!(*copy.borrow(), vec![1, 2]);
        assert_eq!(*memo.borrow(), vec![1, 2, 3]);
    }
}
