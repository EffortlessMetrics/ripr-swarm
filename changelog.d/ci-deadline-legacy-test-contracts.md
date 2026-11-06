<!-- section: Internal / CI -->
- Corrected legacy CI leaf tests to distinguish mandatory identity checks from
  conditional deadline checks and to require disjoint owned and hosted Rust
  setup guards. Hostile controls reject skipped identity verification, removed
  deadline guards, quoted or ambiguous identity-step YAML keys, and setup
  crossing profile or surface boundaries (#8082).
