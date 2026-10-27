<!-- section: Added -->
- A changed `const`/`static` that one same-file function reads as a single
  expression is now decided by evaluating it. When a test's assertion holds
  with the old value and fails with the new one the finding is `exposed`; when
  every related assertion holds with both it is `weakly_exposed`. Control flow,
  lookups, bindings, non-literal expected values, cfg-gated or duplicated
  readers and value-preserving rewrites keep `static_unknown` (RIPR-SPEC-0048,
  #9871).
