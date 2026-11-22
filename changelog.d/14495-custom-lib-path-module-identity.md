<!-- section: Fixed -->
- A library whose `[lib] path` sits under `src/` (for example
  `src/core/engine.rs`) reads module paths from that root: a test calling
  `my_pkg::pricing::discount` now pairs with the owner in
  `src/core/pricing.rs`, and a directory-shaped call such as
  `my_pkg::core::pricing::discount`, which does not compile, no longer reads
  as `exposed` (#14495).
