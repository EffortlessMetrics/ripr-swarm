//! Run-memoized [`owner_pin::unestablished_cfg_gates_of_fn`] (#13868).
//!
//! The refusal-miss split asks for the `cfg` gates of a test and of its owner
//! once per refused assertion, and each answer parsed the whole file. A large
//! test file with many related tests was re-parsed for every one, which made
//! a scoped check of a small edit run past an hour. The answer is a pure
//! function of the source text, the function name and its line, so it is
//! memoized for the run like the other per-source syntax products.

use super::owner_pin;
use super::run_memo::{Product, memoize_with};
use std::collections::BTreeSet;

/// [`owner_pin::unestablished_cfg_gates_of_fn`], computed once per
/// `(source, name, line)` inside a run memo scope.
pub(crate) fn unestablished_cfg_gates_of_fn(
    source: &str,
    name: &str,
    line: usize,
) -> Option<BTreeSet<String>> {
    memoize_with(Product::FnCfgGates, line, name, source, || {
        owner_pin::unestablished_cfg_gates_of_fn(source, name, line)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memoized_gates_match_the_direct_reading_and_parse_once_per_run() {
        let source = "#[cfg(feature = \"x\")]\nfn gated() {}\n\nfn plain() {}\n";
        let direct_gated = owner_pin::unestablished_cfg_gates_of_fn(source, "gated", 2);
        let direct_plain = owner_pin::unestablished_cfg_gates_of_fn(source, "plain", 4);
        assert!(direct_gated.as_ref().is_some_and(|gates| !gates.is_empty()));
        assert_eq!(direct_plain, Some(BTreeSet::new()));
        let _scope = super::super::run_memo::enter();
        let before = super::super::nesting::CLEAN_PARSE_CALLS.with(std::cell::Cell::get);
        for _ in 0..3 {
            assert_eq!(
                unestablished_cfg_gates_of_fn(source, "gated", 2),
                direct_gated
            );
            assert_eq!(
                unestablished_cfg_gates_of_fn(source, "plain", 4),
                direct_plain
            );
        }
        let parses = super::super::nesting::CLEAN_PARSE_CALLS.with(std::cell::Cell::get) - before;
        assert_eq!(parses, 2, "one parse per distinct (name, line) in a run");
    }
}
