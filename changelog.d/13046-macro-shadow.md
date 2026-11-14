<!-- section: Fixed -->
- Resource dependency edges: an unqualified `include_str!` / `include_bytes!` call is now skipped when any indexed Rust source in the crate defines a `macro_rules!` of that name, not only when the definition sits in the same file; previously a shadowing macro from another module or `#[macro_use]` could still yield a false exact resource edge (#13046, advances #5171).
