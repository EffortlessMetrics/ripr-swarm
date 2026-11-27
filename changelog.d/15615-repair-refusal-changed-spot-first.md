<!-- section: Fixed -->
- Repair: when `ripr repair` refuses a change whose spots cannot start, the
  reasons now list the changed spots first and fold the counts of spots
  outside your change into one line, so the changed spot is no longer the
  hidden "(and 1 more)". Extra changed spots point to `ripr check` (#15615).
