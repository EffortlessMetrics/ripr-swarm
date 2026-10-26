<!-- section: Changed -->
- The required Rust run no longer repeats `cargo check --workspace --all-targets` as its own step: Strict workspace Clippy builds the identical unit set, so the compile ran twice for the same result. The `product.rust.workspace_check` gate is retired and `product.rust.clippy` now states that it carries the type-check (#10341).
