<!-- section: Fixed -->
- Rust oracles: the lexical fallback now joins a rustfmt-wrapped uncompared
  `.unwrap_err()` / `.expect_err(..)` through the first depth-0 `;`, so it
  reads `broad_error` / weak like the parser instead of reading nothing.
  Same-row siblings keep separate facts; a `;` inside a message does not
  end the statement; an unclosed `(` / `[` or mismatched closer inherited
  from earlier rows does not mint a new fact
  ([#7955](https://github.com/EffortlessMetrics/ripr-swarm/issues/7955)).
