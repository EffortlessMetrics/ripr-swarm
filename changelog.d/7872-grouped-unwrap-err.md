<!-- section: Fixed -->
- Rust oracles: an uncompared `unwrap_err()` whose receiver is wrapped in
  parentheses reads `broad_error` / weak again when the group holds the
  call itself or only Ok-preserving adapters, such as
  `(check(x)).unwrap_err()` or `(check(x).map_err(f)).unwrap_err()`. A raw
  identifier call such as `r#check(x).unwrap_err()` reads the same.
  `(check(x).and(Err(E::Other))).unwrap_err()` still reads nothing
  ([#7872](https://github.com/EffortlessMetrics/ripr-swarm/issues/7872)).
