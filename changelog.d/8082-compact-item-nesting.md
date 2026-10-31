<!-- section: Fixed -->
- Rust owner matching reuses compact function nesting facts for identical indexed
  source while preserving line ranges, ambiguity and conditional scopes (#8082).
- Share immutable nesting facts through `Arc` when composing with the run-scoped
  owner-pin mutex, preserving source-identity invalidation (#8082).
