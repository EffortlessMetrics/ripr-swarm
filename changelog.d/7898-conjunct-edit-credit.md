<!-- section: Fixed -->
- Rust: a predicate edit that adds or removes a `&&`/`||` operand
  (`other > 0 && 100 <= points` to `100 < points`, in an `if` or a match
  guard) no longer reads `exposed` from test inputs at the surviving
  comparison's boundary; it reads `infection_unknown` and names the old
  predicate ([#7898](https://github.com/EffortlessMetrics/ripr-swarm/issues/7898)).
