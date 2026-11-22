<!-- section: Fixed -->
- Rust checks: an assertion on another function's constant
  (`assert_eq!(label(0), "zero")`, where `label` is a pure function of its
  primitive arguments) no longer credits observation and discrimination to a
  changed predicate its test merely calls. The agent `stages=` row, the JSON
  `ripr` object and `evidence_stages` used to say `discriminate: yes` while
  the human card said the assertion was missing. Such a test now reads
  `reachable_unrevealed` on every surface. An assertion on the changed
  function beside it keeps its credit, and a helper that reads global state
  or an argument held in a binding keeps the earlier reading (#15476).
