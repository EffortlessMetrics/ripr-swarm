<!-- section: Fixed -->
- A Rust keyword (`return`, `while`) in a changed expression no longer relates tests by name, and the name-substring fallback now skips generic tokens, so an owner no test calls reads `no_static_path` instead of `weakly_exposed` (#9172).
