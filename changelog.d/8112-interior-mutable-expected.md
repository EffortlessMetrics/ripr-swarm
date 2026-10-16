<!-- section: Fixed -->
- Rust reveal side no longer reads an interior-mutable `let` (`Cell`, `RefCell`, `Mutex`, `Atomic*`) or a `static mut` expected value as its initializer, so the assertion keeps its credit instead of being wrongly weakened (https://github.com/EffortlessMetrics/ripr/issues/8112).
