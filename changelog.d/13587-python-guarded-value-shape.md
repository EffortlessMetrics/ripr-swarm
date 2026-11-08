<!-- section: Fixed -->
- Read a Python single-name value assignment (`text = f"{text} ..."`) as a
  value-shaped change instead of a predicate, so a value edit inside an
  unchanged `if` body is no longer described as "a condition or comparison
  changed" with a `predicate_boundary` gap id (#13587). The guard line itself,
  comparisons, boolean and conditional right sides, augmented and tuple
  assignments keep the predicate shape. The value shape claims no propagation
  to an output and gives no repair value.
