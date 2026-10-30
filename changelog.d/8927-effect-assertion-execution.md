<!-- section: Fixed -->
- Effect-family findings refuse `assert_eq!` and `assert!` oracles whose execution
  context is not established, including assertions after `return`, in a literal-false
  block, or in an uncalled closure, while retaining reachable effect observers, including `std::`/`core::`-qualified `assert!` and the first of identical same-line assertions (`A; return; A;`) (#8927).
  The full, sharded and compact classified-seam cache generations moved to `1.97` / `0.104` / `0.104`, so older entries are rebuilt (#8927).
  Compound-oracle refusal guidance uses generic assertion wording when the exact assertion macro is unestablished (#8927).
