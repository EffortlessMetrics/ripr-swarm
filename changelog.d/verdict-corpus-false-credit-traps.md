<!-- section: Added -->
- Verdict corpus: 23 authored false-credit cases in `authored-trap-kit`
  (corpus `2026-10-06.2`). Eighteen are traps whose tests cannot notice the
  change: a self-computed expected value via a test helper, `assert_eq!(x, x)`,
  an assert on the input or on a stale clone, a sibling-named or same-named
  function tested instead, an assert after an always-taken return, a bare
  `#[should_panic]` satisfied by an unrelated panic, the owner only in an
  assert message, an empty table loop, an `#[ignore]` pin, `catch_unwind`,
  `assert!(unsigned >= 0)`, a sibling match arm, credit borrowed from a
  same-named function, a name-only relation, a local binding named like the
  owner, and a nested `#[test]`. Five are negative controls that do
  notice it. Truth comes from real mutant runs on rustc 1.95. Defects found:
  [#6537](https://github.com/EffortlessMetrics/ripr-swarm/issues/6537),
  [#6538](https://github.com/EffortlessMetrics/ripr-swarm/issues/6538),
  [#6541](https://github.com/EffortlessMetrics/ripr-swarm/issues/6541),
  [#6544](https://github.com/EffortlessMetrics/ripr-swarm/issues/6544).
- Verdict corpus: 17 more false-credit cases in `authored-trap-reach`
  (same corpus version), where the trap sits outside the assertion: a test
  file no `mod` declares, an integration test `autotests = false` leaves
  unbuilt, an assert on a detached thread, a result multiplied by zero or
  clamped with `.min(1)`, a `const` expected value from the same `const fn`,
  `assert_eq!(f(x), f(x))`, a container's length, and a non-empty `Debug`
  rendering. Eight are negative controls, including value-preserving
  projections and joined or scoped threads. Defects found:
  [#6965](https://github.com/EffortlessMetrics/ripr-swarm/issues/6965),
  [#6966](https://github.com/EffortlessMetrics/ripr-swarm/issues/6966),
  [#6968](https://github.com/EffortlessMetrics/ripr-swarm/issues/6968), and a
  bound self-computed expected value for
  [#5830](https://github.com/EffortlessMetrics/ripr-swarm/issues/5830).
