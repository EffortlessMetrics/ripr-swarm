<!-- section: Fixed -->
- A whitespace reformat beside a real edit no longer widens the changed
  span past an opaque callee: `fns [0](50)` → `fns[0](51)` covers `51`,
  not the indexed call. String-literal whitespace is still a change (#7956).
