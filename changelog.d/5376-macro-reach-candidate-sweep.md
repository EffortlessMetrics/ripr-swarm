<!-- section: Changed -->
- The Rust macro-reach fallback visits only tests that call a reaching name
  or invoke an owner-mentioning macro, and replays the forward witness walk
  only for those candidate callees, so a common owner name inside a
  `macro_rules!` body no longer costs a walk per test (#5376).
