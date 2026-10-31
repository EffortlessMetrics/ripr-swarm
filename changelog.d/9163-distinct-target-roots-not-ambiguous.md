<!-- section: Fixed -->
- A file declared by several distinct Cargo target crate roots (benches, examples, bins, integration tests or manifest-declared targets, such as tracing `benches/shared.rs` or crossbeam-channel `benchmarks/message.rs`) no longer reports `rust_module_ambiguous_parent`. Non-root owners, repeated owner files and disagreeing declarations still fail closed (#9163).
