<!-- section: Fixed -->
- `ripr check` no longer reports a gap for a changed Rust function compiled
  only under a cfg ripr cannot evaluate when a doc example in the same library
  is gated by that cfg and so was not read (anyhow's nightly-only `provide`
  examples). Such a `reachable_unrevealed` finding with no test seen calling
  the owner now reads `static_unknown` under the new
  `rust_gated_doctest_unread` limitation; nothing is credited (#14935).
- Rust doc examples now require a resolved free call to count as calling the
  changed owner. An unbound crate or function name in the example previously
  gave the documented function direct reach credit.
