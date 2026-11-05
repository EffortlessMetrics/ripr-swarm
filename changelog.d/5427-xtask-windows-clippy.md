<!-- section: Internal / CI -->
- `cargo clippy -p xtask --all-targets -- -D warnings` now passes on Windows:
  Unix-only harness helpers in `exact_candidate_harness` and
  `termination_qualification` are cfg-gated instead of surfacing as unused
  imports and dead code on non-Unix hosts (#5427).
