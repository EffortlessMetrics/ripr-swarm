<!-- section: Fixed -->
- Rust: the shared base-owner identity behind the constant-tail and
  unary-tail return readings now reports the `cfg` gates it missed (a body
  inner `#![cfg(..)]`, #15003; gated re-exports, renames and globs, files
  reached only through `include!`, and module-emitting macros, #14952), so a
  `cfg(not(test))` re-export beside a test-only stand-in reads
  `propagation_unknown` instead of `weakly_exposed`. Ungated, unrelated,
  sysroot-rooted and redundant test-only gates keep their credit.
