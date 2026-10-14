<!-- section: Fixed -->
- Rust: a guard binding (`p if 100 <= p =>`) no longer earns boundary credit
  from the scrutinee's test inputs when a nested `match(other)` or a block
  guard (`p if { let p = other; 100 < p } =>`) may bind it to another value.
  A guard edit that deletes or adds a conjunct keeps its arm-head finding
  ([#7922](https://github.com/EffortlessMetrics/ripr-swarm/pull/7922)).
