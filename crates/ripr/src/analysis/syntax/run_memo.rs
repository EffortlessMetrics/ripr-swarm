//! Run-scoped memo for pure per-source syntax products (#5363).
//!
//! One diff check builds several overlapping indexes (the changed packages'
//! admission index, the main index and each widened reach index), and every
//! consumer rescanned the same files: the external-docs scan, same-file
//! local test macro expansion and helper-oracle extraction each re-parse
//! source text that has not changed within the run. Each product is a pure
//! function of its source text, so it is stored under a digest of that text
//! for the lifetime of one scope and dropped when the scope ends.
//!
//! The memo is thread-local: a caller opts in on its own thread with
//! [`enter`]. The analysis thread enters one, and each parallel classification
//! worker enters its own (rayon `map_init` in the Rust language analysis), so
//! workers memoize independently and never share entries. A thread without a
//! scope (tests, other analyses) computes every product directly, exactly as
//! before. Nothing outlives the outermost scope, so no product can be served
//! for content that changed between runs.

use sha2::{Digest, Sha256};
use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// The pure syntax products a run memoizes. A product is part of the key so
/// two functions over the same text never share an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Product {
    ExternalDocs,
    LocalTestMacros,
    FunctionOracles,
    ForeignEntryImport,
}

type Key = (Product, [u8; 32]);

#[derive(Default)]
struct Memo {
    depth: usize,
    entries: HashMap<Key, Rc<dyn Any>>,
}

thread_local! {
    static MEMO: RefCell<Memo> = RefCell::new(Memo::default());
}

/// Keeps the calling thread's memo alive. The outermost scope's drop clears
/// every entry; a nested scope shares the outer scope's entries.
#[must_use = "the memo is cleared when the scope is dropped"]
pub(crate) struct RunMemoScope(());

pub(crate) fn enter() -> RunMemoScope {
    MEMO.with(|memo| memo.borrow_mut().depth += 1);
    RunMemoScope(())
}

impl Drop for RunMemoScope {
    fn drop(&mut self) {
        MEMO.with(|memo| {
            let mut memo = memo.borrow_mut();
            memo.depth = memo.depth.saturating_sub(1);
            if memo.depth == 0 {
                memo.entries.clear();
            }
        });
    }
}

fn key(product: Product, discriminator: usize, extra: &str, source: &str) -> Key {
    let mut hasher = Sha256::new();
    hasher.update(discriminator.to_le_bytes());
    hasher.update(extra.len().to_le_bytes());
    hasher.update(extra.as_bytes());
    hasher.update(source.as_bytes());
    (product, hasher.finalize().into())
}

/// `compute`'s result for `(product, discriminator, source)`, computed once
/// per scope. Outside a scope it is computed on every call. `compute` runs
/// without the memo borrowed, so it may itself memoize other products.
pub(crate) fn memoize<T: Clone + 'static>(
    product: Product,
    discriminator: usize,
    source: &str,
    compute: impl FnOnce() -> T,
) -> T {
    memoize_with(product, discriminator, "", source, compute)
}

/// [`memoize`] for a product that also depends on `extra` (a name or other
/// query beside the source text): the entry is keyed by both.
pub(crate) fn memoize_with<T: Clone + 'static>(
    product: Product,
    discriminator: usize,
    extra: &str,
    source: &str,
    compute: impl FnOnce() -> T,
) -> T {
    if MEMO.with(|memo| memo.borrow().depth == 0) {
        return compute();
    }
    let key = key(product, discriminator, extra, source);
    let cached = MEMO.with(|memo| memo.borrow().entries.get(&key).cloned());
    if let Some(value) = cached.as_ref().and_then(|value| value.downcast_ref::<T>()) {
        return value.clone();
    }
    let value = compute();
    MEMO.with(|memo| {
        memo.borrow_mut()
            .entries
            .insert(key, Rc::new(value.clone()));
    });
    value
}

