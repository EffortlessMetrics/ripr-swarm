<!-- section: Fixed -->
- Rust related tests no longer report a stronger oracle than their assertion
  pins (RIPR-SPEC-0231 rules 2 to 6,
  [#6640](https://github.com/EffortlessMetrics/ripr-swarm/issues/6640)).
  A `matches!` or `assert_matches!` pattern decides the reading: `Err(e)`,
  `Err(_e)` and guarded `Err(..)` bindings are `broad_error` / weak; `Ok(_)`,
  `Some(ref x)` and `None` are `smoke_only`; `Some(_) | None`, ranges and
  slice-length patterns are `relational_check` / weak. `is_ok() || is_err()`
  is a relation, method checks match whole names (`is_okay` is not `is_ok`),
  effect-observer words match whole identifier segments (`is_present()` is not
  a `sent` observer), and only equality-named custom helpers are exact. A
  family strength override never raises an assertion the classifier
  weakened, and an owner pin needs a strong oracle.
