<!-- section: Fixed -->
- A Rust 2015 file that still uses the `try!(..)` macro now parses (2015 is tried after 2024 and 2021) instead of dropping to the lexical fallback, where lines of an added `#[test]` fn were seeded as `no_static_path` warning probes with an unrelated owner (#10054).
