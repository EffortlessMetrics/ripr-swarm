<!-- section: Changed -->
- A private crate-root glob such as `use colored::*` no longer withholds
  assertion credit from descendant tests when the import cannot reach them;
  ambiguous imports and unresolved composition still retain the refusal (#13269).
