<!-- section: Fixed -->
- Rust: a guard binding (`p if 100 <= p =>`) no longer earns boundary credit
  from the scrutinee's test inputs when another value may stand behind it:
  a nested `match(other)`, a block or `let` in the guard, or a constant a
  macro may declare. A guard edit that deletes or adds a conjunct keeps its
  arm-head finding
  ([#7922](https://github.com/EffortlessMetrics/ripr-swarm/pull/7922)).
