<!-- section: Fixed -->
- Rust related tests no longer report a stronger oracle than their
  `matches!` pattern or method check pins (RIPR-SPEC-0231 rules 2 to 6,
  [#6640](https://github.com/EffortlessMetrics/ripr-swarm/issues/6640)).
  `Err(e)`, `Err(_e)` and `Err(..)` bindings are `broad_error` / weak, unless
  a guard equates the binding with a value; `Ok(_)`, `Some(ref x)` and `None`
  are `smoke_only`; `Some(_) | None`, ranges and slice-length patterns are
  `relational_check` / weak. `is_ok() || is_err()` is a relation, method
  checks match whole names (`is_okay` is not `is_ok`), effect-observer words
  match whole identifier segments (`is_present()` is not a `sent` observer),
  and a custom helper is exact only when its name ends in an equality
  segment. Reveal's per-family strength override no longer raises a strength
  the classifier weakened, and the return-value owner pin needs a strong
  oracle.
