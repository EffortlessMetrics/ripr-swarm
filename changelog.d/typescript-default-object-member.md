<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0243 decision 15): a test that calls
  `utils.merge(...)` through a default import of a module ending in
  `export default { merge }` now reaches the function `merge` and, with an
  exact-value assertion, reads `exposed` instead of `no_static_path`. Only a
  plain `{ name }` or `{ name: name }` object qualifies; a spread, method,
  getter, computed key or duplicate key, and any test use of the import other
  than a call of that member (alias, reassignment, shadow), still refuse (#8916).
