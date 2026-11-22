<!-- section: Fixed -->
- `ripr check` no longer reports findings in a file under a `#[path]` module's stem directory (for example `src/impl/price/child.rs` for `#[path = "impl/price.rs"]`), which rustc never compiles; the module's children resolve beside it, as rustc resolves them (#15877)
