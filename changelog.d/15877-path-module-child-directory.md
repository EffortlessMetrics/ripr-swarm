<!-- section: Fixed -->
- `ripr check` no longer reports findings in a file reached only as a `#[path]` module's stem-directory child (for example `src/impl/price/child.rs` for `#[path = "impl/price.rs"]` with `mod child;`) when the module graph is complete; rustc resolves that `mod child;` beside the file, at `src/impl/child.rs` (#15877)
