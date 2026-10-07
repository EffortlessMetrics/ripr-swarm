<!-- section: Fixed -->
- A table-driven test now pairs a boundary input with its exact assertion:
  in `for (amount, want) in [(99, 99), (100, 90)] { assert_eq!(gate(amount),
  want); }` the loop variable carries each row's value, so a row at the
  changed boundary makes the predicate `exposed`. Cells of one row stay
  together, and each owner call now contributes one input row per table
  row instead of only the first. A loop that can `break`, `continue` or
  `return`, or that branches inside its body, gives no rows
  (RIPR-SPEC-0186, #5328).
