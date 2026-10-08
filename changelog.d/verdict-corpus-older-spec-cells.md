<!-- section: Added -->
- Verdict corpus: 25 authored cases for test shapes RIPR specs below 0225
  define, each labeled by real mutant runs: error-seam oracles (exact-value
  only, boxed `Into` wrapper, downcast type pin, local `assert_matches!`),
  an overridden builder default, an env-lookup constant, a hand-rolled mock,
  helper-call boundary inputs, duplicative tests and self-equality,
  assertion-shaped owners, macro-argument and `format!`
  reach, help-text constants, an unsafe block on a shared line, a
  `CARGO_BIN_EXE_*` subprocess, an `sh` command that only names the binary
  variable, a build-script value, and cross-crate `T::owner()` calls on
  owner names a sibling crate shares through a free function, with alias, glob and other-dependency controls
  (#6577).
