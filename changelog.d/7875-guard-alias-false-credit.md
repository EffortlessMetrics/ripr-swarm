<!-- section: Fixed -->
- Rust: a guard binding (`p if 100 <= p =>`) no longer earns boundary credit
  from the scrutinee's test inputs when the binding may be another value:
  a rebinding hidden after a block comment or a string holding `//` or `/*`,
  a `let p` or closure `|p|` in the arm body on the guard's line, a block or
  closure in the guard, or a second `match` written `match(other)`. A guard
  edit that deletes or adds a conjunct keeps its arm-head finding
  ([#7875](https://github.com/EffortlessMetrics/ripr-swarm/issues/7875)).
