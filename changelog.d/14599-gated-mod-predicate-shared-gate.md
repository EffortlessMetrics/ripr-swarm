<!-- section: Fixed -->
- Predicate `assert_eq!` oracles in a test file composed through a gated
  `mod` declaration are admitted only under the shared-gate rule: the owner
  must be reached solely through that same declaration, so an unknown feature
  gate no longer grants credit to an owner the crate also compiles without it
  (RIPR-SPEC-0197, #14537, #14599).
