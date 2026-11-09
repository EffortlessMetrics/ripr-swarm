<!-- section: Changed -->
- TypeScript checks: reach a module-level non-function `const` (regex, string, symbol, lookup object) through the same-module function that reads it, so a changed `const` read only by a tested function reads `static_unknown` instead of a false `no_static_path` (#13592).
