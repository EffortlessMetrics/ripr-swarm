<!-- section: Fixed -->
- Python tests that apply a builtin protocol entry (`reversed`, `len`, `bool`,
  `iter`, `next`, `hash`, `str`, `repr`) to a receiver built from the owner class
  now relate to the matching dunder owner with their own oracle
  (`protocol_entry_call`) instead of leaving the whole class unknown. A strong
  oracle that takes the entry as an asserted operand aligns `direct`; operator
  syntax (`x[i]`, `a == b`) is not yet credited, and `sorted`/`set`/`frozenset`
  around the entry are not transparent because they erase the ordering the
  dunder determines (#8563).
