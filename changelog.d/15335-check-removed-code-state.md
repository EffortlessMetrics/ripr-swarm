<!-- section: Changed -->
- When a `ripr check` diff only removes code, the human `State:` line now reads
  `the diff only removes code, and removed code is never an edit target`
  instead of `no gap selected for repair`. The machine state id
  (`no_actionable_gap`) and all counts are unchanged (#15335).
