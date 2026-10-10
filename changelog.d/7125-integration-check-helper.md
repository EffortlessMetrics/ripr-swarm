<!-- section: Fixed -->
- A check helper in a `tests/*.rs` integration target is now credited the
  same way as a `#[cfg(test)]` helper: its `assert_eq!` is lent to the
  tests that call it, so a changed owner those tests pin can read `exposed`.
  Every #6482 gate stays: eager call, one owner mention, literal-only
  boundary mapping, no shadowed `assert_eq!`. A `Production` helper in
  `src/` (including `src/tests/`), nested `tests/support/`, `benches/`,
  or `examples/` (including `examples/tests/`) is still not test evidence.
  A workspace member nested under `tests/` still credits
  `tests/<name>.rs` relative to its own manifest
  (`tests/harness/tests/gate.rs`); a same-shaped `tests/support/tests/`
  path without that manifest stays uncredited. A nested package the
  workspace `[workspace] exclude`s is not workspace test evidence. A
  declared `[[test]]` with `harness = false` is not libtest-collected, so
  its `#[test]` helper is not credited.
  An undeclared `tests/*.rs` file that `autotests = false` leaves unbuilt
  is dropped before helper credit ([#6965](https://github.com/EffortlessMetrics/ripr-swarm/issues/6965))
  ([#7125](https://github.com/EffortlessMetrics/ripr-swarm/issues/7125)).
