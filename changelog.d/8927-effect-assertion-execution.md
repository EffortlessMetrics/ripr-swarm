<!-- section: Fixed -->
- Effect-family findings refuse `assert_eq!` and `assert!` oracles whose execution
  context is not established, including assertions after `return`, in a literal-false
  block, or in an uncalled closure, while retaining reachable effect observers (#8927).
  The full, sharded and compact classified-seam cache generations moved to `1.87` / `0.93` / `0.93`, so older entries are rebuilt (#8927).
- Reachable effect observers retain credit for qualified standard `assert!` and
  contiguous compact `A; return; A;` siblings, with source, root, binding and
  test-outcome gates preserved (#8927).