/// [`memoize`] for many sources at once, with the products the scope has not
/// yet seen computed on the rayon pool.
///
/// The results are exactly what `sources.iter().map(|s| memoize(product, 0,
/// s, || compute(s)))` returns, in order, and `compute` runs once per
/// distinct source that is not already in the scope. The memo itself is only
/// touched on the calling thread: workers run `compute` and nothing else, so
/// the thread-local scope still holds every product afterwards. A call from a
/// rayon worker, which must not block on a nested fan-out, computes in line.
pub(crate) fn memoize_many<T: Clone + Send + 'static>(
    product: Product,
    sources: &[&str],
    compute: impl Fn(&str) -> T + Sync,
) -> Vec<T> {
    use rayon::prelude::*;
    let fan_out = rayon::current_thread_index().is_none();
    let run = |items: &[&str]| -> Vec<T> {
        if fan_out {
            items.par_iter().map(|source| compute(source)).collect()
        } else {
            items.iter().map(|source| compute(source)).collect()
        }
    };
    if MEMO.with(|memo| memo.borrow().depth == 0) {
        return run(sources);
    }
    let keys: Vec<Key> = sources
        .iter()
        .map(|source| key(product, 0, "", source))
        .collect();
    let recall = |key: &Key| {
        MEMO.with(|memo| {
            memo.borrow()
                .entries
                .get(key)
                .and_then(|value| value.downcast_ref::<T>())
                .cloned()
        })
    };
    let mut seen = std::collections::HashSet::new();
    let missing: Vec<usize> = keys
        .iter()
        .enumerate()
        .filter(|(_, key)| recall(key).is_none() && seen.insert(**key))
        .map(|(position, _)| position)
        .collect();
    let missing_sources: Vec<&str> = missing.iter().map(|position| sources[*position]).collect();
    let computed = run(&missing_sources);
    MEMO.with(|memo| {
        let mut memo = memo.borrow_mut();
        for (position, value) in missing.iter().zip(computed) {
            memo.entries.insert(keys[*position], Rc::new(value));
        }
    });
    // Every key is stored now; a source repeated in `sources` reads the one
    // product computed for its text, as the serial calls would.
    keys.iter()
        .zip(sources)
        .map(|(key, source)| recall(key).unwrap_or_else(|| compute(source)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Product, enter, memoize};
    use std::cell::Cell;

    fn counted(calls: &Cell<usize>, product: Product, line: usize, source: &str) -> usize {
        memoize(product, line, source, || {
            calls.set(calls.get() + 1);
            source.len() + line
        })
    }

    #[test]
    fn computes_every_call_outside_a_scope() {
        let calls = Cell::new(0);
        for _ in 0..3 {
            assert_eq!(counted(&calls, Product::ExternalDocs, 0, "abc"), 3);
        }
        assert_eq!(calls.get(), 3, "no scope means no memo");
    }

    #[test]
    fn computes_once_per_source_within_a_scope() {
        let calls = Cell::new(0);
        let _scope = enter();
        for _ in 0..3 {
            assert_eq!(counted(&calls, Product::ExternalDocs, 0, "abc"), 3);
        }
        assert_eq!(calls.get(), 1);
        assert_eq!(counted(&calls, Product::ExternalDocs, 0, "abcd"), 4);
        assert_eq!(calls.get(), 2, "different text is a different entry");
    }

    #[test]
    fn product_and_discriminator_are_part_of_the_key() {
        let calls = Cell::new(0);
        let _scope = enter();
        counted(&calls, Product::ExternalDocs, 0, "abc");
        counted(&calls, Product::LocalTestMacros, 0, "abc");
        assert_eq!(counted(&calls, Product::FunctionOracles, 7, "abc"), 10);
        assert_eq!(counted(&calls, Product::FunctionOracles, 8, "abc"), 11);
        assert_eq!(calls.get(), 4);
    }

    #[test]
    fn the_outermost_scope_clears_the_memo() {
        let calls = Cell::new(0);
        {
            let _outer = enter();
            {
                let _inner = enter();
                counted(&calls, Product::ExternalDocs, 0, "abc");
            }
            counted(&calls, Product::ExternalDocs, 0, "abc");
            assert_eq!(calls.get(), 1, "a nested scope shares the outer entries");
        }
        let _next = enter();
        counted(&calls, Product::ExternalDocs, 0, "abc");
        assert_eq!(calls.get(), 2, "a later scope starts empty");
    }

    #[test]
    fn a_stored_value_of_another_type_is_recomputed() {
        let _scope = enter();
        let first: u8 = memoize(Product::ExternalDocs, 0, "abc", || 1u8);
        let second: u16 = memoize(Product::ExternalDocs, 0, "abc", || 2u16);
        assert_eq!((first, second), (1, 2));
    }

    #[test]
    fn memoize_many_matches_serial_memoize_and_fills_the_scope() {
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let compute = |source: &str| {
            calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            source.len()
        };
        let sources = ["a", "bb", "a", "ccc"];
        // No scope: every call computes, like `memoize`.
        let unscoped = super::memoize_many(Product::ExternalDocs, &sources, compute);
        assert_eq!(unscoped, vec![1, 2, 1, 3]);
        assert_eq!(calls.swap(0, std::sync::atomic::Ordering::Relaxed), 4);
        let _scope = enter();
        let scoped = super::memoize_many(Product::ExternalDocs, &sources, compute);
        assert_eq!(scoped, vec![1, 2, 1, 3]);
        assert_eq!(
            calls.swap(0, std::sync::atomic::Ordering::Relaxed),
            3,
            "a repeated text is computed once"
        );
        // The products now sit in this thread's scope for the serial path.
        let serial = Cell::new(0);
        assert_eq!(counted(&serial, Product::ExternalDocs, 0, "bb"), 2);
        assert_eq!(serial.get(), 0, "the batch stored its products");
        let again = super::memoize_many(Product::ExternalDocs, &sources, compute);
        assert_eq!(again, vec![1, 2, 1, 3]);
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 0);
        // A different product is a different entry.
        let other = super::memoize_many(Product::LocalTestMacros, &["a"], compute);
        assert_eq!(other, vec![1]);
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn the_query_beside_the_source_is_part_of_the_key() {
        let calls = Cell::new(0);
        let _scope = enter();
        let ask = |extra: &str| {
            super::memoize_with(Product::ForeignEntryImport, 0, extra, "src", || {
                calls.set(calls.get() + 1);
                extra.len()
            })
        };
        assert_eq!((ask("a"), ask("a"), ask("bcd")), (1, 1, 3));
        assert_eq!(calls.get(), 2, "same query reuses, another query computes");
        // An empty query is the plain form's entry.
        assert_eq!(counted(&calls, Product::ForeignEntryImport, 0, "src"), 3);
        assert_eq!(
            super::memoize_with(Product::ForeignEntryImport, 0, "", "src", || 99usize),
            3
        );
    }
}
