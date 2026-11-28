<!-- section: Fixed -->
- The `ripr` crate package once again ships `src/analysis/executable_tests.rs`, a production module that the `*_tests.rs` package exclusion had dropped, so the published crate compiles. `cargo xtask check-package-content` now runs in precommit and `ci-policy`, and fails when a package list leaves out a module that production code declares (#15216).
