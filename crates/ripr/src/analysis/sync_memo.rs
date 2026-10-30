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

use std::collections::BTreeMap;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

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

impl<K: Ord + Clone, V> SyncMemo<BTreeMap<K, Arc<V>>> {
    /// The entry for `key`, built by `build` outside the lock on a miss.
    ///
    /// A guard held while an entry is built stalls every other worker that
    /// needs any entry of this memo, and a guard held after the entry is read
    /// stalls them for the rest of the caller's work (#8797). The entry is
    /// pure in its key, so two workers that miss together build it twice and
    /// the first insert wins; both get the same value.
    pub(in crate::analysis) fn get_or_build(&self, key: &K, build: impl FnOnce() -> V) -> Arc<V> {
        if let Some(found) = self.borrow().get(key) {
            return Arc::clone(found);
        }
        let built = Arc::new(build());
        Arc::clone(self.borrow_mut().entry(key.clone()).or_insert(built))
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
    fn get_or_build_runs_the_builder_outside_the_lock() {
        use std::sync::Arc;
        let memo: SyncMemo<BTreeMap<u8, Arc<u32>>> = SyncMemo::default();
        let first = memo.get_or_build(&1, || {
            // A builder that reads the same memo would panic (and in a
            // release build deadlock) if the lock were still held.
            assert!(memo.borrow().is_empty());
            10
        });
        let rebuilt = std::cell::Cell::new(false);
        let second = memo.get_or_build(&1, || {
            rebuilt.set(true);
            99
        });
        assert!(!rebuilt.get(), "a hit must not run the builder");
        assert_eq!((*first, *second), (10, 10));
        assert!(
            Arc::ptr_eq(&first, &second),
            "a hit returns the stored entry"
        );
    }

    #[test]
    fn get_or_build_keeps_the_first_entry_when_workers_miss_together() {
        use std::sync::Arc;
        let memo: SyncMemo<BTreeMap<u8, Arc<usize>>> = SyncMemo::default();
        let barrier = std::sync::Barrier::new(4);
        let seen: Vec<usize> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..4usize)
                .map(|id| {
                    let (memo, barrier) = (&memo, &barrier);
                    scope.spawn(move || {
                        *memo.get_or_build(&0, || {
                            // Every worker is inside its builder before any
                            // inserts, so all four miss.
                            barrier.wait();
                            id
                        })
                    })
                })
                .collect();
            workers.into_iter().filter_map(|w| w.join().ok()).collect()
        });
        assert_eq!(seen.len(), 4);
        assert!(
            seen.iter().all(|value| *value == seen[0]),
            "every worker must see the one stored entry, got {seen:?}"
        );
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
