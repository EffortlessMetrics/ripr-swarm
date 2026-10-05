<!-- section: Fixed -->
- A field edit inside a one-line struct literal is now probed as that
  field, not as the first field on the line. Before, `Id { counter: 0x00ab_cdef,
  version: 0x1 }` with only `version` edited read `weakly_exposed` and asked
  for an assertion on the unchanged `counter`; the multi-line form already
  read correctly (#6731).
