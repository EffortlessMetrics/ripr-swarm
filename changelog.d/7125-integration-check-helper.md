<!-- section: Fixed -->
- A check helper in a `tests/*.rs` integration target is now credited the
  same way as a `#[cfg(test)]` helper: its `assert_eq!` is lent to the
  tests that call it, so a changed owner those tests pin can read `exposed`.
  Every #6482 gate stays: eager call, one owner mention, literal-only
  boundary mapping, no shadowed `assert_eq!`. A `Production` helper in
  `src/`, `benches/`, or `examples/` is still not test evidence
  ([#7125](https://github.com/EffortlessMetrics/ripr-swarm/issues/7125)).
