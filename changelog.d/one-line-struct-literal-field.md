<!-- section: Fixed -->
- When several same-kind probes share one changed line, the probe now names
  the one whose text changed, not the first that matches. Before,
  `Id { counter: 0x00ab_cdef, version: 0x1 }` with only `version` edited read
  `weakly_exposed` and asked for an assertion on the unchanged `counter`; the
  multi-line form already read correctly. A line that adds an `else if`
  condition next to an unchanged one now probes the added condition (#6731).
