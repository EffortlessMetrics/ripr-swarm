<!-- section: Changed -->
- When every finding left in a `ripr check` run is on removed code (suppressed
  findings are not counted), the human `State:` line now reads
  `every finding left is on removed code, which is never an edit target`
  instead of `no gap selected for repair`. The machine state id
  (`no_actionable_gap`) and all counts are unchanged (#15335).
