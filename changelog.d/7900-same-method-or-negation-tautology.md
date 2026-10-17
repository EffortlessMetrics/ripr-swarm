<!-- section: Fixed -->
- Rust oracles: `x.m() || !x.m()` on the same receiver, including an
  `expect![["true"]].assert_eq(&(..).to_string())` wrapper, now reads
  `relational_check` / weak instead of a medium snapshot or a smoke
  `is_ok` check. Different receivers, different methods, and `&&` forms
  stay as they were
  ([#7900](https://github.com/EffortlessMetrics/ripr-swarm/issues/7900)).
