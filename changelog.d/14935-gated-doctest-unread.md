<!-- section: Fixed -->
- `ripr check` no longer reports a gap for a changed Rust function compiled
  only under a cfg ripr cannot evaluate when a doc example in the same library
  is gated by that cfg and so was not read (anyhow's nightly-only `provide`
  examples). Such a `reachable_unrevealed` finding with no test seen calling
  the owner now reads `static_unknown` under the new
  `rust_gated_doctest_unread` limitation; nothing is credited (#14935).
