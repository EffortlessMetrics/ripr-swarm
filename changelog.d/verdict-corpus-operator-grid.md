<!-- section: Added -->
- Verdict corpus: 56 authored cases cross cargo-mutants operator classes
  (relational boundary, equality, arithmetic, boolean logic, return value,
  statement deletion, match arm and guard, loop accumulator, early return,
  `?`, iterator closure) with five test styles (exact pin, table-driven loop,
  property-style invariant, helper-wrapped assert, no assertion), each
  labeled by real mutant runs. ripr reads 18 ideally, abstains on 14 and
  gives 24 false actionable gaps, all on discriminated cells; 18 of them
  are table-driven or helper-wrapped pins whose assertion ripr refuses
  ([#5328](https://github.com/EffortlessMetrics/ripr-swarm/issues/5328),
  [#6482](https://github.com/EffortlessMetrics/ripr-swarm/issues/6482)).
  No grid case reads false exposed or false silent (RIPR-SPEC-0219).
