<!-- section: Fixed -->
- In a proc-macro crate, an unmodelled-syntax `static_unknown` finding whose owner has a production caller no longer reports `reach no` and "No static test path reaches this change"; it reads `reach unknown` with the derive/attribute-expansion guidance, like its sibling probes (#10088).
