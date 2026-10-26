<!-- section: Fixed -->
- Rust keywords (`return`, `while`) in a changed expression no longer relate tests by name, and the name-substring fallback applies the generic/common-word gate, so an owner no test calls reads `no_static_path` instead of `weakly_exposed`. Words inside string literals are still name tokens (#9172).
