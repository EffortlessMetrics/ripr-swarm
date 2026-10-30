<!-- section: Added -->
- A changed single-line `const`/`static` that one same-file function reads as a single
  expression is now decided by evaluating it. When a test's assertion holds
  with the old value and fails with the new one the finding is `exposed`; when
  every related assertion holds with both it is `weakly_exposed`. Control flow,
  lookups, bindings, non-literal expected values, cfg-gated or duplicated
  readers, self-computed expectations and readers in another module scope keep `static_unknown`. An edit that keeps the value (`10` to
  `5 + 5`) is asked about the neighbouring values and is `exposed` only when
  every neighbour breaks an assertion (RIPR-SPEC-0048,
  #9871).
