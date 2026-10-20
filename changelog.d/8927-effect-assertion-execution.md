<!-- section: Fixed -->
- Effect-family findings refuse `assert_eq!` and `assert!` oracles whose execution
  context is not established, including assertions after `return`, in a literal-false
  block, or in an uncalled closure, while retaining reachable effect observers (#8927).
