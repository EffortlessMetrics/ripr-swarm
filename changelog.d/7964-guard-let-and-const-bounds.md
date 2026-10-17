<!-- section: Fixed -->
- Rust: a guarded side effect that no test enters now also reads
  `weakly_exposed` when the test passes a let-bound literal
  (`let n = 4; add(&mut v, n);`) or the guard compares against a
  file-level integer `const` (`if x > LIMIT`). Before, both shapes read
  `exposed`. Mutable, shadowed or computed bindings, `static` bounds,
  imported names and repeated definitions still keep today's reading
  ([#7964](https://github.com/EffortlessMetrics/ripr-swarm/issues/7964)).
