<!-- section: Fixed -->
- Python predicate-boundary gate no longer fires on a reversing comparison flip
  (`a > b` -> `a < b`) that a strong call on unequal literals discriminates, and
  declines the boundary gap (unknown) when a compared operand is rebound inside
  the owner, such as a generator's loop counter (#9426, #9412).
