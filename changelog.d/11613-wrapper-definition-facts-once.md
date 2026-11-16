<!-- section: Changed -->
- Rust checks: a test module that defines a `macro_rules!` wrapper and calls it many times no longer re-walks the module for every call; a one-line edit check on `rust-lang/regex` drops from about 2.1 s to 1.3 s with identical findings (#11613).
