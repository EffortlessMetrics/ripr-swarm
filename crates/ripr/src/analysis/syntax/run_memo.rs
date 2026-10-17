//! Run-scoped memo for pure per-source syntax products (#5363).
//!
//! One diff check builds several overlapping indexes (the changed packages'
//! admission index, the main index and each widened reach index), and every
//! consumer rescanned the same files: the external-docs scan, same-file
//! local test macro expansion and helper-oracle extraction each re-parse
//! source text that has not changed within the run. Each product is a pure
//! function of its source text, so it is stored under a digest of that text
//! for the lifetime of one analysis call and dropped when the call ends.
//!
//! The memo is thread-local: a caller opts in on its own thread with
//! [`enter`], and a thread without a scope (parallel workers, tests, other
//! analyses) computes every product directly, exactly as before. Nothing
//! outlives the outermost scope, so no product can be served for content
//! that changed between runs.

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

fn key(product: Product, discriminator: usize, source: &str) -> Key {
    let mut hasher = Sha256::new();
    hasher.update(discriminator.to_le_bytes());
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
    if MEMO.with(|memo| memo.borrow().depth == 0) {
        return compute();
    }
    let key = key(product, discriminator, source);
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
}
