<!-- section: Fixed -->
- Suppression `path` globs no longer treat a literal backslash in a Unix file name as a directory separator, so `src/a/**` does not suppress a file named `a\b.rs` under `src/` in check, badge or SARIF output (#16271).
