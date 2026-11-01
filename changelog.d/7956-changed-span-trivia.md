<!-- section: Fixed -->
- A whitespace reformat beside a real edit no longer widens the changed
  span past an opaque callee: the call `fns [0](50)` reformatted to
  `fns[0](51)` is covered as a whole, not just the indexed call.
  String-literal whitespace is still a change (#7956).
