<!-- section: Fixed -->
- Effect-family findings refuse `assert_eq!` and `assert!` oracles whose execution
  context is not established, including assertions after `return`, in a literal-false
  block, or in an uncalled closure, while retaining reachable effect observers, including `std::`/`core::`-qualified `assert!` and the first of identical same-line assertions (`A; return; A;`) (#8927).
  The full, sharded and compact classified-seam cache generations moved to `1.87` / `0.93` / `0.93`, so older entries are rebuilt (#8927).
